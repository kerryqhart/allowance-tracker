# Dirty-tree resolution: closing two data-loss paths in child sync

**Date:** 2026-09-18
**Status:** Draft for review
**Supersedes nothing.** Extends `2026-08-29-lgs-desktop-sync-design.md`, whose
dirty-tree handling this corrects.

## Problem

Two defects found after the lgs desktop sync branch landed. They look
unrelated — one is a stall, one is a deletion — but they share a root cause.

### Defect 1: a dirty tree with no owned file changed stalls sync silently

`commit_dirty_tree_before_merge` (`egui-frontend/src/ui/app_coordinator.rs:1268`)
stages only `FILES_THIS_APP_OWNS` and calls `commit_if_changed`. When that
returns `Ok(None)` — the tree is dirty, but staging produced no tree change —
it logs a warning, returns `ApplyMergeOutcome::DirtyTreeCommitted`, and
requests a re-poll.

Nothing was committed, so the return value is false. The next cycle re-derives
the same divergence, finds the same dirty tree, and refuses again. After
`STALE_HEAD_REFUSAL_LIMIT` consecutive refusals the re-poll is suppressed as
well, so the child stops syncing with nothing visible in the UI.

The sibling fast-forward path, `commit_dirty_tree_to_unblock_fast_forward`
(`:1650`), handles the identical condition correctly: `SyncFailureNotice`,
`SyncStatus::Error`, `Failed`. The two paths disagree, and the merge path is
the wrong one.

Two reachable routes:

- **`parental_control_attempts.csv`.** Tracked in git, but deliberately
  excluded from `FILES_THIS_APP_OWNS` (`backend/sync/paths.rs:19-25`) on the
  reasoning that `ParentalControlRepository` commits it itself. But
  `commit_file_change` (`backend/storage/git/mod.rs:381-390`) swallows commit
  failures with a `warn!` and returns `Ok`. One swallowed failure leaves that
  file dirty indefinitely, and every subsequent merge for that child stalls.

- **A deleted owned file.** Both staging loops skip a file that does not
  exist (`if !child_dir.join(name).exists() { continue; }`), so a deletion is
  never staged. `index.add_path` cannot stage a deletion in any case; that
  needs `index.remove_path`. `git status` reports dirty, staging yields
  nothing, `Ok(None)`, stall.

### Defect 2: `recover_if_dirty` destroys uncommitted rows

`recover_if_dirty` (`backend/sync/child_sync.rs:908`) hard-resets the working
tree when `MERGE_IN_PROGRESS_MARKER` is present. The marker proves a merge
*began and did not finish*. It does not prove the tree's current contents came
from that merge.

After a crash mid-merge the app restarts, and the AWS transport's
non-committing path (`upsert_transaction_no_commit`,
`backend/storage/csv/transaction_repository.rs:225`) can write fresh
MCP-authored rows into that same `transactions.csv` before the next
`apply_merge` runs. `recover_if_dirty` then resets to HEAD and takes those rows
with it. The AWS watermark has already advanced past them, so they cannot be
re-fetched: permanent, silent loss.

This is the same hazard class as the CRITICAL-1 bug the merge path already
fixed. The marker narrowed the window rather than closing it.

The reset also runs *before* the unconditional dirty-tree guard in both call
sites (`:970` and `:1456`), so it pre-empts the code that would have protected
those rows. The existing log message concedes exactly this (`:983-988`):

> "This says nothing about protecting any MCP-authored row that might
> independently be sitting in this dirty tree: that protection is the
> dirty-tree guard below (unconditional, not marker-gated), not this recovery
> step."

### Root cause

Both defects decide what a dirty tree means by **inference** rather than
evidence. Defect 1 infers "nothing owned changed ⇒ nothing to do" when the
real cause is a staging bug and an incomplete ownership list. Defect 2 infers
"marker present ⇒ these contents are that merge's" from a marker that carries
no such information.

Underneath both: `transactions.csv` is written with a plain `std::fs::write`
(`transaction_repository.rs:188`, `app_coordinator.rs:1168`), so a torn or
truncated file is possible. Because `delete_transaction_no_commit` legitimately
removes rows without committing, a file with fewer rows than HEAD is ambiguous
— an MCP deletion, or a truncation. That ambiguity is unanswerable, and it is
why the original author reached for a hard reset. Removing the ambiguity at the
source is what makes every other fix here sound.

## Design

Four changes, in dependency order.

### 1. Atomic writes

New module `backend/storage/atomic.rs`:

```rust
pub fn write_atomic(path: &Path, contents: &[u8]) -> Result<()>
```

Temp file in the same directory (`.<name>.tmp-<pid>-<nonce>`), write,
`File::sync_all()`, `fs::rename`, then fsync the containing directory. The temp
file is removed on every failure path. The dotted prefix keeps it out of the
child-folder UI, and because staging is an explicit allowlist, a stray temp file
can never reach a commit.

This consolidates as much as it adds. The pattern is already hand-rolled in six
places — `allowance_repository.rs:99`, `child_repository.rs:144`,
`global_config_repository.rs:145`, `child_registry.rs:86`,
`sync_persistence.rs:61` and `:96`, `migration.rs:433` — each as a bare `write`
+ `rename` with **no fsync**, which on power loss can still leave a zero-length
file. All of them move to the shared helper, for the same "one definition, not
five" reason `FILES_THIS_APP_OWNS` already exists.

Newly protected, unprotected today:

| Site | File | Today |
|---|---|---|
| `transaction_repository.rs:188` | `transactions.csv` | `fs::write` |
| `app_coordinator.rs:1168` | `transactions.csv` (merged) | `fs::write` |
| `goal_repository.rs:128` | `goals.csv` | `File::create`, in place |

**Deliberate exception: `parental_control_attempts.csv` stays an append.** It is
written with `OpenOptions::new().create(true).append(true)`
(`parental_control_repository.rs:131-137`). An interrupted append leaves at most
a partial trailing line; an interrupted rewrite can lose the whole audit log.
Converting it would make its worst case worse. It still joins the owned list for
*staging* — that is what closes defect 1's first route — and its reader already
tolerates a malformed trailing record. This file therefore carries a weaker
guarantee than the rest of the list: bounded corruption of the last record,
rather than none. That is a conscious trade, recorded here so a later reader
does not "fix" it into a rewrite.

### 2. One staging implementation

`stage_owned_files` (`git/mod.rs:50`) becomes the single implementation and
gains the deletion case:

```
for name in FILES_THIS_APP_OWNS:
    if exists(dir/name):       index.add_path(name)
    else if tracked_in_HEAD:   index.remove_path(name)   // missing today
    else:                      skip
```

The two hand-rolled copies of this loop in `app_coordinator.rs` (`:1284`,
`:1677`) are deleted in favour of calling it, which also retires the "if that
staging step ever needs to change, change it in both places" instruction at
`:1264` — an instruction that is itself evidence the duplication was a
liability.

`FILES_THIS_APP_OWNS` gains `parental_control_attempts.csv`. Its doc comment in
`paths.rs` is rewritten around the rule the guard actually depends on:

> This list is everything the dirty-tree guard may commit.

rather than the current "who commits it first" reasoning. Note that adding the
file changes nothing about what is synced — it is already tracked and already
pushed — only about which commit picks it up.

### 3. One dirty-tree resolution path, with a backstop

`commit_dirty_tree_before_merge` and `commit_dirty_tree_to_unblock_fast_forward`
converge on a shared helper returning:

```rust
enum DirtyTreeResolution {
    Committed(String),        // oid
    NothingOwnedToCommit,
    Failed(String),           // user-facing message
}
```

Each caller maps that to its own outcome enum. On the *success* branch the
fast-forward path keeps recording `FastForwardBlockedNotice` and the merge path
records no notice, exactly as both do today; the two paths converge only on the
failure branches below, where they currently disagree.

On `NothingOwnedToCommit` — now a trustworthy signal, since staging can no
longer miss a deletion or an owned file — **both** paths do what the
fast-forward path already does: `SyncFailureNotice` + `SyncStatus::Error` +
`Failed`. The merge path stops returning `DirtyTreeCommitted` when it committed
nothing. That false return is what made the stall silent.

The backstop then means one specific thing: *a tracked file this app does not
manage is dirty, and a human must resolve it.* Sync for that child stops, and
says so.

### 4. `recover_if_dirty` loses its reset

It becomes:

```rust
pub fn clear_interrupted_merge_marker(repo: &Repository) -> Result<bool>
```

Clears a stale marker if present; returns whether there was one. Callers log
"a previous merge for this child was interrupted" and continue. No
`git2::ResetType::Hard` remains anywhere in the sync paths.

`Recovered` and `Recovered::DiscardedAndReMerged` are deleted.
`write_merge_marker`, `clear_merge_marker` and `MERGE_IN_PROGRESS_MARKER` stay,
with doc comments rewritten to state that the marker is a diagnostic breadcrumb,
not an authorization to discard.

Content handling for an interrupted merge is then the same code path that
handles an ordinary MCP write. That is what stops defect 2 from being a special
case rather than merely patching it.

Retiring the marker entirely is a deliberate **non-goal** here: it is worth
doing only once atomic writes have real runtime behind them, and removing the
safety net and its enabling change together would leave nothing to fall back on.

## What a cycle looks like afterwards

```
open repo
  → clear + log any stale interrupted-merge marker
  → stale-HEAD check
  → if working tree dirty:
        stage owned files (adds, and deletions)
        tree differs from HEAD ? commit it : raise durable failure notice
        discard this merge computation, re-poll (debounced)
  → goals-divergence notice
  → write marker
  → atomic write of merged CSV
  → merge commit
  → clear marker
  → push
```

The dirty-tree guard becomes the only code that touches uncommitted content, and
it only ever commits it.

## Error handling

| Condition | Result |
|---|---|
| Staging failure | `SyncFailureNotice` + `SyncStatus::Error` + `Failed` |
| Commit failure | `SyncFailureNotice` + `SyncStatus::Error` + `Failed` |
| `NothingOwnedToCommit` | `SyncFailureNotice` + `SyncStatus::Error` + `Failed` |
| Atomic write failure | Previous file intact; operation fails; never a partial file |
| Marker clear failure | Logged, non-fatal — the marker is diagnostic only |

`SyncFailureNotice` rather than status alone, because a status write is erased
by the next unrelated sync event; a stall that needs a human must survive that.

`commit_file_change` (`git/mod.rs:388`) starts returning its commit error
instead of swallowing it with a `warn!`. That swallow is what strands
`parental_control_attempts.csv` dirty and reaches defect 1's first route.

## Testing

One regression test per defect route, each of which fails against today's code:

- **Deleted owned file.** Delete a tracked `transactions.csv`, run
  `apply_merge`: the deletion is staged, a commit lands, sync progresses.
- **Dirty `parental_control_attempts.csv`.** Same shape, defect 1's second
  route.
- **Backstop.** Dirty a tracked file outside the owned list (`notes.txt` — the
  shape `app_coordinator.rs:3140` already sets up): `Failed` plus a
  `SyncFailureNotice` the UI can display, not a silent `warn!`.
- **No-silent-stall invariant.** The property behind all three: after the guard
  runs on any dirty tree, either the tree is clean or a durable failure notice
  exists. Never neither. This is exactly what defect 1 violated.
- **Crash plus MCP row.** Marker present *and* an uncommitted MCP-authored row:
  the row survives the next `apply_merge`. Defect 2's regression test.
- **Atomic write.** Content correct after success; no temp residue after either
  success or failure; a failed write leaves the prior file byte-identical.

Existing `two_machine_sync`, `wire_compat` and `codec_real_data` suites run
unchanged — the on-disk format does not move.

## Scope boundaries

Out of scope, and unchanged by this work:

- `goals.csv` semantic merge (a known, recorded gap in the lgs design).
- `.git/index.lock` and index-corruption recovery.
- The AWS transport's non-committing write strategy (Task 16's decision stands).
- Retiring `MERGE_IN_PROGRESS_MARKER` entirely — a follow-up, per above.

## Files touched

| File | Change |
|---|---|
| `backend/storage/atomic.rs` | New — `write_atomic` |
| `backend/storage/mod.rs` | Register the module |
| `backend/storage/csv/transaction_repository.rs` | Atomic ledger write |
| `backend/storage/csv/goal_repository.rs` | Atomic `goals.csv` write |
| `backend/storage/csv/allowance_repository.rs` | Use shared helper |
| `backend/storage/csv/child_repository.rs` | Use shared helper |
| `backend/storage/csv/global_config_repository.rs` | Use shared helper |
| `backend/storage/csv/child_registry.rs` | Use shared helper |
| `backend/storage/csv/migration.rs` | Use shared helper |
| `backend/domain/sync_persistence.rs` | Use shared helper |
| `backend/storage/git/mod.rs` | Staging deletions; stop swallowing commit errors |
| `backend/sync/paths.rs` | Add `parental_control_attempts.csv`; rewrite doc |
| `backend/sync/child_sync.rs` | `recover_if_dirty` → `clear_interrupted_merge_marker` |
| `egui-frontend/src/ui/app_coordinator.rs` | Shared dirty-tree helper; backstop; atomic merged write |
