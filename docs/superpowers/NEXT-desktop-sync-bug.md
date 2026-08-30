# Handoff: desktop-to-desktop sync does not propagate

**Found:** 2026-08-28, while verifying an unrelated reviewer claim during the child-registry work.
**Status:** diagnosed, not fixed. Needs its own brainstorm → spec → plan cycle.
**Related:** `docs/superpowers/specs/2026-08-28-child-registry-design.md` (Deferred work section).

## The bug

Two Macs running this app never see each other's edits. Only the MCP server's
writes ever reach a desktop.

## The trace

```
shared/src/sync.rs:57      enum SyncSource { Local, Remote }     ← two values, no device identity
http_remote.rs:64,104      .header("X-Sync-Source", "local")     ← every desktop stamps its own writes
entities.rs:30-33          Some("local") => Local, _ => Remote   ← server records that verbatim
sync_manager.rs:190        if event.source == Local { continue } ← every desktop then skips them
sync_manager.rs:182-185    max_sequence updated BEFORE the skip  ← watermark advances anyway
```

`Local` means "a desktop app wrote this", not "I wrote this". There is no
device or installation identity anywhere in the protocol. So Machine B skips
100% of Machine A's events — and because the watermark advances past them
first, they can never be re-fetched.

Writes that OMIT the header default to `Remote` and do propagate. That is the
MCP server, which is why MCP-added expenses show up and desktop edits do not.

## Why it went unnoticed

Only one machine has ever run this app. Setting up the second is what would
have surfaced it.

## It contradicts the original design

`docs/superpowers/specs/2026-04-17-bidirectional-sync-design.md:91-108` — the
pull flow has NO source filter at all; echo suppression was supposed to come
from `event_id` dedup at step 6. The `Local` skip is a later addition that does
not fit a two-value enum.

## Fix shape (not yet designed)

Give the protocol a device/installation id. Skip on `origin_device == me`
rather than on a global "was a desktop" flag. Open questions for the brainstorm:
where the device id is minted and persisted; whether existing events need
backfilling or the watermark can simply be reset; and whether the watermark
should advance past a skipped event at all.

## Do not start here

Brainstorm it properly — the watermark-advance-before-skip interaction is the
part most likely to bite, and a wrong fix silently loses events rather than
failing loudly.
