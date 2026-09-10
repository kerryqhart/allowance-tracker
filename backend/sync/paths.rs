use std::path::{Path, PathBuf};

/// The set of paths the sync-safety guard needs, all explicitly injected by
/// the caller rather than resolved internally.
///
/// `home` in particular exists so that [`is_cloud_synced`] can stay a pure
/// predicate: the guard compares candidates against
/// `home/Library/Mobile Documents` and `home/Documents`, and if it called
/// `dirs::home_dir()` itself it could not be exercised by a unit test — it
/// would be verified once by hand and then silently regress. For a guard
/// whose entire job is preventing the original bug this project exists to
/// fix (iCloud replicating a live `.git` directory and corrupting the
/// index, packs, or refs), a silent regression is unacceptable.
#[derive(Debug, Clone)]
pub struct SyncPaths {
    pub data_dir: PathBuf,
    pub children_root: PathBuf,
    pub lgs_binary: PathBuf,
    pub cloud_root: Option<PathBuf>,
    pub home: PathBuf,
}

/// Which rule fired when [`is_cloud_synced`] rejects a candidate path.
///
/// A guard that only says "no" is one the user cannot act on — every variant
/// carries a [`Reason::message`] that names the hazard so the UI can show the
/// user something they can actually do something about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    InsideCloudRoot,
    MobileDocuments,
    DocumentsSyncOn,
}

impl Reason {
    pub fn message(&self) -> &'static str {
        match self {
            Reason::InsideCloudRoot =>
                "that folder is inside the lgs cloud root — two systems replicating the same bytes is the failure this design exists to avoid",
            Reason::MobileDocuments =>
                "that folder is in iCloud Drive, which writes conflict copies inside .git and can corrupt the repository",
            Reason::DocumentsSyncOn =>
                "Desktop & Documents Folders syncing is on, so iCloud would replicate this repository's .git",
        }
    }
}

/// Reject any candidate git-working-directory path that a cloud-drive sync
/// agent could also be replicating.
///
/// This is a pure predicate over `env` (a [`SyncPaths`]) and
/// `documents_is_symlink` — both supplied by the caller. It must never call
/// `dirs::home_dir()`, read the filesystem, or shell out: doing so would make
/// it untestable, and an untestable guard for this particular failure mode
/// is the one place a silent regression means corrupting the user's data.
///
/// `documents_is_symlink` is the caller's observation of whether
/// `~/Documents` is a symlink — the reliable signal that Desktop & Documents
/// Folders syncing is on. Do NOT switch this to read the
/// `FXICloudDriveDocuments` Finder preference instead: on the machine this
/// was verified on, that preference reads `1` while the feature is actually
/// OFF (`~/Documents` is a real directory, and the entries inside iCloud
/// Drive are hand-made symlinks pointing back OUT to the local folders).
/// Trusting the pref produces a false positive there, which is worse than a
/// false negative here — it would block a safe setup, and the next person
/// to "fix" the annoyance might delete the check instead of the pref.
pub fn is_cloud_synced(
    candidate: &Path,
    env: &SyncPaths,
    documents_is_symlink: bool,
) -> Option<Reason> {
    if let Some(root) = &env.cloud_root {
        if candidate.starts_with(root) {
            return Some(Reason::InsideCloudRoot);
        }
    }
    if candidate.starts_with(env.home.join("Library/Mobile Documents")) {
        return Some(Reason::MobileDocuments);
    }
    if documents_is_symlink && candidate.starts_with(env.home.join("Documents")) {
        return Some(Reason::DocumentsSyncOn);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn env() -> SyncPaths {
        SyncPaths {
            data_dir: PathBuf::from("/Users/k/Documents/Allowance Tracker"),
            children_root: PathBuf::from("/Users/k/Library/Application Support/Allowance Tracker/children"),
            lgs_binary: PathBuf::from("/Users/k/Library/Application Support/Allowance Tracker/bin/lgs"),
            cloud_root: Some(PathBuf::from("/Users/k/Library/CloudStorage/ProtonDrive-x/Code")),
            home: PathBuf::from("/Users/k"),
        }
    }

    #[test]
    fn rejection_table() {
        let cases: Vec<(&str, bool, Option<Reason>)> = vec![
            ("/Users/k/Library/Application Support/Allowance Tracker/children/keiko", false, None),
            ("/Users/k/Library/CloudStorage/ProtonDrive-x/Code/inside", false, Some(Reason::InsideCloudRoot)),
            ("/Users/k/Library/Mobile Documents/com~apple~CloudDocs/x", false, Some(Reason::MobileDocuments)),
            // Documents is only a hazard when Desktop & Documents sync is really on.
            ("/Users/k/Documents/Allowance Tracker/keiko", true, Some(Reason::DocumentsSyncOn)),
            ("/Users/k/Documents/Allowance Tracker/keiko", false, None),
        ];
        for (path, docs_symlink, expected) in cases {
            let got = is_cloud_synced(std::path::Path::new(path), &env(), docs_symlink);
            assert_eq!(got, expected, "for {path} (symlink={docs_symlink})");
        }
    }

    // The brief's five rows are necessary but not sufficient for a guard
    // this safety-critical. These pin the edge cases that a careless rewrite
    // (especially one that swaps `Path::starts_with` for a string comparison)
    // would get wrong.

    /// The candidate can equal the cloud root exactly, not just be a child
    /// of it — `Path::starts_with` treats an exact match as a match, and the
    /// guard must too.
    #[test]
    fn cloud_root_itself_is_rejected() {
        let e = env();
        let root = e.cloud_root.clone().unwrap();
        assert_eq!(is_cloud_synced(&root, &e, false), Some(Reason::InsideCloudRoot));
    }

    /// `~/Documents` itself (not just something under it) is a hazard once
    /// Desktop & Documents sync is really on.
    #[test]
    fn documents_itself_is_rejected_when_symlinked() {
        let e = env();
        let docs = e.home.join("Documents");
        assert_eq!(is_cloud_synced(&docs, &e, true), Some(Reason::DocumentsSyncOn));
    }

    /// Mobile Documents is a hazard at any depth underneath it, not just one
    /// level down.
    #[test]
    fn mobile_documents_rejects_at_any_depth() {
        let e = env();
        let deep = e.home.join("Library/Mobile Documents/com~apple~CloudDocs/a/b/c/d");
        assert_eq!(is_cloud_synced(&deep, &e, false), Some(Reason::MobileDocuments));
    }

    /// A sibling directory that merely SHARES A PREFIX STRING with the cloud
    /// root (cloud root `/x/Code`, candidate `/x/CodeOther/child`) must be
    /// accepted. `Path::starts_with` compares components, so this is already
    /// correct — but if anyone ever rewrites the check with a string
    /// comparison (`candidate.to_str().starts_with(root_str)`), this is
    /// exactly how the bug would show up. Pinned here so that regression is
    /// caught immediately.
    #[test]
    fn sibling_directory_sharing_a_prefix_string_with_cloud_root_is_accepted() {
        let mut e = env();
        e.cloud_root = Some(PathBuf::from("/x/Code"));
        e.home = PathBuf::from("/x/home");
        let candidate = PathBuf::from("/x/CodeOther/child");
        assert_eq!(is_cloud_synced(&candidate, &e, false), None);
    }

    /// No cloud root configured yet (e.g. before the first `lgs register`)
    /// must not reject anything on that basis — there is nothing to compare
    /// against, so that rule simply does not fire.
    #[test]
    fn no_cloud_root_configured_accepts_an_otherwise_fine_path() {
        let mut e = env();
        e.cloud_root = None;
        let candidate = PathBuf::from(
            "/Users/k/Library/Application Support/Allowance Tracker/children/keiko",
        );
        assert_eq!(is_cloud_synced(&candidate, &e, false), None);
    }
}
