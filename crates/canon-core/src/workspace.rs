//! Workspace root and dynamic socket discovery.
//!
//! Provides deterministic resolution for Git repositories, linked Git worktrees,
//! subdirectories, and non-Git workspaces, ensuring collision-free IPC sockets.

use std::path::{Path, PathBuf};

/// 64-bit SplitMix pseudo-random mixer.
#[inline]
pub const fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Hashes a string deterministically using FNV-1a mixed with SplitMix64.
#[inline]
pub fn splitmix64_str(s: &str) -> u64 {
    let mut h: u64 = 0xCBF2_9CE4_8422_2325;
    for &b in s.as_bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0100_0000_01B3);
    }
    splitmix64(h)
}

/// Resolves the canonical root directory of the active workspace.
///
/// Hierarchy:
/// 1. Ascend looking for `.git` (file or directory, handling worktrees and standard repos).
/// 2. Ascend looking for `.canon.toml` or `.groundtruth.toml`.
/// 3. Fallback to `start_dir`.
pub fn resolve_workspace_root(start_dir: &Path) -> PathBuf {
    let start = if start_dir.is_file() {
        start_dir.parent().unwrap_or(start_dir)
    } else {
        start_dir
    };

    let canonical_start = std::fs::canonicalize(start).unwrap_or_else(|_| start.to_path_buf());

    // 1. Check for .git boundary (works for directories and worktree gitdir pointer files)
    for ancestor in canonical_start.ancestors() {
        if ancestor.join(".git").exists() {
            return ancestor.to_path_buf();
        }
    }

    // 2. Check for config file boundary
    for ancestor in canonical_start.ancestors() {
        if ancestor.join(".canon.toml").exists() || ancestor.join(".groundtruth.toml").exists() {
            return ancestor.to_path_buf();
        }
    }

    // 3. Fallback to start directory
    canonical_start
}

/// Derives a deterministic, length-safe Unix domain socket path for a workspace root.
///
/// Guarantees the path remains under POSIX `sun_path` length caps (104 bytes on macOS, 108 bytes on Linux).
pub fn derive_socket_path(workspace_root: &Path) -> PathBuf {
    let canonical =
        std::fs::canonicalize(workspace_root).unwrap_or_else(|_| workspace_root.to_path_buf());
    let hash = splitmix64_str(&canonical.to_string_lossy());
    let socket_name = format!("canon-{:012x}.sock", hash & 0x0000_FFFF_FFFF_FFFF);
    std::env::temp_dir().join(socket_name)
}

/// Resolves the active socket path, giving precedence to explicit overrides.
pub fn resolve_active_socket(explicit_socket: Option<&str>, start_dir: &Path) -> PathBuf {
    if let Some(sock) = explicit_socket {
        if !sock.trim().is_empty() {
            return PathBuf::from(sock);
        }
    }
    let root = resolve_workspace_root(start_dir);
    derive_socket_path(&root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_resolve_workspace_root_git_repo() {
        let dir = tempdir().unwrap();
        let git_dir = dir.path().join(".git");
        std::fs::create_dir(&git_dir).unwrap();

        let sub = dir.path().join("crates").join("canon-core");
        std::fs::create_dir_all(&sub).unwrap();

        let resolved = resolve_workspace_root(&sub);
        let expected = std::fs::canonicalize(dir.path()).unwrap();
        assert_eq!(resolved, expected);
    }

    #[test]
    fn test_resolve_workspace_root_git_worktree() {
        let dir = tempdir().unwrap();
        let git_file = dir.path().join(".git");
        std::fs::write(&git_file, "gitdir: /path/to/main/.git/worktrees/wt\n").unwrap();

        let sub = dir.path().join("frontend").join("src");
        std::fs::create_dir_all(&sub).unwrap();

        let resolved = resolve_workspace_root(&sub);
        let expected = std::fs::canonicalize(dir.path()).unwrap();
        assert_eq!(resolved, expected);
    }

    #[test]
    fn test_resolve_workspace_root_config_fallback() {
        let dir = tempdir().unwrap();
        let config_file = dir.path().join(".canon.toml");
        std::fs::write(&config_file, "[daemon]\nenable_file_watcher = true\n").unwrap();

        let sub = dir.path().join("sub").join("nested");
        std::fs::create_dir_all(&sub).unwrap();

        let resolved = resolve_workspace_root(&sub);
        let expected = std::fs::canonicalize(dir.path()).unwrap();
        assert_eq!(resolved, expected);
    }

    #[test]
    fn test_resolve_workspace_root_no_markers() {
        let dir = tempdir().unwrap();
        let sub = dir.path().join("sub");
        std::fs::create_dir_all(&sub).unwrap();

        let resolved = resolve_workspace_root(&sub);
        let expected = std::fs::canonicalize(&sub).unwrap();
        assert_eq!(resolved, expected);
    }

    #[test]
    fn test_derive_socket_path_length_and_format() {
        let dir = tempdir().unwrap();
        let socket_path = derive_socket_path(dir.path());
        let path_str = socket_path.to_str().unwrap();

        // Must fit within POSIX sun_path limit (104 bytes)
        assert!(
            path_str.len() < 104,
            "socket path '{}' exceeds 104 bytes ({})",
            path_str,
            path_str.len()
        );

        let file_name = socket_path.file_name().unwrap().to_str().unwrap();
        assert!(file_name.starts_with("canon-"));
        assert!(file_name.ends_with(".sock"));
        assert_eq!(file_name.len(), 23); // canon- (6) + 12 hex + .sock (5)
    }

    #[test]
    fn test_derive_socket_path_deterministic() {
        let dir1 = tempdir().unwrap();
        let dir2 = tempdir().unwrap();

        let sock1_a = derive_socket_path(dir1.path());
        let sock1_b = derive_socket_path(dir1.path());
        assert_eq!(sock1_a, sock1_b);

        let sock2 = derive_socket_path(dir2.path());
        assert_ne!(sock1_a, sock2);
    }

    #[test]
    fn test_resolve_active_socket_explicit_override() {
        let dir = tempdir().unwrap();
        let resolved = resolve_active_socket(Some("/tmp/custom-socket.sock"), dir.path());
        assert_eq!(resolved, PathBuf::from("/tmp/custom-socket.sock"));

        let derived = resolve_active_socket(None, dir.path());
        assert_ne!(derived, PathBuf::from("/tmp/custom-socket.sock"));
        assert!(
            derived
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("canon-")
        );
    }
}
