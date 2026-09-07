# [2a] Critique — Deiko Deshimaru

**Spec reviewed:** `docs/superpowers/specs/2026-08-29-lgs-desktop-sync-design.md`
**Reviewer:** Deiko Deshimaru, Distinguished Engineer
**Date:** 2026-08-29

---

## Overall Verdict

**Needs rework** — but on a small number of specific, fixable points. The
architecture is right. Two of the load-bearing mechanical claims are not.

The core judgment — that iCloud-as-transport over a live `.git` is the actual
bug, that AWS was never the desktop-to-desktop path, and that a semantic merge
is mandatory because `balance` is a stored running total — is correct and well
argued. I want this design to ship. The concerns below are about the parts where
the spec asserts a mechanism it has not verified against lgs's source.

## Top Concerns

### Concern 1: The pull will not see the other machine's commits

**What I see:** "Sync lifecycle → Pull: triggered on startup, on window focus,
and on a 30-second timer. After fetching: up to date / fast-forward / diverged."
The spec never names a refspec.

**Why it matters:** `local-git-sync/src/durability/engine.rs:400` (`reconcile`)
is documented as *"Never moves a head backward or over a divergence."* Trace the
concurrent case. Machine A commits `a1`, pushes to A's bare, the daemon
publishes it. Machine B has committed `b1` to B's bare. B's daemon reconciles:
authoritative is `a1`, local `refs/heads/main` is `b1`, neither is an ancestor of
the other → the branch is pushed onto `diverged` and **`refs/heads/main` in B's
bare is left at `b1`**.

So `git fetch lgs` — which fetches `refs/heads/*` — returns *B's own tip*. The
app classifies "up to date." The divergence is invisible. Machine B never merges,
`advance_once` (`engine.rs:203-210`) classifies the branch `Diverged` and refuses
to publish forever, and both machines sit in a permanent stall that neither the
app nor the user can see. This is precisely the eternal-inconsistency outcome the
spec set out to avoid.

The good news is that lgs already solved this: `reconcile` writes the
authoritative tip to `refs/lgs-auth/heads/<branch>` **before** the divergence
check (`engine.rs:449-451`), and `import_bundles` has already brought the objects
in. The peer's tip is sitting in the bare under a namespace the spec does not
mention.

**Recommendation:** State the refspec explicitly. The pull is
`+refs/lgs-auth/heads/*:refs/remotes/lgs-auth/*` (plus `refs/heads/*` to detect
the simple fast-forward case), and the merge input is
`refs/remotes/lgs-auth/main`, not `refs/remotes/lgs/main`. Add a two-bare
integration test that reproduces the diverged case end to end — the current
"convergence" test as described would pass against a plain bare and still miss
this, because a plain bare accepts nothing and rejects nothing the way lgs does.

### Concern 2: "No command-line dependencies at all" is false, and `install-service` does not start anything

**What I see:** two claims in *lgs integration*: "With a bundled lgs, the app has
no command-line dependencies at all," and "No daemon → run `lgs install-service`
(launchd, starts at login)."

**Why it matters:** lgs is a `git`-CLI wrapper. `src/repo/mod.rs:25,41,66`,
`durability/bundles.rs:106`, `durability/manifest.rs:293`,
`durability/stranded.rs:53`, `daemon/sync.rs:414` and `cli.rs:724` are all bare
`Command::new("git")`, and the smart-HTTP endpoint spawns `git http-backend`
(`daemon/server.rs:223-224`). On a Mac without Xcode Command Line Tools,
`/usr/bin/git` is the stub that raises a GUI dialog and exits non-zero. The Xcode
CLT prerequisite the spec says bundling removes is not removed — it is relocated
from the app to the daemon, where its failure is less visible.

Second, `cli::install_service` (`cli.rs:1034-1053`) writes the plist and then
**prints** `launchctl bootstrap` / `launchctl kickstart` instructions. Nothing is
loaded and nothing runs until the next login. A non-technical user who launches
the app for the first time gets a plist and no daemon, which means no clone URL,
which means no sync — and the remedy the CLI offers is a terminal command. That
defeats the no-terminal goal at the first step of first run.

Third, `service::install()` uses `std::env::current_exe()` (`service.rs:129`), so
the plist's `ProgramArguments` will point *inside the .app bundle*. Move the app
to `/Applications` after first run, rename it, or delete it, and launchd retries a
missing path forever under `KeepAlive`. Sync stops with no signal.

**Recommendation:** Say plainly that `git` is a runtime prerequisite and specify
detection plus a human-readable message when it is absent (or bundle a git, which
I would not do). Decide who bootstraps the service: either the app runs
`launchctl bootstrap gui/<uid>` itself — one more CLI dependency, but a
controllable one — or lgs grows an `install-service --load`. Copy the `lgs`
binary out of the bundle to a stable path (`~/Library/Application Support/Allowance
Tracker/bin/lgs`) and point the plist there, so the daemon survives the app
moving.

### Concern 3: Symmetric merge does not give you convergence without a canonical serialization

**What I see:** the property `merge(A,B) == merge(B,A)`, and the convergence test
"assert identical trees."

**Why it matters:** the merge output is not what lands on disk. What lands is
whatever `write_transactions_internal`
(`backend/storage/csv/transaction_repository.rs:133-168`) writes, and it writes
the `Vec` in whatever order it holds — `store_transaction` (`:229`) *pushes to the
end*. File order is insertion order, not sorted. Two machines that agree on the
row *set* can still produce byte-different CSVs, different tree hashes, a fresh
divergence on the next cycle, and a merge-commit ping-pong that never terminates.

Worse, the balance recomputation is order-dependent.
`validate_all_balances` (`balance_service.rs:216`) and
`recalculate_balances_from_date` (`:42`) both accumulate over
`sort_by(|a, b| a.date.cmp(&b.date))` — a sort with **no tiebreak**. Two
transactions with the same `date` can be ordered either way, each machine assigns
different `balance` values to them, and `validate_all_balances` passes on both.
Same rows, different money, and both machines think they are right.

**Recommendation:** Define a total order — `(date, id)` — and make it the
canonical on-disk row order, applied on every write, not just after a merge.
Make the balance accumulation use the same total order. Then state the
convergence property as *byte-identical `transactions.csv`*, which is the thing
that actually terminates the loop, and property-test the sort tiebreak
independently.

### Concern 4: `balance` is derived state stored per row, and it will manufacture conflicts

**What I see:** the row-merge table treats a row as `changed` or `unchanged`
against the base, and separately says "after the row merge, recompute balances."

**Why it matters:** a non-tail insert on machine A rewrites the `balance` column
of *every subsequent row*. Against the merge base, all of those rows read
`changed`. If machine B also inserted, the table's `changed / changed` case fires
for most of the file, and the resolution rule discards one side's rows wholesale
— including rows where the only difference was a derived column that is about to
be recomputed anyway.

The resolution rule itself has no per-row provenance to work from. "Later
committer timestamp wins" needs to know *which commit last touched this row*, and
because every write rewrites the whole file, `git blame` attributes every row to
the most recent commit. In practice the rule degenerates to "the branch with the
later tip wins every conflicting row," which is file-level last-writer-wins
dressed as a row merge — and it is decided by two unsynchronized wall clocks. A
Mac 20 seconds fast wins every contested row on that repo, permanently.

**Recommendation:** Exclude `balance` from the conflict comparison entirely.
Compare rows on their intrinsic fields only — `id`, `child_id`, `date`,
`description`, `amount`, `type` — and treat `balance` as regenerated output. Then
state explicitly which commit's timestamp the rule reads (branch tip, I assume)
and acknowledge in the spec that this is clock-skew-sensitive last-writer-wins at
branch granularity, not per-row. Note also that with `balance` excluded, genuine
`changed/changed` conflicts become rare, which makes the crude rule acceptable —
that argument is available to you, it just is not made.

### Concern 5: AWS is a second writer into the same git repos, and the spec treats it as a race rather than a topology

**What I see:** "The AWS sync writes the same CSVs… `ApplyRemoteEntity` and a git
merge can race on one file. They serialize through the same path."

**Why it matters:** serialization is necessary and not sufficient. Three
consequences the spec does not account for:

1. `ApplyRemoteEntity` runs on the UI thread
   (`egui-frontend/src/ui/app_coordinator.rs:459`) and writes through the
   repositories, which call `commit_file_change`. One MCP write therefore
   produces an *independent commit on each machine* for the same logical change.
   Divergence is not the exception case — it is the steady state whenever the MCP
   server is active.
2. `recalculate_balances_from_date` emits an `Updated` `SyncEvent` per changed row
   (`balance_service.rs:85-96`). A merge that ingests 40 remote rows and
   rebalances will push 40 events at AWS. Write amplification, on a path the spec
   is deliberately not fixing.
3. **Resurrection.** A row deleted on machine A and correctly dropped by the git
   merge on machine B can be re-created on both machines by an AWS replay, then
   committed, then pushed — and it now has no base entry, so the next merge reads
   it as an add and keeps it. A deleted transaction comes back and stays.

**Recommendation:** Add a short "two transports over one dataset" section that
names the amplification and the resurrection channel and says which one wins.
The cheapest honest answer is probably to suppress git commits on the
`ApplyRemoteEntity` path (let the merge/recompute produce one commit) and to
suppress `SyncNotifier` emission during merge-driven recalculation. Also take a
real lock: a per-child mutex held across read-modify-write, not an ordering
convention — `write_transactions_internal` truncates before writing, so a
concurrent reader sees a truncated file, and `git checkout` on the fast-forward
path rewrites the working tree with no coordination at all.

### Concern 6: The spec requires changes in two repositories but scopes only one

**What I see:** the component-boundary table lists five units, all in
allowance-tracker. The lgs side appears only as "an already-installed service
must make `install-service` a detected no-op."

**Why it matters:** that sentence is a requirement lgs does not satisfy —
`service::install()` unconditionally overwrites the plist, and the launchd label
is shared, so the app would silently repoint an existing cargo-installed daemon at
the binary in its bundle. Combined with "adopt, don't reinstall," the daemon is
then *never upgraded*: every app release ships a newer bundled CLI against a
daemon that keeps running the old binary, and the `outdated` state the spec plans
to relay verbatim becomes the permanent condition rather than the exception. The
spec's own rule ("do not claim a project is backed up while `outdated` holds")
would then mean the app never reports a project as backed up.

**Recommendation:** Name the lgs-side work as an explicit dependency with its own
sequencing: idempotent `install-service`, a version handshake with a documented
minimum, and a defined upgrade path for a daemon the app installed. Then state
the degraded mode honestly — with no daemon, commits still land locally and the
app remains fully usable; only cross-machine replication stalls. That is a good
property and the spec should claim it.

## Questions the spec does not answer

- What happens when the daemon's port (8418) is taken? `clone_url` is
  `http://localhost:{port}/{name}.git`, so the remote URL is port-dependent and
  can change under the app. Is the remote re-resolved from `status --json` on
  every cycle, or written once at migration and left to go stale?
- Does the statically linked libgit2 in this build have smart-HTTP **push**
  (`receive-pack`) enabled? The whole transport depends on it and it is not
  verified anywhere in the spec.
- `lgs restart` "interrupts any `git push` or `git fetch` in flight"
  (`cli.rs:1076-1083`). What does the app do with a push that fails mid-stream
  during a daemon restart — and does the retry-on-next-cycle story cover it?
- What becomes of the `Availability` / `Downloading` model
  (`backend/domain/child_availability.rs`)? Once children live in Application
  Support the dataless branch is unreachable, but during migration both regimes
  coexist. Retained, retired, or narrowed?
- Two-machine migration ordering: machine A migrates on Monday, machine B on
  Friday. A's writes go to lgs, B's go to the old iCloud folder, and B's
  migration then creates an unrelated root. The empty-base union keeps the rows,
  but any row A deleted in that window resurrects. Should the second machine
  check `lgs projects --json` for an existing `allowance-<child_id>` and adopt
  rather than `git init`?
- The app already has a conflict UI (`SyncMessage::ConflictDetected`,
  `sync.conflicts`). The spec says "no conflict UI." Two conflict models will
  coexist — is the AWS one left in place untouched?

## What I thought was well-handled

The reframing is the most valuable thing in this document. Overturning a
diagnosis you had already written down, on the grounds that its premise about
what AWS was *for* was wrong, is the hard kind of correction to make.

Specific things done well: the argument that textual merge is disqualified by the
stored running balance is airtight and correctly identifies where the money can
go silently wrong. The symmetry requirement and the rejection of "prefer ours" is
exactly the right instinct — permanent divergence is the failure mode I look for
first, and you found it before I did. The empty-base union for the
independent-migration case is correct and its justification ("nothing can be
shown to have been deleted") is the right reasoning. Copy-then-repoint with the
registry written last, and never deleting the old folder, is the correct safety
shape. Keeping `merge` I/O-free so it is exhaustively testable is the right
boundary in the right place. And recording that the cold-start path inherits an
unproven assumption from lgs's own un-run acceptance checklist is the kind of
honesty most specs omit.

The guard against registering a repo inside a cloud-synced path, and detecting
Desktop-&-Documents sync by symlink rather than the Finder pref, is a small
detail that shows the failure mode was actually understood rather than
paraphrased.

## Closing

Fix Concern 1 and this design works; leave it and it stalls silently the first
time both Macs are edited in the same window. Concerns 2 and 3 are the difference
between a design that is true and one that reads as true. Concerns 4–6 want
paragraphs, not redesigns. Revise and I expect to approve.
