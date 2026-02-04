# Batch A: Correctness Fixes Design

**Date:** 2025-02-04
**Scope:** 4 correctness fixes that prevent silent failures or incorrect behavior

---

## Overview

| Item | Issue | Locations | Fix |
|------|-------|-----------|-----|
| 2.1 | Hardcoded EST ignores daylight saving | 2 files | Use `chrono::Local` |
| 2.2 | `.unwrap_or_default()` silently drops errors | 5 production occurrences | Log warning and skip malformed records |
| 2.3 | `.lock().unwrap()` panics on poisoned lock | 8 occurrences | Use `.unwrap_or_else(\|e\| e.into_inner())` |
| 2.4 | Path sanitization has no traversal protection | 1 file | Add `..` detection and canonicalization |

**Total:** 5 files modified, ~30 line changes, no API changes, no new dependencies

---

## 2.1 Timezone Fix

**Problem:** Code hardcodes EST (UTC-5), which is wrong during daylight saving time (EDT is UTC-4).

**Current:**
```rust
let eastern_offset = chrono::FixedOffset::west_opt(5 * 3600).unwrap(); // EST (UTC-5)
```

**Fix:** Use `chrono::Local` for system timezone with automatic DST handling.

**Files:**
- `backend/domain/transaction_service.rs:82`
- `backend/storage/csv/transaction_repository.rs:88`

---

## 2.2 unwrap_or_default Fix

**Problem:** Silently returns empty/zero values when errors occur, masking data issues.

**Safe to keep (legitimate "empty if missing"):**
- `calendar.rs:303` - empty transaction list for day
- `data_directory_service.rs:307,374,535` - file_name() OsStr
- `connection.rs:280,575` - file_name() pattern

**Need fixing (mask real errors in transaction_repository.rs):**
- Line 279: amount parse
- Line 340: balance parse
- Line 345: description
- Line 358: transaction_type
- Line 372: source

**Fix:** Log warning and skip malformed records rather than using bad data.

---

## 2.3 lock().unwrap() Fix

**Problem:** Panics if mutex is poisoned.

**Fix:** Use `.unwrap_or_else(|e| e.into_inner())` to recover data from poisoned lock.

**Files:**
- `backend/storage/csv/connection.rs`: lines 75, 139, 145, 171, 500, 549
- `backend/domain/calendar.rs`: lines 451, 463

**Rationale:** In single-user desktop app, poisoned lock data is almost certainly valid. Recovering is better than crashing.

---

## 2.4 Path Sanitization Fix

**Problem:** No protection against path traversal (`../`).

**Fix:** Add traversal detection and canonicalization:
- Reject paths containing `..`
- Canonicalize to resolve any sneaky traversal
- Change return type to `Result<String, ExportError>`

**File:** `backend/domain/export_service.rs`

---

## Testing Strategy

- Existing tests should pass (timezone change only affects DST periods)
- Add test for `..` rejection in path sanitization
- Add test for valid path canonicalization
