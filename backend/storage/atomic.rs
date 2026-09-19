//! The single atomic-write primitive for files this app owns.
//!
//! # What this guarantees, precisely
//!
//! **Atomicity — no reader ever observes a partial file.** From `rename(2)`
//! alone: a reader gets the complete old file or the complete new one, never
//! a mixture. This survives process death, since the page cache outlives the
//! process. This is the property the dirty-tree guard depends on — it lets
//! "the file on disk is a complete, intentional state" be an assumption
//! rather than a hope.
//!
//! **Durability — deliberately NOT claimed beyond this:** a power loss or
//! kernel panic may cost the *most recent write*, never the file's
//! integrity. `File::sync_all()` is `fsync(2)`, which on macOS does not
//! flush the drive's own volatile cache; only `fcntl(F_FULLFSYNC)` does.
//! `F_FULLFSYNC` is deliberately not used: it costs tens of milliseconds on
//! every transaction write to buy survival of the last write, which is not
//! what any of this protects against. The `sync_all()` below is kept not for
//! APFS — whose copy-on-write checkpoints already order data before metadata
//! — but for the odd volume: an external disk, a network mount, HFS+.
//!
//! There is deliberately no directory fsync: it buys a guarantee macOS does
//! not honour.

use anyhow::{Context, Result};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use tempfile::NamedTempFile;

/// Default mode for a file this app creates. Matches what `std::fs::write`
/// produces under the usual umask, so adopting this function does not change
/// the permissions of files that did not exist before.
const DEFAULT_MODE: u32 = 0o644;

/// Write `contents` to `path` atomically.
///
/// Signature mirrors [`std::fs::write`] so every adoption is a mechanical
/// identifier swap.
///
/// Hand-rolling this was considered and rejected: "the temp file is removed
/// on every failure path" is a promise a human has to keep by hand at every
/// `?`, which is exactly what `Drop` exists to make unnecessary.
/// `NamedTempFile` also supplies collision-safe naming, which six
/// hand-rolled `path.with_extension("tmp")` copies in this codebase did not.
pub fn write(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> Result<()> {
    let path = path.as_ref();
    let dir = path.parent().unwrap_or_else(|| Path::new("."));

    let mut tmp = NamedTempFile::new_in(dir)
        .with_context(|| format!("creating a temp file alongside {}", path.display()))?;

    tmp.write_all(contents.as_ref())
        .with_context(|| format!("writing temp contents for {}", path.display()))?;

    // `NamedTempFile` creates at 0600. Persisting without this would TIGHTEN
    // an existing 0644 file rather than merely failing to preserve it.
    let mode = std::fs::metadata(path)
        .map(|m| m.permissions().mode() & 0o777)
        .unwrap_or(DEFAULT_MODE);
    tmp.as_file()
        .set_permissions(std::fs::Permissions::from_mode(mode))
        .with_context(|| format!("setting mode {mode:o} for {}", path.display()))?;

    tmp.as_file()
        .sync_all()
        .with_context(|| format!("flushing temp contents for {}", path.display()))?;

    tmp.persist(path)
        .map_err(|e| e.error)
        .with_context(|| format!("renaming temp file into place at {}", path.display()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::*;

    #[test]
    fn writes_the_contents() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f.csv");
        write(&path, b"hello").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"hello");
    }

    #[test]
    fn replaces_existing_contents() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f.csv");
        std::fs::write(&path, b"old").unwrap();
        write(&path, b"new").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"new");
    }

    #[test]
    fn leaves_no_temp_residue_on_success() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path().join("f.csv"), b"x").unwrap();
        let entries: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(entries, vec!["f.csv".to_string()], "only the target file may remain");
    }

    /// The failure is induced deterministically by making the parent
    /// directory unwritable, which fails `NamedTempFile::new_in` for a
    /// non-root user on macOS while leaving the existing file readable.
    /// Named explicitly so this does not get written as `#[ignore]`.
    #[test]
    fn a_failed_write_leaves_the_prior_file_byte_identical() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f.csv");
        std::fs::write(&path, b"original").unwrap();

        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o555)).unwrap();
        let result = write(&path, b"replacement");
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755)).unwrap();

        assert!(result.is_err(), "a write into an unwritable directory must fail");
        assert_eq!(
            std::fs::read(&path).unwrap(),
            b"original",
            "the prior file must be byte-identical after a failed write"
        );
        let entries: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(entries, vec!["f.csv".to_string()], "no temp residue after failure");
    }

    /// `NamedTempFile` creates at 0600. Persisting without setting the mode
    /// would silently TIGHTEN an existing 0644 file, not merely fail to
    /// preserve it.
    #[test]
    fn preserves_the_existing_files_mode() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f.csv");
        std::fs::write(&path, b"old").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();

        write(&path, b"new").unwrap();

        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o644, "an existing file's mode must survive the replace");
    }

    #[test]
    fn a_new_file_defaults_to_0644() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("new.csv");
        write(&path, b"x").unwrap();

        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o644);
    }
}
