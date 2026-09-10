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
    // Write to a temp name then rename, so a crash mid-copy cannot leave a
    // truncated binary that launchd would happily keep executing.
    let tmp = dst.with_extension("tmp");
    std::fs::copy(src, &tmp).with_context(|| format!("copying {src:?} -> {tmp:?}"))?;
    std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))?;
    std::fs::rename(&tmp, dst).with_context(|| format!("renaming into {dst:?}"))?;
    Ok(())
}

/// Resolve the bundled binary and copy it to the stable path.
pub fn ensure_lgs_binary(env: &SyncPaths) -> Result<PathBuf> {
    let bundled = bundled_lgs_path()?;
    copy_binary(&bundled, &env.lgs_binary)?;
    Ok(env.lgs_binary.clone())
}

fn bundled_lgs_path() -> Result<PathBuf> {
    let exe = std::env::current_exe().context("resolving current exe")?;
    // .app/Contents/MacOS/<exe> -> .app/Contents/Resources/lgs
    if let Some(macos_dir) = exe.parent() {
        let candidate = macos_dir.join("../Resources/lgs");
        if candidate.exists() {
            return Ok(candidate);
        }
    }
    // Dev builds: target/debug/lgs, produced by build.rs.
    let dev = exe.parent().map(|p| p.join("lgs")).unwrap_or_default();
    if dev.exists() {
        return Ok(dev);
    }
    anyhow::bail!("bundled lgs binary not found next to {exe:?}")
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

    /// This does not attempt to simulate git's absence (that would require
    /// manipulating PATH, out of scope here) — it only pins that the check
    /// runs to completion on this machine and hands back a plain bool rather
    /// than panicking, e.g. on a broken pipe or an unexpected exit status.
    #[test]
    fn git_is_available_does_not_panic() {
        let available: bool = git_is_available();
        println!("git_is_available() = {available}");
    }
}
