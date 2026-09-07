// egui-frontend/tests/spike_git2_http.rs
//! Spike: does `git2` with default-features = false speak smart-HTTP to lgs?
//! Ignored by default — it needs a live daemon. Run explicitly:
//!   cargo test -p allowance_tracker_egui --test spike_git2_http -- --ignored --nocapture

#[test]
#[ignore]
fn git2_can_clone_fetch_and_push_over_local_http() {
    let url = std::env::var("LGS_CLONE_URL")
        .expect("set LGS_CLONE_URL to a clone_url from `lgs status --json`");
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
    repo.commit(Some("HEAD"), &sig, &sig, "spike", &tree, &[&parent]).unwrap();

    // 3. Push — the operation most likely to be missing.
    let mut remote = repo.find_remote("origin").unwrap();
    let head = repo.head().unwrap();
    let branch = head.shorthand().unwrap();
    let refspec = format!("refs/heads/{branch}:refs/heads/{branch}");
    remote.push(&[refspec.as_str()], None)
        .expect("PUSH FAILED — receive-pack unsupported under these feature flags");
    println!("push: OK");

    // 4. Fetch the lgs-auth mirror namespace the real design depends on.
    remote.fetch(&["+refs/lgs-auth/heads/*:refs/remotes/lgs-auth/*"], None, None)
        .expect("FETCH of refs/lgs-auth/* FAILED");
    println!("fetch refs/lgs-auth/*: OK");
}
