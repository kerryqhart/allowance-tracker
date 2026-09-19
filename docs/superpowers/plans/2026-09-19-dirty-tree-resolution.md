# Dirty-Tree Resolution Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close two data-loss paths in child sync — a dirty tree that stalls a child permanently and silently, and a crash-marker hard reset that destroys uncommitted MCP rows.

**Architecture:** Make the owned-file writes atomic so "is this file complete?" is answerable; stage *tracked* paths rather than an allowlist so the stall class becomes unreachable; parse-validate before committing so removing the hard reset cannot propagate corruption; and collapse both dirty-tree paths onto one `Result`-returning resolver.

**Tech Stack:** Rust, `git2` (libgit2), `tempfile`, `thiserror`, `csv`, `egui`. macOS-only application.

**Spec:** `docs/superpowers/specs/2026-09-18-dirty-tree-resolution-design.md`

## Global Constraints

- **macOS only.** No `F_FULLFSYNC` (tens of ms on every transaction write, buys only last-write survival). No directory fsync (macOS does not honour the guarantee). `File::sync_all()` before rename is kept, for non-APFS volumes.
- **Never `add_all(["*"])`.** Literal-filename pathspecs or `update_all` only. The danger was always the `*`, never `add_all`.
- **No user-facing prose inside sync-engine errors.** Error variants carry structure (which file, which condition); the UI composes the sentence. `Display` exists for logs and the error chain.
- **`backend/` is part of the `egui-frontend` crate**, wired via `#[path = "../../backend/mod.rs"]` in `egui-frontend/src/lib.rs:16`. Paths like `crate::backend::storage::atomic` resolve from inside that crate.
- **`thiserror = "1.0"` is already a dependency** (`egui-frontend/Cargo.toml:30`). `tempfile = "3.0"` is currently dev-only (`:63`) and is promoted in Task 1.
- **TDD throughout.** Write the failing test, run it, watch it fail for the stated reason, then implement. A test that cannot fail on a broken implementation is not evidence.
- **Test command:** `cargo test --manifest-path /Users/kerryhart/Code/allowance-tracker/Cargo.toml --workspace`. Single test: add `-- --exact <path::to::test>`.
- **Commit at the end of every task.**

## File Structure

| File | Responsibility |
|---|---|
| `backend/storage/atomic.rs` | **New.** The single atomic write primitive. Nothing else. |
| `backend/storage/mod.rs` | Registers `atomic` alongside `traits`/`csv`/`git` (`:41-46`). |
| `backend/storage/csv/transaction_repository.rs` | Ledger writes become atomic; `let _` disposition. |
| `backend/storage/csv/goal_repository.rs` | `goals.csv` write becomes atomic (render to `Vec<u8>` first). |
| `backend/storage/csv/parental_control_repository.rs` | Readers become `flexible`; append stays an append. |
| `backend/storage/csv/{allowance,child,global_config}_repository.rs`, `child_registry.rs`, `migration.rs`, `backend/domain/sync_persistence.rs` | Hand-rolled temp+rename replaced by the shared helper. |
| `backend/storage/git/mod.rs` | `stage_owned_files` (narrow list, for `commit_merge`/migration only); `#[must_use]`; dead code removal. |
| `backend/sync/paths.rs` | `FILES_THIS_APP_OWNS` + the two-tier doc comment. |
| `backend/sync/child_sync.rs` | `merge_diverged` extraction; `clear_interrupted_merge_marker`. |
| `egui-frontend/src/ui/app_coordinator.rs` | `DirtyTreeError`, `resolve_dirty_tree`, `fail_sync`; both callers rewired. |
| `egui-frontend/src/ui/state/sync_state.rs` | Notice severity and blocking-first ordering. |
| `egui-frontend/src/ui/components/settings/lgs_sync_modal.rs` | Composes user-facing sentences; "Show the folder". |
| `egui-frontend/src/ui/test_support.rs` | **New,** `#[cfg(test)]`. `ChildRepoFixture` + the cycle driver + the invariant helper. Inside the crate, so crate-private sync internals stay private. |

---

### Task 1: The atomic write primitive

**Files:**
- Create: `backend/storage/atomic.rs`
- Modify: `backend/storage/mod.rs:41-46`, `egui-frontend/Cargo.toml:62-63`
- Test: inline `#[cfg(test)] mod tests` in `backend/storage/atomic.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: `pub fn crate::backend::storage::atomic::write(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> anyhow::Result<()>`. Signature deliberately mirrors `std::fs::write` so later adoptions are an identifier swap.

- [ ] **Step 1: Promote `tempfile` to a real dependency**

In `egui-frontend/Cargo.toml`, add to `[dependencies]` (keep the `[dev-dependencies]` entry — dev-deps and deps are separate lists and tests use it directly):

```toml
tempfile = "3.0"
```

- [ ] **Step 2: Register the module**

In `backend/storage/mod.rs`, after line 41's `pub mod traits;`:

```rust
pub mod atomic;
```

- [ ] **Step 3: Write the failing tests**

Create `backend/storage/atomic.rs` containing ONLY the test module for now:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_the_contents() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f.csv");
        write(&path, b"hello").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"hello");
    }

    #[test]
    fn replaces_existing_contents() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f.csv");
        std::fs::write(&path, b"old").unwrap();
        write(&path, b"new").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"new");
    }

    #[test]
    fn leaves_no_temp_residue_on_success() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path().join("f.csv"), b"x").unwrap();
        let entries: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(entries, vec!["f.csv".to_string()], "only the target file may remain");
    }

    /// The failure is induced deterministically by making the parent
    /// directory unwritable, which fails `NamedTempFile::new_in` for a
    /// non-root user on macOS while leaving the existing file readable.
    /// Named explicitly so this does not get written as `#[ignore]`.
    #[test]
    fn a_failed_write_leaves_the_prior_file_byte_identical() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f.csv");
        std::fs::write(&path, b"original").unwrap();

        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o555)).unwrap();
        let result = write(&path, b"replacement");
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755)).unwrap();

        assert!(result.is_err(), "a write into an unwritable directory must fail");
        assert_eq!(
            std::fs::read(&path).unwrap(),
            b"original",
            "the prior file must be byte-identical after a failed write"
        );
        let entries: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(entries, vec!["f.csv".to_string()], "no temp residue after failure");
    }

    /// `NamedTempFile` creates at 0600. Persisting without setting the mode
    /// would silently TIGHTEN an existing 0644 file, not merely fail to
    /// preserve it.
    #[test]
    fn preserves_the_existing_files_mode() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f.csv");
        std::fs::write(&path, b"old").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();

        write(&path, b"new").unwrap();

        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o644, "an existing file's mode must survive the replace");
    }

    #[test]
    fn a_new_file_defaults_to_0644() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("new.csv");
        write(&path, b"x").unwrap();

        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o644);
    }
}
```

- [ ] **Step 4: Run the tests to verify they fail**

Run: `cargo test --manifest-path /Users/kerryhart/Code/allowance-tracker/Cargo.toml -p allowance-tracker-egui storage::atomic`
Expected: compile error — `cannot find function 'write' in this scope`.

- [ ] **Step 5: Implement**

Prepend to `backend/storage/atomic.rs`, above the test module:

```rust
//! The single atomic-write primitive for files this app owns.
//!
//! # What this guarantees, precisely
//!
//! **Atomicity — no reader ever observes a partial file.** From `rename(2)`
//! alone: a reader gets the complete old file or the complete new one, never
//! a mixture. This survives process death, since the page cache outlives the
//! process. This is the property the dirty-tree guard depends on — it lets
//! "the file on disk is a complete, intentional state" be an assumption
//! rather than a hope.
//!
//! **Durability — deliberately NOT claimed beyond this:** a power loss or
//! kernel panic may cost the *most recent write*, never the file's
//! integrity. `File::sync_all()` is `fsync(2)`, which on macOS does not
//! flush the drive's own volatile cache; only `fcntl(F_FULLFSYNC)` does.
//! `F_FULLFSYNC` is deliberately not used: it costs tens of milliseconds on
//! every transaction write to buy survival of the last write, which is not
//! what any of this protects against. The `sync_all()` below is kept not for
//! APFS — whose copy-on-write checkpoints already order data before metadata
//! — but for the odd volume: an external disk, a network mount, HFS+.
//!
//! There is deliberately no directory fsync: it buys a guarantee macOS does
//! not honour.

use anyhow::{Context, Result};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use tempfile::NamedTempFile;

/// Default mode for a file this app creates. Matches what `std::fs::write`
/// produces under the usual umask, so adopting this function does not change
/// the permissions of files that did not exist before.
const DEFAULT_MODE: u32 = 0o644;

/// Write `contents` to `path` atomically.
///
/// Signature mirrors [`std::fs::write`] so every adoption is a mechanical
/// identifier swap.
///
/// Hand-rolling this was considered and rejected: "the temp file is removed
/// on every failure path" is a promise a human has to keep by hand at every
/// `?`, which is exactly what `Drop` exists to make unnecessary.
/// `NamedTempFile` also supplies collision-safe naming, which six
/// hand-rolled `path.with_extension("tmp")` copies in this codebase did not.
pub fn write(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> Result<()> {
    let path = path.as_ref();
    let dir = path.parent().unwrap_or_else(|| Path::new("."));

    let mut tmp = NamedTempFile::new_in(dir)
        .with_context(|| format!("creating a temp file alongside {}", path.display()))?;

    tmp.write_all(contents.as_ref())
        .with_context(|| format!("writing temp contents for {}", path.display()))?;

    // `NamedTempFile` creates at 0600. Persisting without this would TIGHTEN
    // an existing 0644 file rather than merely failing to preserve it.
    let mode = std::fs::metadata(path)
        .map(|m| m.permissions().mode() & 0o777)
        .unwrap_or(DEFAULT_MODE);
    tmp.as_file()
        .set_permissions(std::fs::Permissions::from_mode(mode))
        .with_context(|| format!("setting mode {mode:o} for {}", path.display()))?;

    tmp.as_file()
        .sync_all()
        .with_context(|| format!("flushing temp contents for {}", path.display()))?;

    tmp.persist(path)
        .map_err(|e| e.error)
        .with_context(|| format!("renaming temp file into place at {}", path.display()))?;

    Ok(())
}
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test --manifest-path /Users/kerryhart/Code/allowance-tracker/Cargo.toml -p allowance-tracker-egui storage::atomic`
Expected: PASS, 6 tests.

- [ ] **Step 7: Commit**

```bash
git -C /Users/kerryhart/Code/allowance-tracker add backend/storage/atomic.rs backend/storage/mod.rs egui-frontend/Cargo.toml
git -C /Users/kerryhart/Code/allowance-tracker commit -m "feat(storage): atomic::write on tempfile, with mode preservation"
```

---

### Task 2: Prove the atomicity, not just the happy path

Task 1's six tests all pass with `sync_all` deleted and would pass against a
plain `fs::write` for everything except the failure case. This task adds the
two tests that can actually fail on a broken implementation.

**Files:**
- Modify: `backend/storage/atomic.rs`
- Test: same file's `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: `atomic::write` from Task 1.
- Produces: `#[cfg(test)] atomic::write_with_recorder(path, contents, &mut Vec<Syscall>)` and `pub(crate) enum Syscall { Write, SyncAll, Rename }` — test-only, used by no production code.

- [ ] **Step 1: Write the failing concurrent-reader test with its negative control**

Add to the `tests` module in `backend/storage/atomic.rs`:

```rust
    /// Reads `path` in a tight loop for `rounds` iterations, returning every
    /// distinct byte-string it observed. Shared by the real test and its
    /// negative control so they cannot drift apart.
    fn observe_while<F>(path: &std::path::Path, writer: F) -> Vec<Vec<u8>>
    where
        F: FnOnce() + Send + 'static,
    {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        let done = Arc::new(AtomicBool::new(false));
        let done_writer = Arc::clone(&done);
        let handle = std::thread::spawn(move || {
            writer();
            done_writer.store(true, Ordering::SeqCst);
        });

        let mut seen: Vec<Vec<u8>> = Vec::new();
        while !done.load(Ordering::SeqCst) {
            if let Ok(bytes) = std::fs::read(path) {
                if !seen.contains(&bytes) {
                    seen.push(bytes);
                }
            }
        }
        handle.join().unwrap();
        seen
    }

    /// The versions the writer cycles through. Long enough that a
    /// non-atomic write is overwhelmingly likely to be caught mid-flight.
    fn versions() -> Vec<Vec<u8>> {
        (0..40u8)
            .map(|i| std::iter::repeat(b'a' + (i % 26)).take(200_000).collect())
            .collect()
    }

    #[test]
    fn a_reader_never_observes_a_partial_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("transactions.csv");
        let all = versions();
        std::fs::write(&path, &all[0]).unwrap();

        let write_path = path.clone();
        let to_write = all.clone();
        let seen = observe_while(&path, move || {
            for v in &to_write {
                write(&write_path, v).unwrap();
            }
        });

        for observed in &seen {
            assert!(
                all.contains(observed),
                "a reader observed {} bytes that match no complete version — \
                 atomic::write let a partial file become visible",
                observed.len()
            );
        }
    }

    /// The negative control. Without this, the test above is not evidence:
    /// it would pass against an implementation with no atomicity at all if
    /// the timing simply never caught a partial read. This asserts the
    /// harness CAN catch one.
    #[test]
    fn the_negative_control_shows_a_plain_write_is_observably_partial() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("transactions.csv");
        let all = versions();
        std::fs::write(&path, &all[0]).unwrap();

        let write_path = path.clone();
        let to_write = all.clone();
        let seen = observe_while(&path, move || {
            for v in &to_write {
                std::fs::write(&write_path, v).unwrap();
            }
        });

        assert!(
            seen.iter().any(|observed| !all.contains(observed)),
            "the harness never caught a partial read even against plain fs::write — \
             it therefore proves nothing about atomic::write; increase the payload \
             size or the round count rather than deleting this test"
        );
    }
```

- [ ] **Step 2: Write the failing syscall-order test**

For a durability primitive the call sequence *is* the contract; there is no
other observable behaviour. Add to the same module:

```rust
    #[test]
    fn performs_write_then_sync_then_rename_in_that_order() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f.csv");
        let mut recorded = Vec::new();

        write_with_recorder(&path, b"x", &mut recorded).unwrap();

        assert_eq!(
            recorded,
            vec![Syscall::Write, Syscall::SyncAll, Syscall::Rename],
            "the contents must be flushed before the rename makes them visible, \
             and there must be no directory fsync (see the module docs)"
        );
    }
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test --manifest-path /Users/kerryhart/Code/allowance-tracker/Cargo.toml -p allowance-tracker-egui storage::atomic`
Expected: compile error — `cannot find function 'write_with_recorder'`; the two reader tests compile and pass (the control proves the harness works).

- [ ] **Step 4: Refactor `write` to record its sequence under test**

Replace the body of `write` in `backend/storage/atomic.rs` with a thin
wrapper over a recording implementation:

```rust
/// The syscalls `write` performs, in order. Test-only: the sequence is the
/// contract for a durability primitive, and asserting it is the only way to
/// stop a future edit silently dropping the flush.
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Syscall {
    Write,
    SyncAll,
    Rename,
}

pub fn write(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> Result<()> {
    #[cfg(test)]
    {
        let mut sink = Vec::new();
        return write_inner(path.as_ref(), contents.as_ref(), Some(&mut sink));
    }
    #[cfg(not(test))]
    {
        write_inner(path.as_ref(), contents.as_ref())
    }
}

#[cfg(test)]
pub(crate) fn write_with_recorder(
    path: impl AsRef<Path>,
    contents: impl AsRef<[u8]>,
    recorder: &mut Vec<Syscall>,
) -> Result<()> {
    write_inner(path.as_ref(), contents.as_ref(), Some(recorder))
}
```

And give `write_inner` the body Task 1 wrote, with a recording call after
each of the three steps. Under `#[cfg(not(test))]` the parameter does not
exist, so there is zero production cost:

```rust
fn write_inner(
    path: &Path,
    contents: &[u8],
    #[cfg(test)] recorder: Option<&mut Vec<Syscall>>,
) -> Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    #[cfg(test)]
    let mut recorder = recorder;

    let mut tmp = NamedTempFile::new_in(dir)
        .with_context(|| format!("creating a temp file alongside {}", path.display()))?;

    tmp.write_all(contents)
        .with_context(|| format!("writing temp contents for {}", path.display()))?;
    #[cfg(test)]
    if let Some(r) = recorder.as_deref_mut() { r.push(Syscall::Write); }

    let mode = std::fs::metadata(path)
        .map(|m| m.permissions().mode() & 0o777)
        .unwrap_or(DEFAULT_MODE);
    tmp.as_file()
        .set_permissions(std::fs::Permissions::from_mode(mode))
        .with_context(|| format!("setting mode {mode:o} for {}", path.display()))?;

    tmp.as_file()
        .sync_all()
        .with_context(|| format!("flushing temp contents for {}", path.display()))?;
    #[cfg(test)]
    if let Some(r) = recorder.as_deref_mut() { r.push(Syscall::SyncAll); }

    tmp.persist(path)
        .map_err(|e| e.error)
        .with_context(|| format!("renaming temp file into place at {}", path.display()))?;
    #[cfg(test)]
    if let Some(r) = recorder.as_deref_mut() { r.push(Syscall::Rename); }

    Ok(())
}
```

If the `#[cfg(test)]`-on-a-parameter form fights the compiler, use two
`write_inner` definitions behind `#[cfg(test)]` / `#[cfg(not(test))]` rather
than threading an always-present `Option` into production code.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --manifest-path /Users/kerryhart/Code/allowance-tracker/Cargo.toml -p allowance-tracker-egui storage::atomic`
Expected: PASS, 9 tests.

- [ ] **Step 6: Record what CI cannot prove**

Append to `docs/lgs-sync-acceptance-checklist.md`, in the manual-verification
list:

```markdown
### Power-loss durability of `atomic::write`

**Not verifiable in CI, and deliberately not claimed.** `atomic::write`
guarantees that no reader observes a partial file (from `rename(2)`). It does
NOT guarantee the most recent write survives a power cut: `fsync(2)` on macOS
does not flush the drive's volatile cache, and `F_FULLFSYNC` was rejected as
too expensive per write. Killing the process with SIGKILL proves nothing
either — the page cache outlives the process. Accepted unverified.
```

- [ ] **Step 7: Commit**

```bash
git -C /Users/kerryhart/Code/allowance-tracker add backend/storage/atomic.rs docs/lgs-sync-acceptance-checklist.md
git -C /Users/kerryhart/Code/allowance-tracker commit -m "test(storage): prove atomic::write's atomicity with a negative control and a syscall-order assertion"
```

---

### Task 3: Adopt `atomic::write` at the three unprotected sites

**Files:**
- Modify: `backend/storage/csv/transaction_repository.rs:179-192`
- Modify: `backend/storage/csv/goal_repository.rs:122-137`
- Modify: `egui-frontend/src/ui/app_coordinator.rs:1168`
- Test: `backend/storage/csv/goal_repository.rs` inline tests

**Interfaces:**
- Consumes: `atomic::write` (Task 1).
- Produces: no signature changes. `write_goals_internal` keeps returning `Result<PathBuf>`.

- [ ] **Step 1: Write the failing test for the goals writer**

The transactions writer is already covered by the existing suite; `goals.csv`
changes shape (streaming writer → render-then-write) and needs its own proof
that the bytes are unchanged. Add to `goal_repository.rs`'s test module:

```rust
    #[test]
    fn write_goals_internal_produces_the_same_bytes_as_a_streaming_writer() {
        let (repo, child_id, _temp) = repo_with_child();
        let goals = vec![sample_goal(&child_id, "g-1"), sample_goal(&child_id, "g-2")];

        let path = repo.write_goals_internal(&child_id, &goals).unwrap();
        let written = std::fs::read_to_string(&path).unwrap();

        let mut expected = csv::Writer::from_writer(Vec::new());
        for goal in &goals {
            expected.serialize(GoalRecord::from(goal.clone())).unwrap();
        }
        expected.flush().unwrap();
        let expected = String::from_utf8(expected.into_inner().unwrap()).unwrap();

        assert_eq!(written, expected, "switching to a buffered render must not change a byte");
    }
```

If `repo_with_child` and `sample_goal` do not already exist in that module,
build them from the same `Backend::with_data_dir` + `create_child` shape used
by `app_with_git_backed_child` (`app_coordinator.rs:2233`), and read back via
`read_goals`.

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test --manifest-path /Users/kerryhart/Code/allowance-tracker/Cargo.toml -p allowance-tracker-egui goal_repository`
Expected: FAIL — the helper or the byte comparison does not exist yet.

- [ ] **Step 3: Convert the transactions writer**

In `backend/storage/csv/transaction_repository.rs`, replace `std::fs::write` at `:188`:

```rust
        crate::backend::storage::atomic::write(&file_path, text)
            .with_context(|| format!("writing {}", file_path.display()))?;
```

- [ ] **Step 4: Convert the goals writer**

In `backend/storage/csv/goal_repository.rs`, replace the body of
`write_goals_internal` (`:128-135`). Render to memory first, then one atomic
write — no closure-taking variant for one caller:

```rust
        let mut wtr = csv::Writer::from_writer(Vec::new());
        for goal in goals {
            let record = GoalRecord::from(goal.clone());
            wtr.serialize(record)?;
        }
        let bytes = wtr.into_inner()?;
        crate::backend::storage::atomic::write(&file_path, &bytes)?;
```

Remove the now-unused `BufWriter` import if nothing else in the file uses it.

- [ ] **Step 5: Convert the merged-CSV write**

In `egui-frontend/src/ui/app_coordinator.rs`, replace `:1168`:

```rust
        if let Err(e) = crate::backend::storage::atomic::write(child_dir.join("transactions.csv"), csv) {
```

- [ ] **Step 6: Run the full suite**

Run: `cargo test --manifest-path /Users/kerryhart/Code/allowance-tracker/Cargo.toml --workspace`
Expected: PASS, no regressions (597 baseline plus this task's new test).

- [ ] **Step 7: Commit**

```bash
git -C /Users/kerryhart/Code/allowance-tracker add backend/storage/csv/transaction_repository.rs backend/storage/csv/goal_repository.rs egui-frontend/src/ui/app_coordinator.rs
git -C /Users/kerryhart/Code/allowance-tracker commit -m "fix(storage): atomic writes for transactions.csv and goals.csv"
```

---

### Task 4: Consolidate the six hand-rolled temp+rename writers

Each is a bare `write` + `rename` with hand-maintained cleanup and a
colliding `.tmp` name. This is consolidation, not new protection.

**Files:**
- Modify: `backend/storage/csv/allowance_repository.rs:98-100`
- Modify: `backend/storage/csv/child_repository.rs:143-145`
- Modify: `backend/storage/csv/global_config_repository.rs:144-146`
- Modify: `backend/storage/csv/child_registry.rs:85-87`
- Modify: `backend/storage/csv/migration.rs:432-434`
- Modify: `backend/domain/sync_persistence.rs:60-62` and `:95-97`

**Interfaces:**
- Consumes: `atomic::write` (Task 1).
- Produces: no signature changes.

- [ ] **Step 1: Replace each site**

At every site the shape is the same — find the temp path, the `fs::write`,
and the `fs::rename`, and replace all three lines with one call. For example,
in `allowance_repository.rs`:

```rust
        // Was: write to `yaml_path.with_extension("tmp")`, then rename.
        crate::backend::storage::atomic::write(&yaml_path, yaml_content)?;
```

Delete the now-unused `temp_path` / `temp` local at each site. Do not change
any surrounding error handling or logging.

- [ ] **Step 2: Confirm no hand-rolled copies remain**

Run: `grep -rn "with_extension(\"tmp\")" /Users/kerryhart/Code/allowance-tracker/backend`
Expected: no matches.

- [ ] **Step 3: Run the full suite**

Run: `cargo test --manifest-path /Users/kerryhart/Code/allowance-tracker/Cargo.toml --workspace`
Expected: PASS. `sync_persistence.rs`'s existing torn-read tests (`:195`, `:239`) must still pass — they are the regression proof for this task.

- [ ] **Step 4: Commit**

```bash
git -C /Users/kerryhart/Code/allowance-tracker add backend/storage/csv backend/domain/sync_persistence.rs
git -C /Users/kerryhart/Code/allowance-tracker commit -m "refactor(storage): one atomic write helper instead of six hand-rolled copies"
```

---

### Task 5: Make the `parental_control_attempts.csv` claim true

The append stays — an interrupted append damages the trailing line, while an
interrupted rewrite can lose the whole audit log. But the spec's original
claim that the reader tolerates a malformed trailing record was **false**:
both readers use a default non-`flexible` `csv::Reader` with `result?`, so one
torn line makes the log both unreadable and (via `get_next_id`) unwritable.

**Files:**
- Modify: `backend/storage/csv/parental_control_repository.rs:107-121` (`get_next_id`), `:173-200` (`load_parental_control_attempts_from_directory`)
- Test: same file's `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: nothing.
- Produces: no signature changes; both readers become tolerant of malformed records.

- [ ] **Step 1: Write the failing test**

```rust
    /// An interrupted append leaves a partial trailing line. That must cost
    /// the trailing record and nothing else — not the whole log, and not the
    /// ability to append ever again.
    #[test]
    fn a_truncated_trailing_line_costs_only_that_record() {
        let (repo, child_id, _temp) = repo_with_child();
        repo.record_parental_control_attempt(&child_id, "1234", false).unwrap();
        repo.record_parental_control_attempt(&child_id, "5678", false).unwrap();

        let dir = repo.attempts_dir(&child_id).unwrap();
        let path = dir.join("parental_control_attempts.csv");

        // Simulate the interrupted append: a trailing line with too few fields.
        let mut text = std::fs::read_to_string(&path).unwrap();
        text.push_str("99,partial");
        std::fs::write(&path, text).unwrap();

        let attempts = repo.get_parental_control_attempts(&child_id, None).unwrap();
        assert_eq!(attempts.len(), 2, "every prior record must still be readable");

        // And the log must still be appendable — `get_next_id` is the path
        // that would otherwise be permanently blocked.
        repo.record_parental_control_attempt(&child_id, "0000", true).unwrap();
        let after = repo.get_parental_control_attempts(&child_id, None).unwrap();
        assert_eq!(after.len(), 3, "a torn line must not block future appends");
    }
```

Match `record_parental_control_attempt`'s real signature and `attempts_dir`'s
visibility when writing this — read `:236-260` first. If `attempts_dir` is
private, resolve the path through `connection.child_dir` as
`resolution_tests.rs:267` does.

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test --manifest-path /Users/kerryhart/Code/allowance-tracker/Cargo.toml -p allowance-tracker-egui parental_control`
Expected: FAIL — the read returns `Err(UnequalLengths)` and the `unwrap` panics.

- [ ] **Step 3: Make both readers tolerant**

In `get_next_id` (`:107`), build the reader with `flexible(true)` and skip
records that fail to parse rather than propagating:

```rust
        let mut csv_reader = csv::ReaderBuilder::new().flexible(true).from_reader(reader);

        let mut max_id = 0i64;
        for result in csv_reader.records() {
            // A torn trailing line from an interrupted append must cost that
            // record and nothing else. Propagating here would make the log
            // permanently unwritable, since every future append calls this.
            let Ok(record) = result else { continue };
            if !record.is_empty() {
                if let Ok(id) = record[0].parse::<i64>() {
                    if id > max_id {
                        max_id = id;
                    }
                }
            }
        }
```

Apply the same `ReaderBuilder::new().flexible(true)` and `let Ok(record) =
result else { continue }` in
`load_parental_control_attempts_from_directory` (`:173`). Where that loop
also deserializes into `ParentalControlAttemptRecord`, skip a record whose
field count is short rather than indexing past the end.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --manifest-path /Users/kerryhart/Code/allowance-tracker/Cargo.toml -p allowance-tracker-egui parental_control`
Expected: PASS, including the four pre-existing tests at `:301`, `:333`, `:351`, `:395`.

- [ ] **Step 5: Commit**

```bash
git -C /Users/kerryhart/Code/allowance-tracker add backend/storage/csv/parental_control_repository.rs
git -C /Users/kerryhart/Code/allowance-tracker commit -m "fix(storage): a torn append costs one record, not the whole attempts log"
```

---

### Task 6: `git/mod.rs` housekeeping

**Files:**
- Modify: `backend/storage/git/mod.rs:50-60` (`stage_owned_files`), `:311-315`, `:367-376`, `:351` (`commit_file_change`), `:410-446` (dead code)
- Modify: the five `let _ =` call sites

**Interfaces:**
- Consumes: `FILES_THIS_APP_OWNS`.
- Produces: `fn stage_owned_files(repo: &Repository) -> Result<()>` — one parameter, not two. `commit_file_change` gains `#[must_use]`.

- [ ] **Step 1: Drop the redundant parameter**

`stage_owned_files(repo, repo_path)` takes two arguments that must agree,
with nothing enforcing it. `repo.workdir()` supplies the second:

```rust
fn stage_owned_files(repo: &Repository) -> Result<()> {
    let repo_path = repo
        .workdir()
        .ok_or_else(|| anyhow::anyhow!("staging owned files requires a working directory"))?;
    let mut index = repo.index()?;
    for name in FILES_THIS_APP_OWNS {
        if !repo_path.join(name).exists() {
            continue;
        }
        index.add_path(Path::new(name))?;
    }
    index.write()?;
    Ok(())
}
```

Update both callers (`:315`, `:376`) to `stage_owned_files(&repo)?`.

Note this function keeps the **narrow list** and keeps skipping missing
files. It now serves only `commit_merge` and migration, where the narrow list
is correct. The dirty-tree guard gets different staging in Task 11.

- [ ] **Step 2: Mark the commit result must-use**

Above `pub fn commit_file_change` (`:351`):

```rust
    /// Returns `Ok(())` even when the commit itself failed — the failure is
    /// logged, not propagated, because a failed *commit* must not fail the
    /// user's *write*: the data is already on disk.
    ///
    /// `#[must_use]` so that discarding this is a deliberate act. The
    /// dirty-tree guard (`resolve_dirty_tree`) is what actually recovers a
    /// file left uncommitted by a failure here — it stages every tracked
    /// path on the next sync cycle, which is why propagating this error was
    /// considered and rejected as redundant.
    #[must_use = "a commit failure leaves the file uncommitted until the next sync cycle's guard picks it up"]
```

- [ ] **Step 3: Replace the five `let _ =` with explicit dispositions**

At `transaction_repository.rs:201`, `allowance_repository.rs:106`,
`goal_repository.rs:100`, `child_repository.rs:107`,
`parental_control_repository.rs:163` — replace each `let _ = self.git_manager
.commit_file_change(...)` with:

```rust
        if let Err(e) = self.git_manager.commit_file_change(&child_dir, "transactions.csv", &action_description) {
            // Deliberately non-fatal: the data is already on disk, and the
            // sync guard commits any tracked file left dirty on its next
            // cycle. Logged rather than discarded so this is visible.
            warn!("git commit for transactions.csv did not complete: {e}");
        }
```

Adjust the filename, path variable and log text per site. Ensure `warn!` (or
`log::warn!`) is in scope at each.

- [ ] **Step 4: Delete the dead forwarders**

Remove the six zero-caller methods at `git/mod.rs:410-446`:
`init_repo_sync`, `ensure_repo_exists_sync`, `add_all_sync`, `commit_sync`,
`has_uncommitted_changes_sync`, `commit_file_change_sync`.

Confirm first: `grep -rn "_sync(" /Users/kerryhart/Code/allowance-tracker/backend /Users/kerryhart/Code/allowance-tracker/egui-frontend/src` must show no calls to these six names.

- [ ] **Step 5: Run the full suite**

Run: `cargo test --manifest-path /Users/kerryhart/Code/allowance-tracker/Cargo.toml --workspace`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git -C /Users/kerryhart/Code/allowance-tracker add backend/storage
git -C /Users/kerryhart/Code/allowance-tracker commit -m "refactor(git): drop redundant param, must_use on commit_file_change, delete dead forwarders"
```

---

### Task 7: The two-tier ownership model, and a test that enforces it

**Files:**
- Modify: `backend/sync/paths.rs:5-28`
- Test: new `#[cfg(test)]` test in `backend/sync/paths.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: `FILES_THIS_APP_OWNS` unchanged in membership; its doc comment now states the two-tier rule.

- [ ] **Step 1: Write the failing contract test**

This is the test that closes the *class* rather than the instance:
`.allowance_redirect` already lives in a child directory, and a future
`budgets.csv` would walk into the same stall.

Add to `backend/sync/paths.rs`:

```rust
#[cfg(test)]
mod ownership_contract {
    use super::FILES_THIS_APP_OWNS;

    /// Files that legitimately live in a child directory and are NOT staged
    /// by `commit_merge`. Every entry needs a reason.
    const EXEMPT: &[&str] = &[
        // Committed directly by ParentalControlRepository, and reached by the
        // dirty-tree guard as a tracked path. Deliberately not swept into a
        // merge commit as a side effect.
        "parental_control_attempts.csv",
        // A pointer to a relocated child folder; local to this machine and
        // never synced. See migration.rs:250.
        ".allowance_redirect",
    ];

    /// Exercise every repository write path against a fresh child directory,
    /// then assert every tracked file that appeared is either owned or
    /// explicitly exempt. Without this, the next file this app learns to
    /// write silently walks into the stall class defect 1 came from.
    #[test]
    fn every_file_this_app_writes_is_owned_or_explicitly_exempt() {
        let (backend, child_id, _temp) = backend_with_child_exercising_every_write_path();
        let child_dir = backend
            .csv_connection
            .child_dir(&shared::ChildId::from(child_id.as_str()))
            .unwrap();

        let repo = git2::Repository::open(&child_dir).unwrap();
        let head_tree = repo.head().unwrap().peel_to_commit().unwrap().tree().unwrap();

        let mut unaccounted = Vec::new();
        head_tree
            .walk(git2::TreeWalkMode::PreOrder, |_, entry| {
                if let Some(name) = entry.name() {
                    let known = FILES_THIS_APP_OWNS.contains(&name) || EXEMPT.contains(&name);
                    if !known && entry.kind() == Some(git2::ObjectType::Blob) {
                        unaccounted.push(name.to_string());
                    }
                }
                git2::TreeWalkResult::Ok
            })
            .unwrap();

        assert!(
            unaccounted.is_empty(),
            "these tracked files are neither in FILES_THIS_APP_OWNS nor EXEMPT: {unaccounted:?}. \
             Add each to the owned list (if a merge commit should carry it) or to EXEMPT with a \
             reason — leaving it unaccounted for is how defect 1 happened."
        );
    }
}
```

Write `backend_with_child_exercising_every_write_path` in the same module: build a
`Backend::with_data_dir` as `app_with_git_backed_child` does
(`app_coordinator.rs:2233`), then call, in order — `create_child`,
`set_active_child`, `create_transaction`, a goal creation through
`goal_service`, an allowance-config update through `allowance_service`, and
`record_parental_control_attempt`. Each must be a real service call, not a
file write, so the test tracks what the *app* does rather than what the test
does.

- [ ] **Step 2: Run it to verify it fails or passes for the right reason**

Run: `cargo test --manifest-path /Users/kerryhart/Code/allowance-tracker/Cargo.toml -p allowance-tracker-egui ownership_contract`
Expected: PASS once the helper compiles — and if it FAILS, the failure names a real unaccounted file, which is a genuine finding to resolve before continuing rather than a test to weaken.

- [ ] **Step 3: Rewrite the doc comment for the two-tier model**

Replace `backend/sync/paths.rs:5-26` (everything above the `const`):

```rust
/// Filenames this app owns inside a child's per-child git repo.
///
/// # Two tiers, deliberately
///
/// This narrow list governs what **`commit_merge` and migration** stage. It
/// exists to stop `add_all(["*"])` sweeping untracked strays — `.DS_Store`,
/// editor swap files, anything macOS or an editor drops in a child's data
/// directory — into a commit that gets pushed into that child's synced
/// history permanently. The danger was always the `*`, never `add_all`.
///
/// The **dirty-tree guard is deliberately NOT governed by this list.** It
/// stages every *tracked* path via `index.update_all`, because a file
/// already tracked in HEAD is already in the pushed history — committing its
/// modification is not the hazard this list guards against, and refusing to
/// commit it is what stalled a child's sync permanently and silently
/// (see `2026-09-18-dirty-tree-resolution-design.md`, defect 1).
///
/// `parental_control_attempts.csv` is owned by this system and lives in the
/// same directory, but is deliberately NOT in this list: it is committed
/// directly by `ParentalControlRepository`, and the dirty-tree guard reaches
/// it as a tracked path, so it does not need a merge commit sweeping it in
/// as a side effect.
///
/// This project has already been misled by duplicate definitions of lists
/// like this drifting apart; keep it to one. `ownership_contract`'s test
/// below enforces that every file this app writes is either in this list or
/// explicitly exempt.
```

- [ ] **Step 4: Run the full suite**

Run: `cargo test --manifest-path /Users/kerryhart/Code/allowance-tracker/Cargo.toml --workspace`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git -C /Users/kerryhart/Code/allowance-tracker add backend/sync/paths.rs
git -C /Users/kerryhart/Code/allowance-tracker commit -m "feat(sync): two-tier ownership model, enforced by a completeness contract test"
```

---

### Task 8: Extract `merge_diverged` so the cycle is drivable in tests

`Cycle::Diverged`'s merge computation is inline in `cycle_against`
(`child_sync.rs:452-484`), which needs a live remote. The test driver in Task
10 must run the *real* decision code, so this is a prerequisite, not a tidy-up.

**Files:**
- Modify: `backend/sync/child_sync.rs:452-484`
- Test: `backend/sync/child_sync.rs` inline tests

**Interfaces:**
- Consumes: `read_rows`, `provenance`, `goals_diverged` (all already in this module).
- Produces: `pub(crate) fn merge_diverged(repo: &Repository, ours_oid: Oid, auth_oid: Oid, base_oid: Option<Oid>) -> Result<CycleOutcome>` — always returns `CycleOutcome::Merged`.

- [ ] **Step 1: Write the failing test**

```rust
    /// The merge computation must be reachable without a remote, so tests
    /// can drive the real classify→merge→apply loop rather than a
    /// reimplementation of it.
    #[test]
    fn merge_diverged_computes_the_same_result_without_a_remote() {
        let dir = tempfile::tempdir().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let work_path = dir.path().to_path_buf();

        std::fs::write(work_path.join(TRANSACTIONS_FILE), TX_A).unwrap();
        let base = commit_all(&repo, "base", &[]);
        let base_commit = repo.find_commit(base).unwrap();

        std::fs::write(work_path.join(TRANSACTIONS_FILE), TX_OURS).unwrap();
        let ours = commit_all(&repo, "ours", &[&base_commit]);
        let theirs = commit_with_files(
            &repo,
            "theirs",
            &[&base_commit],
            &[(TRANSACTIONS_FILE, TX_A)],
            1_700_000_500,
        );

        let outcome = merge_diverged(&repo, ours, theirs, Some(base)).unwrap();
        match outcome {
            CycleOutcome::Merged { parents, .. } => {
                assert_eq!(parents, (ours.to_string(), theirs.to_string()));
            }
            other => panic!("expected Merged, got {other:?}"),
        }
    }
```

Reuse whatever `commit_all` / `commit_with_files` helpers already exist in
this module's test section (there is a `commit_with_files` at
`child_sync.rs:1130`); do not add a third copy.

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test --manifest-path /Users/kerryhart/Code/allowance-tracker/Cargo.toml -p allowance-tracker-egui child_sync::tests::merge_diverged`
Expected: FAIL — `cannot find function 'merge_diverged'`.

- [ ] **Step 3: Extract the function**

Add to `backend/sync/child_sync.rs`, near `read_rows`:

```rust
/// Compute the merge for a diverged cycle. Extracted from `cycle_against`'s
/// `Cycle::Diverged` arm so the same code can be driven in tests without a
/// remote or a daemon — the sync loop's decisions must be testable by
/// construction, not only through a live fetch.
///
/// Reads all three sides as blobs and resolves provenance here, so the merge
/// itself never walks history.
pub(crate) fn merge_diverged(
    repo: &Repository,
    ours_oid: Oid,
    auth_oid: Oid,
    base_oid: Option<Oid>,
) -> Result<CycleOutcome> {
    let base = base_oid.map(|o| read_rows(repo, o)).transpose()?;
    let ours = Sided {
        rows: read_rows(repo, ours_oid)?,
        provenance: provenance(repo, ours_oid)?,
    };
    let theirs = Sided {
        rows: read_rows(repo, auth_oid)?,
        provenance: provenance(repo, auth_oid)?,
    };

    let diverged = goals_diverged(repo, ours_oid, auth_oid)?;
    if diverged {
        log::warn!(
            "goals.csv diverged between the local tip ({ours_oid}) and the authoritative peer \
             tip ({auth_oid}); allowance_core::merge does not model goals, so this cycle's \
             transactions merge proceeds but goals.csv is left exactly as it is locally. Any \
             goal edits made on the other machine are NOT reflected here and must be reconciled \
             by hand."
        );
    }

    let outcome = merge(base.as_deref(), &ours, &theirs);
    Ok(CycleOutcome::Merged {
        rows: outcome.rows,
        parents: (ours_oid.to_string(), auth_oid.to_string()),
        decisions: outcome.decisions,
        goals_diverged: diverged,
    })
}
```

Replace the `Cycle::Diverged` arm in `cycle_against` with:

```rust
            Cycle::Diverged => {
                let auth_oid = auth_oid.expect("Cycle::Diverged implies classify saw Some(auth)");
                merge_diverged(&repo, ours_oid, auth_oid, base_oid)
            }
```

- [ ] **Step 4: Run the tests**

Run: `cargo test --manifest-path /Users/kerryhart/Code/allowance-tracker/Cargo.toml --workspace`
Expected: PASS — this is a pure extraction, so every existing `child_sync` test must still pass unchanged.

- [ ] **Step 5: Commit**

```bash
git -C /Users/kerryhart/Code/allowance-tracker add backend/sync/child_sync.rs
git -C /Users/kerryhart/Code/allowance-tracker commit -m "refactor(sync): extract merge_diverged so the cycle is drivable without a remote"
```

---

### Task 9: `recover_if_dirty` stops resetting

**Files:**
- Modify: `backend/sync/child_sync.rs:863-935`
- Modify: `egui-frontend/src/ui/app_coordinator.rs:958-993`, `:1452-1474`
- Test: invert `backend/sync/child_sync.rs:1453-1470`

**Interfaces:**
- Consumes: `MERGE_IN_PROGRESS_MARKER`, `clear_merge_marker`.
- Produces: `pub fn clear_interrupted_merge_marker(repo: &Repository) -> Result<bool>`. `Recovered` and `recover_if_dirty` are deleted.

- [ ] **Step 1: Invert the existing test**

Replace `a_dirty_tree_with_the_marker_present_is_discarded_and_the_marker_cleared`
(`child_sync.rs:1453`) with:

```rust
    /// The marker proves a merge BEGAN and did not finish. It does not prove
    /// the tree's current contents came from that merge — the AWS transport's
    /// non-committing path can write MCP-authored rows into the same file
    /// after the crash and before the next cycle. Discarding on the marker's
    /// say-so destroyed those rows, and the watermark had already advanced
    /// past them (defect 2).
    #[test]
    fn a_dirty_tree_with_the_marker_present_is_preserved_and_the_marker_cleared() {
        let dir = tempfile::tempdir().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        std::fs::write(dir.path().join(TRANSACTIONS_FILE), TX_A).unwrap();
        commit_all(&repo, "base", &[]);

        write_merge_marker(&repo, "ours-oid", "theirs-oid").unwrap();
        std::fs::write(repo.workdir().unwrap().join(TRANSACTIONS_FILE), "an MCP row written after the crash").unwrap();

        let had_marker = clear_interrupted_merge_marker(&repo).unwrap();

        assert!(had_marker, "the marker was present and must be reported");
        assert_eq!(
            std::fs::read_to_string(dir.path().join(TRANSACTIONS_FILE)).unwrap(),
            "an MCP row written after the crash",
            "the dirty content must be PRESERVED — the dirty-tree guard commits it, and \
             discarding it here is the data loss this change exists to remove"
        );
        assert!(
            !repo.path().join(MERGE_IN_PROGRESS_MARKER).exists(),
            "the stale marker must be cleared"
        );
    }

    #[test]
    fn no_marker_reports_false_and_touches_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        std::fs::write(dir.path().join(TRANSACTIONS_FILE), TX_A).unwrap();
        commit_all(&repo, "base", &[]);
        std::fs::write(dir.path().join(TRANSACTIONS_FILE), "uncommitted AWS row").unwrap();

        assert!(!clear_interrupted_merge_marker(&repo).unwrap());
        assert_eq!(
            std::fs::read_to_string(dir.path().join(TRANSACTIONS_FILE)).unwrap(),
            "uncommitted AWS row"
        );
    }
```

Also delete `a_dirty_tree_with_no_marker_is_left_untouched` (`:1434`) if the
second test above now covers it, and the `Recovered::Clean` assertions at
`:1425`, `:1481`, `:1495`.

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test --manifest-path /Users/kerryhart/Code/allowance-tracker/Cargo.toml -p allowance-tracker-egui child_sync`
Expected: FAIL — `cannot find function 'clear_interrupted_merge_marker'`.

- [ ] **Step 3: Replace the function**

Delete `pub enum Recovered` (`:863-882`) and `pub fn recover_if_dirty`
(`:884-935`). In their place:

```rust
/// Clear a stale interrupted-merge marker, reporting whether one was there.
///
/// # This used to hard-reset the working tree. It must never do that again.
///
/// [`MERGE_IN_PROGRESS_MARKER`] proves a merge **began and did not finish**.
/// It does NOT prove the working tree's current contents came from that
/// merge. After a crash the app restarts and the AWS transport's
/// non-committing path (`upsert_transaction_no_commit`) can write fresh
/// MCP-authored rows into the same `transactions.csv` before the next
/// `apply_merge` runs. Resetting on the marker's say-so destroyed those
/// rows — and the AWS watermark had already advanced past them, so they
/// could never be re-fetched.
///
/// The marker is now a diagnostic breadcrumb, not an authorization to
/// discard. Content handling belongs to the dirty-tree guard
/// (`resolve_dirty_tree` in `app_coordinator.rs`), which commits what is on
/// disk after parse-validating it — the same path an ordinary MCP write
/// takes, which is what stops an interrupted merge from being a special
/// case.
///
/// # Convergence after an interrupted merge
///
/// The guard commits the partially merged content as an ordinary
/// *single-parent* commit, so `theirs` never becomes an ancestor through
/// that commit. This still converges: `theirs` is in the object database,
/// the next cycle re-classifies against it and recomputes the merge, and
/// `allowance_core::merge` de-duplicates rows that are `intrinsic_eq` on
/// both sides, so rows already committed are not doubled. The same row set
/// is reached by a different commit topology.
pub fn clear_interrupted_merge_marker(repo: &Repository) -> Result<bool> {
    if !merge_marker_present(repo) {
        return Ok(false);
    }
    clear_merge_marker(repo)?;
    Ok(true)
}
```

- [ ] **Step 4: Rewire both call sites**

In `app_coordinator.rs`, replace the `match recover_if_dirty(&repo)` block at
`:970` with:

```rust
        // A marker means a PREVIOUS merge for this child was interrupted. It
        // is a breadcrumb only — the dirty-tree guard below owns all content
        // handling, including for this case. See
        // `clear_interrupted_merge_marker`'s doc comment for why resetting
        // here destroyed MCP-authored rows.
        match clear_interrupted_merge_marker(&repo) {
            Ok(true) => log::warn!(
                "A previous merge for child {child_id} was interrupted; its marker has been \
                 cleared. Any uncommitted content is left exactly as it is — the dirty-tree \
                 guard below commits it."
            ),
            Ok(false) => {}
            Err(e) => log::warn!(
                "Could not clear child {child_id}'s interrupted-merge marker (continuing — the \
                 marker is diagnostic only): {e}"
            ),
        }
```

Apply the same replacement at `:1456` in `apply_fast_forward`, with "before
checking out {to}" wording. Note both become non-fatal: a marker-clear failure
no longer aborts the operation, because nothing depends on it.

Update the import at `app_coordinator.rs:34`: drop `recover_if_dirty` and
`Recovered`, add `clear_interrupted_merge_marker`.

- [ ] **Step 5: Run the suite**

Run: `cargo test --manifest-path /Users/kerryhart/Code/allowance-tracker/Cargo.toml --workspace`
Expected: `child_sync` passes. `app_coordinator.rs:2331`
(`a_dirty_tree_from_a_prior_crash_is_recovered_before_applying_the_next_merge`)
now FAILS — it asserts "the crash's garbage must be gone". That is the test
which asserted defect 2 as intended behaviour; Task 11 rewrites it. Leave it
failing and note it.

- [ ] **Step 6: Commit**

```bash
git -C /Users/kerryhart/Code/allowance-tracker add backend/sync/child_sync.rs egui-frontend/src/ui/app_coordinator.rs
git -C /Users/kerryhart/Code/allowance-tracker commit -m "fix(sync): the crash marker is a breadcrumb, not an authorization to discard

Known-failing until Task 11: a_dirty_tree_from_a_prior_crash_is_recovered_
before_applying_the_next_merge asserts the removed behaviour."
```

---

### Task 10: Test infrastructure — fixture, cycle driver, invariant helper

Both defects walked past a suite with 5,000-case property tests and eight git
integration tests. Defect 1 is a **liveness** bug and every merge-path test is
single-shot; the fast-forward path has the suite's only progress assertion,
and it is the path that got this right. This task builds the capability that
catches that class without knowing the defect exists.

**Files:**
- Create: `egui-frontend/src/ui/test_support.rs`
- Modify: `egui-frontend/src/ui/mod.rs`

**Placement — decided, not optional.** This lives *inside* the crate behind
`#[cfg(test)]`, not in `egui-frontend/tests/`. An integration test is a
separate crate and cannot see `apply_merge`, `apply_fast_forward` (private
methods), `working_tree_dirty` or `merge_diverged` (`pub(crate)`). Moving the
helper is correct; widening production visibility so a test can reach it is
not.

**Interfaces:**
- Consumes: `classify`, `Cycle`, `merge_diverged` (Task 8), `CycleOutcome`.
- Produces:
  - `ChildRepoFixture::new() -> Self`, `.with_peer_commit(&[(&str, &str)]) -> Self`, `.with_dirty(&str, &str) -> Self`, `.with_deleted(&str) -> Self`, `.with_marker() -> Self`, `.build() -> (AllowanceTrackerApp, String, TempDir, git2::Oid)`
  - `run_cycles_until_terminal(&mut AllowanceTrackerApp, &str, git2::Oid, u8) -> Result<Terminal, Vec<String>>`
  - `enum Terminal { Applied, UpToDate, FailedWithNotice }`
  - `assert_resolved_or_explained(&AllowanceTrackerApp, &git2::Repository, &str)`

- [ ] **Step 1: Write the fixture builder**

Create `egui-frontend/src/ui/test_support.rs`. This replaces three
copies of `app_with_git_backed_child` (`app_coordinator.rs:2233`, `:2846`,
`:3274`) and three of `commit_with_files` (`:2210`, `:2827`,
`child_sync.rs:1130`) — the cross-product table in Task 12 cannot be written
on top of a copy-pasted setup function.

```rust
//! One fixture for child-repo sync tests: a real child with a real git repo,
//! a planted "peer" commit, and whatever dirty/deleted/interrupted state the
//! scenario needs.

use allowance_tracker_egui::backend::Backend;
use allowance_tracker_egui::ui::AllowanceTrackerApp;
use git2::Repository;

pub struct ChildRepoFixture {
    peer_files: Option<Vec<(String, String)>>,
    dirty: Vec<(String, String)>,
    deleted: Vec<String>,
    marker: bool,
}

impl ChildRepoFixture {
    pub fn new() -> Self {
        Self { peer_files: None, dirty: Vec::new(), deleted: Vec::new(), marker: false }
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

        let child_dir = backend
            .csv_connection
            .child_dir(&shared::ChildId::from(child.id.as_str()))
            .expect("child dir");
        let repo = Repository::open(&child_dir).expect("open child repo");
        let head = repo.head().unwrap().peel_to_commit().unwrap();

        // The peer commit: written straight into the object database,
        // reachable from no ref — exactly what ChildSyncEngine::cycle would
        // have fetched into refs/remotes/lgs-auth/main, with no real remote.
        let peer_tip = self.peer_files.as_ref().map(|files| {
            let sig = git2::Signature::new(
                "Peer",
                "peer@example.com",
                &git2::Time::new(1_700_000_500, 0),
            )
            .unwrap();
            let mut builder = repo.treebuilder(None).unwrap();
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
            allowance_tracker_egui::backend::sync::child_sync::write_merge_marker(
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
```

Note `build()` reproduces `app_coordinator.rs:2233`'s construction sequence
exactly, and the peer-commit block reproduces `commit_with_files`
(`app_coordinator.rs:2210`). Do not substitute a different construction — the
point of the fixture is that every test starts from the same known state.
Once this compiles, delete the three copies of `app_with_git_backed_child`
and the three of `commit_with_files` and point their callers here.

- [ ] **Step 2: Write the cycle driver**

Append to the same file:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Terminal {
    Applied,
    UpToDate,
    FailedWithNotice,
}

/// Drive the real classify → merge → apply loop against a fixed peer tip
/// until it reaches a terminal state, or give up after `max` cycles.
///
/// # Why this exists
///
/// Defect 1 was a LIVENESS failure: a single `apply_merge` returning
/// `DirtyTreeCommitted` having committed nothing is not visibly wrong in
/// isolation — only on the second, third and `STALE_HEAD_REFUSAL_LIMIT`th
/// cycle. Every merge-path test in this suite was single-shot, which is
/// exactly why the defect shipped. The one progress assertion that existed
/// lived on the fast-forward path (`app_coordinator.rs:3178`), and that is
/// the path the design got right.
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
    use allowance_tracker_egui::backend::sync::child_sync::{
        classify, merge_diverged, Cycle, CycleOutcome,
    };

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
                let CycleOutcome::Merged { rows, parents, decisions, .. } = computed.unwrap() else {
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

    // Did the loop at least converge?
    let child_dir = app
        .backend()
        .csv_connection
        .child_dir(&shared::ChildId::from(child_id))
        .unwrap();
    let repo = Repository::open(&child_dir).unwrap();
    let head = repo.head().unwrap().peel_to_commit().unwrap().id();
    if classify(Some(&head.to_string()), Some(&peer_tip.to_string()), None) == Cycle::UpToDate {
        return Ok(Terminal::Applied);
    }

    Err(trace)
}
```

Because this module is inside the crate behind `#[cfg(test)]`, the private
`apply_merge` / `apply_fast_forward` methods and the `pub(crate)`
`merge_diverged` / `working_tree_dirty` are all reachable without changing a
single production visibility. Drop the `allowance_tracker_egui::` prefixes
used in the sketch above and import with `crate::` paths instead.

- [ ] **Step 3: Write the invariant helper**

```rust
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
    let clean = !allowance_tracker_egui::backend::sync::child_sync::working_tree_dirty(repo).unwrap();
    let advanced = head_now != head_before;
    let explained = app.sync.sync_failures.iter().any(|n| n.child_id == child_id);

    assert!(
        (clean && advanced) || explained,
        "neither resolved nor explained for child {child_id}: tree_clean={clean}, \
         head_advanced={advanced} ({head_before} -> {head_now}), notice_present={explained}. \
         This is the exact shape of defect 1 — a dirty tree that neither progresses nor reports."
    );
}
```

- [ ] **Step 4: Register the module**

In `egui-frontend/src/ui/mod.rs`:

```rust
#[cfg(test)]
pub mod test_support;
```

The `#[cfg(test)]` gate means none of this compiles into the shipped binary.

- [ ] **Step 5: Verify it compiles and the driver works against a passing case**

Write one smoke test using the fixture with a clean tree and a peer commit
that fast-forwards, asserting `run_cycles_until_terminal` returns
`Ok(Terminal::UpToDate)` or `Ok(Terminal::Applied)` within 3 cycles.

Run: `cargo test --manifest-path /Users/kerryhart/Code/allowance-tracker/Cargo.toml --workspace`
Expected: the smoke test passes; `app_coordinator.rs:2331` still fails from Task 9.

- [ ] **Step 6: Commit**

```bash
git -C /Users/kerryhart/Code/allowance-tracker add egui-frontend/src/ui/test_support.rs egui-frontend/src/ui/mod.rs
git -C /Users/kerryhart/Code/allowance-tracker commit -m "test(sync): fixture builder, cycle-progress driver, and the resolved-or-explained invariant"
```

---

### Task 11: One dirty-tree resolution, staging tracked paths, parse-validated

The core of both fixes.

**Files:**
- Modify: `egui-frontend/src/ui/app_coordinator.rs:1105-1125` (guard call site), `:1268-1372` (merge path), `:1650-1760` (fast-forward path)
- Test: rewrite `app_coordinator.rs:2331`

**Interfaces:**
- Consumes: `working_tree_dirty`, `allowance_core::codec::parse_transactions`.
- Produces:
  - `enum DirtyTreeError { Stage(git2::Error), Commit(anyhow::Error), Unparseable { file: &'static str }, NothingToCommit }`
  - `fn resolve_dirty_tree(repo: &git2::Repository, message: &str) -> Result<git2::Oid, DirtyTreeError>`
  - `fn fail_sync(&mut self, child_id: &str, err: &DirtyTreeError)`

- [ ] **Step 1: Rewrite the test that asserted defect 2**

Replace `a_dirty_tree_from_a_prior_crash_is_recovered_before_applying_the_next_merge`
(`app_coordinator.rs:2331`). Its old assertion — "the crash's garbage must be
gone" — *was* defect 2, asserted as intended behaviour with a doc comment
explaining why it was correct.

```rust
    /// Defect 2's regression test. The crash marker proves a merge BEGAN,
    /// not that the tree's contents came from it: after the crash, the AWS
    /// transport's non-committing path can write MCP-authored rows into the
    /// same file. The previous behaviour hard-reset them away, and the AWS
    /// watermark had already advanced past them — permanent, silent loss.
    ///
    /// This test previously asserted the opposite ("the crash's garbage must
    /// be gone"), which is why the defect shipped.
    #[test]
    fn an_mcp_row_written_after_a_crash_survives_the_next_merge() {
        let (mut app, child_id, _temp, peer) = ChildRepoFixture::new()
            .with_peer_commit(&[("transactions.csv", "id,child_id,date,description,amount,balance,type\n")])
            .with_marker()
            .build();
        let peer = peer.unwrap();

        let child_dir = app
            .backend()
            .csv_connection
            .child_dir(&shared::ChildId::from(child_id.as_str()))
            .unwrap();

        // A well-formed MCP row lands in the dirty tree after the crash.
        let mcp_csv = format!(
            "id,child_id,date,description,amount,balance,type\n\
             in-mcp-1,{child_id},2026-02-01T12:00:00+00:00,MCP gift,7.00,17.00,income\n"
        );
        std::fs::write(child_dir.join("transactions.csv"), &mcp_csv).unwrap();

        let terminal = run_cycles_until_terminal(&mut app, &child_id, peer, 5);
        assert!(terminal.is_ok(), "must reach a terminal state, got trace: {terminal:?}");

        let repo = Repository::open(&child_dir).unwrap();
        let head_csv = {
            let tree = repo.head().unwrap().peel_to_commit().unwrap().tree().unwrap();
            let entry = tree.get_path(std::path::Path::new("transactions.csv")).unwrap();
            let blob = repo.find_blob(entry.id()).unwrap();
            String::from_utf8(blob.content().to_vec()).unwrap()
        };
        assert!(
            head_csv.contains("MCP gift"),
            "the MCP row written after the crash must survive into committed history — \
             the AWS watermark has already advanced past it, so discarding it loses it for good"
        );
        assert!(
            !repo.path().join(crate::backend::sync::child_sync::MERGE_IN_PROGRESS_MARKER).exists(),
            "the stale marker must be cleared"
        );
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test --manifest-path /Users/kerryhart/Code/allowance-tracker/Cargo.toml -p allowance-tracker-egui an_mcp_row_written_after_a_crash`
Expected: FAIL — the guard still stages only the owned allowlist and there is no parse gate.

- [ ] **Step 3: Add the error type**

Near `ApplyMergeOutcome` in `app_coordinator.rs`:

```rust
/// Why a dirty working tree could not be resolved into a commit.
///
/// Variants carry STRUCTURE, not prose. The `Display` text below is for logs
/// and the error chain, where a developer is the reader; the user-facing
/// sentence is composed by the sync modal, which has the child's display name
/// and folder path. Prose produced down here is the mechanism by which
/// "stage", "dirty tree" and "oid" reached a parent's screen.
#[derive(Debug, thiserror::Error)]
pub enum DirtyTreeError {
    #[error("could not stage local changes")]
    Stage(#[source] git2::Error),
    #[error("could not commit local changes")]
    Commit(#[source] anyhow::Error),
    #[error("{file} could not be read as transaction data")]
    Unparseable { file: &'static str },
    /// Should never happen: `working_tree_dirty` ignores untracked files and
    /// `update_all` stages every tracked change, so any tree that reaches the
    /// guard has something to stage. Kept as an honest "something unexpected
    /// happened" rather than deleted.
    #[error("the working tree reported changes but nothing tracked was modified")]
    NothingToCommit,
}
```

- [ ] **Step 4: Implement the resolver**

```rust
/// Commit a dirty working tree so a merge or fast-forward can proceed —
/// the single implementation behind both paths.
///
/// # Why tracked paths, not the owned allowlist
///
/// `FILES_THIS_APP_OWNS` exists to stop `add_all(["*"])` sweeping untracked
/// strays into pushed history. A file already tracked in HEAD is already in
/// that history, so committing its modification is not that hazard — and
/// refusing to commit it is what stalled a child's sync permanently and
/// silently. `index.update_all` stages modifications and deletions of
/// tracked entries and never adds an untracked path, so the anti-`add_all`
/// invariant is preserved exactly. The danger was always the `*`.
///
/// # Why parse-validate first
///
/// Atomic writes bound THIS app's corruption after the upgrade. They say
/// nothing about a `transactions.csv` already torn on disk at upgrade time,
/// damaged by a partial restore, or written by an older build. With the hard
/// reset gone nothing else validates, so committing unparseable bytes would
/// push them to the peer and break `read_rows` on BOTH machines — turning a
/// one-machine corruption into a two-machine outage.
fn resolve_dirty_tree(
    repo: &git2::Repository,
    message: &str,
) -> Result<git2::Oid, DirtyTreeError> {
    let workdir = repo
        .workdir()
        .ok_or_else(|| DirtyTreeError::Commit(anyhow::anyhow!("repository has no working directory")))?
        .to_path_buf();

    let tx_path = workdir.join("transactions.csv");
    if tx_path.exists() {
        let text = std::fs::read_to_string(&tx_path)
            .map_err(|e| DirtyTreeError::Commit(anyhow::Error::from(e)))?;
        if allowance_core::codec::parse_transactions(&text).is_err() {
            return Err(DirtyTreeError::Unparseable { file: "transactions.csv" });
        }
    }

    let mut index = repo.index().map_err(DirtyTreeError::Stage)?;
    index
        .update_all(["*"].iter(), None)
        .map_err(DirtyTreeError::Stage)?;
    index.write().map_err(DirtyTreeError::Stage)?;

    let gm = GitManager::new();
    match gm.commit_if_changed(&workdir, message) {
        Ok(Some(oid_str)) => git2::Oid::from_str(&oid_str)
            .map_err(|e| DirtyTreeError::Commit(anyhow::Error::from(e))),
        Ok(None) => Err(DirtyTreeError::NothingToCommit),
        Err(e) => Err(DirtyTreeError::Commit(e)),
    }
}
```

- [ ] **Step 5: Add `fail_sync` and collapse the eight copies**

As a method on `AllowanceTrackerApp`:

```rust
    /// One place where a dirty-tree failure becomes user-visible state.
    /// Replaces eight copies of "format a message, set status, record a
    /// notice, return Failed" across the two dirty-tree functions.
    fn fail_sync(&mut self, child_id: &str, err: &DirtyTreeError) {
        log::error!("Sync failed for child {child_id}: {err:#}");
        self.sync.status = SyncStatus::Error(format!("Sync failed for {child_id}: {err}"));
        self.sync.record_sync_failure(SyncFailureNotice {
            child_id: child_id.to_string(),
            message: err.to_string(),
        });
    }
```

- [ ] **Step 6: Rewire both callers**

Replace the body of `commit_dirty_tree_before_merge` (`:1268`):

```rust
    fn commit_dirty_tree_before_merge(&mut self, child_id: &str, repo: &git2::Repository) -> ApplyMergeOutcome {
        let message = "sync: commit local changes before applying a peer merge";
        match resolve_dirty_tree(repo, message) {
            Ok(oid) => {
                log::warn!(
                    "Child {child_id}'s working tree was dirty when a merge was about to be \
                     applied (an MCP write landed since the tips this merge was computed \
                     against — this design's normal steady state, not a crash); committed it as \
                     {oid} and discarded the already-computed merge, which was based on \
                     now-stale tips. The next cycle recomputes from the new HEAD."
                );
                self.request_stale_head_repoll(child_id);
                ApplyMergeOutcome::DirtyTreeCommitted
            }
            Err(e) => {
                self.fail_sync(child_id, &e);
                ApplyMergeOutcome::Failed
            }
        }
    }
```

Extract the existing `note_stale_head_refusal` match block (`:1340-1368`) into
`fn request_stale_head_repoll(&mut self, child_id: &str)` so both the merge
path and any future caller share it verbatim.

Replace `commit_dirty_tree_to_unblock_fast_forward`'s body (`:1650`) the same
way, keeping its success branch's `record_fast_forward_blocked` and its own
`PollNow` send — that is the one place the two paths genuinely differ.

Delete the now-unused `FILES_THIS_APP_OWNS` import from `app_coordinator.rs`
if nothing else in the file uses it.

- [ ] **Step 7: Run the tests**

Run: `cargo test --manifest-path /Users/kerryhart/Code/allowance-tracker/Cargo.toml --workspace`
Expected: PASS, including Task 9's known-failing test now rewritten.

- [ ] **Step 8: Commit**

```bash
git -C /Users/kerryhart/Code/allowance-tracker add egui-frontend/src/ui/app_coordinator.rs
git -C /Users/kerryhart/Code/allowance-tracker commit -m "fix(sync): one dirty-tree resolver staging tracked paths, parse-validated

Closes the permanent silent stall: the guard no longer refuses a dirty tree
whose changes fall outside the owned allowlist."
```

---

### Task 12: Coverage — every stall route, through the cycle driver

**Files:**
- Modify: `egui-frontend/src/ui/app_coordinator.rs` test module (or `egui-frontend/tests/dirty_tree.rs` if the driver lives there)

**Interfaces:**
- Consumes: everything from Tasks 10 and 11.
- Produces: no production interfaces.

- [ ] **Step 1: Write the three stall-route tests**

Each runs through `run_cycles_until_terminal`, not a single `apply_merge`
call — defect 1 only shows itself across cycles.

```rust
    /// Defect 1, route 2: a deleted tracked file. `index.add_path` cannot
    /// stage a deletion, and the old staging loop skipped files that do not
    /// exist — so `git status` said dirty, staging produced nothing, and the
    /// merge refused on every future cycle forever.
    #[test]
    fn a_deleted_tracked_file_does_not_stall_sync() {
        let (mut app, child_id, _temp, peer) = ChildRepoFixture::new()
            .with_peer_commit(&[("transactions.csv", "id,child_id,date,description,amount,balance,type\n")])
            .with_deleted("goals.csv")
            .build();
        assert_progress(&mut app, &child_id, peer.unwrap());
    }

    /// Defect 1, route 1: `parental_control_attempts.csv` is tracked but
    /// outside the owned list, and `commit_file_change` swallows commit
    /// failures — one swallowed failure left it dirty forever.
    #[test]
    fn a_dirty_parental_control_log_does_not_stall_sync() {
        let (mut app, child_id, _temp, peer) = ChildRepoFixture::new()
            .with_peer_commit(&[("transactions.csv", "id,child_id,date,description,amount,balance,type\n")])
            .with_dirty("parental_control_attempts.csv", "1,1234,false\n2,5678,false\n")
            .build();
        assert_progress(&mut app, &child_id, peer.unwrap());
    }

    /// Defect 1, route 3: `delete_allowance_config` removes a tracked owned
    /// file with no commit — user-triggerable, no crash and no swallowed
    /// error required. Found in panel review.
    #[test]
    fn deleting_the_allowance_config_does_not_stall_sync() {
        let (mut app, child_id, _temp, peer) = ChildRepoFixture::new()
            .with_peer_commit(&[("transactions.csv", "id,child_id,date,description,amount,balance,type\n")])
            .build();
        app.backend()
            .allowance_service
            .delete_allowance_config(&child_id)
            .expect("delete allowance config");
        assert_progress(&mut app, &child_id, peer.unwrap());
    }

    /// A tracked file this app does not manage. Under the tracked-path
    /// staging this RESOLVES rather than failing — it is already in pushed
    /// history, so committing its modification is not the hazard the owned
    /// list guards against.
    #[test]
    fn a_dirty_tracked_unowned_file_resolves_rather_than_stalling() {
        let (mut app, child_id, _temp, peer) = ChildRepoFixture::new()
            .with_peer_commit(&[("transactions.csv", "id,child_id,date,description,amount,balance,type\n")])
            .build();
        let child_dir = app.backend().csv_connection
            .child_dir(&shared::ChildId::from(child_id.as_str())).unwrap();

        // TRACKED, not untracked — `working_tree_dirty` sets
        // include_untracked(false), so an untracked file would never enter
        // the guard and this test would pass for the wrong reason.
        std::fs::write(child_dir.join("notes.txt"), "committed once").unwrap();
        let gm = GitManager::new();
        gm.add_file(&child_dir, "notes.txt").unwrap();
        gm.commit(&child_dir, "track notes.txt").unwrap();
        std::fs::write(child_dir.join("notes.txt"), "now modified").unwrap();

        let repo = Repository::open(&child_dir).unwrap();
        assert!(
            crate::backend::sync::child_sync::working_tree_dirty(&repo).unwrap(),
            "precondition: the guard must actually be reached — a fixture that stops \
             reproducing this condition must fail loudly, not pass vacuously"
        );

        assert_progress(&mut app, &child_id, peer.unwrap());
    }

    fn assert_progress(app: &mut AllowanceTrackerApp, child_id: &str, peer: git2::Oid) {
        let child_dir = app.backend().csv_connection
            .child_dir(&shared::ChildId::from(child_id)).unwrap();
        let head_before = Repository::open(&child_dir).unwrap()
            .head().unwrap().peel_to_commit().unwrap().id();

        match run_cycles_until_terminal(app, child_id, peer, 5) {
            Ok(_) => {}
            Err(trace) => panic!(
                "sync never reached a terminal state in 5 cycles — this is the stall. Trace:\n{}",
                trace.join("\n")
            ),
        }

        let repo = Repository::open(&child_dir).unwrap();
        assert_resolved_or_explained(app, &repo, child_id, head_before);
    }
```

- [ ] **Step 2: Write the parse-gate test**

```rust
    /// With the hard reset gone, nothing else validates. Committing
    /// unparseable bytes would push them to the peer and break read_rows on
    /// BOTH machines — a one-machine corruption becoming a two-machine
    /// outage, strictly worse than the defect being fixed.
    #[test]
    fn an_unparseable_transactions_csv_is_refused_not_committed() {
        let (mut app, child_id, _temp, peer) = ChildRepoFixture::new()
            .with_peer_commit(&[("transactions.csv", "id,child_id,date,description,amount,balance,type\n")])
            .with_dirty("transactions.csv", "this is not a csv at all\x00\x01")
            .build();
        let child_dir = app.backend().csv_connection
            .child_dir(&shared::ChildId::from(child_id.as_str())).unwrap();
        let repo = Repository::open(&child_dir).unwrap();
        let head_before = repo.head().unwrap().peel_to_commit().unwrap().id();

        let terminal = run_cycles_until_terminal(&mut app, &child_id, peer.unwrap(), 3);
        assert_eq!(terminal, Ok(Terminal::FailedWithNotice));

        let head_after = Repository::open(&child_dir).unwrap()
            .head().unwrap().peel_to_commit().unwrap().id();
        assert_eq!(head_before, head_after, "nothing may be committed");
        assert_eq!(
            std::fs::read(child_dir.join("transactions.csv")).unwrap(),
            b"this is not a csv at all\x00\x01",
            "the file must be left exactly as found — refused, not repaired, not discarded"
        );
        assert!(app.sync.sync_failures.iter().any(|n| n.child_id == child_id));
    }
```

- [ ] **Step 3: Write the cross-product table**

The space is finite and enumerable, so this is a deterministic table rather
than a proptest — a shrinker would hand back a misleading minimal case.

```rust
    /// Every combination of file state the guard can meet. Deterministic and
    /// exhaustive: five owned files plus a tracked-unowned representative,
    /// each unchanged / modified / deleted.
    #[test]
    fn the_guard_resolves_or_explains_every_dirty_tree_shape() {
        #[derive(Debug, Clone, Copy)]
        enum State { Unchanged, Modified, Deleted }

        const FILES: &[&str] = &[
            "transactions.csv",
            "goals.csv",
            "child.yaml",
            "allowance_config.yaml",
            "parental_control_attempts.csv",
        ];

        for file in FILES {
            for state in [State::Unchanged, State::Modified, State::Deleted] {
                let mut fixture = ChildRepoFixture::new().with_peer_commit(&[(
                    "transactions.csv",
                    "id,child_id,date,description,amount,balance,type\n",
                )]);
                fixture = match state {
                    State::Unchanged => fixture,
                    // Content that still parses — the unparseable case has
                    // its own test with its own expected outcome.
                    State::Modified if *file == "transactions.csv" => fixture.with_dirty(
                        file,
                        "id,child_id,date,description,amount,balance,type\n",
                    ),
                    State::Modified => fixture.with_dirty(file, "modified: true\n"),
                    State::Deleted => fixture.with_deleted(file),
                };

                let (mut app, child_id, _temp, peer) = fixture.build();
                let child_dir = app.backend().csv_connection
                    .child_dir(&shared::ChildId::from(child_id.as_str())).unwrap();
                let head_before = Repository::open(&child_dir).unwrap()
                    .head().unwrap().peel_to_commit().unwrap().id();

                let result = run_cycles_until_terminal(&mut app, &child_id, peer.unwrap(), 5);
                assert!(
                    result.is_ok(),
                    "{file} in state {state:?} never reached a terminal state: {result:?}"
                );

                let repo = Repository::open(&child_dir).unwrap();
                assert_resolved_or_explained(&app, &repo, &child_id, head_before);
            }
        }
    }
```

- [ ] **Step 4: Run to verify they fail against pre-Task-11 behaviour**

If Tasks 9 and 11 are already applied these will pass. To prove they would
have caught the defects, temporarily revert `resolve_dirty_tree`'s
`update_all` to the old owned-list staging, confirm the three stall-route
tests FAIL with the trace message, then restore. Do not skip this — a test
that has never been seen to fail is not evidence.

- [ ] **Step 5: Run the full suite**

Run: `cargo test --manifest-path /Users/kerryhart/Code/allowance-tracker/Cargo.toml --workspace`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git -C /Users/kerryhart/Code/allowance-tracker add egui-frontend
git -C /Users/kerryhart/Code/allowance-tracker commit -m "test(sync): all three stall routes, the parse gate, and an exhaustive dirty-tree table"
```

---

### Task 13: Notice severity, sorted blocking-first

`FastForwardBlockedNotice` means "handled, nothing for you to do"; the
backstop means "this child is not syncing until you act". They currently
render identically in red.

**Files:**
- Modify: `egui-frontend/src/ui/state/sync_state.rs:76-96`, `:181-211`
- Test: same file

**Interfaces:**
- Consumes: nothing.
- Produces: `pub enum NoticeSeverity { Informational, Blocking }`; `SyncFailureNotice` gains `pub severity: NoticeSeverity`; `pub fn blocking_notices(&self) -> Vec<&SyncFailureNotice>`.

- [ ] **Step 1: Write the failing test**

```rust
    #[test]
    fn blocking_notices_sort_ahead_of_informational_ones() {
        let mut state = SyncUiState::new();
        state.record_sync_failure(SyncFailureNotice {
            child_id: "child-a".to_string(),
            message: "informational".to_string(),
            severity: NoticeSeverity::Informational,
        });
        state.record_sync_failure(SyncFailureNotice {
            child_id: "child-b".to_string(),
            message: "blocking".to_string(),
            severity: NoticeSeverity::Blocking,
        });

        let ordered = state.notices_blocking_first();
        assert_eq!(ordered[0].child_id, "child-b", "a blocking notice must never be scrolled below an informational one");
    }

    #[test]
    fn a_child_with_no_blocking_notice_is_not_badged() {
        let mut state = SyncUiState::new();
        state.record_sync_failure(SyncFailureNotice {
            child_id: "child-a".to_string(),
            message: "informational".to_string(),
            severity: NoticeSeverity::Informational,
        });
        assert!(!state.has_blocking_notice());
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test --manifest-path /Users/kerryhart/Code/allowance-tracker/Cargo.toml -p allowance-tracker-egui sync_state`
Expected: FAIL — `NoticeSeverity` does not exist.

- [ ] **Step 3: Implement**

```rust
/// How much the user needs to care. `FastForwardBlockedNotice` and a stalled
/// child currently render identically in red; a parent seeing two red lines
/// against the same child has no way to tell which is the emergency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum NoticeSeverity {
    /// Something happened and was handled. No action needed.
    Informational,
    /// This child is not syncing until a human acts.
    Blocking,
}
```

Add `pub severity: NoticeSeverity` to `SyncFailureNotice`, then:

```rust
    /// Blocking first, so the one that matters is never the one scrolled out
    /// of a 120px box.
    pub fn notices_blocking_first(&self) -> Vec<&SyncFailureNotice> {
        let mut all: Vec<&SyncFailureNotice> = self.sync_failures.iter().collect();
        all.sort_by(|a, b| b.severity.cmp(&a.severity));
        all
    }

    /// Drives the child-picker badge.
    pub fn has_blocking_notice(&self) -> bool {
        self.sync_failures.iter().any(|n| n.severity == NoticeSeverity::Blocking)
    }
```

Update `fail_sync` (Task 11) to pass `NoticeSeverity::Blocking`, and every
other `SyncFailureNotice` construction site to name its severity explicitly —
no `Default`, so each is a decision.

- [ ] **Step 4: Run and commit**

Run: `cargo test --manifest-path /Users/kerryhart/Code/allowance-tracker/Cargo.toml --workspace`
Expected: PASS.

```bash
git -C /Users/kerryhart/Code/allowance-tracker add egui-frontend/src/ui
git -C /Users/kerryhart/Code/allowance-tracker commit -m "feat(ui): notice severity, blocking-first ordering"
```

---

### Task 14: Say it in the user's language, and give them an action

**Files:**
- Modify: `egui-frontend/src/ui/components/settings/lgs_sync_modal.rs:614` and its notice-line renderer

**Interfaces:**
- Consumes: `NoticeSeverity`, `RegistryEntry` (`child_registry.rs:22`, carries `label` and `path`).
- Produces: no new public interfaces.

- [ ] **Step 1: Compose the sentence at the UI boundary**

A parent knows *Amélie*; they do not know `child_7f3a…`, a git oid, or what
"stage" means. `RegistryEntry` already carries both the display name and the
folder path — the product was choosing not to use them.

In the notice renderer, resolve the child's `label` from the registry and
build the text there rather than printing `notice.message` raw:

```rust
// The sentence is composed HERE, where the display name and folder path
// live — not inside the sync engine. Wording changes must never be edits to
// app_coordinator.rs.
let name = registry_label_for(&notice.child_id).unwrap_or_else(|| notice.child_id.clone());
let headline = match notice.severity {
    NoticeSeverity::Blocking => format!("{name}'s sync is paused."),
    NoticeSeverity::Informational => format!("{name}: sync note"),
};
let detail = "A file in this child's folder was changed outside the app, and the app \
              will not overwrite it. Once it is resolved, syncing resumes on its own — \
              there is nothing to press.";
```

- [ ] **Step 2: Add "Show the folder"**

```rust
if ui.button("Show the folder").clicked() {
    if let Some(path) = registry_path_for(&notice.child_id) {
        let _ = std::process::Command::new("open").arg(path).spawn();
    }
}
```

Without this the remediation instruction is "open Terminal and run git",
which is not an instruction this product can give.

- [ ] **Step 3: Render blocking notices first**

Replace the three separate iterations in `render_sync_notices` with one pass
over `self.sync.notices_blocking_first()` for the failures, keeping the
`goals_diverged` and `fast_forward_blocked` loops as they are — the
single-`SyncNotice` collapse is a named follow-up, not this task.

- [ ] **Step 4: Verify manually and commit**

Run: `cargo check --manifest-path /Users/kerryhart/Code/allowance-tracker/Cargo.toml --workspace`
Then launch the app and confirm the modal renders. If a project skill covers launching the app, use it.

```bash
git -C /Users/kerryhart/Code/allowance-tracker add egui-frontend/src/ui/components/settings/lgs_sync_modal.rs
git -C /Users/kerryhart/Code/allowance-tracker commit -m "feat(ui): name the child, explain the pause, offer the folder"
```

---

### Task 15: A badge where the user is actually looking

`SyncStatus` is written in roughly thirty places and **read by no UI component
at all**. `SyncFailureNotice`'s only reader is a 120px scroll area inside
Settings → "Sync with another Mac…" — a modal a parent opens perhaps twice in
the life of the app. A stall there is invisible.

**Files:**
- Modify: the child-picker component (find with `grep -rln "child_picker\|render_child_selector" egui-frontend/src/ui/components`)

**Interfaces:**
- Consumes: `SyncUiState::has_blocking_notice` (Task 13).
- Produces: none.

- [ ] **Step 1: Add the badge**

Next to the child picker, when `app.sync.has_blocking_notice()`:

```rust
// The child picker is where the affected child may be the SELECTED one,
// showing a balance the other Mac does not share — the moment the user is
// most likely to be misled. A notice that only lives in a settings modal
// does not reach them here.
if app.sync.has_blocking_notice() {
    let badge = ui.add(
        egui::Label::new(egui::RichText::new("⚠ Sync paused").color(egui::Color32::from_rgb(200, 80, 40)))
            .sense(egui::Sense::click()),
    );
    if badge.clicked() {
        app.open_sync_settings_modal();
    }
    badge.on_hover_text("One or more children are not syncing. Click for details.");
}
```

Wire `open_sync_settings_modal` to whatever the existing settings-modal
opener is — do not add a second mechanism.

- [ ] **Step 2: Record that `SyncStatus` has no reader**

Add above the `status` field in `sync_state.rs`:

```rust
    /// **Currently read by no UI component.** Written in ~30 places in
    /// `app_coordinator.rs` and rendered nowhere — a status write alone does
    /// NOT tell the user anything. Anything the user must see goes through
    /// `sync_failures` with `NoticeSeverity::Blocking`, which the child-picker
    /// badge surfaces.
```

- [ ] **Step 3: Verify and commit**

Run: `cargo test --manifest-path /Users/kerryhart/Code/allowance-tracker/Cargo.toml --workspace`

```bash
git -C /Users/kerryhart/Code/allowance-tracker add egui-frontend/src/ui
git -C /Users/kerryhart/Code/allowance-tracker commit -m "feat(ui): surface a paused child where the user is already looking"
```

---

### Task 16: Correct the acceptance checklist's false coverage claim

`.github/workflows/ci.yml` runs `cargo test --workspace` — no `--ignored`, no
`LGS_BINARY` — while `two_machine_sync.rs:103` and `codec_real_data.rs:18` are
both `#[ignore]`d. The checklist's "What CI already covers" tells a reader not
to hand-test the two-machine harness, which nothing tests.

**Files:**
- Modify: `docs/lgs-sync-acceptance-checklist.md`

- [ ] **Step 1: Correct the entry**

Replace the "What CI already covers" claim about the two-machine harness:

```markdown
**Correction (2026-09-19):** `two_machine_sync` and `codec_real_data` are
`#[ignore]`d and CI runs plain `cargo test --workspace`, so **neither has ever
run in CI.** Do not treat the two-machine path as covered. Even when run by
hand, that harness commits `ledger.txt` through raw `GitManager` calls — it
never drives `ChildSyncEngine` or `apply_merge`, so no automated test anywhere
converges two app instances over real financial data through the real code.
That is the structural reason both dirty-tree defects shipped.

Adding a CI job that installs `lgs` and runs `cargo test --workspace --
--ignored` is tracked as follow-up 2 in the dirty-tree resolution spec. An
ignored test that never runs is documentation, not coverage.
```

- [ ] **Step 2: Commit**

```bash
git -C /Users/kerryhart/Code/allowance-tracker add docs/lgs-sync-acceptance-checklist.md
git -C /Users/kerryhart/Code/allowance-tracker commit -m "docs: correct the checklist's false CI-coverage claim"
```

---

## Verification

After Task 16, before declaring the work complete:

- [ ] `cargo test --manifest-path /Users/kerryhart/Code/allowance-tracker/Cargo.toml --workspace` — full pass, with the count at or above the 597 baseline plus the new tests.
- [ ] `grep -rn "ResetType::Hard" backend/ egui-frontend/src/` — no matches in the sync paths.
- [ ] `grep -rn "with_extension(\"tmp\")" backend/` — no matches.
- [ ] `grep -rn "let _ = self.git_manager.commit_file_change" backend/` — no matches.
- [ ] `grep -rn "recover_if_dirty\|Recovered::" backend/ egui-frontend/src/` — no matches.
- [ ] Launch the app, open Settings → "Sync with another Mac…", confirm the notice area renders.

## Follow-ups (explicitly NOT in this plan)

Recorded in the spec's scope boundaries; do not let them creep in:

1. Retire `MERGE_IN_PROGRESS_MARKER` entirely.
2. A CI job running `--ignored` with `lgs` installed.
3. Collapse the three notice `Vec`s into one `SyncNotice`.
4. A "working tree clean or explained" stage in the "Check sync" flow.
5. A merge rule for `parental_control_attempts.csv`.
6. Two app instances converging real financial data through the real code.
