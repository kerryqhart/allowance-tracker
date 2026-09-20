# NEXT: the fast-forward dirty-tree gap, and the emptied-ledger notice

**Written:** 2026-09-20, at the end of the dirty-tree-resolution work (merged as `dc1ae0d`).
**Spec this continues:** `docs/superpowers/specs/2026-09-18-dirty-tree-resolution-design.md` — see its
`### Named follow-ups`, items 7 and the notice question below.

Two pieces of work were deliberately left undone. Both are recorded here with
enough context that a fresh session needs no archaeology.

---

## Piece 1 — `apply_fast_forward` has no proactive dirty-tree check

### What is wrong

`apply_merge` checks `working_tree_dirty` unconditionally before it does
anything else. `apply_fast_forward` does not. It discovers a dirty tree only
**reactively**, when `checkout_tree` raises `GIT_ECONFLICT` — and libgit2's
SAFE checkout only acts on paths that differ between HEAD and the target, so
a conflict fires only if the peer's incoming commit happens to touch the same
path that is dirty locally.

When it doesn't — the peer's commit is a pure fast-forward touching some other
file — the checkout succeeds, HEAD advances, the cycle terminates normally, and
sync looks perfectly healthy. But the local dirty or deleted file is never
staged, committed, or pushed.

Observed directly during Task 12:
`head_advanced=true, tree_clean=false, notice_present=false`.

### What it is NOT

It is **not defect 1**, and it is **not a regression from this branch**. Both
were checked and confirmed during the work:

- Defect 1 was a *liveness* failure — the merge refused every cycle, HEAD never
  moved, the machine ingested nothing. Here the fast-forward succeeds and peer
  data keeps arriving. What is lost is the **outbound** direction only.
- Pre-branch, `apply_fast_forward` called the marker-gated `recover_if_dirty`,
  which with no marker present returned `Clean` *without inspecting the tree*.
  Behaviour was identical. The branch changed only the marker-present sub-case,
  making it strictly safer (no more hard reset).

### Actual consequence

The two machines silently disagree. It self-heals on any machine that writes
locally — a local commit creates divergence, and `apply_merge`'s unconditional
check then resolves it — and on any later peer commit that touches the same
path. **It persists indefinitely only on a receive-only machine.**

Severity: Important. No data loss, no stall.

### Acceptance criteria already written

Two `#[ignore]`d regression tests in `egui-frontend/src/ui/app_coordinator.rs`
(`apply_merge_tests`) carry the full repro and analysis in their doc comments:

- `a_deleted_tracked_file_does_not_stall_sync`
- `deleting_the_allowance_config_does_not_stall_sync`

Delete the two `#[ignore]` attributes; they are the definition of done. They do
not care which policy below you pick — they assert only that the child makes
progress and the local change survives.

### The design decision to make first

This is the part worth thinking about before writing code, and it is genuinely
a choice — the topology is not.

`classify` (`backend/sync/child_sync.rs:121-130`) is pure graph fact: if the
peer's tip descends from ours, that **is** a fast-forward. No policy there.
What we do about it is entirely ours.

**Option A — mirror the existing pattern.** Hoist a `working_tree_dirty` check
to the top of `apply_fast_forward` and route a dirty tree into
`resolve_dirty_tree`, exactly as `commit_dirty_tree_to_unblock_fast_forward`
already does on the conflict path. The dirty content is committed, HEAD moves,
the cycle reclassifies as `Diverged` next tick, and the merge path resolves it.

Cost: two commits (a local one, then a merge one) and two cycles per occurrence,
plus a window where the child sits in a "blocked, will retry" state.
Benefit: identical in shape to what the conflict path already does, so there is
one pattern in the codebase rather than two.

**Option B — resolve in one cycle.** Recognise that a dirty tree during a
fast-forward is a divergence in the making, and write a single two-parent commit
directly: base = HEAD, ours = HEAD-plus-dirty, theirs = peer tip, content = the
merged union. `GitManager::commit_merge` (`backend/storage/git/mod.rs:296`)
already takes arbitrary parents, so the machinery exists.

Cost: a second code path to understand.
Benefit: one commit instead of two, one cycle instead of two, no blocked window.

**Constraint either option must respect.** `Cycle::Ahead`'s doc comment records
a Critical-1 regression where treating "we are ahead" as "diverged" produced an
*unbounded* stream of empty merge commits while a push was pending. Any
one-cycle merge must fire **only when the tree is genuinely dirty**, never as a
general policy, or it walks straight back into that bug.

### Coverage gap to close in the same change

`the_guard_resolves_every_dirty_tree_shape_on_the_merge_path` forces every cell
onto the `Diverged` path. It covers every file × state shape on the **merge**
path only and can never catch a fast-forward regression. Extend it — or add a
sibling table — that drives the same cells through the fast-forward path.

### Effort

Small. The infrastructure is all in place: `resolve_dirty_tree`, `fail_sync`,
`DirtyTreeError`, `ChildRepoFixture`, `run_cycles_until_terminal`,
`assert_resolved_or_explained`. The design decision above is the real content;
the code is contained.

---

## Piece 2 — the emptied-ledger notice is not actionable in one case

### What is wrong

`resolve_dirty_tree` now refuses with `DirtyTreeError::WouldEmptyLedger` when
`transactions.csv` on disk holds no rows while HEAD still holds some. That
refusal is correct and must not be removed — see below — but it creates one
case the user cannot resolve from inside the app.

Deleting a child's **last** transaction, either through the swallowed-commit
path (`commit_file_change` logs and continues) or through the deliberately
non-committing AWS path (`delete_transaction_no_commit` / `delete_local_entity`),
leaves exactly that state. The parent gets a permanent blocking notice whose
only exits are adding a transaction back, or restoring from a backup.

### Why the refusal must stay

Without it, the guard commits and pushes the emptied ledger. `read_rows` maps a
missing or empty file to zero rows, and `allowance_core::merge`
(`allowance-core/src/merge.rs:61-67`) reads "present in base, unchanged by us,
absent from ours" as `Decision::Deleted` — so the next merge **erases every row
on both machines**. This was found as a Critical in the final whole-branch review
and confirmed by inducing the failure: with the check disabled, the header-only
shape really does commit and push an emptied ledger.

A narrow, visible, recoverable liveness cost beats silent two-machine data loss.
That trade is deliberate.

### The real question

Fixing the *wording* so the notice names its escape hatches is half an hour and
worth doing regardless.

Fixing it *properly* means answering something harder: **how does the app
distinguish a deliberate "clear everything" from corruption that happens to look
identical?** On disk they are the same bytes. That is the same ambiguity that
made the original author reach for a hard reset, and it is the reason this is
flagged as a design question rather than a patch.

Candidate directions, none chosen:

- An explicit confirm-empty affordance in the UI, so a deliberate full clear
  carries an intent marker the guard can honour.
- Have the app's own delete path always commit, so only the AWS path can produce
  the ambiguous state — narrows it without resolving it, and touches Task 16's
  deliberate non-committing decision.
- Let the notice offer a one-click "yes, I meant to empty this" that performs the
  commit the guard refused.

### Effort

Wording fix: trivial. Design fix: needs brainstorming, and probably shares a spec
with Piece 1, since both are about how sync failures reach the user.

---

## Useful context for whoever picks this up

- **The whole branch is merged** as `dc1ae0d` on `main`; 24 commits,
  `9980b57..05e5a9f`. Suite: 627 passed, 0 failed, 6 ignored.
- **The 6 ignored** are 4 pre-existing plus the 2 named above. Nothing else is
  skipped.
- **Test capability that already exists and should be used:**
  `ChildRepoFixture` (`egui-frontend/src/ui/test_support.rs`) builds a child with
  a real git repo and a planted peer commit; `run_cycles_until_terminal` drives
  the real `classify` → `merge_diverged` → `apply_*` loop and returns the outcome
  sequence on failure, so a stall reads as "the same outcome 5 times with HEAD
  unmoved"; `assert_resolved_or_explained` asserts the invariant (tree clean AND
  HEAD advanced, or a failure notice exists), with `assert_resolves_cleanly` as
  the stricter form for "this resolves" claims.
- **`test_support` is a `#[cfg(test)]` DESCENDANT of `app_coordinator`**, not a
  sibling. Private items reach descendants only; a sibling module cannot see
  `apply_merge` / `apply_fast_forward` and hits E0624. Do not "fix" this by
  widening production visibility.
- **When planting a peer commit, seed its tree from the parent's tree**, never
  from `treebuilder(None)`. An empty treebuilder drops every file the commit does
  not name, and a real checkout then deletes those from disk. A live test in this
  repo was silently deleting `child.yaml` that way before it was caught.
- **A test that has never been seen to fail is not evidence.** Three tests on the
  dirty-tree branch passed for the wrong reason and were only caught by reverting
  the fix and watching them stay green. Budget for that step.

## Things that are fine and do not need revisiting

- `parental_control_attempts.csv` is **not** a production stall route. The
  service hardcodes `"global"` at all three call sites and `attempts_dir` maps
  `"global"` to the base data directory, so that file never reaches a child's git
  repo in production. The per-child path exists and is tested, but nothing calls
  it. Any test naming it is a tracked-but-unowned *shape*, not a route.
- The narrow `FILES_THIS_APP_OWNS` list is correct for `commit_merge` and
  migration. The dirty-tree guard deliberately does not use it — it stages every
  tracked path via `index.update_all`, because a tracked file is already in
  pushed history. `backend/sync/paths.rs`'s doc comment explains the two tiers.
- `atomic::write` claims no power-loss durability, on purpose. `F_FULLFSYNC` was
  considered and rejected — tens of milliseconds per transaction write to buy
  survival of the last write, which is not what any of this protects against.
