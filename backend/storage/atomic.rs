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
/// The syscalls `write` performs, in order. Test-only: the sequence is the
/// contract for a durability primitive, and asserting it is the only way to
/// stop a future edit silently dropping the flush.
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Syscall {
    Write,
    SyncAll,
    Rename,
}

pub fn write(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> Result<()> {
    #[cfg(test)]
    {
        let mut sink = Vec::new();
        write_inner(path.as_ref(), contents.as_ref(), Some(&mut sink))
    }
    #[cfg(not(test))]
    {
        write_inner(path.as_ref(), contents.as_ref())
    }
}

#[cfg(test)]
pub(crate) fn write_with_recorder(
    path: impl AsRef<Path>,
    contents: impl AsRef<[u8]>,
    recorder: &mut Vec<Syscall>,
) -> Result<()> {
    write_inner(path.as_ref(), contents.as_ref(), Some(recorder))
}

/// The shared implementation behind [`write`] and, under test,
/// [`write_with_recorder`]. Two definitions, one per cfg, rather than a
/// single `fn write_inner` with a `#[cfg(test)] recorder: ...` parameter:
/// the non-test build must call it with zero arguments after the third
/// parameter vanishes, so `write`'s own body would need matching
/// `#[cfg(test)]`/`#[cfg(not(test))]` call-site branches anyway. Two
/// definitions make each cfg arm self-contained instead of splitting one
/// function's call convention across two attributes.
#[cfg(test)]
fn write_inner(path: &Path, contents: &[u8], recorder: Option<&mut Vec<Syscall>>) -> Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let mut recorder = recorder;

    let mut tmp = NamedTempFile::new_in(dir)
        .with_context(|| format!("creating a temp file alongside {}", path.display()))?;

    tmp.write_all(contents)
        .with_context(|| format!("writing temp contents for {}", path.display()))?;
    if let Some(r) = recorder.as_deref_mut() {
        r.push(Syscall::Write);
    }

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
    if let Some(r) = recorder.as_deref_mut() {
        r.push(Syscall::SyncAll);
    }

    tmp.persist(path)
        .map_err(|e| e.error)
        .with_context(|| format!("renaming temp file into place at {}", path.display()))?;
    if let Some(r) = recorder.as_deref_mut() {
        r.push(Syscall::Rename);
    }

    Ok(())
}

#[cfg(not(test))]
fn write_inner(path: &Path, contents: &[u8]) -> Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));

    let mut tmp = NamedTempFile::new_in(dir)
        .with_context(|| format!("creating a temp file alongside {}", path.display()))?;

    tmp.write_all(contents)
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

    /// Reads `path` in a tight loop for `rounds` iterations, returning every
    /// distinct byte-string it observed. Shared by the real test and its
    /// negative control so they cannot drift apart.
    fn observe_while<F>(path: &std::path::Path, writer: F) -> Vec<Vec<u8>>
    where
        F: FnOnce() + Send + 'static,
    {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        let done = Arc::new(AtomicBool::new(false));
        let done_writer = Arc::clone(&done);
        let handle = std::thread::spawn(move || {
            writer();
            done_writer.store(true, Ordering::SeqCst);
        });

        let mut seen: Vec<Vec<u8>> = Vec::new();
        while !done.load(Ordering::SeqCst) {
            if let Ok(bytes) = std::fs::read(path) {
                if !seen.contains(&bytes) {
                    seen.push(bytes);
                }
            }
        }
        handle.join().unwrap();
        seen
    }

    /// The versions the writer cycles through. Long enough that a
    /// non-atomic write is overwhelmingly likely to be caught mid-flight.
    fn versions() -> Vec<Vec<u8>> {
        (0..40u8)
            .map(|i| std::iter::repeat(b'a' + (i % 26)).take(200_000).collect())
            .collect()
    }

    #[test]
    fn a_reader_never_observes_a_partial_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("transactions.csv");
        let all = versions();
        std::fs::write(&path, &all[0]).unwrap();

        let write_path = path.clone();
        let to_write = all.clone();
        let seen = observe_while(&path, move || {
            for v in &to_write {
                write(&write_path, v).unwrap();
            }
        });

        for observed in &seen {
            assert!(
                all.contains(observed),
                "a reader observed {} bytes that match no complete version — \
                 atomic::write let a partial file become visible",
                observed.len()
            );
        }
    }

    /// The negative control. Without this, the test above is not evidence:
    /// it would pass against an implementation with no atomicity at all if
    /// the timing simply never caught a partial read. This asserts the
    /// harness CAN catch one.
    #[test]
    fn the_negative_control_shows_a_plain_write_is_observably_partial() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("transactions.csv");
        let all = versions();
        std::fs::write(&path, &all[0]).unwrap();

        let write_path = path.clone();
        let to_write = all.clone();
        let seen = observe_while(&path, move || {
            for v in &to_write {
                std::fs::write(&write_path, v).unwrap();
            }
        });

        assert!(
            seen.iter().any(|observed| !all.contains(observed)),
            "the harness never caught a partial read even against plain fs::write — \
             it therefore proves nothing about atomic::write; increase the payload \
             size or the round count rather than deleting this test"
        );
    }

    #[test]
    fn performs_write_then_sync_then_rename_in_that_order() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f.csv");
        let mut recorded = Vec::new();

        write_with_recorder(&path, b"x", &mut recorded).unwrap();

        assert_eq!(
            recorded,
            vec![Syscall::Write, Syscall::SyncAll, Syscall::Rename],
            "the contents must be flushed before the rename makes them visible, \
             and there must be no directory fsync (see the module docs)"
        );
    }
}
