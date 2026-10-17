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
}
