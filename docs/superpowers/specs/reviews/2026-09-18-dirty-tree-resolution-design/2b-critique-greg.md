# [2B] Critique — Greg Grubberstone

**Spec reviewed:** `docs/superpowers/specs/2026-09-18-dirty-tree-resolution-design.md`
**Reviewer:** Greg Grubberstone, Senior Engineer
**Date:** 2026-09-18

---

## Overall Verdict

Approve with changes.

The diagnosis is right and the four changes are the right four changes. My
complaints are all about *shape*, not direction. One of them (Concern 1) means
the fix as written doesn't actually fix anything.

## Top Concerns

### Concern 1: `commit_file_change` returning its error fixes nothing — every caller discards it

**What I see:** Under "Error handling": *"`commit_file_change` (`git/mod.rs:388`)
starts returning its commit error instead of swallowing it with a `warn!`."*

Every call site in the tree:

```
backend/storage/csv/transaction_repository.rs:201:      let _ = self.git_manager.commit_file_change(
backend/storage/csv/allowance_repository.rs:106:       let _ = self.git_manager.commit_file_change(
backend/storage/csv/goal_repository.rs:100:            let _ = self.git_manager.commit_file_change(
backend/storage/csv/child_repository.rs:107:           let _ = self.git_manager.commit_file_change(
backend/storage/csv/parental_control_repository.rs:163: let _ = self.git_manager.commit_file_change(
```

Five for five. All `let _ =`.

**Why it matters:** The spec names this swallow as the thing that strands
`parental_control_attempts.csv` dirty and reaches defect 1's first route. Moving
the swallow from inside the function to the call site changes the type signature
and nothing else. The file still ends up dirty, the merge still stalls. We will
ship this, declare the route closed, and hit the same stall.

**Recommendation:** The spec must name all five call sites and say what each one
does with the error. My default: propagate. `write_transactions` already returns
`Result<()>`; a failed commit of the user's own transaction is not a warning, it
is a failed write. `parental_control_repository.rs:163` is the one I'd argue
about — it's an audit-log append on a failure path — but argue about it in the
spec, not in a `let _`. Put `#[must_use]` on the return so the next `let _` has
to be written deliberately.

### Concern 2: `DirtyTreeResolution` is a `Result` wearing a costume

**What I see:** Section 3 proposes

```rust
enum DirtyTreeResolution {
    Committed(String),
    NothingOwnedToCommit,
    Failed(String),
}
```

And then the error table says staging failure, commit failure, and
`NothingOwnedToCommit` all produce the identical triple: `SyncFailureNotice` +
`SyncStatus::Error` + `Failed`. In *both* callers.

**Why it matters:** Three enum variants, two of which no consumer distinguishes.
`Failed(String)` is the stringly-typed-error anti-pattern (Rust Design Patterns,
"anti-patterns"; Effective Rust Item 4): the `git2::Error` cause chain is
flattened into a display string at the point of failure, so the log line loses
the underlying reason exactly when you need it. And the success branch is
`Committed(String)` — an oid as a `String`, when `git2::Oid` is `Copy` and
already the right type.

**Recommendation:** This is `Result`. Write it as `Result`:

```rust
#[derive(Debug, thiserror::Error)]
enum DirtyTreeError {
    #[error("could not stage its local {file}")]
    Stage { file: &'static str, #[source] source: git2::Error },
    #[error("could not commit its local changes")]
    Commit(#[source] anyhow::Error),
    #[error("a tracked file this app does not manage is dirty — resolve it by hand")]
    NothingOwned,
}

fn resolve_dirty_tree(repo: &Repository, message: &str) -> Result<git2::Oid, DirtyTreeError>
```

`thiserror` is already a dependency of `egui-frontend`. The `Display` impl *is*
the user-facing message — one definition, not a `String` passed hand to hand.
Both callers collapse to:

```rust
match resolve_dirty_tree(repo, &message) {
    Ok(oid) => { /* the one place the two paths genuinely differ */ }
    Err(e)  => { self.fail_sync(child_id, &e.to_string()); Outcome::Failed }
}
```

While you're there: `commit_dirty_tree_before_merge` and
`commit_dirty_tree_to_unblock_fast_forward` between them contain eight copies of
"format a message, set `sync.status`, call `record_sync_failure`, return
`Failed`." Add `fn fail_sync(&mut self, child_id: &str, message: &str)` and
delete the other seven.

### Concern 3: removing the hard reset leaves no validity gate for files written before this ships

**What I see:** Section 4 deletes `git2::ResetType::Hard` from the sync paths
entirely, on the grounds that section 1's atomic writes remove the torn-file
ambiguity. The existing `recover_if_dirty` doc comment (`child_sync.rs:896-904`)
names a torn `fs::write` as one of the two things it exists to discard.

**Why it matters:** Atomic writes only protect files written by builds that have
them. A user upgrading with a half-written `transactions.csv` already on disk —
or one left by `goal_repository.rs:128`'s streaming `csv::Writer`, which is
exactly the un-atomic path this spec is fixing — now takes a different route: the
dirty-tree guard commits it and pushes it to the peer. Permanently, into shared
history, where it becomes a merge base. The hard reset was a bad answer to a real
question, and the spec removes the answer without handling the question during
the window where it still applies.

**Recommendation:** Replace the reset with a validity gate, not with nothing.
Before the dirty-tree guard commits, parse `transactions.csv` with
`allowance_core::codec::parse_transactions` — the function `child_sync::read_rows`
already calls. If it fails to parse, that's a `SyncFailureNotice`, not a commit.
Five lines, no new dependency, no reset, and it preserves the one property the
reset was protecting. Add the test: dirty *and* unparseable ⇒ failure notice, no
commit, file untouched.

### Concern 4: the staging loop hand-rolls what libgit2 already does

**What I see:** Section 2's three-branch loop: `exists` ⇒ `add_path`,
`tracked_in_HEAD` ⇒ `remove_path`, else skip. Plus a HEAD-tree lookup per file.

**Why it matters:** Not a disaster — but `git2::Index` already has this, with the
pathspec restriction that keeps `.DS_Store` out:

```rust
index.add_all(FILES_THIS_APP_OWNS.iter(), IndexAddOption::DEFAULT, None)?; // new + modified
index.update_all(FILES_THIS_APP_OWNS.iter(), None)?;                       // deletions of tracked paths
index.write()?;
```

Two calls instead of a loop with an `exists()` stat, a HEAD peel, a tree lookup,
and three branches. The pathspecs are literal filenames, so the "never
`add_all([\"*\"])`" invariant that the doc comments defend at length is preserved
exactly — the danger was always the `*`, never `add_all`.

**Recommendation:** Try it first. If `update_all`'s tracked-paths-only semantics
turn out not to cover the deletion case, keep the loop and record in the spec
*why* — one sentence. Separately: drop the `repo_path: &Path` parameter from
`stage_owned_files`. `repo.workdir()` supplies it, and two parameters that must
agree is an invariant nobody is enforcing.

### Concern 5: `write_atomic` re-implements `tempfile`, badly

**What I see:** *"Temp file in the same directory (`.<name>.tmp-<pid>-<nonce>`),
write, `File::sync_all()`, `fs::rename`, then fsync the containing directory. The
temp file is removed on every failure path."*

**Why it matters:** "Removed on every failure path" is a promise a human keeps by
hand at every `?`. That is precisely what `Drop` exists to make unnecessary.
`tempfile` is already in `egui-frontend`'s `[dev-dependencies]` — promote it and
the whole thing is:

```rust
pub fn write(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> Result<()> {
    let path = path.as_ref();
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let mut tmp = NamedTempFile::new_in(dir)?;   // collision-safe naming, cleanup on drop
    tmp.write_all(contents.as_ref())?;
    tmp.as_file().sync_all()?;
    tmp.persist(path)?;                          // the rename
    Ok(())
}
```

Collision-safe naming, cleanup-on-every-failure-path, and the rename, from a
crate that has had this correct for a decade.

Two API notes. **Signature:** mirror `std::fs::write` exactly —
`impl AsRef<Path>`, `impl AsRef<[u8]>`. Then all nine conversions are a
mechanical identifier swap and nothing else. **Name:** `atomic::write`, not
`atomic::write_atomic`; `atomic::write_atomic` stutters (Rust API Guidelines,
C-WORD-ORDER / naming conventions).

For `goal_repository.rs:128`, render to a `Vec<u8>` first
(`csv::Writer::from_writer(Vec::new())`, then `into_inner()`) and call the same
function. Do not invent a closure-taking `write_atomic_with` for one caller.

### Concern 6 (minor): the directory fsync promises macOS durability macOS won't give you

`File::sync_all` on macOS is `fsync(2)`, which does *not* guarantee the data
reached the physical device — that's `F_FULLFSYNC`. This is a macOS-only app
(`Library/Application Support`, `.DS_Store`, iCloud guards). The property this
spec actually needs is "a reader never sees a partial file," and `rename` alone
delivers that. The file `sync_all` before rename is worth keeping for the
zero-length-after-crash case. The directory fsync buys a guarantee the platform
doesn't honour. Drop it, or say in the spec that it's best-effort.

### Concern 7 (minor): dead code in the file you're already editing

`git/mod.rs:410-446` has six `*_sync` methods — `init_repo_sync`,
`ensure_repo_exists_sync`, `add_all_sync`, `commit_sync`,
`has_uncommitted_changes_sync`, `commit_file_change_sync` — each a one-line
forward to the non-`_sync` version. Zero callers in the tree. You're editing this
file anyway. Delete them.

## Questions the spec does not answer

- The atomic-write test asserts "a failed write leaves the prior file
  byte-identical." How is that failure induced deterministically? Name the
  mechanism (read-only parent directory? a path whose parent is a file?) or the
  test gets written as `#[ignore]` and never runs.
- `commit_file_change` gates on `has_uncommitted_changes` (any dirtiness at all)
  before calling `commit`, while staging is a narrow allowlist. Once `commit`'s
  error propagates, can that pairing produce a hard error where it previously
  produced a benign empty commit?
- `parental_control_attempts.csv` joins the owned list, but no merge rule covers
  it — a peer's appends are dropped on merge the same way `goals.csv`'s are,
  except `goals.csv` at least raises `GoalsDivergedNotice`. Pre-existing, but
  this change makes the guard commit it more often. Intentional?

## What I thought was well-handled

The root-cause framing is correct and earns the four changes: both defects really
are "inferred what a dirty tree means instead of establishing it." The dependency
ordering — atomic writes land before the reset is removed — is the right call and
is argued rather than asserted. The append-only exception for
`parental_control_attempts.csv` with the reasoning recorded in-spec ("so a later
reader does not 'fix' it into a rewrite") is exactly how to handle a deliberate
asymmetry. The no-silent-stall invariant test is the right test: it's a property,
not three examples. Retiring the marker is correctly deferred. And
`clear_interrupted_merge_marker(&Repository) -> Result<bool>` is fine as written —
a two-state answer with one caller does not need an enum; `HashSet::remove` and
this codebase's own `delete_transaction_no_commit` return `bool` for the same
reason.

## Closing

The design is right. Concern 1 is a must-fix — as written, the
`commit_file_change` change is a no-op and one of the two routes into defect 1
stays open. Concerns 2, 4 and 5 are shape changes that make the diff smaller than
the spec currently proposes, not larger. Fix 1 and 3, take 2/4/5, and ship it.
