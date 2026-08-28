# [2A] Critique — Deiko Deshimaru

**Spec reviewed:** `docs/superpowers/specs/2026-08-28-child-registry-design.md`
**Reviewer:** Deiko Deshimaru, Distinguished Engineer
**Date:** 2026-08-28

---

## Overall Verdict

**Approve with changes.**

The diagnosis is correct and the core move — modelling location as a first-class
machine-local fact rather than deriving it from identity — is the right one. My
concerns are not with the registry. They are with what the registry *implies*
about the rest of the system, which the spec has not followed all the way
through. Three of the five concerns below are data-integrity issues, not
aesthetics.

## Top Concerns

### Concern 1: `child_dir` is a pure lookup, and the write paths below it will fabricate a folder

**What I see:** `pub fn child_dir(&self, child_id: &str) -> Result<PathBuf>; // registry lookup, no I/O` (Design → *`CsvConnection` becomes a resolver*). Every repository is then cut over to it mechanically.

Today, `TransactionRepository::read_transactions` calls
`connection.ensure_transactions_file_exists(...)` first, and that method does
`fs::create_dir_all(&child_dir)` followed by writing a bare CSV header
(`connection.rs:118-135`). Under the scan-based world this was harmless: a child
whose folder was gone was never discovered, so nothing ever asked for its path.
Under the registry the child is *known* whether or not the folder is there.

**Why it matters:** Concretely. Machine B, iCloud Drive signed out or the account
re-provisioned. `~/Library/Mobile Documents/com~apple~CloudDocs` still exists as
a directory; the `keiko_hart` folder under it does not. The registry still names
that path. `check_and_issue_pending_allowances` runs at startup
(`app_state.rs:122`), resolves the active child, calls into
`read_transactions_by_id` → `ensure_transactions_file_exists` → `create_dir_all`
recreates the whole path, writes an empty `transactions.csv`. Balance now reads
zero. `get_pending_allowance_dates` looks back 90 days and finds every one of
them pending. The app issues up to 13 weeks of allowances against a phantom zero
balance, git-commits them, and the sync thread pushes every one to DynamoDB,
where Machine A picks them up as real.

The registry converts "child not found" — a safe, self-limiting failure — into
"child found at a path we will happily create." That is a meaningful loss of a
safety property, and the spec's availability model does not protect against it,
because the availability model lives in the *UI roster* while the damage happens
in the *storage layer*.

**Recommendation:** `child_dir` must not be the only gate.

- Either make `child_dir` do one `stat` of `child.yaml` and return
  `Err(Unavailable)` when it is missing (a `stat` does not materialize a dataless
  file — the spec establishes this itself), or split the API into
  `child_dir(&self, id)` for reads and `child_dir_for_write(&self, id)` that
  requires proof of availability.
- Remove `create_dir_all` from `ensure_transactions_file_exists`. Creating a
  child folder is a registration-time act, not a side effect of a read.
- Add a test: a registered child whose folder has been deleted causes every read
  and write path to error, and creates nothing on disk.

### Concern 2: the "no remote bootstrap" boundary does not hold once a child is registered

**What I see:** Non-goals: *"iCloud carries the data; sync-service stays a live-updates channel between machines that already have the child registered."*

But registration is exactly what makes a child eligible for polling.
`poll_remote` asks the UI thread for local child IDs
(`sync_thread.rs:272-290`), the handler answers from `list_children`
(`app_coordinator.rs:416-425`), and `poll_child` reads
`*self.watermarks.get(child_id).unwrap_or(&0)` (`sync_manager.rs:178`). A
newly registered child has no watermark, so the first poll is
`get_events_since(child_id, 0)` — the **entire remote event history**. And since
the client never sends `x-sync-source`, the service stamps every pushed event
`SyncSource::Remote` (`sync-service/src/routes/entities.rs:30-33`), so the
`source == Local` skip in `poll_child` filters nothing. Nothing is elided.

**Why it matters:** The user clicks *Add existing child…* on the new Mac at
14:00:00. The roster worker begins prefetching a cold multi-megabyte folder from
iCloud. Within 30 seconds the sync thread polls, gets the full history, and
starts calling `upsert_transaction_from_sync`, which does a read-modify-write of
the *whole* `transactions.csv`. Two independent writers — `fileproviderd`
materializing the file and the app rewriting it — race on the same inode. The
benign outcome is an iCloud "conflicted copy". The malign one is the app winning
with a partial view and the real history being uploaded away.

So remote bootstrap is not out of scope. It happens by accident, at the worst
possible moment, without being designed. Declaring it a non-goal does not make
it not occur; it only means nobody chose the semantics.

**Recommendation:**

- Gate the `GetChildIdsRequest` answer on roster status: only children in
  `Available` are polled. A `Downloading` or `Unavailable` child must not be
  polled and must not be pushed. This is a small change and it is the load-bearing
  one.
- State the watermark policy for a newly registered child explicitly. If the
  intent really is "iCloud carries the data," then registration should seed the
  watermark from the remote's current max sequence, so the replay is skipped, and
  a full replay becomes an explicit user action ("Resync from remote"). If the
  intent is to allow the replay, say so, and say that it must not begin until
  the folder is `Available`.

### Concern 3: startup allowance issuance runs on the main thread before the roster exists

**What I see:** *"Render paths must never touch the filesystem."* But
`AllowanceTrackerApp::new` calls `check_and_issue_pending_allowances()` at
`app_state.rs:122`, before the roster worker is even conceived of in this design,
and `app_coordinator.rs:642` calls it again later.

**Why it matters:** This is a synchronous read of `allowance_config.yaml` and
`transactions.csv` for the active child, followed by writes, executed before the
first frame. On a cold iCloud folder it blocks; offline it blocks until
`fileproviderd` gives up. The user sees a bouncing Dock icon and no window at
all — strictly worse than the mid-frame freeze the spec is trying to eliminate,
because there is not even a "Downloading from iCloud…" label to look at.

**Recommendation:** Sequence it. Allowance issuance is triggered by the roster
reporting `Available` for the active child, not by `App::new`. It is a natural
consumer of the same `mpsc` completion message the roster already sends.

### Concern 4: the prefetch list is incomplete, and `.git` is the part that will hurt

**What I see:** Step 3 prefetches four files: `child.yaml`,
`allowance_config.yaml`, `transactions.csv`, `goals.csv`, and concludes that
"by the time a child is selectable, every synchronous repository read downstream
hits a warm local file."

That is not true. A child folder also contains
`parental_control_attempts.csv` (`parental_control_repository.rs:85-96` — the
spec lists this file as machine-local base-dir state in the `children.yaml`
section, but there is both a global one *and* a per-child one), and it contains
a full `.git`. Every transaction, goal, and profile write calls
`GitManager::commit_file_change` (`git/mod.rs:164-201`), which runs
`add` / `status` / `commit` against the object store — synchronously, on the UI
thread. On a freshly added iCloud child that pages in the entire object database
mid-frame. The freeze does not disappear; it relocates to the first write.

Separately, and larger: this design is what makes two machines share one
iCloud-hosted `.git` for the first time. Concurrent commits from two machines
into one file-synced repository produce `index.lock` collisions, conflicted-copy
files inside `.git/objects`, and divergent refs with no merge. The spec mentions
`.git` only as history "worth preserving" in the redirect stubs. It does not ask
what happens to the live one.

**Recommendation:** Add `parental_control_attempts.csv` to the prefetch. Then
take a position on `.git` and write it down — prefetch it, or disable git
commits for children whose folder is in a file-sync container, or accept the
corruption risk explicitly. Any of the three is defensible; silence is not.

### Concern 5: registry lifecycle is not tied to child lifecycle, and the ownership model is unstated

**What I see:** `ChildRegistry` exposes `register` / `deregister` / `repoint`,
all `&mut self`. Nothing in the spec says who calls `deregister`.

Two gaps follow. First, `ChildRepository::delete_child` does
`fs::remove_dir_all(&child_dir)` (`child_repository.rs:251-262`) and never
touches the registry — a deleted child leaves a permanent `PathMissing` entry.
Worse, `delete_local_entity` for `EntityType::Child`
(`app_coordinator.rs:613-617`) routes a *remote* delete into that same call, so
deleting a child on Machine A now issues `remove_dir_all` against the shared
iCloud folder on Machine B. Under the old redirect model this was already
possible; under the registry it is reachable through a normal UI path on a
second machine, which is new.

Second, `&mut self` does not compose with the surrounding code.
`CsvConnection` is `Clone` and is held by six repositories, some by value and
some behind `Arc`; it keeps its one piece of mutable state as
`Arc<Mutex<PathBuf>>` for exactly this reason (`connection.rs:13`). The registry
needs the same treatment — `Arc<RwLock<ChildRegistry>>` — and the roster worker
needs a defined snapshot discipline. The Testing section asserts "a registry
mutation mid-load does not produce a stale roster" but the Design section
proposes no mechanism that would make that true; a generation counter carried on
the worker's result messages is the usual answer.

Third, a smaller ordering question falls out: `store_child` will now resolve
through `child_dir`, so **Create new child…** must register the entry *before*
writing `child.yaml`, or `child_dir` must have a documented escape hatch for
creation. The spec does not say which.

**Recommendation:** Add a short "registry and child lifecycle" subsection that
states the ordering for create (mkdir → register → write `child.yaml`), for
local delete (deregister → remove folder), and for remote-driven delete
(deregister only — do not `remove_dir_all` a shared iCloud folder from a
sync event). Specify `Arc<RwLock<_>>` and the roster generation counter.

## Questions the spec does not answer

- Who calls `deregister`, on local delete and on a remote `Child` delete event?
- What watermark does a newly registered child start with, and is polling gated
  on `Available`?
- What is the create-child ordering now that `store_child` resolves through the
  registry?
- What is the position on two machines committing into one iCloud-hosted `.git`?
- After a sync-applied rename arrives for a child, who refreshes the cached
  `label`? The roster is only rebuilt "whenever the registry changes," and a
  rename does not change the registry.
- Downgrade: a `version: 2` registry read by a `version: 1` binary refuses to
  load and shows an empty roster. If the user then re-adds children, does the
  newer binary see duplicates?

## What I thought was well-handled

The diagnosis is the strongest part of this document. "Location was modelled as
a property derived from identity" is exactly right, and the three-resolvers
table with the latent rename bug is the kind of finding that justifies a
refactor on its own. Pinning it with a regression test that fails on `main` is
the correct discipline.

The dataless-file research is real research, and `stat` before `read` is the
right primitive — no FFI, no `brctl`, no new dependency. Refusing to drop an
unloadable entry, and refusing to silently dedup a duplicate id, are both the
correct calls: this app's failure mode has always been silence, and the spec
names that. The `FileAvailability` trait is the right seam, and migration
reading the id from `child.yaml` rather than inferring it from the folder name
is the detail that most designs get wrong.

The sequencing is good, and phase 2 being observable-but-inert is a genuinely
useful safety property. One reassurance on the phase 3 risk: the resolver call
sites are confined to ten files, all but two of them in
`backend/storage/csv/`. The blast radius is smaller than the Risks section
fears.

Leaving the redirect stubs on disk is the right conservative choice.

## Closing

The registry itself is sound and I would not change its shape. What needs
another pass is the perimeter: the storage layer must not be able to fabricate a
child folder from a registry entry, sync must not poll a child that is not yet
`Available`, and startup allowance issuance must move behind the roster. Address
concerns 1–3 and this is ready to implement; 4 and 5 can be resolved with a
paragraph each rather than a redesign.
