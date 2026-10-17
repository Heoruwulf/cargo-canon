//! Non-destructive advisory tool hook handler for agent integrations.

use crate::error::DaemonError;
use canon_core::model::SliceMode;
use canon_slice::{SymbolIndex, slice_target};
use std::path::Path;

/// Executes non-destructive post-read advisory context injection.
///
/// CRITICAL INVARIANT: The target file on disk is guaranteed strictly byte-identical
/// before and after this function executes. Injected context is emitted strictly out-of-band.
pub fn execute_post_read_hook(
    file_path: &Path,
    index: Option<&SymbolIndex>,
) -> Result<String, DaemonError> {
    if !file_path.is_file() {
        return Err(DaemonError::Watcher(format!(
            "target file '{}' does not exist or is not a regular file",
            file_path.display()
        )));
    }

    // Read original bytes to ensure byte-identity invariant
    let original_bytes = std::fs::read(file_path).map_err(DaemonError::Io)?;

    // Slice out-of-band
    let bundle = slice_target(file_path, SliceMode::Bundle, index)?;
    let advisory_block = bundle.format_markdown();

    // Verify invariant: file bytes on disk remain completely unmodified
    let check_bytes = std::fs::read(file_path).map_err(DaemonError::Io)?;
    if original_bytes != check_bytes {
        // Restore if an external race modified it (defensive invariant protection)
        let _ = std::fs::write(file_path, &original_bytes);
        return Err(DaemonError::Watcher(
            "CRITICAL INVARIANT VIOLATION: target file was modified during post-read hook execution".into()
        ));
    }

    Ok(advisory_block)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_non_destructive_invariant() {
        let mut file = NamedTempFile::new().unwrap();
        let content = b"pub fn calculate(x: u32) -> u32 {\n    x * 2\n}\n";
        file.write_all(content).unwrap();
        file.flush().unwrap();

        let advisory = execute_post_read_hook(file.path(), None).unwrap();
        assert!(advisory.contains("# Canon Context Slice"));
        assert!(advisory.contains("pub fn calculate"));

        // Verify file is 100% byte-identical
        let disk_bytes = std::fs::read(file.path()).unwrap();
        assert_eq!(disk_bytes, content);
    }

    #[test]
    fn test_non_destructive_invariant_tsx() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("Dashboard.tsx");
        let content = b"export interface Props { id: string; }\nexport function Dashboard(props: Props) {\n    return <div>{props.id}</div>;\n}\n";
        std::fs::write(&file_path, content).unwrap();

        let advisory = execute_post_read_hook(&file_path, None).unwrap();
        assert!(advisory.contains("# Canon Context Slice"));
        assert!(advisory.contains("export interface Props"));
        assert!(advisory.contains("```typescript"));

        // Critical invariant: file bytes on disk MUST be strictly identical
        let disk_bytes = std::fs::read(&file_path).unwrap();
        assert_eq!(disk_bytes, content);
    }

    #[test]
    fn test_non_destructive_invariant_go() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("service.go");
        let content = b"package service\n\ntype User struct {\n    ID string `json:\"id\"`\n}\n\nfunc (u *User) GetID() string {\n    return u.ID\n}\n";
        std::fs::write(&file_path, content).unwrap();

        let advisory = execute_post_read_hook(&file_path, None).unwrap();
        assert!(advisory.contains("# Canon Context Slice"));
        assert!(advisory.contains("type User struct"));
        assert!(advisory.contains("func (u *User) GetID() string;"));
        assert!(advisory.contains("```go"));

        // Critical invariant: file bytes on disk MUST remain 100% strictly identical
        let disk_bytes = std::fs::read(&file_path).unwrap();
        assert_eq!(disk_bytes, content);
    }
}
