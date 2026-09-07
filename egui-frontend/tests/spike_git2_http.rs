// egui-frontend/tests/spike_git2_http.rs
//! Spike: does `git2` with default-features = false speak smart-HTTP to lgs?
//! Ignored by default — it needs a live daemon and the `lgs` CLI plus `git`
//! on PATH. Run explicitly:
//!   cargo test -p allowance-tracker-egui --test spike_git2_http -- --ignored --nocapture
//!
//! Round 1 of this spike asserted "OK" from libgit2's own return values alone.
//! That is not sufficient:
//!   - `Remote::push` can return `Ok(())` even when the server rejected an
//!     individual ref update; only a `push_update_reference` callback surfaces
//!     that. So push success is verified two ways: via the callback, and
//!     independently via `git ls-remote` against the real remote.
//!   - `refs/lgs-auth/heads/<branch>` is written by the daemon's reconcile
//!     pass, which runs on its own poll interval plus a cloud round-trip —
//!     not synchronously on push. A `fetch` whose refspec matches nothing on
//!     the remote yet is not an error, so "fetch: OK" alone is indistinguishable
//!     from "the refspec matched zero refs". This version triggers a sync and
//!     polls the real remote (bounded, no blind sleep) until the ref exists
//!     before trusting the fetch.

use std::process::Command;
use std::time::{Duration, Instant};

/// Derive the lgs project name from a `clone_url` of the form
/// `http://localhost:<port>/<name>.git`.
fn project_name_from_clone_url(url: &str) -> String {
    let file = url
        .rsplit('/')
        .next()
        .expect("clone_url has no path segment");
    file.trim_end_matches(".git").to_string()
}

/// Ask the real remote (via the plain `git` CLI, not git2) which refs exist
/// matching `refspec`. Returns (oid, refname) pairs. This exists so the
/// verification does not trust the same git2 code path it is checking.
fn git_ls_remote(url: &str, refspec: &str) -> Vec<(String, String)> {
    let output = Command::new("git")
        .args(["ls-remote", url, refspec])
        .output()
        .expect("failed to run `git ls-remote` — is git on PATH?");
    assert!(
        output.status.success(),
        "git ls-remote {url} {refspec} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let oid = parts.next()?.to_string();
            let name = parts.next()?.to_string();
            Some((oid, name))
        })
        .collect()
}

#[test]
#[ignore]
fn git2_can_clone_fetch_and_push_over_local_http() {
    let url = std::env::var("LGS_CLONE_URL")
        .expect("set LGS_CLONE_URL to a clone_url from `lgs status --json`");
    let project = project_name_from_clone_url(&url);
    let dir = tempfile::tempdir().unwrap();
    let work = dir.path().join("work");

    // 1. Clone.
    let repo = git2::Repository::clone(&url, &work)
        .expect("CLONE FAILED — libgit2 has no usable http transport");
    println!("clone: OK");

    // 2. Commit something.
    std::fs::write(work.join("spike.txt"), "spike").unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("spike.txt")).unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let sig = git2::Signature::now("spike", "spike@example.com").unwrap();
    let parent = repo.head().unwrap().peel_to_commit().unwrap();
    let commit_oid = repo
        .commit(Some("HEAD"), &sig, &sig, "spike", &tree, &[&parent])
        .unwrap();

    // 3. Push — the operation most likely to be missing.
    let mut remote = repo.find_remote("origin").unwrap();
    let head = repo.head().unwrap();
    let branch = head.shorthand().unwrap().to_string();
    let refspec = format!("refs/heads/{branch}:refs/heads/{branch}");

    // `Remote::push` can return `Ok(())` while the server rejected an
    // individual ref update — only this callback surfaces that.
    let mut rejection: Option<String> = None;
    {
        let mut callbacks = git2::RemoteCallbacks::new();
        callbacks.push_update_reference(|refname, status| {
            if let Some(msg) = status {
                rejection = Some(format!("{refname}: {msg}"));
            }
            Ok(())
        });
        let mut opts = git2::PushOptions::new();
        opts.remote_callbacks(callbacks);
        remote
            .push(&[refspec.as_str()], Some(&mut opts))
            .expect("PUSH FAILED — receive-pack unsupported under these feature flags");
    }
    if let Some(msg) = rejection {
        panic!(
            "PUSH REJECTED at ref level (libgit2's push() returned Ok, but the \
             server refused the ref update) — {msg}"
        );
    }

    // Independently confirm the push actually landed on the remote — do not
    // trust libgit2's own view of what it just did.
    let remote_branch_ref = format!("refs/heads/{branch}");
    let remote_refs = git_ls_remote(&url, &remote_branch_ref);
    let remote_oid = remote_refs
        .iter()
        .find(|(_, name)| *name == remote_branch_ref)
        .map(|(oid, _)| oid.clone())
        .unwrap_or_else(|| {
            panic!(
                "push: git2 reported OK, but `git ls-remote` shows no {remote_branch_ref} \
                 at all on the remote"
            )
        });
    assert_eq!(
        remote_oid,
        commit_oid.to_string(),
        "push: git2 reported OK, but the remote's {remote_branch_ref} ({remote_oid}) \
         does not match the commit that was pushed ({commit_oid})"
    );
    println!("push: OK (verified via `git ls-remote`: {remote_branch_ref} == {commit_oid})");

    // 4. Fetch the lgs-auth mirror namespace the real design depends on.
    //
    // refs/lgs-auth/heads/<branch> is written by the daemon's reconcile pass
    // (poll interval + a cloud round-trip), not synchronously on push. Force
    // a sync, then poll the real remote via `git ls-remote` — bounded, no
    // blind sleep — until the ref actually exists there. Only then does a
    // subsequent git2 fetch of it mean anything.
    let sync_status = Command::new("lgs")
        .args(["sync", &project])
        .status()
        .expect("failed to run `lgs sync` — is lgs on PATH?");
    assert!(sync_status.success(), "`lgs sync {project}` exited non-zero");

    let lgs_auth_ref = format!("refs/lgs-auth/heads/{branch}");
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut remote_lgs_auth_oid: Option<String> = None;
    while Instant::now() < deadline {
        let refs = git_ls_remote(&url, "refs/lgs-auth/*");
        if let Some((oid, _)) = refs.iter().find(|(_, name)| *name == lgs_auth_ref) {
            remote_lgs_auth_oid = Some(oid.clone());
            break;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    let remote_lgs_auth_oid = match remote_lgs_auth_oid {
        Some(oid) => oid,
        None => panic!(
            "FETCH TARGET NEVER MATERIALIZED — {lgs_auth_ref} did not appear on the \
             remote within 60s of `lgs sync {project}`. This is the legitimate finding, \
             not a test bug: reconcile either did not run, ran but did not publish this \
             ref, or needs longer than 60s. The design's fetch step \
             (\"## Fetch — the refspec matters\") depends on this ref existing promptly \
             after push, so this would block Phase 2/3 as currently specified."
        ),
    };

    remote
        .fetch(
            &["+refs/lgs-auth/heads/*:refs/remotes/lgs-auth/*"],
            None,
            None,
        )
        .expect("FETCH of refs/lgs-auth/* FAILED");

    let local_ref = repo
        .find_reference(&format!("refs/remotes/lgs-auth/{branch}"))
        .unwrap_or_else(|e| {
            panic!(
                "fetch returned OK, but refs/remotes/lgs-auth/{branch} was not created \
                 locally: {e}"
            )
        });
    let local_oid = local_ref
        .target()
        .expect("refs/remotes/lgs-auth/<branch> is not a direct (non-symbolic) reference");
    assert_eq!(
        local_oid.to_string(),
        remote_lgs_auth_oid,
        "fetched refs/remotes/lgs-auth/{branch} ({local_oid}) does not match what \
         `git ls-remote` saw on the server ({remote_lgs_auth_oid})"
    );
    println!(
        "fetch refs/lgs-auth/*: OK (refs/remotes/lgs-auth/{branch} == {local_oid}, \
         confirmed non-trivial transfer via ls-remote + reconcile)"
    );
}
