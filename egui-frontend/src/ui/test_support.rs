//! One fixture for child-repo sync tests: a real child with a real git repo,
//! a planted "peer" commit, and whatever dirty/deleted/interrupted state the
//! scenario needs.
//!
//! Also home to [`run_cycles_until_terminal`], the driver that exercises the
//! real classify -> merge -> apply loop repeatedly against a fixed peer tip,
//! and [`assert_resolved_or_explained`], the invariant defect 1 violated.
//!
//! This module lives inside the crate (not `egui-frontend/tests/`, which is
//! a separate crate) specifically so it can reach `apply_merge` /
//! `apply_fast_forward` (private methods on `AllowanceTrackerApp`) and
//! `merge_diverged` / `working_tree_dirty` (`pub(crate)` in
//! `backend::sync::child_sync`) without widening any production visibility.

use crate::backend::domain::commands::allowance::UpdateAllowanceConfigCommand;
use crate::backend::domain::commands::child::{CreateChildCommand, SetActiveChildCommand};
use crate::backend::domain::commands::goal::CreateGoalCommand;
use crate::backend::domain::commands::transactions::CreateTransactionCommand;
use crate::backend::storage::csv::ParentalControlRepository;
use crate::backend::storage::traits::ParentalControlStorage;
use crate::backend::Backend;
use crate::ui::app_state::AllowanceTrackerApp;
use git2::Repository;

pub struct ChildRepoFixture {
    peer_files: Option<Vec<(String, String)>>,
    dirty: Vec<(String, String)>,
    deleted: Vec<String>,
    marker: bool,
    with_goal: bool,
    with_allowance_config: bool,
    with_parental_control_log: bool,
}

impl ChildRepoFixture {
    pub fn new() -> Self {
        Self {
            peer_files: None,
            dirty: Vec::new(),
            deleted: Vec::new(),
            marker: false,
            with_goal: false,
            with_allowance_config: false,
            with_parental_control_log: false,
        }
    }

    /// Create a real goal via `GoalService` before HEAD/the peer tip are
    /// captured, so `goals.csv` is genuinely tracked in HEAD (and carried
    /// into the peer's tree, same as `child.yaml`/`transactions.csv`
    /// already are). Without this, `.with_dirty("goals.csv", ..)` would
    /// write a brand-new UNTRACKED file (never staged, so
    /// `working_tree_dirty` — which ignores untracked paths — would never
    /// even see it) and `.with_deleted("goals.csv")` would silently no-op
    /// (the file would not exist yet to delete). Neither would exercise the
    /// guard at all — passing vacuously, the exact failure mode
    /// `a_dirty_tracked_unowned_file_resolves_rather_than_stalling` already
    /// warns against for its own fixture.
    pub fn with_goal(mut self) -> Self {
        self.with_goal = true;
        self
    }

    /// Create a real allowance config via `AllowanceService` before HEAD is
    /// captured, for the same reason as [`Self::with_goal`]: so
    /// `allowance_config.yaml` is a genuinely tracked file, not one that
    /// `.with_dirty`/`.with_deleted` would silently fail to exercise.
    pub fn with_allowance_config(mut self) -> Self {
        self.with_allowance_config = true;
        self
    }

    /// Record a real parental-control attempt directly through
    /// `ParentalControlRepository` (bypassing `ParentalControlService`,
    /// which always writes under the literal `"global"` pseudo-id and so
    /// never lands this file in a real child's directory in production —
    /// see `backend::domain::parental_control_service`). This mirrors
    /// exactly how `ownership_contract`'s completeness test
    /// (`backend/sync/paths.rs`) exercises the same per-child write path,
    /// and is the only way to get `parental_control_attempts.csv` tracked
    /// in a child's repo at all. Same reasoning as [`Self::with_goal`] for
    /// why this must happen before HEAD/the peer tip are captured.
    pub fn with_parental_control_log(mut self) -> Self {
        self.with_parental_control_log = true;
        self
    }

    /// Plant a commit in the object database reachable from no ref —
    /// exactly what `ChildSyncEngine::cycle` would have fetched into
    /// `refs/remotes/lgs-auth/main`, without needing a real remote.
    pub fn with_peer_commit(mut self, files: &[(&str, &str)]) -> Self {
        self.peer_files =
            Some(files.iter().map(|(n, c)| (n.to_string(), c.to_string())).collect());
        self
    }

    /// Leave `file` modified relative to HEAD and uncommitted — this app's
    /// designed steady state under the AWS transport, not a fault.
    pub fn with_dirty(mut self, file: &str, contents: &str) -> Self {
        self.dirty.push((file.to_string(), contents.to_string()));
        self
    }

    /// Delete a tracked file without committing the deletion.
    pub fn with_deleted(mut self, file: &str) -> Self {
        self.deleted.push(file.to_string());
        self
    }

    /// Write the interrupted-merge marker, as `apply_merge` does immediately
    /// before its first working-tree write.
    pub fn with_marker(mut self) -> Self {
        self.marker = true;
        self
    }

    /// Returns the app, the child id, the tempdir guard (hold it for the
    /// whole test), and the peer tip oid when one was planted.
    pub fn build(self) -> (AllowanceTrackerApp, String, tempfile::TempDir, Option<git2::Oid>) {
        let temp = tempfile::TempDir::new().expect("temp dir");
        let backend = Backend::with_data_dir(temp.path().to_path_buf(), None).expect("backend");
        let child = backend
            .child_service
            .create_child(CreateChildCommand {
                name: "Test Kid".to_string(),
                birthdate: "2015-01-01".to_string(),
            })
            .expect("create child")
            .child;
        backend
            .child_service
            .set_active_child(SetActiveChildCommand { child_id: child.id.clone() })
            .expect("set active child");
        // An ordinary transaction write initializes the git repo via
        // commit_file_change, exactly as production does.
        backend
            .transaction_service
            .create_transaction(CreateTransactionCommand {
                description: "Allowance".to_string(),
                amount: 10.0,
                date: None,
            })
            .expect("create transaction");

        // These three run BEFORE `head`/the peer tip are captured below, so
        // whichever of `goals.csv` / `allowance_config.yaml` /
        // `parental_control_attempts.csv` this fixture opts into are
        // genuinely tracked in HEAD (and carried unchanged into the peer's
        // tree) — see each builder method's doc comment for why that
        // ordering matters.
        if self.with_goal {
            backend
                .goal_service
                .create_goal(CreateGoalCommand {
                    child_id: None,
                    description: "Save for a bike".to_string(),
                    target_amount: 50.0,
                })
                .expect("create goal");
        }
        if self.with_allowance_config {
            backend
                .allowance_service
                .update_allowance_config(UpdateAllowanceConfigCommand {
                    child_id: None,
                    amount: 5.0,
                    day_of_week: 5,
                    is_active: true,
                    use_age_based_amount: false,
                })
                .expect("update allowance config");
        }
        if self.with_parental_control_log {
            let parental_control_repository =
                ParentalControlRepository::new((*backend.csv_connection).clone());
            parental_control_repository
                .record_parental_control_attempt(&child.id, "wrong guess", false)
                .expect("record parental control attempt");
        }

        let child_dir = backend
            .csv_connection
            .child_dir(&shared::ChildId::from(child.id.as_str()))
            .expect("child dir");
        let repo = Repository::open(&child_dir).expect("open child repo");
        let head = repo.head().unwrap().peel_to_commit().unwrap();

        // The peer commit: written straight into the object database,
        // reachable from no ref — exactly what ChildSyncEngine::cycle would
        // have fetched into refs/remotes/lgs-auth/main, with no real remote.
        //
        // Deviation from the brief's sketch: the tree builder is seeded from
        // HEAD's own tree (`repo.treebuilder(Some(&head.tree()))`), not an
        // empty one (`repo.treebuilder(None)`). A real peer commit never
        // drops `child.yaml` / `goals.csv` / `allowance_config.yaml` — only
        // `transactions.csv` differs in these scenarios. Seeding from `None`
        // produces a peer tree that holds ONLY the file(s) passed to
        // `with_peer_commit`, so `apply_fast_forward`'s real checkout (which
        // removes anything tracked in the old tree but absent from the
        // target tree) deletes `child.yaml` from disk and every subsequent
        // `child_dir()` lookup fails with "no child.yaml is there". This
        // fixture bug was caught by this task's own fast-forward smoke test
        // — exactly the kind of thing a single-shot test would never
        // exercise, since nothing after the checkout ever re-reads the
        // child's metadata.
        let head_tree = head.tree().unwrap();
        let peer_tip = self.peer_files.as_ref().map(|files| {
            let sig = git2::Signature::new(
                "Peer",
                "peer@example.com",
                &git2::Time::new(1_700_000_500, 0),
            )
            .unwrap();
            let mut builder = repo.treebuilder(Some(&head_tree)).unwrap();
            for (name, content) in files {
                let blob_id = repo.blob(content.as_bytes()).unwrap();
                builder.insert(name.as_str(), blob_id, 0o100644).unwrap();
            }
            let tree_id = builder.write().unwrap();
            let tree = repo.find_tree(tree_id).unwrap();
            repo.commit(None, &sig, &sig, "peer edit", &tree, &[&head]).unwrap()
        });

        // The marker goes on BEFORE any working-tree change, as the real
        // apply_merge writes it immediately before its first write.
        if self.marker {
            crate::backend::sync::child_sync::write_merge_marker(
                &repo,
                &head.id().to_string(),
                "a-prior-peer-oid",
            )
            .expect("write marker");
        }

        for (file, contents) in &self.dirty {
            std::fs::write(child_dir.join(file), contents).expect("write dirty file");
        }

        // Deliberately NOT staged: an uncommitted deletion is the state that
        // used to stall sync forever, because add_path cannot stage one.
        for file in &self.deleted {
            let path = child_dir.join(file);
            if path.exists() {
                std::fs::remove_file(&path).expect("remove tracked file");
            }
        }

        let app = AllowanceTrackerApp::new_for_test(backend);
        (app, child.id, temp, peer_tip)
    }
}

impl Default for ChildRepoFixture {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Terminal {
    Applied,
    UpToDate,
    FailedWithNotice,
}

/// Drive the real classify -> merge -> apply loop against a fixed peer tip
/// until it reaches a terminal state, or give up after `max` cycles.
///
/// # Why this exists
///
/// Defect 1 was a LIVENESS failure: a single `apply_merge` returning
/// `DirtyTreeCommitted` having committed nothing is not visibly wrong in
/// isolation — only on the second, third and `STALE_HEAD_REFUSAL_LIMIT`th
/// cycle. Every merge-path test in this suite was single-shot, which is
/// exactly why the defect shipped. The one progress assertion that existed
/// lived on the fast-forward path (`app_coordinator.rs`), and that is the
/// path the design got right.
///
/// On failure returns the outcome sequence, so the message reads "the same
/// outcome DirtyTreeCommitted 5 times with HEAD unmoved" rather than
/// "assertion failed: false".
pub fn run_cycles_until_terminal(
    app: &mut AllowanceTrackerApp,
    child_id: &str,
    peer_tip: git2::Oid,
    max: u8,
) -> Result<Terminal, Vec<String>> {
    use crate::backend::sync::child_sync::{classify, merge_diverged, Cycle, CycleOutcome};

    let mut trace: Vec<String> = Vec::new();

    for _ in 0..max {
        let child_dir = app
            .backend()
            .csv_connection
            .child_dir(&shared::ChildId::from(child_id))
            .unwrap();
        let repo = Repository::open(&child_dir).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap().id();
        let base = repo.merge_base(head, peer_tip).ok();

        let cycle = classify(
            Some(&head.to_string()),
            Some(&peer_tip.to_string()),
            base.map(|b| b.to_string()).as_deref(),
        );

        match cycle {
            Cycle::UpToDate | Cycle::Ahead => {
                trace.push(format!("{cycle:?} at head {head}"));
                return Ok(Terminal::UpToDate);
            }
            Cycle::FastForward => {
                let outcome = app.apply_fast_forward(child_id, &peer_tip.to_string());
                trace.push(format!("FastForward -> {outcome:?} at head {head}"));
            }
            Cycle::Diverged => {
                let computed = merge_diverged(&repo, head, peer_tip, base);
                let CycleOutcome::Merged { rows, parents, decisions, .. } = computed.unwrap()
                else {
                    unreachable!("merge_diverged always returns Merged");
                };
                let outcome = app.apply_merge(child_id, rows, &parents, &decisions);
                trace.push(format!("Diverged -> {outcome:?} at head {head}"));
            }
        }

        if !app.sync.sync_failures.iter().any(|n| n.child_id == child_id) {
            continue;
        }
        return Ok(Terminal::FailedWithNotice);
    }

    // Did the loop at least converge? Task 10 review fix: this used to pass
    // `None` as the base, so `classify` could only report `UpToDate` when
    // `head == peer_tip` exactly. After a real merge HEAD is a NEW two-parent
    // commit, never byte-equal to `peer_tip`, so `Terminal::Applied` was
    // effectively unreachable on the merge path — a run that only converged
    // on this final cycle was misreported as `Err(trace)`, a false failure
    // that (with Task 12's tight cycle bounds) would look exactly like the
    // stall this driver exists to detect. Computing the real merge base
    // (as every iteration above already does) fixes it: once `peer_tip` is
    // an ancestor of `head`, `classify` correctly reports `Ahead`, which
    // this function treats the same as `UpToDate` — nothing left to apply.
    let child_dir = app
        .backend()
        .csv_connection
        .child_dir(&shared::ChildId::from(child_id))
        .unwrap();
    let repo = Repository::open(&child_dir).unwrap();
    let head = repo.head().unwrap().peel_to_commit().unwrap().id();
    let base = repo.merge_base(head, peer_tip).ok();
    let final_cycle = classify(
        Some(&head.to_string()),
        Some(&peer_tip.to_string()),
        base.map(|b| b.to_string()).as_deref(),
    );
    if matches!(final_cycle, Cycle::UpToDate | Cycle::Ahead) {
        return Ok(Terminal::Applied);
    }

    Err(trace)
}

/// The invariant defect 1 violated: after the guard runs on any dirty tree,
/// the system has either made progress or said why. Never neither.
///
/// "HEAD advanced" is load-bearing. The weaker form — "the tree is clean or a
/// notice exists" — holds vacuously if the guard commits something unrelated
/// and leaves the real problem for the next cycle.
pub fn assert_resolved_or_explained(
    app: &AllowanceTrackerApp,
    repo: &Repository,
    child_id: &str,
    head_before: git2::Oid,
) {
    let head_now = repo.head().unwrap().peel_to_commit().unwrap().id();
    let clean =
        !crate::backend::sync::child_sync::working_tree_dirty(repo).unwrap();
    let advanced = head_now != head_before;
    let explained = app.sync.sync_failures.iter().any(|n| n.child_id == child_id);

    assert!(
        (clean && advanced) || explained,
        "neither resolved nor explained for child {child_id}: tree_clean={clean}, \
         head_advanced={advanced} ({head_before} -> {head_now}), notice_present={explained}. \
         This is the exact shape of defect 1 — a dirty tree that neither progresses nor reports."
    );
}

#[cfg(test)]
mod smoke_tests {
    use super::*;

    /// Proves the driver actually drives the real classify -> merge -> apply
    /// loop, not a reimplementation of it: a clean tree plus a peer commit
    /// that fast-forwards must reach a terminal state within a small cycle
    /// bound.
    #[test]
    fn fast_forward_reaches_a_terminal_state() {
        let (mut app, child_id, _temp, peer_tip) = ChildRepoFixture::new()
            .with_peer_commit(&[(
                "transactions.csv",
                "id,child_id,date,description,amount,balance,type\n\
                 in-1-a,test-kid,2026-01-01T00:00:00+00:00,Allowance,10.00,10.00,allowance\n\
                 ex-2-a,test-kid,2026-01-02T00:00:00+00:00,Slime,-5.00,5.00,expense\n",
            )])
            .build();
        let peer_tip = peer_tip.expect("peer commit was planted");

        let result = run_cycles_until_terminal(&mut app, &child_id, peer_tip, 3);
        assert!(
            matches!(result, Ok(Terminal::UpToDate) | Ok(Terminal::Applied)),
            "expected a terminal state within 3 cycles, got {result:?}"
        );
    }

    /// Stronger evidence than the fast-forward case: this drives a GENUINE
    /// three-way merge (not a no-op where "theirs" is byte-identical to
    /// base — see Task 8's `merge_diverged_computes_the_same_result_without_a_remote`,
    /// which this smoke test deliberately does not imitate). Local HEAD is
    /// advanced past the fixture's base commit with a second real
    /// transaction, while the planted peer commit diverges from that same
    /// base with different content, so `classify` must actually return
    /// `Cycle::Diverged` and `merge_diverged` must actually combine two
    /// different row sets.
    #[test]
    fn a_genuine_divergence_is_merged_and_reaches_a_terminal_state() {
        let (mut app, child_id, _temp, peer_tip) = ChildRepoFixture::new()
            .with_peer_commit(&[(
                "transactions.csv",
                "id,child_id,date,description,amount,balance,type\n\
                 in-1-a,test-kid,2026-01-01T00:00:00+00:00,Allowance,10.00,10.00,allowance\n\
                 ex-3-a,test-kid,2026-01-03T00:00:00+00:00,Book,-3.00,7.00,expense\n",
            )])
            .build();
        let peer_tip = peer_tip.expect("peer commit was planted");

        // Advance our own local HEAD past the fixture's base commit with a
        // second, independent transaction — this is what makes the peer
        // commit (planted as a sibling of the SAME base) a real divergence
        // rather than a fast-forward.
        app.backend()
            .transaction_service
            .create_transaction(CreateTransactionCommand {
                description: "Toy".to_string(),
                amount: -2.0,
                date: None,
            })
            .expect("create local transaction");

        let child_dir = app
            .backend()
            .csv_connection
            .child_dir(&shared::ChildId::from(child_id.as_str()))
            .unwrap();
        let repo = Repository::open(&child_dir).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap().id();
        let base = repo.merge_base(head, peer_tip).ok();
        assert_eq!(
            crate::backend::sync::child_sync::classify(
                Some(&head.to_string()),
                Some(&peer_tip.to_string()),
                base.map(|b| b.to_string()).as_deref(),
            ),
            crate::backend::sync::child_sync::Cycle::Diverged,
            "fixture setup must produce a genuine divergence, not a fast-forward"
        );

        let result = run_cycles_until_terminal(&mut app, &child_id, peer_tip, 3);
        assert!(
            matches!(result, Ok(Terminal::UpToDate) | Ok(Terminal::Applied)),
            "expected a terminal state within 3 cycles, got {result:?}"
        );

        let repo = Repository::open(&child_dir).unwrap();
        assert_resolved_or_explained(&app, &repo, &child_id, head);
    }
}
