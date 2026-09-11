// egui-frontend/tests/two_machine_sync.rs
mod common;
use common::two_machine::{Machine, TwoMachineHarness};

use allowance_tracker_egui::backend::storage::git::{ensure_lgs_remote, fetch_lgs, push_lgs, GitManager};
use std::time::{Duration, Instant};

/// `LGS_BINARY` gates this test so an ordinary `cargo test` on a machine
/// without `lgs` (or in a CI image that hasn't installed it) skips cleanly
/// instead of failing. Also `#[ignore]`d below, belt and suspenders — this
/// spawns two real daemon processes and is not something a default test run
/// should do implicitly.
fn harness() -> Option<TwoMachineHarness> {
    let bin = std::env::var("LGS_BINARY").ok()?;
    Some(TwoMachineHarness::new(bin.into()))
}

/// Poll `lgs status --json` on `m` until `project` reads `durability.state ==
/// "backed_up"`, or panic once `timeout` elapses. Bounded, not a blind sleep:
/// `SyncNow` only nudges the daemon's async reconcile loop (see
/// `local-git-sync/src/daemon/ipc.rs`), so the cloud write it triggers can
/// trail the CLI call returning by a small, variable amount (observed ~1-2s
/// in the Task 1 spike; bounded here at 60s to absorb slower CI).
fn wait_until_backed_up(h: &TwoMachineHarness, m: &Machine, project: &str, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    loop {
        let json = h.lgs(m, &["status", "--json"]);
        let v: serde_json::Value =
            serde_json::from_str(&json).expect("parsing `lgs status --json`");
        let backed_up = v["projects"].as_array().into_iter().flatten().any(|p| {
            p["name"] == project && p["durability"]["state"] == "backed_up"
        });
        if backed_up {
            return;
        }
        if Instant::now() >= deadline {
            panic!(
                "'{project}' never reached durability.state == \"backed_up\" within {timeout:?}; \
                 last `lgs status --json` was: {json}"
            );
        }
        std::thread::sleep(Duration::from_millis(300));
    }
}

/// Poll (fetching `refs/lgs-auth/*` from `url` into `repo` each attempt) until
/// `refs/remotes/lgs-auth/<branch>` resolves to `expected`, or panic with a
/// clear, non-vacuous message once `timeout` elapses. Never weakens to "the
/// ref merely exists" — a stale or wrong-machine value at that ref would pass
/// that check and prove nothing about *this* divergence.
fn wait_for_auth_tip(
    repo: &git2::Repository,
    branch: &str,
    expected: git2::Oid,
    timeout: Duration,
) -> git2::Oid {
    let deadline = Instant::now() + timeout;
    let mut last_seen: Option<git2::Oid> = None;
    loop {
        fetch_lgs(repo).expect("fetch refs/lgs-auth/* and refs/heads/* from the lgs remote");
        let auth_ref = format!("refs/remotes/lgs-auth/{branch}");
        if let Ok(r) = repo.find_reference(&auth_ref) {
            if let Some(oid) = r.target() {
                last_seen = Some(oid);
                if oid == expected {
                    return oid;
                }
            }
        }
        if Instant::now() >= deadline {
            panic!(
                "refs/remotes/lgs-auth/{branch} never converged on {expected} within {timeout:?}; \
                 last observed value there: {last_seen:?} (None means the ref never appeared at all)"
            );
        }
        std::thread::sleep(Duration::from_millis(300));
    }
}

/// Write `content` to `file` under `work`, stage, and commit via `gm`.
/// Returns the new commit's oid as a string.
fn write_and_commit(gm: &GitManager, work: &std::path::Path, file: &str, content: &str) -> String {
    std::fs::write(work.join(file), content).expect("writing fixture file");
    gm.add_all(work).expect("staging fixture file");
    gm.commit(work, &format!("commit {file}")).expect("committing fixture file")
}

#[test]
#[ignore]
fn concurrent_edits_are_visible_through_the_lgs_auth_mirror() {
    // The test that would have caught the refspec bug. A plain bare repo
    // neither accepts nor rejects the way lgs does, so this needs real daemons.
    let Some(h) = harness() else {
        eprintln!("set LGS_BINARY to run; skipping");
        return;
    };
    let project = "two-machine-sync-test";
    let gm = GitManager::new();

    // --- Seed: A gets a real working repo with one commit on `main`. ---
    //
    // Registering a project and cloning it before any commit has been pushed
    // yields a repo with an UNBORN HEAD ("reference 'refs/heads/master' not
    // found") — the Task 1 spike's fixture gotcha. Seeding with a real commit
    // and pushing it before B ever clones anything is how this harness avoids
    // rediscovering it.
    gm.init_repo(&h.a.work).expect("git init A's working repo");
    {
        // Force the branch name explicitly rather than trust whatever the
        // ambient git/libgit2 default happens to be on the machine running
        // this test — every assertion below is keyed on "main" by name.
        let repo = git2::Repository::open(&h.a.work).unwrap();
        repo.set_head("refs/heads/main").unwrap();
    }
    let _seed_oid = write_and_commit(&gm, &h.a.work, "ledger.txt", "seed\n");

    // --- Register the project on A, and push the seed commit into A's own bare. ---
    h.lgs(&h.a, &["add", &h.a.work.to_string_lossy(), "--name", project]);
    let a_url = h.a.clone_url(project);
    let repo_a = git2::Repository::open(&h.a.work).unwrap();
    ensure_lgs_remote(&repo_a, &a_url).expect("point A's working repo at its own lgs remote");
    push_lgs(&repo_a, "main").expect("push A's seed commit into A's own bare");

    // --- Publish A's bare to the shared cloud, and wait (bounded) for it to land. ---
    h.lgs(&h.a, &["sync", project]);
    wait_until_backed_up(&h, &h.a, project, Duration::from_secs(60));

    // --- Bring the project onto B from the cloud: this is the real
    // multi-machine onboarding path (`lgs restore`), not a raw git clone. It
    // registers the project on B, restores B's own bare from the cloud, and
    // clones B's own bare into B's working copy — which now has real history
    // (the seed commit), again avoiding the unborn-HEAD trap. ---
    h.lgs(&h.b, &["restore", project, &h.b.work.to_string_lossy()]);

    // --- The real divergence: independent commits from the same seed. ---
    let a_tip = write_and_commit(&gm, &h.a.work, "ledger.txt", "seed\nA's entry\n");
    let b_tip = write_and_commit(&gm, &h.b.work, "ledger.txt", "seed\nB's entry\n");
    assert_ne!(
        a_tip, b_tip,
        "test bug: A's and B's post-seed commits must differ for this to be a real divergence"
    );
    let a_tip = git2::Oid::from_str(&a_tip).unwrap();
    let b_tip = git2::Oid::from_str(&b_tip).unwrap();

    // Push each machine's new tip into its own bare (each is a fast-forward
    // from that machine's own bare's perspective — the divergence is between
    // machines, not within either one's own history yet).
    push_lgs(&repo_a, "main").expect("push A's divergent commit into A's own bare");
    let repo_b = git2::Repository::open(&h.b.work).unwrap();
    ensure_lgs_remote(&repo_b, &h.b.clone_url(project))
        .expect("point B's working repo at its own lgs remote (renaming restore's `origin`)");
    push_lgs(&repo_b, "main").expect("push B's divergent commit into B's own bare");

    // --- Publish A, ingest into B, publish B, ingest into A. ---
    h.sync_both_ways(project);

    // reconcile is asynchronous relative to the CLI call that triggers it, so
    // poll (bounded) rather than assume `sync_both_ways` alone was enough.
    let landed = wait_for_auth_tip(&repo_b, "main", a_tip, Duration::from_secs(60));
    assert_eq!(
        landed, a_tip,
        "refs/remotes/lgs-auth/main on B must resolve to A's tip"
    );

    // The test this harness exists for: `refs/heads/main` on B — fetched
    // above via LGS_HEADS_REFSPEC into refs/remotes/lgs/main, but checked
    // here at B's own local branch — must still be B's own commit, genuinely
    // different from A's. lgs's reconcile never moves a head backward or
    // across a divergence; if it silently fast-forwarded B's own branch onto
    // A's tip instead of mirroring it under refs/lgs-auth/*, this would catch
    // it by finding no divergence left to observe.
    let repo_b_reopened = git2::Repository::open(&h.b.work).unwrap();
    let auth = repo_b_reopened
        .find_reference("refs/remotes/lgs-auth/main")
        .expect(
            "the peer tip must be reachable under refs/lgs-auth/*; \
             fetching refs/heads/* alone would return B's own commit",
        );
    assert_eq!(auth.target().unwrap(), a_tip, "refs/remotes/lgs-auth/main must be A's tip");

    let b_head = repo_b_reopened
        .find_reference("refs/heads/main")
        .expect("B's own local main branch must still exist");
    assert_eq!(
        b_head.target().unwrap(),
        b_tip,
        "refs/heads/main on B must still be B's own commit — divergence must be real, \
         not silently resolved by the sync itself"
    );
    assert_ne!(
        auth.target().unwrap(),
        b_head.target().unwrap(),
        "refs/lgs-auth/main and refs/heads/main on B must genuinely differ under divergence"
    );

    println!(
        "OK: B's refs/heads/main == {b_tip} (its own commit), \
         refs/remotes/lgs-auth/main == {a_tip} (A's peer tip) — genuine divergence, both visible."
    );
}
