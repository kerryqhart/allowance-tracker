//! # Git Versioning Module
//!
//! This module provides git repository management for child directories in the allowance tracker.
//! Each child directory becomes its own git repository with automatic versioning of data files.
//!
//! ## Features
//!
//! - Initialize git repositories in child directories
//! - Stage and commit file changes automatically
//! - Non-blocking git operations (errors are logged but don't fail main operations)
//! - Standard commit messages for different file types
//!
//! ## Usage
//!
//! ```rust,no_run
//! use allowance_tracker_egui::backend::storage::git::GitManager;
//!
//! fn example() -> anyhow::Result<()> {
//!     let git_manager = GitManager::new();
//!
//!     // Ensure repository exists
//!     git_manager.ensure_repo_exists("/path/to/child/directory")?;
//!
//!     // Commit a file change
//!     git_manager.commit_file_change(
//!         "/path/to/child/directory",
//!         "transactions.csv",
//!         "Added $5.00 allowance transaction"
//!     )?;
//!     Ok(())
//! }
//! ```

use anyhow::{Context, Result};
use git2::{Repository, Signature, IndexAddOption};
use log::{info, warn, debug};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Injectable so tests can construct committer-timestamp ties. Real usage
/// (`GitManager::new`) points this at wall-clock time; only `with_clock`
/// injects a fake, and only tests call `with_clock`.
type Clock = fn() -> i64;

fn real_clock() -> i64 {
    chrono::Utc::now().timestamp()
}

/// Git manager for handling local repository operations
#[derive(Clone, Debug)]
pub struct GitManager {
    /// Default author name for commits
    author_name: String,
    /// Default author email for commits
    author_email: String,
    /// Source of the committer/author timestamp used in `signature()`.
    /// Injectable only via `with_clock`, so `commit()`'s timestamps stay
    /// real-time everywhere except tests that need to construct a
    /// committer-timestamp tie (the tiebreak-by-oid branch of the merge
    /// resolution rule cannot otherwise be exercised).
    clock: Clock,
}

impl GitManager {
    /// Create a new GitManager with default configuration
    pub fn new() -> Self {
        Self {
            author_name: "Allowance Tracker".to_string(),
            author_email: "allowance@tracker.local".to_string(),
            clock: real_clock,
        }
    }

    /// Create a new GitManager with custom author information
    pub fn with_author(author_name: String, author_email: String) -> Self {
        Self {
            author_name,
            author_email,
            clock: real_clock,
        }
    }

    /// Create a new GitManager whose commit timestamps come from `clock`
    /// instead of the wall clock. Exists so tests can construct committer-
    /// timestamp ties.
    pub fn with_clock(clock: Clock) -> Self {
        Self {
            author_name: "Allowance Tracker".to_string(),
            author_email: "noreply@localhost".to_string(),
            clock,
        }
    }

    /// Build a signature using the injected clock rather than
    /// `Signature::now()`, so timestamps are controllable in tests.
    fn signature(&self) -> Result<Signature<'static>> {
        let when = git2::Time::new((self.clock)(), 0);
        Ok(Signature::new(&self.author_name, &self.author_email, &when)?)
    }

    /// Initialize a git repository in the specified directory
    pub fn init_repo<P: AsRef<Path>>(&self, repo_path: P) -> Result<()> {
        let repo_path = repo_path.as_ref();
        debug!("Initializing git repository at: {:?}", repo_path);
        Repository::init(repo_path)?;
        Ok(())
    }

    /// Ensure a git repository exists at the specified path
    pub fn ensure_repo_exists<P: AsRef<Path>>(&self, repo_path: P) -> Result<()> {
        let repo_path = repo_path.as_ref();
        if !self.is_git_repository(repo_path) {
            debug!("Repository doesn't exist, initializing at: {:?}", repo_path);
            self.init_repo(repo_path)?;
        }
        Ok(())
    }

    /// Stage a specific file for commit
    pub fn add_file<P: AsRef<Path>>(&self, repo_path: P, file_path: &str) -> Result<()> {
        let repo_path = repo_path.as_ref();
        debug!("Staging file '{}' in repository: {:?}", file_path, repo_path);
        let repo = Repository::open(repo_path)?;
        let mut index = repo.index()?;
        index.add_path(Path::new(file_path))?;
        index.write()?;
        Ok(())
    }

    /// Unstage a specific file from the index — the removal counterpart to
    /// [`Self::add_file`]. A no-op (not an error) when the path is not
    /// currently in the index, so a best-effort cleanup caller (Task 20's
    /// `check_sync`) can call this without first having to know whether the
    /// file was ever actually staged.
    pub fn remove_file<P: AsRef<Path>>(&self, repo_path: P, file_path: &str) -> Result<()> {
        let repo_path = repo_path.as_ref();
        debug!("Unstaging file '{}' in repository: {:?}", file_path, repo_path);
        let repo = Repository::open(repo_path)?;
        let mut index = repo.index()?;
        if index.get_path(Path::new(file_path), 0).is_some() {
            index.remove_path(Path::new(file_path))?;
            index.write()?;
        }
        Ok(())
    }

    /// Stage all changes in the repository
    pub fn add_all<P: AsRef<Path>>(&self, repo_path: P) -> Result<()> {
        let repo_path = repo_path.as_ref();
        debug!("Staging all changes in repository: {:?}", repo_path);
        let repo = Repository::open(repo_path)?;
        let mut index = repo.index()?;
        index.add_all(["*"].iter(), IndexAddOption::DEFAULT, None)?;
        index.write()?;
        Ok(())
    }

    /// Create a commit with the staged changes
    pub fn commit<P: AsRef<Path>>(&self, repo_path: P, message: &str) -> Result<String> {
        let repo_path = repo_path.as_ref();
        debug!("Creating commit in repository: {:?} with message: {}", repo_path, message);

        let repo = Repository::open(repo_path)?;
        let signature = self.signature()?;

        let mut index = repo.index()?;
        let tree_id = index.write_tree()?;
        let tree = repo.find_tree(tree_id)?;

        // Get the parent commit (HEAD) if it exists
        let parent_commit = match repo.head() {
            Ok(head) => {
                let commit = head.peel_to_commit()?;
                Some(commit)
            }
            Err(_) => None, // No HEAD yet (initial commit)
        };

        let commit_id = match &parent_commit {
            Some(parent) => {
                repo.commit(
                    Some("HEAD"),
                    &signature,
                    &signature,
                    message,
                    &tree,
                    &[parent],
                )?
            }
            None => {
                repo.commit(
                    Some("HEAD"),
                    &signature,
                    &signature,
                    message,
                    &tree,
                    &[],
                )?
            }
        };

        Ok(commit_id.to_string())
    }

    /// Same as [`Self::commit`], but returns `Ok(None)` instead of creating
    /// a commit when the currently-staged tree is byte-identical to HEAD's
    /// tree (i.e. staging produced no actual change).
    ///
    /// Review round 4, Minor-3: `commit` itself has no such guard, and
    /// every EXISTING caller already avoids calling it when there is
    /// nothing to commit (`commit_file_change` checks
    /// `has_uncommitted_changes` first) — so `commit`'s contract and every
    /// caller/test that relies on it staying `Result<String>` are left
    /// untouched here. This exists for callers that stage a specific,
    /// narrow set of files rather than checking status first (e.g.
    /// `apply_fast_forward`'s unblock path in `app_coordinator.rs`, which
    /// stages only the files this app owns — see Important-1 in the same
    /// review round — and so can legitimately end up with nothing new
    /// staged, e.g. if the conflicting content was in a file this app does
    /// not track). Without this guard, that caller would otherwise create
    /// a content-free commit on every single peer advance it is ever
    /// invoked for.
    pub fn commit_if_changed<P: AsRef<Path>>(&self, repo_path: P, message: &str) -> Result<Option<String>> {
        let repo_path = repo_path.as_ref();
        let repo = Repository::open(repo_path)?;
        let signature = self.signature()?;

        let mut index = repo.index()?;
        let tree_id = index.write_tree()?;

        let parent_commit = match repo.head() {
            Ok(head) => Some(head.peel_to_commit()?),
            Err(_) => None,
        };

        if let Some(parent) = &parent_commit {
            if parent.tree_id() == tree_id {
                debug!(
                    "commit_if_changed: staged tree {} is identical to HEAD's tree in {:?} — \
                     nothing to commit",
                    tree_id, repo_path
                );
                return Ok(None);
            }
        }

        let tree = repo.find_tree(tree_id)?;
        let commit_id = match &parent_commit {
            Some(parent) => {
                repo.commit(Some("HEAD"), &signature, &signature, message, &tree, &[parent])?
            }
            None => repo.commit(Some("HEAD"), &signature, &signature, message, &tree, &[])?,
        };

        Ok(Some(commit_id.to_string()))
    }

    /// Create a merge commit (or an ordinary commit, when given a single
    /// parent) from the current index, using the injected clock for both
    /// author and committer.
    ///
    /// One commit, not two: a separate recompute commit would produce a tree
    /// that never satisfies the merge's fixed-point property.
    ///
    /// `self.signature()` (not `Signature::now()`) governs the committer
    /// timestamp here specifically so tests can construct committer-
    /// timestamp TIES — the only way to exercise the merge's
    /// tie-break-by-oid branch. Building this on `GitManager` rather than as
    /// a free function (like `fetch_lgs`/`push_lgs`) is what lets it reuse
    /// that private clock plumbing instead of duplicating it.
    pub fn commit_merge(
        &self,
        repo_path: &Path,
        message: &str,
        parents: &[&str],
    ) -> Result<String> {
        if parents.is_empty() {
            anyhow::bail!("commit_merge requires at least one parent commit; a zero-parent call would silently create a root commit");
        }

        debug!("Creating merge commit in repository: {:?} with {} parent(s)", repo_path, parents.len());

        let repo = Repository::open(repo_path)?;
        let signature = self.signature()?;

        // Stage the current working state so the merge commit's tree
        // reflects it, not whatever was left in the index from an earlier
        // operation.
        let mut index = repo.index()?;
        index.add_all(["*"].iter(), IndexAddOption::DEFAULT, None)?;
        index.write()?;
        let tree_id = index.write_tree()?;
        let tree = repo.find_tree(tree_id)?;

        let parent_commits: Vec<git2::Commit> = parents
            .iter()
            .map(|oid_str| -> Result<git2::Commit> {
                let oid = git2::Oid::from_str(oid_str)?;
                Ok(repo.find_commit(oid)?)
            })
            .collect::<Result<Vec<_>>>()?;
        let parent_refs: Vec<&git2::Commit> = parent_commits.iter().collect();

        let commit_id = repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            message,
            &tree,
            &parent_refs,
        )?;

        Ok(commit_id.to_string())
    }

    /// Check if repository has uncommitted changes
    pub fn has_uncommitted_changes<P: AsRef<Path>>(&self, repo_path: P) -> Result<bool> {
        let repo_path = repo_path.as_ref();
        debug!("Checking for uncommitted changes in repository: {:?}", repo_path);

        let repo = Repository::open(repo_path)?;
        let statuses = repo.statuses(None)?;

        Ok(!statuses.is_empty())
    }

    /// Commit file changes with staging (convenience method)
    pub fn commit_file_change<P: AsRef<Path>>(
        &self,
        repo_path: P,
        filename: &str,
        action_description: &str
    ) -> Result<()> {
        let repo_path = repo_path.as_ref();
        info!("Committing file change to {}: {} in repository: {:?}",
              filename, action_description, repo_path);

        // Ensure repo exists
        self.ensure_repo_exists(repo_path)?;

        // Stage the file
        if let Err(e) = self.add_file(repo_path, filename) {
            warn!("Failed to stage file {}: {}. Trying add_all instead.", filename, e);
            self.add_all(repo_path)?;
        }

        // Create commit message
        let message = format!("Update {}: {}", filename, action_description);

        // Only commit if there are changes
        if self.has_uncommitted_changes(repo_path)? {
            match self.commit(repo_path, &message) {
                Ok(commit_id) => {
                    info!("Created commit {} for {}", commit_id, filename);
                }
                Err(e) => {
                    warn!("Failed to create commit for {}: {}", filename, e);
                }
            }
        } else {
            debug!("No changes to commit for {}", filename);
        }

        Ok(())
    }

    /// Get the git directory path for a given repository path
    pub fn get_git_dir<P: AsRef<Path>>(&self, repo_path: P) -> PathBuf {
        repo_path.as_ref().join(".git")
    }

    /// Check if a directory is a git repository
    pub fn is_git_repository<P: AsRef<Path>>(&self, repo_path: P) -> bool {
        let repo_path = repo_path.as_ref();
        Repository::open(repo_path).is_ok()
    }

    // ========== SYNCHRONOUS VERSIONS FOR EGUI FRONTEND ==========
    // These are aliases since git2 is already synchronous

    /// Initialize a git repository in the specified directory (synchronous)
    pub fn init_repo_sync<P: AsRef<Path>>(&self, repo_path: P) -> Result<()> {
        self.init_repo(repo_path)
    }

    /// Ensure a git repository exists at the specified path (synchronous)
    pub fn ensure_repo_exists_sync<P: AsRef<Path>>(&self, repo_path: P) -> Result<()> {
        self.ensure_repo_exists(repo_path)
    }

    /// Stage all changes in the repository (synchronous)
    pub fn add_all_sync<P: AsRef<Path>>(&self, repo_path: P) -> Result<()> {
        self.add_all(repo_path)
    }

    /// Create a commit with the staged changes (synchronous)
    pub fn commit_sync<P: AsRef<Path>>(&self, repo_path: P, message: &str) -> Result<String> {
        self.commit(repo_path, message)
    }

    /// Check if repository has uncommitted changes (synchronous)
    pub fn has_uncommitted_changes_sync<P: AsRef<Path>>(&self, repo_path: P) -> Result<bool> {
        self.has_uncommitted_changes(repo_path)
    }

    /// Commit file changes with staging (synchronous version)
    pub fn commit_file_change_sync<P: AsRef<Path>>(
        &self,
        repo_path: P,
        filename: &str,
        action_description: &str
    ) -> Result<()> {
        self.commit_file_change(repo_path, filename, action_description)
    }
}

impl Default for GitManager {
    fn default() -> Self {
        Self::new()
    }
}

// ========== REMOTE OPERATIONS (lgs sync) ==========
//
// These are free functions, not `GitManager` methods: they operate on an
// already-open `git2::Repository` and need no author/clock configuration,
// which keeps them trivially testable against a local bare repo in a
// tempdir instead of a running lgs daemon.

/// Clone a remote repository to a local path.
///
/// Used by migration/onboarding paths that need a plain git2 clone (as
/// opposed to `lgs restore`, which performs its own clone and names the
/// remote `origin` — see `ensure_lgs_remote` below).
pub fn clone_repo<P: AsRef<Path>>(url: &str, into: P) -> Result<Repository> {
    Ok(Repository::clone(url, into.as_ref())?)
}

/// Ensure exactly one remote, named `lgs`, points at `url`.
///
/// `lgs restore` clones a project with the remote named `origin`; the
/// migration path (Task 18) names it `lgs` directly. Onboarding (Task 19)
/// must be able to hand either layout to the sync loop and get one
/// consistent remote name out of it. Idempotent, and self-heals a stale URL
/// (the daemon's port is configurable, so a URL frozen into `.git/config` at
/// migration time would otherwise break push forever after a port change).
pub fn ensure_lgs_remote(repo: &git2::Repository, url: &str) -> Result<()> {
    match repo.find_remote("lgs") {
        Ok(r) if r.url() == Some(url) => Ok(()),
        Ok(_) => Ok(repo.remote_set_url("lgs", url)?),
        Err(_) => {
            // `lgs restore` clones with the remote named `origin`.
            if let Ok(origin) = repo.find_remote("origin") {
                if origin.url() == Some(url) {
                    repo.remote_rename("origin", "lgs")?;
                    return Ok(());
                }
            }
            repo.remote("lgs", url)?;
            Ok(())
        }
    }
}

/// THE refspec. lgs's `reconcile` is documented as never moving a head
/// backward or over a divergence (local-git-sync `durability/engine.rs:399`).
/// When two machines have diverged, the peer's commits do NOT appear at
/// `refs/heads/*` in the local bare — that ref still points at this
/// machine's own tip. A fetch of `refs/heads/*` alone would therefore return
/// our own commit, the app would conclude "up to date", and the merge would
/// never run — both machines stall permanently and invisibly.
///
/// lgs writes the authoritative peer tip to `refs/lgs-auth/heads/<branch>`
/// *before* the divergence check, and that is the ref the sync loop's merge
/// must read from — hence `refs/remotes/lgs-auth/*` as the fetch
/// destination, not `refs/remotes/lgs/*`.
pub const LGS_AUTH_REFSPEC: &str = "+refs/lgs-auth/heads/*:refs/remotes/lgs-auth/*";
/// This machine's own view of the peer's advertised heads. Fetched
/// alongside `LGS_AUTH_REFSPEC` for completeness, but the sync loop's merge
/// input is `refs/remotes/lgs-auth/*`, never this one — see above.
pub const LGS_HEADS_REFSPEC: &str = "+refs/heads/*:refs/remotes/lgs/*";

/// Review Important-2: the per-tick budget/shutdown check in
/// `run_child_sync_cycles` (`sync_thread.rs`) only gates STARTING a child's
/// cycle — it cannot interrupt a fetch or push already in flight. A fetch
/// wedged on a stalled cloud-drive path, or a push to a daemon that stopped
/// responding mid-transfer, would otherwise block this one libgit2 call
/// forever, and since this all runs synchronously on the one background
/// sync thread, that takes the AWS poll, the 30s timer, and shutdown down
/// with it. This is what actually bounds those calls: a deadline captured
/// right before the call, checked from inside libgit2's own progress
/// callbacks, with `false`/`Err` telling libgit2 to abort the transfer.
/// 30s is generous for what is always local-machine-to-local-daemon
/// traffic.
const NETWORK_TIMEOUT: Duration = Duration::from_secs(30);

/// Fetch both the authoritative peer tip (`refs/lgs-auth/*`) and the remote's
/// own `refs/heads/*` from the `lgs` remote.
pub fn fetch_lgs(repo: &git2::Repository) -> Result<()> {
    fetch_lgs_with_deadline(repo, Instant::now() + NETWORK_TIMEOUT)
}

/// The actual implementation behind [`fetch_lgs`], with the deadline
/// exposed so a test can assert the abort path with a deadline that has
/// already elapsed, rather than waiting out the real 30s bound.
fn fetch_lgs_with_deadline(repo: &git2::Repository, deadline: Instant) -> Result<()> {
    let mut remote = repo.find_remote("lgs")?;

    let mut callbacks = git2::RemoteCallbacks::new();
    // `transfer_progress` is invoked repeatedly as the indexer receives and
    // processes objects — the one periodic, bool-returning hook libgit2
    // gives fetch. Returning `false` here is what actually aborts an
    // in-flight transfer, not merely "gives up waiting" the way a
    // `Command`-level timeout would for a process it does not control the
    // internals of.
    callbacks.transfer_progress(move |_progress| Instant::now() < deadline);
    // Belt and suspenders: textual sideband progress (when the remote sends
    // any) is checked the same way, in case a stall happens before the
    // indexer has anything to report.
    callbacks.sideband_progress(move |_msg| Instant::now() < deadline);

    let mut fetch_options = git2::FetchOptions::new();
    fetch_options.remote_callbacks(callbacks);

    remote
        .fetch(&[LGS_AUTH_REFSPEC, LGS_HEADS_REFSPEC], Some(&mut fetch_options), None)
        .with_context(|| format!("fetching from lgs (bounded to {NETWORK_TIMEOUT:?})"))?;
    Ok(())
}

/// Push `branch` to the `lgs` remote, non-forced.
///
/// Wires a `push_update_reference` callback and turns any per-ref rejection
/// into an `Err`. This is not optional ceremony: libgit2's `Remote::push`
/// returns `Ok(())` even when the server rejected an individual ref update
/// (e.g. a non-fast-forward) unless this callback is registered — silently
/// treating a rejected push as success is exactly the failure mode that
/// would make sync look healthy while stalling both machines.
pub fn push_lgs(repo: &git2::Repository, branch: &str) -> Result<()> {
    push_lgs_with_deadline(repo, branch, Instant::now() + NETWORK_TIMEOUT)
}

/// The actual implementation behind [`push_lgs`], with the deadline exposed
/// for the same reason as [`fetch_lgs_with_deadline`].
///
/// # Push has weaker cancellation coverage than fetch
///
/// Unlike fetch's `transfer_progress`, libgit2 (and the git2-rs bindings
/// over it) exposes no periodic, bool-returning progress callback for the
/// push side: `push_transfer_progress` and `pack_progress` are both
/// `FnMut(..)` with NO return value — they cannot abort anything, only
/// observe. The two hooks wired below are the strongest the API surface
/// allows:
/// - `sideband_progress` CAN return `false` to cancel, and fires for both
///   fetch and push — but only if the remote actually sends textual
///   sideband progress messages, which a bare `git http-backend` (what lgs
///   runs) is not guaranteed to do.
/// - `push_negotiation` fires exactly once, between negotiation and the
///   actual upload, so a deadline already passed by then still aborts
///   before any data is sent. It cannot interrupt a transfer already under
///   way.
///
/// Net effect: a push that hangs mid-upload against a remote that never
/// emits sideband text is NOT guaranteed to be caught by this. Reported
/// plainly in this task's report rather than left implicit.
fn push_lgs_with_deadline(repo: &git2::Repository, branch: &str, deadline: Instant) -> Result<()> {
    let mut remote = repo.find_remote("lgs")?;
    let refspec = format!("refs/heads/{branch}:refs/heads/{branch}");

    let mut callbacks = git2::RemoteCallbacks::new();
    callbacks.push_update_reference(|refname, status| match status {
        None => Ok(()),
        Some(msg) => Err(git2::Error::from_str(&format!(
            "lgs rejected push of {refname}: {msg}"
        ))),
    });
    callbacks.sideband_progress(move |_msg| Instant::now() < deadline);
    callbacks.push_negotiation(move |_updates| {
        if Instant::now() < deadline {
            Ok(())
        } else {
            Err(git2::Error::from_str(
                "push aborted: network timeout exceeded before negotiation completed",
            ))
        }
    });

    let mut push_options = git2::PushOptions::new();
    push_options.remote_callbacks(callbacks);

    remote
        .push(&[refspec.as_str()], Some(&mut push_options))
        .with_context(|| format!("pushing to lgs (bounded to {NETWORK_TIMEOUT:?})"))?;
    Ok(())
}

/// Fetch exactly one caller-supplied refspec from the `lgs` remote, bounded
/// the same way as [`fetch_lgs`]. Exists for callers that need a ref OTHER
/// than `refs/heads/*`/`refs/lgs-auth/*` — e.g. `check_sync`'s disposable
/// `refs/sync-check/probe`, which must never touch the branch refspecs
/// `fetch_lgs` fetches. Never used for the branch sync loop itself; that
/// stays on `fetch_lgs` unchanged.
pub fn fetch_lgs_refspec(repo: &git2::Repository, refspec: &str) -> Result<()> {
    fetch_lgs_refspec_with_deadline(repo, refspec, Instant::now() + NETWORK_TIMEOUT)
}

/// The actual implementation behind [`fetch_lgs_refspec`], with the deadline
/// exposed for the same reason as [`fetch_lgs_with_deadline`].
fn fetch_lgs_refspec_with_deadline(repo: &git2::Repository, refspec: &str, deadline: Instant) -> Result<()> {
    let mut remote = repo.find_remote("lgs")?;

    let mut callbacks = git2::RemoteCallbacks::new();
    callbacks.transfer_progress(move |_progress| Instant::now() < deadline);
    callbacks.sideband_progress(move |_msg| Instant::now() < deadline);

    let mut fetch_options = git2::FetchOptions::new();
    fetch_options.remote_callbacks(callbacks);

    remote
        .fetch(&[refspec], Some(&mut fetch_options), None)
        .with_context(|| format!("fetching {refspec} from lgs (bounded to {NETWORK_TIMEOUT:?})"))?;
    Ok(())
}

/// Push exactly one caller-supplied refspec to the `lgs` remote, bounded the
/// same way as [`push_lgs`]. Exists for callers that need to push (or, with
/// an empty source side, delete) a ref other than a branch — see
/// [`fetch_lgs_refspec`]'s doc comment for why. Same rejection handling as
/// [`push_lgs`]: a per-ref rejection is turned into an `Err`, never silently
/// swallowed.
pub fn push_lgs_refspec(repo: &git2::Repository, refspec: &str) -> Result<()> {
    push_lgs_refspec_with_deadline(repo, refspec, Instant::now() + NETWORK_TIMEOUT)
}

/// The actual implementation behind [`push_lgs_refspec`], with the deadline
/// exposed for the same reason as [`push_lgs_with_deadline`].
fn push_lgs_refspec_with_deadline(repo: &git2::Repository, refspec: &str, deadline: Instant) -> Result<()> {
    let mut remote = repo.find_remote("lgs")?;

    let mut callbacks = git2::RemoteCallbacks::new();
    callbacks.push_update_reference(|refname, status| match status {
        None => Ok(()),
        Some(msg) => Err(git2::Error::from_str(&format!("lgs rejected push of {refname}: {msg}"))),
    });
    callbacks.sideband_progress(move |_msg| Instant::now() < deadline);
    callbacks.push_negotiation(move |_updates| {
        if Instant::now() < deadline {
            Ok(())
        } else {
            Err(git2::Error::from_str(
                "push aborted: network timeout exceeded before negotiation completed",
            ))
        }
    });

    let mut push_options = git2::PushOptions::new();
    push_options.remote_callbacks(callbacks);

    remote
        .push(&[refspec], Some(&mut push_options))
        .with_context(|| format!("pushing {refspec} to lgs (bounded to {NETWORK_TIMEOUT:?})"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_git_manager_creation() {
        let git_manager = GitManager::new();
        assert_eq!(git_manager.author_name, "Allowance Tracker");
        assert_eq!(git_manager.author_email, "allowance@tracker.local");
    }

    #[test]
    fn test_is_git_repository_nonexistent() {
        let git_manager = GitManager::new();
        assert!(!git_manager.is_git_repository("/nonexistent/path"));
    }

    #[test]
    fn test_init_and_commit() {
        let temp_dir = tempfile::tempdir().unwrap();
        let git_manager = GitManager::new();

        // Initialize repo
        git_manager.init_repo(temp_dir.path()).unwrap();
        assert!(git_manager.is_git_repository(temp_dir.path()));

        // Create a test file
        std::fs::write(temp_dir.path().join("test.txt"), "hello").unwrap();

        // Commit it
        git_manager.add_file(temp_dir.path(), "test.txt").unwrap();
        let commit_id = git_manager.commit(temp_dir.path(), "Initial commit").unwrap();
        assert!(!commit_id.is_empty());
    }

    #[test]
    fn remove_file_unstages_a_tracked_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let gm = GitManager::new();
        gm.init_repo(temp_dir.path()).unwrap();
        std::fs::write(temp_dir.path().join("test.txt"), "hello").unwrap();
        gm.add_file(temp_dir.path(), "test.txt").unwrap();
        gm.commit(temp_dir.path(), "initial").unwrap();

        std::fs::remove_file(temp_dir.path().join("test.txt")).unwrap();
        gm.remove_file(temp_dir.path(), "test.txt").unwrap();

        let commit_id = gm.commit(temp_dir.path(), "remove test.txt").unwrap();
        let repo = git2::Repository::open(temp_dir.path()).unwrap();
        let commit = repo.find_commit(git2::Oid::from_str(&commit_id).unwrap()).unwrap();
        let tree = commit.tree().unwrap();
        assert!(tree.get_name("test.txt").is_none(), "test.txt must be gone from the new tree");
    }

    /// A best-effort cleanup caller may not know whether a file was ever
    /// actually staged (e.g. an earlier stage failed before staging it).
    /// This must be a quiet no-op, not an error.
    #[test]
    fn remove_file_is_a_no_op_when_the_path_was_never_staged() {
        let temp_dir = tempfile::tempdir().unwrap();
        let gm = GitManager::new();
        gm.init_repo(temp_dir.path()).unwrap();
        std::fs::write(temp_dir.path().join("other.txt"), "x").unwrap();
        gm.add_file(temp_dir.path(), "other.txt").unwrap();
        gm.commit(temp_dir.path(), "initial").unwrap();

        // "never-staged.txt" was never added — removing it must not error.
        gm.remove_file(temp_dir.path(), "never-staged.txt").unwrap();
    }

    #[test]
    fn commit_if_changed_creates_a_commit_when_the_tree_actually_differs() {
        let temp_dir = tempfile::tempdir().unwrap();
        let gm = GitManager::new();
        gm.init_repo(temp_dir.path()).unwrap();
        std::fs::write(temp_dir.path().join("test.txt"), "hello").unwrap();
        gm.add_file(temp_dir.path(), "test.txt").unwrap();

        let result = gm.commit_if_changed(temp_dir.path(), "initial").unwrap();
        assert!(result.is_some(), "a real content change must produce a commit");
    }

    /// Review round 4, Minor-3: staging nothing new (or re-staging content
    /// already matching HEAD) must not produce a content-free commit.
    #[test]
    fn commit_if_changed_returns_none_when_nothing_actually_changed() {
        let temp_dir = tempfile::tempdir().unwrap();
        let gm = GitManager::new();
        gm.init_repo(temp_dir.path()).unwrap();
        std::fs::write(temp_dir.path().join("test.txt"), "hello").unwrap();
        gm.add_file(temp_dir.path(), "test.txt").unwrap();
        let first = gm.commit_if_changed(temp_dir.path(), "initial").unwrap();
        assert!(first.is_some(), "precondition: the first commit must have been created");

        // Nothing on disk changed since that commit; re-staging the same
        // file re-adds byte-identical content to the index.
        gm.add_file(temp_dir.path(), "test.txt").unwrap();
        let second = gm.commit_if_changed(temp_dir.path(), "no-op").unwrap();
        assert!(second.is_none(), "staging identical content must not create an empty commit");

        // HEAD must not have moved.
        let repo = git2::Repository::open(temp_dir.path()).unwrap();
        assert_eq!(
            repo.head().unwrap().peel_to_commit().unwrap().id().to_string(),
            first.unwrap()
        );
    }

    /// Creates a commit with an empty tree directly via the object database,
    /// so it works against a bare repo (no working directory / index).
    fn commit_empty_tree(
        repo: &git2::Repository,
        message: &str,
        parents: &[&git2::Commit],
    ) -> git2::Oid {
        let sig =
            git2::Signature::new("Test", "test@example.com", &git2::Time::new(1_700_000_000, 0))
                .unwrap();
        let tree_id = repo.treebuilder(None).unwrap().write().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        repo.commit(None, &sig, &sig, message, &tree, parents).unwrap()
    }

    #[test]
    fn ensure_lgs_remote_is_idempotent_and_renames_origin() {
        // `lgs restore` clones and names the remote `origin`; migration names it
        // `lgs`. One name must exist in the system or the two paths disagree.
        let dir = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();
        repo.remote("origin", "http://localhost:8418/p.git").unwrap();

        ensure_lgs_remote(&repo, "http://localhost:8418/p.git").unwrap();
        assert!(repo.find_remote("lgs").is_ok());

        ensure_lgs_remote(&repo, "http://localhost:8418/p.git").unwrap();
        assert!(repo.find_remote("lgs").is_ok(), "second call must not fail");
    }

    #[test]
    fn ensure_lgs_remote_updates_a_stale_port() {
        // clone_url carries the daemon port, which is configurable. A URL frozen
        // into .git/config at migration time breaks push forever after a port change.
        let dir = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();
        repo.remote("lgs", "http://localhost:8418/p.git").unwrap();
        ensure_lgs_remote(&repo, "http://localhost:9999/p.git").unwrap();
        assert_eq!(
            repo.find_remote("lgs").unwrap().url().unwrap(),
            "http://localhost:9999/p.git"
        );
    }

    #[test]
    fn ensure_lgs_remote_when_both_origin_and_lgs_already_exist() {
        // Migration and onboarding must never collide: if a repo somehow has
        // both remotes (e.g. re-run migration after a restore), `lgs` wins and
        // `origin` is left untouched.
        let dir = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();
        repo.remote("origin", "http://localhost:8418/other.git").unwrap();
        repo.remote("lgs", "http://localhost:8418/p.git").unwrap();

        ensure_lgs_remote(&repo, "http://localhost:9999/p.git").unwrap();

        assert_eq!(
            repo.find_remote("lgs").unwrap().url().unwrap(),
            "http://localhost:9999/p.git"
        );
        assert_eq!(
            repo.find_remote("origin").unwrap().url().unwrap(),
            "http://localhost:8418/other.git",
            "origin must be left alone when lgs already exists"
        );
    }

    #[test]
    fn commit_uses_the_injected_clock_so_ties_are_constructible() {
        // With Signature::now() hardcoded, no test can build a committer-timestamp
        // tie, so the tiebreak-by-oid branch of the resolution rule ships uncovered.
        let dir = tempfile::tempdir().unwrap();
        let gm = GitManager::with_clock(|| 1_700_000_000);
        gm.init_repo(dir.path()).unwrap();
        std::fs::write(dir.path().join("f.txt"), "x").unwrap();
        gm.add_all(dir.path()).unwrap();
        let oid = gm.commit(dir.path(), "m").unwrap();
        let repo = git2::Repository::open(dir.path()).unwrap();
        let commit = repo.find_commit(git2::Oid::from_str(&oid).unwrap()).unwrap();
        assert_eq!(commit.committer().when().seconds(), 1_700_000_000);
    }

    #[test]
    fn fetch_lgs_lands_the_auth_ref_at_refs_remotes_lgs_auth() {
        // This is the test that would have caught the refspec bug: lgs writes
        // the authoritative peer tip to refs/lgs-auth/heads/<branch>, never to
        // refs/heads/<branch>, so a fetch using the wrong refspec would report
        // success while landing nothing the merge can actually read.
        let bare_dir = tempfile::tempdir().unwrap();
        let bare = git2::Repository::init_bare(bare_dir.path()).unwrap();
        let auth_oid = commit_empty_tree(&bare, "authoritative peer tip", &[]);
        bare.reference("refs/lgs-auth/heads/main", auth_oid, true, "test")
            .unwrap();

        let work_dir = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(work_dir.path()).unwrap();
        repo.remote("lgs", bare_dir.path().to_str().unwrap()).unwrap();

        fetch_lgs(&repo).unwrap();

        let landed = repo
            .find_reference("refs/remotes/lgs-auth/main")
            .expect("refs/lgs-auth/heads/main must land at refs/remotes/lgs-auth/main");
        assert_eq!(landed.target().unwrap(), auth_oid);
    }

    #[test]
    fn push_lgs_fails_when_the_server_rejects_a_non_fast_forward() {
        // libgit2's `Remote::push` returns `Ok(())` on a per-ref rejection
        // unless a `push_update_reference` callback is wired — a real trap
        // that would make a stalled sync look healthy. Force a rejection by
        // advancing the remote's `main` independently of the local repo (as
        // if a peer had pushed), then pushing a non-fast-forward local tip.
        let bare_dir = tempfile::tempdir().unwrap();
        let bare = git2::Repository::init_bare(bare_dir.path()).unwrap();

        let work_dir = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(work_dir.path()).unwrap();
        repo.remote("lgs", bare_dir.path().to_str().unwrap()).unwrap();

        let base_oid = commit_empty_tree(&repo, "base", &[]);
        repo.reference("refs/heads/main", base_oid, true, "init").unwrap();

        // First push: a brand-new remote branch, always a fast-forward.
        push_lgs(&repo, "main").unwrap();
        assert_eq!(
            bare.find_reference("refs/heads/main").unwrap().target().unwrap(),
            base_oid
        );

        // Simulate a peer pushing independently: advance the bare's main to an
        // unrelated commit that our local repo knows nothing about.
        let peer_oid = commit_empty_tree(&bare, "peer's independent commit", &[]);
        bare.reference("refs/heads/main", peer_oid, true, "peer push")
            .unwrap();

        // Advance our local main from the old base — this is now a
        // non-fast-forward relative to the remote's current tip.
        let base_commit = repo.find_commit(base_oid).unwrap();
        let local_child_oid = commit_empty_tree(&repo, "our diverged commit", &[&base_commit]);
        repo.reference("refs/heads/main", local_child_oid, true, "advance local")
            .unwrap();

        let result = push_lgs(&repo, "main");
        assert!(
            result.is_err(),
            "a non-fast-forward push must surface as Err, not a silent Ok"
        );
        assert_eq!(
            bare.find_reference("refs/heads/main").unwrap().target().unwrap(),
            peer_oid,
            "the rejected push must not have moved the remote ref"
        );
    }

    // --- Review Important-2: bounded fetch/push -----------------------------
    //
    // Honest limitation, stated rather than hidden: these local, in-process
    // bare-repo remotes transfer their (tiny) content essentially instantly,
    // so there is no way to construct a genuinely SLOW local remote here to
    // prove a transfer already in flight gets interrupted mid-transfer —
    // doing that for real would need a deliberately slow network-level
    // remote (e.g. a custom smart-HTTP server that stalls mid-response),
    // which is out of scope for a unit test and out of bounds for this
    // task's "local bare repos in tempdirs only" safety constraint. What IS
    // both real and reliably testable without any of that: a deadline that
    // has ALREADY elapsed before the call starts must abort rather than
    // silently completing — proving the callbacks are actually wired to the
    // real libgit2 call (not merely present in the source) and that
    // `Instant::now() < deadline` really does return `false` and really
    // does propagate as an aborted transfer, not swallowed.

    #[test]
    fn fetch_aborts_when_the_deadline_has_already_elapsed() {
        let bare_dir = tempfile::tempdir().unwrap();
        let bare = git2::Repository::init_bare(bare_dir.path()).unwrap();
        let auth_oid = commit_empty_tree(&bare, "authoritative peer tip", &[]);
        bare.reference("refs/lgs-auth/heads/main", auth_oid, true, "test").unwrap();

        let work_dir = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(work_dir.path()).unwrap();
        repo.remote("lgs", bare_dir.path().to_str().unwrap()).unwrap();

        let already_past = Instant::now() - Duration::from_secs(1);
        let result = fetch_lgs_with_deadline(&repo, already_past);

        assert!(
            result.is_err(),
            "a fetch whose deadline already elapsed before the call must abort, not complete \
             normally as if unbounded"
        );
        assert!(
            repo.find_reference("refs/remotes/lgs-auth/main").is_err(),
            "an aborted fetch must not have landed the auth ref"
        );
    }

    #[test]
    fn fetch_with_a_generous_deadline_still_succeeds() {
        // The other half: the deadline check must not be so eager that it
        // breaks the ordinary, fast, well-within-budget case.
        let bare_dir = tempfile::tempdir().unwrap();
        let bare = git2::Repository::init_bare(bare_dir.path()).unwrap();
        let auth_oid = commit_empty_tree(&bare, "authoritative peer tip", &[]);
        bare.reference("refs/lgs-auth/heads/main", auth_oid, true, "test").unwrap();

        let work_dir = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(work_dir.path()).unwrap();
        repo.remote("lgs", bare_dir.path().to_str().unwrap()).unwrap();

        let generous = Instant::now() + Duration::from_secs(30);
        fetch_lgs_with_deadline(&repo, generous).unwrap();

        assert_eq!(
            repo.find_reference("refs/remotes/lgs-auth/main").unwrap().target().unwrap(),
            auth_oid
        );
    }

    #[test]
    fn push_aborts_when_the_deadline_has_already_elapsed() {
        // `push_negotiation` fires exactly once, unconditionally, for every
        // push — unlike fetch's `transfer_progress`, this does not depend
        // on how much data ends up moving, so an already-past deadline is
        // guaranteed to be observed here.
        let bare_dir = tempfile::tempdir().unwrap();
        git2::Repository::init_bare(bare_dir.path()).unwrap();

        let work_dir = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(work_dir.path()).unwrap();
        repo.remote("lgs", bare_dir.path().to_str().unwrap()).unwrap();
        let base_oid = commit_empty_tree(&repo, "base", &[]);
        repo.reference("refs/heads/main", base_oid, true, "init").unwrap();

        let already_past = Instant::now() - Duration::from_secs(1);
        let result = push_lgs_with_deadline(&repo, "main", already_past);

        assert!(
            result.is_err(),
            "a push whose deadline already elapsed before negotiation must abort"
        );
        let bare = git2::Repository::open_bare(bare_dir.path()).unwrap();
        assert!(
            bare.find_reference("refs/heads/main").is_err(),
            "an aborted push must not have landed anything on the remote"
        );
    }

    #[test]
    fn push_with_a_generous_deadline_still_succeeds() {
        let bare_dir = tempfile::tempdir().unwrap();
        git2::Repository::init_bare(bare_dir.path()).unwrap();

        let work_dir = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(work_dir.path()).unwrap();
        repo.remote("lgs", bare_dir.path().to_str().unwrap()).unwrap();
        let base_oid = commit_empty_tree(&repo, "base", &[]);
        repo.reference("refs/heads/main", base_oid, true, "init").unwrap();

        let generous = Instant::now() + Duration::from_secs(30);
        push_lgs_with_deadline(&repo, "main", generous).unwrap();

        let bare = git2::Repository::open_bare(bare_dir.path()).unwrap();
        assert_eq!(bare.find_reference("refs/heads/main").unwrap().target().unwrap(), base_oid);
    }

    /// Task 20's `check_sync` needs a ref OTHER than a branch — pins that
    /// `push_lgs_refspec`/`fetch_lgs_refspec` round-trip an arbitrary
    /// namespace (`refs/sync-check/probe`) rather than being hardcoded to
    /// `refs/heads/*` the way `push_lgs`/`fetch_lgs` are.
    #[test]
    fn push_and_fetch_refspec_round_trip_a_non_branch_ref() {
        let bare_dir = tempfile::tempdir().unwrap();
        git2::Repository::init_bare(bare_dir.path()).unwrap();

        let work_dir = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(work_dir.path()).unwrap();
        repo.remote("lgs", bare_dir.path().to_str().unwrap()).unwrap();

        let oid = commit_empty_tree(&repo, "probe", &[]);
        repo.reference("refs/sync-check/probe", oid, true, "probe").unwrap();

        push_lgs_refspec(&repo, "refs/sync-check/probe:refs/sync-check/probe").unwrap();

        let bare = git2::Repository::open_bare(bare_dir.path()).unwrap();
        assert_eq!(
            bare.find_reference("refs/sync-check/probe").unwrap().target().unwrap(),
            oid,
            "the probe ref must land on the remote under its own name"
        );

        // Fetch it back into a distinct local tracking ref, proving the
        // round trip through the remote rather than just trusting the push.
        fetch_lgs_refspec(
            &repo,
            "+refs/sync-check/probe:refs/remotes/sync-check-probe/probe",
        )
        .unwrap();
        assert_eq!(
            repo.find_reference("refs/remotes/sync-check-probe/probe").unwrap().target().unwrap(),
            oid
        );
    }

    /// A force-prefixed push refspec (`+`) must overwrite an unrelated,
    /// non-fast-forward ref on the remote rather than being rejected —
    /// this is exactly what lets `check_sync` self-heal from a previous
    /// run's leftover, un-cleaned-up probe ref instead of failing forever.
    #[test]
    fn push_refspec_with_force_prefix_overwrites_a_non_fast_forward_ref() {
        let bare_dir = tempfile::tempdir().unwrap();
        let bare = git2::Repository::init_bare(bare_dir.path()).unwrap();
        let stale_oid = commit_empty_tree(&bare, "stale leftover probe", &[]);
        bare.reference("refs/sync-check/probe", stale_oid, true, "leftover").unwrap();

        let work_dir = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(work_dir.path()).unwrap();
        repo.remote("lgs", bare_dir.path().to_str().unwrap()).unwrap();
        let fresh_oid = commit_empty_tree(&repo, "fresh probe", &[]);
        repo.reference("refs/sync-check/probe", fresh_oid, true, "fresh").unwrap();

        // Without the `+` prefix this would be a non-fast-forward (the two
        // commits share no history) and would be rejected.
        push_lgs_refspec(&repo, "+refs/sync-check/probe:refs/sync-check/probe").unwrap();

        assert_eq!(
            bare.find_reference("refs/sync-check/probe").unwrap().target().unwrap(),
            fresh_oid,
            "a forced push must overwrite the stale leftover ref"
        );
    }

    /// An empty source side (`:refs/...`) is a delete refspec — pins that
    /// `push_lgs_refspec` supports deleting the remote's ref, which is how
    /// `check_sync`'s `Cleanup` stage removes the probe ref it pushed.
    #[test]
    fn push_refspec_with_empty_source_deletes_the_remote_ref() {
        let bare_dir = tempfile::tempdir().unwrap();
        let bare = git2::Repository::init_bare(bare_dir.path()).unwrap();
        let oid = commit_empty_tree(&bare, "probe", &[]);
        bare.reference("refs/sync-check/probe", oid, true, "probe").unwrap();

        let work_dir = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(work_dir.path()).unwrap();
        repo.remote("lgs", bare_dir.path().to_str().unwrap()).unwrap();

        push_lgs_refspec(&repo, ":refs/sync-check/probe").unwrap();

        let bare = git2::Repository::open_bare(bare_dir.path()).unwrap();
        assert!(
            bare.find_reference("refs/sync-check/probe").is_err(),
            "a delete refspec must remove the ref from the remote"
        );
    }

    #[test]
    fn clone_repo_clones_from_a_local_bare_repo() {
        let bare_dir = tempfile::tempdir().unwrap();
        let bare = git2::Repository::init_bare(bare_dir.path()).unwrap();
        let oid = commit_empty_tree(&bare, "seed", &[]);
        bare.reference("refs/heads/main", oid, true, "init").unwrap();
        bare.set_head("refs/heads/main").unwrap();

        let dest_dir = tempfile::tempdir().unwrap();
        let dest_path = dest_dir.path().join("clone");
        let cloned = clone_repo(bare_dir.path().to_str().unwrap(), &dest_path).unwrap();

        let head_commit = cloned.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(head_commit.id(), oid);
    }

    #[test]
    fn commit_merge_creates_a_two_parent_merge_commit_with_the_injected_timestamp() {
        let dir = tempfile::tempdir().unwrap();
        let gm = GitManager::with_clock(|| 1_700_000_500);
        gm.init_repo(dir.path()).unwrap();

        std::fs::write(dir.path().join("f1.txt"), "one").unwrap();
        gm.add_all(dir.path()).unwrap();
        let parent1 = gm.commit(dir.path(), "first parent").unwrap();

        // A second, unrelated commit object to act as the other merge parent
        // (as if it were the peer's tip fetched via refs/lgs-auth/*).
        let repo = git2::Repository::open(dir.path()).unwrap();
        let parent2_oid = commit_empty_tree(&repo, "second parent", &[]);
        let parent2 = parent2_oid.to_string();

        // The file the merge is expected to actually commit, proving the
        // resulting tree reflects working state rather than being empty.
        std::fs::write(dir.path().join("merged.txt"), "merged content").unwrap();

        let merge_oid = gm
            .commit_merge(dir.path(), "merge commit", &[&parent1, &parent2])
            .unwrap();

        let repo = git2::Repository::open(dir.path()).unwrap();
        let commit = repo
            .find_commit(git2::Oid::from_str(&merge_oid).unwrap())
            .unwrap();

        assert_eq!(commit.parent_count(), 2);
        let parent_ids: std::collections::HashSet<String> =
            commit.parent_ids().map(|id| id.to_string()).collect();
        assert!(parent_ids.contains(&parent1));
        assert!(parent_ids.contains(&parent2));

        // The property Task 15's tie-break tests depend on: the committer
        // timestamp comes from the injected clock, not wall-clock time.
        assert_eq!(commit.committer().when().seconds(), 1_700_000_500);

        let tree = commit.tree().unwrap();
        let entry = tree
            .get_name("merged.txt")
            .expect("merge commit's tree must contain the file written before the call");
        let blob = repo.find_blob(entry.id()).unwrap();
        assert_eq!(blob.content(), b"merged content");
    }

    #[test]
    fn commit_merge_with_a_single_parent_behaves_like_an_ordinary_commit() {
        let dir = tempfile::tempdir().unwrap();
        let gm = GitManager::with_clock(|| 1_700_000_600);
        gm.init_repo(dir.path()).unwrap();

        std::fs::write(dir.path().join("f1.txt"), "one").unwrap();
        gm.add_all(dir.path()).unwrap();
        let parent1 = gm.commit(dir.path(), "first").unwrap();

        std::fs::write(dir.path().join("f2.txt"), "two").unwrap();
        let oid = gm.commit_merge(dir.path(), "second", &[&parent1]).unwrap();

        let repo = git2::Repository::open(dir.path()).unwrap();
        let commit = repo.find_commit(git2::Oid::from_str(&oid).unwrap()).unwrap();
        assert_eq!(commit.parent_count(), 1);
        assert_eq!(commit.parent_id(0).unwrap().to_string(), parent1);
    }

    #[test]
    fn commit_merge_with_zero_parents_errors_rather_than_creating_a_root_commit() {
        let dir = tempfile::tempdir().unwrap();
        let gm = GitManager::new();
        gm.init_repo(dir.path()).unwrap();
        std::fs::write(dir.path().join("f.txt"), "x").unwrap();

        let result = gm.commit_merge(dir.path(), "no parents", &[]);
        assert!(
            result.is_err(),
            "zero parents must error, not silently create a root commit"
        );
    }
}
