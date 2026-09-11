use crate::backend::sync::paths::SyncPaths;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

/// lgs shells out to `git` for every repository operation, and spawns
/// `git http-backend` to serve the remote. Bundling lgs does NOT remove this
/// dependency — on a Mac without Xcode Command Line Tools, /usr/bin/git is a
/// stub that opens a dialog and exits non-zero.
pub fn git_is_available() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub const GIT_MISSING_MESSAGE: &str = "Sync needs Apple's Command Line Tools, which include git. \
Open Terminal and run: xcode-select --install";

pub fn copy_binary(src: &Path, dst: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("creating {parent:?}"))?;
    }
    // Write to a temp name in the SAME directory as the destination (not a
    // system temp dir), so the rename below is same-filesystem and therefore
    // atomic — a crash mid-copy cannot leave a truncated binary that launchd
    // would happily keep executing.
    let tmp = dst.with_extension("tmp");
    std::fs::copy(src, &tmp).with_context(|| format!("copying {src:?} -> {tmp:?}"))?;
    std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))
        .with_context(|| format!("setting permissions on {tmp:?}"))?;
    // If the rename fails, don't leave the `.tmp` staging file behind — but
    // keep the rename's own error as the reported cause, not a cleanup error.
    std::fs::rename(&tmp, dst)
        .map_err(|err| {
            let _ = std::fs::remove_file(&tmp);
            err
        })
        .with_context(|| format!("renaming {tmp:?} into {dst:?}"))?;
    Ok(())
}

/// Resolve the bundled binary and copy it to the stable path.
pub fn ensure_lgs_binary(env: &SyncPaths) -> Result<PathBuf> {
    let bundled = bundled_lgs_path()?;
    copy_binary(&bundled, &env.lgs_binary)?;
    Ok(env.lgs_binary.clone())
}

/// Where the bundled `lgs` binary might be, checked in order.
///
/// Exactly where `cargo-bundle` places a `resources` entry that lives
/// outside the crate directory (like `../target/release/lgs`) was not
/// verified against a real produced `.app` — if `cargo-bundle` strips
/// resource paths relative to a common ancestor instead of flattening them,
/// `lgs` could land at `Contents/Resources/target/release/lgs` rather than
/// the flat `Contents/Resources/lgs` a single-candidate resolver would
/// assume. Rather than bet on one layout, this checks several plausible
/// ones, so a packaging surprise degrades to "found it in a different spot"
/// instead of "release builds are silently broken while every test stays
/// green".
fn bundled_lgs_candidates(exe_dir: &Path) -> Vec<PathBuf> {
    vec![
        // .app/Contents/MacOS/<exe> -> .app/Contents/Resources/lgs
        exe_dir.join("../Resources/lgs"),
        // In case cargo-bundle preserves the `target/release/` portion of a
        // resource path that lives outside the crate directory.
        exe_dir.join("../Resources/target/release/lgs"),
        // Dev builds: target/<profile>/lgs, produced by build.rs next to the
        // binary itself.
        exe_dir.join("lgs"),
    ]
}

fn bundled_lgs_path() -> Result<PathBuf> {
    let exe = std::env::current_exe().context("resolving current exe")?;
    let exe_dir = exe.parent().unwrap_or_else(|| Path::new("."));
    let candidates = bundled_lgs_candidates(exe_dir);

    for candidate in &candidates {
        if candidate.exists() {
            return Ok(candidate.clone());
        }
    }

    let tried = candidates
        .iter()
        .map(|p| format!("{p:?}"))
        .collect::<Vec<_>>()
        .join(", ");
    anyhow::bail!("bundled lgs binary not found; tried: {tried}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn copies_the_binary_out_of_the_bundle_and_is_idempotent() {
        // The plist points at whatever path we install from
        // (lgs service.rs uses current_exe()), so it must be a stable location
        // outside the .app — otherwise moving the app to /Applications leaves
        // launchd retrying a missing path forever under KeepAlive.
        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();
        let src = src_dir.path().join("lgs");
        std::fs::write(&src, b"#!/bin/sh\nexit 0\n").unwrap();

        let dst = dst_dir.path().join("bin").join("lgs");
        copy_binary(&src, &dst).unwrap();
        assert!(dst.exists());

        copy_binary(&src, &dst).unwrap();
        assert!(dst.exists(), "re-running must not fail or corrupt the target");
    }

    #[test]
    fn copied_binary_is_executable() {
        use std::os::unix::fs::PermissionsExt;
        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();
        let src = src_dir.path().join("lgs");
        std::fs::write(&src, b"x").unwrap();
        let dst = dst_dir.path().join("lgs");
        copy_binary(&src, &dst).unwrap();
        let mode = std::fs::metadata(&dst).unwrap().permissions().mode();
        assert_eq!(mode & 0o111, 0o111, "must be executable by owner/group/other");
    }

    /// A stale binary surviving an app update is the exact failure mode this
    /// design exists to avoid: the destination must end up with the NEW
    /// bytes, not silently keep whatever was there before.
    #[test]
    fn copy_replaces_an_existing_older_binary() {
        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();
        let src = src_dir.path().join("lgs");
        std::fs::write(&src, b"new version bytes").unwrap();

        let dst = dst_dir.path().join("lgs");
        std::fs::write(&dst, b"old stale version").unwrap();

        copy_binary(&src, &dst).unwrap();

        let contents = std::fs::read(&dst).unwrap();
        assert_eq!(
            contents, b"new version bytes",
            "the old binary must be replaced, not kept"
        );
    }

    /// The destination's parent directory (e.g. the `bin/` in `.../bin/lgs`)
    /// does not exist yet on a first run — `ensure_lgs_binary` must create it
    /// rather than fail.
    #[test]
    fn copy_creates_missing_parent_directory() {
        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();
        let src = src_dir.path().join("lgs");
        std::fs::write(&src, b"binary").unwrap();

        let dst = dst_dir.path().join("does").join("not").join("exist").join("lgs");
        assert!(!dst.parent().unwrap().exists());

        copy_binary(&src, &dst).unwrap();

        assert!(dst.exists());
    }

    /// The atomic write-then-rename must not leave its `.tmp` staging file
    /// behind after a successful copy — the destination directory should
    /// contain only the final binary.
    #[test]
    fn no_temp_artifact_is_left_behind_after_a_successful_copy() {
        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();
        let src = src_dir.path().join("lgs");
        std::fs::write(&src, b"binary").unwrap();

        let dst = dst_dir.path().join("bin").join("lgs");
        copy_binary(&src, &dst).unwrap();

        let entries: Vec<_> = std::fs::read_dir(dst.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(
            entries,
            vec![std::ffi::OsString::from("lgs")],
            "no temp artifact should remain: {entries:?}"
        );
    }

    /// This asserts the local dev environment (git is present on this
    /// machine) rather than simulating git's absence — that would require
    /// manipulating PATH, out of scope here. The point is still to fail if
    /// the function ever panics instead of returning a plain bool, e.g. on a
    /// broken pipe or an unexpected exit status.
    #[test]
    fn git_is_available_does_not_panic() {
        let available: bool = git_is_available();
        assert!(available, "git is expected to be present on this dev machine");
    }

    /// A failed rename must not leave the `.tmp` staging file behind. Force
    /// the rename to fail by making `dst`'s parent a location where `dst`
    /// itself is a directory — `rename(tmp_file, existing_dir)` fails on
    /// every POSIX system — then check the parent holds nothing named after
    /// the temp file's extension.
    #[test]
    fn failed_rename_does_not_leave_a_temp_artifact_behind() {
        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();
        let src = src_dir.path().join("lgs");
        std::fs::write(&src, b"binary").unwrap();

        // `dst` is itself a directory, so `fs::rename(tmp, dst)` fails
        // (cannot rename a file onto a directory).
        let dst = dst_dir.path().join("lgs");
        std::fs::create_dir(&dst).unwrap();

        let result = copy_binary(&src, &dst);
        assert!(result.is_err(), "renaming a file onto a directory must fail");

        let tmp = dst.with_extension("tmp");
        assert!(!tmp.exists(), "the .tmp staging file must be cleaned up on failure");
    }

    /// If none of the candidate locations have `lgs`, the error must name
    /// every path tried — a packaging mistake (e.g. cargo-bundle placing the
    /// binary somewhere other than the flat `Contents/Resources/lgs` this
    /// resolver expects) must be diagnosable from the message alone.
    #[test]
    fn bundled_lgs_missing_error_names_every_candidate_tried() {
        let exe = std::env::current_exe().unwrap();
        let exe_dir = exe.parent().unwrap().to_path_buf();
        let expected_candidates = bundled_lgs_candidates(&exe_dir);

        // None of the candidates exist under the test binary's own directory
        // (target/debug/deps/...), so this exercises the real not-found path
        // rather than a synthetic one.
        for c in &expected_candidates {
            assert!(!c.exists(), "test assumption violated: {c:?} unexpectedly exists");
        }

        let err = bundled_lgs_path().expect_err("no candidate should exist here");
        let message = format!("{err}");
        for c in &expected_candidates {
            let formatted = format!("{c:?}");
            assert!(
                message.contains(&formatted),
                "error message must name {formatted}; got: {message}"
            );
        }
    }
}
