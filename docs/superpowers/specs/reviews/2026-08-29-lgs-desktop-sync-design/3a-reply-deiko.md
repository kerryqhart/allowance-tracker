# [3a] Reply to Deiko Deshimaru's Critique

**Spec:** `docs/superpowers/specs/2026-08-29-lgs-desktop-sync-design.md`
**Reviewer addressed:** Deiko Deshimaru
**Reply date:** 2026-09-06

---

## Overall Response

All six concerns accepted, four of them verbatim. Concern 1 is the most valuable
finding in the review round — it was verified against lgs source before revision
and it is correct: the design as written would have stalled silently the first
time both Macs were edited in one window. Concern 6 identified a logical trap in
a decision the author had already made and approved, which is the hardest kind of
finding to land.

## Point-by-Point

### Concern 1: The pull will not see the other machine's commits

**Verdict:** Accepted.

**Our response:** Verified independently before accepting. `engine.rs:399`
documents `reconcile` as "Never moves a head backward or over a divergence", the
`refs/lgs-auth/heads/*` mirror exists (`engine.rs:20`), and a test comment at
`engine.rs:825` states its intended use outright — "authoritative main is
available to merge under `refs/lgs-auth/`".

The spec's *Pull* section now names the refspec explicitly
(`+refs/lgs-auth/heads/*:refs/remotes/lgs-auth/*` alongside `refs/heads/*`) and
states that the merge input is `refs/remotes/lgs-auth/main`. The testing section
now requires a two-bare integration test reproducing the diverged case, with your
reasoning recorded: a plain bare repo neither accepts nor rejects the way lgs
does, so the original convergence test would have passed and still missed this.

### Concern 2: "No command-line dependencies at all" is false, and `install-service` does not start anything

**Verdict:** Accepted, all three parts.

**Our response:** All verified — `Command::new("git")` throughout
`repo/mod.rs:25,41,66`; `cli.rs:1034-1053` writes the plist and *prints*
`launchctl` instructions; `service.rs:129` uses `current_exe()`.

The claim is retracted from the spec. `git` is now declared a runtime
prerequisite with detection and a human-readable message when absent. The app
performs `launchctl bootstrap` / `kickstart` itself rather than relying on
`install-service` to start anything. The bundled binary is copied out to
`~/Library/Application Support/Allowance Tracker/bin/lgs` and the plist points
there, so the daemon survives the app being moved, renamed, or updated.

We did not take the "bundle a git" option, per your own recommendation against it.

### Concern 3: Symmetric merge does not give convergence without a canonical serialization

**Verdict:** Accepted, and extended.

**Our response:** Canonical total order `(date, id)` is now applied on **every
write**, not only after a merge, and the balance accumulation uses the same
order. The convergence property is restated over bytes.

Ted reached the same conclusion by a different route and added two properties we
also adopted (idempotence and fixed-point). Your specific point that the
*recompute* is order-dependent, and that `validate_all_balances` passes on both
machines while they hold different money, is called out in the spec as the reason
the tiebreak is not cosmetic.

### Concern 4: `balance` is derived state stored per row, and it will manufacture conflicts

**Verdict:** Accepted.

**Our response:** `balance` is excluded from conflict comparison entirely; rows
compare on intrinsic fields only (`id`, `child_id`, `date`, `description`,
`amount`, `type`) and `balance` is treated as regenerated output.

Your closing observation — that with `balance` excluded, genuine `changed/changed`
conflicts become rare, which is what makes the crude rule acceptable — has been
written into the spec as the actual justification for the rule. You were right
that the argument was available and unmade.

The spec now also states which commit's timestamp the rule reads. Ted raised the
same ambiguity from the testing side; the resolution is that provenance is passed
into the merge explicitly by the caller, so the rule's input is defined and the
merge stays pure. The spec acknowledges plainly that this is clock-skew-sensitive
last-writer-wins at branch granularity.

### Concern 5: AWS is a second writer, treated as a race rather than a topology

**Verdict:** Accepted.

**Our response:** A "two transports over one dataset" section has been added
naming write amplification and the resurrection channel. Git commits are
suppressed on the `ApplyRemoteEntity` path and `SyncNotifier` emission is
suppressed during merge-driven recalculation (the `with_sync_notifier(None)`
builder already exists).

On locking: accepted, but resolved differently than proposed. Rather than a
per-child mutex, all working-tree mutation moves to the UI thread — the
background thread does `fetch` and `push` only, which touch `.git` and never the
working tree. That preserves the existing "UI owns all repo I/O" invariant rather
than adding a second concurrency rule. Your underlying objection stands and is
addressed: `write_transactions_internal` truncates before writing, so no
concurrent reader can observe it.

### Concern 6: The spec requires changes in two repositories but scopes only one

**Verdict:** Accepted.

**Our response:** This was the finding that changed a decision rather than a
paragraph. The trap you identified is real: "adopt, don't reinstall" plus a
bundled CLI that advances every release means `outdated` becomes the steady state,
and the spec's own rule against claiming backed-up while `outdated` holds would
have meant the app never reports a project as backed up.

Resolution, approved by the author: the app records whether **it** installed the
daemon, and upgrades only a daemon it owns. A pre-existing daemon is adopted and
never touched. A minimum-version handshake with a documented floor covers the
adopted case.

lgs-side work is now named as an explicit dependency: idempotent
`install-service`, and a defined upgrade path for an app-installed daemon.

The degraded mode is now claimed explicitly, as you suggested — with no daemon,
commits still land locally and the app is fully usable; only cross-machine
replication stalls.

## Questions Answered

### Q: What happens when the daemon's port (8418) is taken?

A: The remote URL is re-resolved from `lgs status --json` on startup and
`.git/config` reconciled, rather than trusting a URL written at migration time.
Added to the spec. Greg raised this independently.

### Q: Does the statically linked libgit2 in this build have smart-HTTP push enabled?

A: Unverified, and now a **blocking spike before planning**. The build declares
`git2 = { version = "0.19", default-features = false }`
(`egui-frontend/Cargo.toml:48`). Greg raised the same question; the spec now
records it as a spike whose result must be written back before implementation.

### Q: What does the app do with a push interrupted by `lgs restart`?

A: Covered by the existing retry story — an unpushed commit is durable and the
next cycle carries it. Now stated explicitly in the spec rather than implied.

### Q: What becomes of the `Availability` / `Downloading` model?

A: Narrowed, not retired. The dataless branch stays reachable during migration
while both regimes coexist, and is removed once no child resolves to a
cloud-drive path. Added to the spec.

### Q: Two-machine migration ordering — should the second machine adopt rather than `git init`?

A: Yes. Accepted and specified: migration checks `lgs projects --json` for an
existing `allowance-<child_id>` and adopts it rather than creating an unrelated
root. This closes the resurrection window you described.

### Q: The app already has a conflict UI. Two conflict models will coexist?

A: The AWS `ConflictDetected` path is left in place untouched — it belongs to the
deferred AWS spec. The spec now says "no conflict UI *for git merges*" rather
than making a global claim.

## Closing

Concern 1 alone justified the review. Nothing from your critique is left open.
The revised spec carries a change appendix attributing each change to its source.
