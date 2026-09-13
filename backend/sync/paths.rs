use std::path::{Path, PathBuf};

/// Filenames this app owns inside a child's per-child git repo.
///
/// The single source of truth for "what does this app write into a child's
/// directory" — shared by
/// `AllowanceTrackerApp::commit_dirty_tree_to_unblock_fast_forward`
/// (`egui-frontend/src/ui/app_coordinator.rs`), which stages only these into
/// its unblock commit rather than `add_all(["*"])` (which would pick up
/// whatever untracked strays happen to be sitting in the child's data
/// directory — `.DS_Store`, editor swap files, anything macOS or an editor
/// drops there — and commit them into this child's synced history
/// permanently once pushed), and by `backend::sync::migration_lgs`'s commit
/// step, for the same reason. This project has already been misled by
/// duplicate definitions of lists like this drifting apart; keep it to one.
///
/// `parental_control_attempts.csv` is also owned by this system and lives in
/// the same per-child directory, but is deliberately NOT included here: see
/// the call sites for why each excludes it (a later merge commit still
/// captures it, or migration handles it separately/not at all yet).
pub(crate) const FILES_THIS_APP_OWNS: &[&str] =
    &["transactions.csv", "goals.csv", "child.yaml", "allowance_config.yaml"];

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
    /// The candidate contains `..` components that cannot be resolved
    /// lexically (a `ParentDir` would pop above the root). The guard cannot
    /// prove such a path is safe, so it fails closed rather than guessing.
    NotNormalisable,
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
            Reason::NotNormalisable =>
                "that path contains `..` components that cannot be resolved, so it cannot be checked for cloud syncing",
        }
    }
}

/// Lexically normalise `candidate`'s `.` and `..` components, without ever
/// touching the filesystem (no `canonicalize` — that would hit disk and
/// destroy the property that makes [`is_cloud_synced`] pure and testable).
///
/// `Path::starts_with` does purely literal, component-wise comparison and
/// never interprets `..`/`.` — so without this step, something like
/// `<children_root>/../../../Library/Mobile Documents/x` would compare as
/// unrelated to `~/Library/Mobile Documents` even though it resolves into
/// iCloud at the OS level. That is a false NEGATIVE — an accepted path whose
/// `.git` silently ends up inside iCloud — which is the dangerous direction
/// for this guard, so it must be closed.
///
/// Returns `None` if a `ParentDir` component would pop above the root: the
/// path escapes somewhere this function cannot reason about, and a guard
/// that cannot prove a path is safe must say no rather than guess. Callers
/// treat that as [`Reason::NotNormalisable`] — fail closed.
///
/// This normalisation is lexical, not physical: in the presence of
/// symlinks, POSIX resolves a real `..` against the *target* of a preceding
/// symlink's parent, not against the lexical parent computed here. That can
/// disagree with what the filesystem would actually do. This is the same
/// limitation `is_cloud_synced` already has with respect to symlinks in
/// `candidate` generally (see its doc comment) — resolving symlinks before
/// calling this guard is the caller's responsibility, not something this
/// pure function can do.
fn normalize_lexically(path: &Path) -> Option<PathBuf> {
    use std::path::Component;

    let mut stack: Vec<Component> = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => {
                // "./" contributes nothing.
            }
            Component::ParentDir => match stack.last() {
                Some(Component::Normal(_)) => {
                    stack.pop();
                }
                // Nothing to pop above the root (or nothing at all): give up
                // rather than guess where this path actually lands.
                _ => return None,
            },
            other => stack.push(other),
        }
    }

    let mut result = PathBuf::new();
    for component in stack {
        result.push(component.as_os_str());
    }
    Some(result)
}

/// Component-wise `starts_with`, case-insensitively.
///
/// macOS's default APFS volume is case-INSENSITIVE, but `Path::starts_with`
/// (and `OsStr` equality generally) is case-sensitive — so on a real Mac,
/// `/Users/k/Library/Mobile Documents` and
/// `/users/k/library/mobile documents` name the *same* directory, but a
/// case-sensitive comparison would treat the second as unrelated and accept
/// it. That is a false NEGATIVE: a "different-looking" path that is actually
/// the hazardous one.
///
/// Comparing case-insensitively trades that for the possibility of a false
/// POSITIVE on a genuinely case-sensitive volume (rare on macOS, but
/// possible) — rejecting two paths that really are distinct because they
/// only differ by case. A false positive here merely inconveniences the
/// user (the guard says no to a folder that was actually fine); a false
/// negative corrupts their `.git`. Given the choice, this guard fails
/// closed: case-insensitive comparison stays the default in both
/// directions. Do not reverse this trade without re-deriving it.
///
/// Comparison stays component-wise (not a raw string prefix check) so that
/// `/x/Code` still does not match `/x/CodeOther/child` — only case folding
/// changes, not the component-boundary semantics `Path::starts_with`
/// already gets right.
fn starts_with_ignore_case(candidate: &Path, prefix: &Path) -> bool {
    let mut candidate_components = candidate.components();
    for prefix_component in prefix.components() {
        let Some(candidate_component) = candidate_components.next() else {
            return false;
        };
        let a = candidate_component.as_os_str().to_string_lossy().to_lowercase();
        let b = prefix_component.as_os_str().to_string_lossy().to_lowercase();
        if a != b {
            return false;
        }
    }
    true
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
///
/// `candidate` is normalised lexically for `..`/`.` (see
/// [`normalize_lexically`]) but is otherwise NOT resolved: this function
/// cannot see through symlinks anywhere in `candidate`. If some component of
/// `candidate` is itself a symlink into a cloud-synced location, this guard
/// will not detect it. Resolving symlinks in the candidate path (e.g. with
/// `std::fs::canonicalize`, filesystem access this function deliberately
/// does not perform) is the caller's responsibility before calling this
/// guard, not something a pure predicate can do on its own.
pub fn is_cloud_synced(
    candidate: &Path,
    env: &SyncPaths,
    documents_is_symlink: bool,
) -> Option<Reason> {
    let normalized = match normalize_lexically(candidate) {
        Some(p) => p,
        None => return Some(Reason::NotNormalisable),
    };
    if let Some(root) = &env.cloud_root {
        if starts_with_ignore_case(&normalized, root) {
            return Some(Reason::InsideCloudRoot);
        }
    }
    if starts_with_ignore_case(&normalized, &env.home.join("Library/Mobile Documents")) {
        return Some(Reason::MobileDocuments);
    }
    if documents_is_symlink && starts_with_ignore_case(&normalized, &env.home.join("Documents")) {
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

    // `..` traversal: `Path::starts_with` never interprets `ParentDir`, so
    // without lexical normalisation a candidate can walk itself into a
    // rejected location while comparing as unrelated. These pin the fix.
    // `env().children_root` is `/Users/k/Library/Application Support/Allowance
    // Tracker/children` — four `..` from there lands back at `/Users/k`.

    #[test]
    fn dot_dot_escaping_into_the_cloud_root_is_rejected() {
        let e = env();
        let candidate = e.children_root.join(
            "../../../../Library/CloudStorage/ProtonDrive-x/Code/inside",
        );
        assert_eq!(is_cloud_synced(&candidate, &e, false), Some(Reason::InsideCloudRoot));
    }

    #[test]
    fn dot_dot_escaping_into_mobile_documents_is_rejected() {
        let e = env();
        let candidate = e.children_root.join("../../../../Library/Mobile Documents/x");
        assert_eq!(is_cloud_synced(&candidate, &e, false), Some(Reason::MobileDocuments));
    }

    #[test]
    fn dot_dot_escaping_into_documents_is_rejected_when_symlinked() {
        let e = env();
        let candidate = e.children_root.join("../../../../Documents/Allowance Tracker/keiko");
        assert_eq!(is_cloud_synced(&candidate, &e, true), Some(Reason::DocumentsSyncOn));
    }

    /// A benign `..` that never leaves a safe area must still be accepted —
    /// the normaliser must resolve it, not blanket-reject anything
    /// containing a dot-dot.
    #[test]
    fn a_dot_dot_that_stays_outside_any_cloud_location_is_accepted() {
        let mut e = env();
        e.cloud_root = Some(PathBuf::from("/x/Code"));
        e.home = PathBuf::from("/x/home");
        let candidate = PathBuf::from("/x/CodeOther/child/../child2");
        assert_eq!(is_cloud_synced(&candidate, &e, false), None);
    }

    /// A `..` that would pop above the filesystem root cannot be resolved
    /// lexically. The guard must fail closed rather than guess.
    #[test]
    fn a_dot_dot_popping_above_root_is_not_normalisable() {
        let e = env();
        let candidate = PathBuf::from("/../etc/passwd");
        assert_eq!(is_cloud_synced(&candidate, &e, false), Some(Reason::NotNormalisable));
    }

    /// A redundant `./` component must be dropped and must not change the
    /// verdict either way.
    #[test]
    fn a_redundant_current_dir_component_does_not_change_the_verdict() {
        let e = env();
        let accepted = e.children_root.join("./keiko");
        assert_eq!(is_cloud_synced(&accepted, &e, false), None);

        let rejected = PathBuf::from("/Users/k/Library/Mobile Documents/./com~apple~CloudDocs/x");
        assert_eq!(is_cloud_synced(&rejected, &e, false), Some(Reason::MobileDocuments));
    }

    // Case sensitivity: macOS's default APFS volume is case-insensitive, so
    // these lowercase/mixed-case spellings name the same real directories as
    // the canonical-cased ones above and must be rejected the same way.

    #[test]
    fn lowercase_cloud_root_is_still_rejected() {
        let e = env();
        let candidate =
            PathBuf::from("/users/k/library/cloudstorage/protondrive-x/code/inside");
        assert_eq!(is_cloud_synced(&candidate, &e, false), Some(Reason::InsideCloudRoot));
    }

    #[test]
    fn lowercase_mobile_documents_is_still_rejected() {
        let e = env();
        let candidate = PathBuf::from("/users/k/library/mobile documents/x");
        assert_eq!(is_cloud_synced(&candidate, &e, false), Some(Reason::MobileDocuments));
    }

    #[test]
    fn mixed_case_documents_is_still_rejected_when_symlinked() {
        let e = env();
        let candidate = PathBuf::from("/Users/k/DOCUMENTS/Allowance Tracker/keiko");
        assert_eq!(is_cloud_synced(&candidate, &e, true), Some(Reason::DocumentsSyncOn));
    }

    /// Case-insensitive comparison must not resurrect the prefix-string bug:
    /// `/x/Code` and `/x/CodeOther/child` are still distinct components even
    /// when both are folded to lowercase.
    #[test]
    fn case_insensitive_comparison_does_not_break_the_prefix_sibling_case() {
        let mut e = env();
        e.cloud_root = Some(PathBuf::from("/x/Code"));
        e.home = PathBuf::from("/x/home");
        let candidate = PathBuf::from("/x/CODEOTHER/child");
        assert_eq!(is_cloud_synced(&candidate, &e, false), None);
    }
}
