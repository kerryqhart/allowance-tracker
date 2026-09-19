# [2A] Critique — Deiko Deshimaru

**Spec reviewed:** `docs/superpowers/specs/2026-09-18-dirty-tree-resolution-design.md`
**Reviewer:** Deiko Deshimaru, Distinguished Engineer
**Date:** 2026-09-18

---

## Overall Verdict

**Approve with changes.**

The root-cause framing — "both defects decide what a dirty tree means by
inference rather than evidence" — is the right abstraction, and the four
changes follow from it honestly. My concerns are about two things the design
asserts but does not establish: that the new backstop is *resolvable*, and that
the guard's "only ever commits it" property is *safe* for bytes it did not
write.

## Top Concerns

### Concern 1: The backstop contradicts a stated goal of the system it extends

**What I see:** Design §3 converts `NothingOwnedToCommit` on the merge path
into `SyncFailureNotice` + `SyncStatus::Error` + `Failed`, and states: "Sync for
that child stops, and says so." The predecessor spec's first goal
(`2026-08-29-lgs-desktop-sync-design.md`, "Goals") is: "**Two** Macs converge on
the same child data, **without a terminal**."

There is no in-app path from the backstop to a resolved state. The notice text
the fast-forward path already uses — "resolve them manually" — is actionable
only from a shell. And the notice's only render site is
`render_sync_notices` at `lgs_sync_modal.rs:614`, called from
`lgs_sync_modal.rs:164` — inside the LGS sync settings modal. A user who never
opens that modal sees a transient `SyncStatus::Error` that the next unrelated
sync event overwrites, which is very nearly the silent stall defect 1 describes.
The spec trades an infinite silent stall for an infinite *nearly*-silent stall.

**Why it matters:** Concretely: a past version, a backup restore, or a
hand-edit leaves `notes.txt` tracked and modified in
`children/keiko_hart/`. Every cycle from then on: dirty → stage owned →
`Ok(None)` → `Failed`. Forever, with no circuit breaker (unlike
`note_stale_head_refusal`, which at least bounds its own noise). The child
silently stops converging and the only signal lives two clicks deep in
settings.

**Recommendation:** Two options, and I would take the first.

1. **Widen the guard from "owned" to "tracked."** The allowlist exists to stop
   `add_all(["*"])` sweeping *untracked* strays (`.DS_Store`, swap files) into a
   pushed commit — that is exactly what `FILES_THIS_APP_OWNS`'s doc comment says
   at `paths.rs:9-17`. A file already tracked in HEAD is *already in the pushed
   history*; committing its modification is not the hazard the allowlist guards
   against. `index.update_all(["*"], ...)` is precisely this primitive: it
   stages modifications and deletions of already-tracked entries and never adds
   an untracked path. That makes `NothingOwnedToCommit` genuinely unreachable,
   removes the stall class rather than reporting it, and keeps the anti-`add_all`
   invariant intact. Keep `FILES_THIS_APP_OWNS` for `commit_merge` and
   migration, where the narrow list is still right.
2. If the hard fail stays: put the *specific dirty paths* in the notice message
   (the guard already has `repo.statuses()` in hand), render it in the startup
   banner or the main sync surface rather than only in the settings modal, and
   state in the spec that resolving it requires a terminal — a knowing exception
   to the predecessor's goal, not an oversight.

### Concern 2: The guard commits bytes it never validated, and pushes them

**What I see:** §"What a cycle looks like afterwards" concludes: "The
dirty-tree guard becomes the only code that touches uncommitted content, and it
only ever commits it." The root-cause section argues that atomic writes remove
the torn/truncated ambiguity "at the source," which is "what makes every other
fix here sound."

That argument holds only for writes *this binary performs after the upgrade*.
It does not cover: a `transactions.csv` already half-written on disk by the
current `fs::write` at `transaction_repository.rs:188` when the user upgrades; a
file damaged by anything other than this app (disk error, iCloud conflict copy
on a pre-migration child, a partially restored backup, a hand-edit); or the
`goals.csv` `File::create` in-place path on the old binary.

**Why it matters:** Removing the hard reset removes the *only* code that
handled a corrupt working tree. With the reset gone, the guard commits the
corrupt bytes and `push_with_retry` propagates them to the peer. `read_rows`
(`child_sync.rs:978-990`) then fails to parse HEAD's `transactions.csv` on
*both* machines, and `apply_merge` can no longer compute anything. A single-
machine corruption becomes a two-machine outage — a strictly worse failure than
defect 2, which was one machine losing rows.

**Recommendation:** Before committing, parse-validate. The guard already has
everything it needs: `allowance_core::codec::parse_transactions` is used
identically in `read_rows`. If `transactions.csv` on disk does not parse, do
*not* commit it — raise the durable failure notice instead. This is cheap, it
closes exactly the gap the hard reset used to (badly) cover, and it is the
honest replacement for "discard." Say so explicitly in the spec so the next
reader understands that atomic writes bound *this app's* corruption, not all
corruption.

### Concern 3: The `commit_file_change` change is inert as specified

**What I see:** §"Error handling": "`commit_file_change` (`git/mod.rs:388`)
starts returning its commit error instead of swallowing it with a `warn!`. That
swallow is what strands `parental_control_attempts.csv` dirty."

All five callers discard the result: `transaction_repository.rs:201`,
`allowance_repository.rs:106`, `parental_control_repository.rs:163`,
`child_repository.rs:107`, `goal_repository.rs:100` — every one is
`let _ = self.git_manager.commit_file_change(...)`.

**Why it matters:** Changing the return value changes nothing observable. The
file is still left dirty; the failure is still invisible. The spec presents this
as one of two fixes for defect 1's first route when only the other one (adding
the file to the owned list) actually does the work. That is the kind of
"looks-fixed" change that survives review and then misleads whoever debugs this
next year.

**Recommendation:** Either (a) drop it from scope and say the owned-list
addition is the whole fix, or (b) carry it through: make at least the write
paths surface the failure (a `StartupNotice` or a sync notice), and update all
five call sites. Half of (b) is worse than (a).

### Concern 4: The `parental_control_attempts.csv` exception is less bounded than claimed

**What I see:** §1 asserts "an interrupted append leaves at most a partial
trailing line" and "its reader already tolerates a malformed trailing record."

The reader does not. `parental_control_repository.rs:186-187` is
`for result in csv_reader.records() { let record = result?; ... }` over a default
`Reader::from_reader` — `flexible` is false anywhere in the backend (I checked).
A truncated append produces `UnequalLengths` or `UnterminatedQuote`, and `?`
propagates it, failing the *entire* read. Worse, `get_next_id`
(`:112-121`) has the identical `result?` and runs on every subsequent append —
so one torn line makes the audit log both unreadable *and* unwritable from then
on.

**Why it matters:** The spec records a conscious trade ("bounded corruption of
the last record, rather than none") on a premise that is false. The actual worst
case is total loss of read access to the audit log plus a permanently blocked
append path — which is roughly the outcome the exception was chosen to avoid.
And since the file is joining `FILES_THIS_APP_OWNS`, that torn state now also
gets committed and pushed to the peer.

**Recommendation:** Keep the append (the reasoning for it is sound), but make
the claim true: set `.flexible(true)` on both readers and skip records that
don't have four fields, or replace `result?` with a skip-and-log. Add a test
that a file with a truncated trailing line still reads all prior records and
still yields a correct next id. Then the recorded trade is accurate.

### Concern 5: A third reachable route for defect 1, and a `commit_merge` behaviour change

**What I see:** The spec names two routes to the stall. There is a third, and
it is user-triggerable without any crash or swallowed error:
`AllowanceRepository::delete_allowance_config` (`allowance_repository.rs:189-190`)
calls `std::fs::remove_file` on `allowance_config.yaml` — an owned, tracked file
— with no commit at all. From that moment the tree is permanently dirty, staging
skips the missing file, `Ok(None)`, stall. This strengthens the case for the
fix; it should be named and tested.

Separately, adding the deletion branch to `stage_owned_files` changes
`commit_merge` too (`git/mod.rs:315`), not just the dirty-tree guard. The spec
motivates the branch only for the guard. A merge commit will now record
deletions of owned files, which interacts with the known `goals.csv`
non-merge gap.

**Recommendation:** Add `delete_allowance_config` as a third route and a fourth
regression test. Add an explicit test that a merge commit does *not* record a
`goals.csv` deletion in any normal flow. Also: key the deletion branch on
presence in the *index*, not only in HEAD (`index.get_path`) — a staged-add that
was subsequently deleted from the worktree is in the index but not HEAD and
would otherwise still be missed.

## Questions the spec does not answer

- **Second-parent loss on crash recovery.** After a crash between the atomic
  write and `commit_merge`, the new flow commits the merged content as an
  *ordinary single-parent* commit labelled "commit local changes before applying
  a peer merge." `theirs` never becomes an ancestor. The next cycle re-merges
  from the new tip. I believe this converges (merge dedupes `intrinsic_eq` rows),
  but the spec should state the argument rather than leave it implied — it is the
  price of "same code path as an ordinary MCP write," and it deserves naming.
- **macOS durability.** `File::sync_all()` on APFS issues `fsync`, which returns
  once data reaches the drive's cache. `F_FULLFSYNC` is what survives power loss
  on Apple hardware. Is the claim "process crash safe" or "power-loss safe"? The
  §1 text ("on power loss can still leave a zero-length file") implies the
  latter. Either use `F_FULLFSYNC` or scope the guarantee honestly.
- **Does `Failed` every cycle need a circuit breaker?** `record_sync_failure`
  dedupes by `child_id` (`sync_state.rs:202-204`), so the UI doesn't accumulate —
  but the log and the work do, every tick, indefinitely.
- **Permissions on `rename`.** The temp file gets fresh `0644 & umask`, not the
  replaced file's mode. Intentional, or worth a `set_permissions` before rename?

## What I thought was well-handled

The root-cause paragraph is the best part of this document. Naming "inference
rather than evidence" as the shared defect, and then ordering the four changes
so that the enabling one (atomic writes) precedes the one that depends on it
(removing the reset), is exactly the right structure — it is a coherent design,
not two patches stapled together.

Three other things I want to name so they are not mistaken for "no comment":
the **no-silent-stall invariant test** ("either the tree is clean or a durable
failure notice exists; never neither") is the right shape — a property, not three
examples. The **`parental_control_attempts.csv` exception** is exactly how a
deviation should be recorded: stated, justified, and explicitly fenced against a
future "fix" (the premise needs correcting, but the discipline is right).
And retiring `MERGE_IN_PROGRESS_MARKER` as a deliberate non-goal — "removing
the safety net and its enabling change together would leave nothing to fall back
on" — is the kind of sequencing judgment that usually goes unstated.

Retiring the duplicated staging loop and the "change it in both places"
instruction at `app_coordinator.rs:1264` is overdue. That comment was a standing
liability, and the spec is right to treat its existence as evidence.

## Closing

Ready with adjustments. Concerns 1 and 2 are the ones I would not proceed
without: the backstop needs either a resolution path or an honest admission that
it requires a terminal, and the guard must not commit-and-push bytes it has not
validated. Concerns 3 and 4 are corrections to claims the spec makes that do not
hold — fix the claims or fix the code, but do not ship the current wording.
