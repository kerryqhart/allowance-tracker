# 3.1 Consolidate TransactionMapper Design

**Date:** 2025-02-04
**Effort:** ~15 minutes

---

## Problem

`TransactionMapper::to_dto()` is duplicated in 3 places with identical logic:
- `backend/domain/mappers.rs` - canonical implementation
- `backend/domain/export_service.rs:20-38` - duplicate
- `backend/domain/calendar.rs:20-38` - duplicate

## Solution

1. Delete duplicate `TransactionMapper` structs from `export_service.rs` and `calendar.rs`
2. Import `transaction_to_dto` from `backend/domain/mappers`
3. Update call sites to use the imported function

## Files Changed

- `backend/domain/export_service.rs` - delete struct, add import, update call
- `backend/domain/calendar.rs` - delete struct, add import, update call

## No Changes Needed

- `backend/domain/mappers.rs` - already canonical
- `egui-frontend/src/ui/mappers.rs` - already delegates to canonical
