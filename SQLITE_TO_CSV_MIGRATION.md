# SQLite to CSV Migration Investigation Report

**Date:** January 21, 2025  
**Goal:** Migrate from hybrid architecture (SQLite + CSV) to pure CSV architecture  
**Data Location:** All data should reside in `~/Documents/Allowance Tracker` only

## Problem Statement

The application currently uses a **hybrid architecture** where:
- **Children, Allowances, Parental Control** → SQLite database (`src-tauri/keyvalue.db`)
- **Transactions, Balance** → CSV files in Documents folder

When user deleted `src-tauri/data/` folder, children and allowances still appeared because they're loaded from the SQLite database, not the Documents folder.

## Current Hybrid Architecture Analysis

### SQLite-Based Services (Using `DbConnection`)
❌ **These currently use the SQLite database (`keyvalue.db`):**

1. **ChildService** - Children data stored in database
2. **AllowanceService** - Allowance configs stored in database  
3. **ParentalControlService** - Parental control attempts stored in database

### CSV-Based Services (Using `CsvConnection`)
✅ **These currently use CSV files in Documents folder:**

1. **TransactionService** - Transaction data stored in CSV files
2. **BalanceService** - Balance calculations based on CSV data

## File Organization Inventory

### Files That Will Be Moved to `storage/sqlite/`

**From `storage/`:**
- `connection.rs` → `sqlite/connection.rs` 
- `db.rs` → `sqlite/db.rs`

**From `storage/repositories/`:**
- `transaction_repository.rs` → `sqlite/repositories/transaction_repository.rs`
- `child_repository.rs` → `sqlite/repositories/child_repository.rs`
- `allowance_repository.rs` → `sqlite/repositories/allowance_repository.rs`
- `parental_control_repository.rs` → `sqlite/repositories/parental_control_repository.rs`
- `mod.rs` → `sqlite/repositories/mod.rs`

## Import Dependencies That Will Break

### Domain Layer Files Using SQLite:
- `domain/child_service.rs` - imports `DbConnection`, `ChildRepository`
- `domain/allowance_service.rs` - imports `DbConnection`, `AllowanceRepository`
- `domain/parental_control_service.rs` - imports `DbConnection`, `ParentalControlRepository`
- `domain/balance_service.rs` - test imports `DbConnection`
- `domain/transaction_service.rs` - test imports `ChildRepository`

### IO Layer Files Using SQLite:
- `io/rest/allowance_apis.rs` - test imports `DbConnection`
- `io/rest/transaction_apis.rs` - test imports `DbConnection`  
- `io/rest/money_management_apis.rs` - test imports `DbConnection`

### Backend Initialization:
- `backend/mod.rs` - creates `DbConnection` and SQLite-based services

### Storage Module:
- `storage/mod.rs` - re-exports `DbConnection` and SQLite repositories

## Architecture Change Required

### Current Backend Initialization (Lines 75-83 in `mod.rs`):
```rust
let child_service = ChildService::new(db_conn.clone());           // ❌ Uses SQLite
let parental_control_service = ParentalControlService::new(db_conn.clone()); // ❌ Uses SQLite
let allowance_service = AllowanceService::new(db_conn.clone());   // ❌ Uses SQLite
```

### Target Backend Initialization:
```rust
let child_service = ChildService::new(csv_conn.clone());          // ✅ Use CSV
let parental_control_service = ParentalControlService::new(csv_conn.clone()); // ✅ Use CSV
let allowance_service = AllowanceService::new(csv_conn.clone());  // ✅ Use CSV
```

## SQLite Code Catalog

### Pure SQLite Infrastructure (6 files):
- `connection.rs` - SQLite connection management
- `db.rs` - Legacy database operations
- `repositories/transaction_repository.rs` - SQLite transaction repository
- `repositories/child_repository.rs` - SQLite child repository
- `repositories/allowance_repository.rs` - SQLite allowance repository
- `repositories/parental_control_repository.rs` - SQLite parental control repository

### SQLite Dependencies (13 locations):
- **Domain services:** 3 services need constructor changes
- **Test code:** 9 test functions need updating
- **Backend initialization:** 1 location needs service switching

## Data Migration Implications

### Current Data Sources:
- **Children:** `keyvalue.db` → needs to become `~/Documents/Allowance Tracker/{child_name}/child.yaml`
- **Allowances:** `keyvalue.db` → needs to become `~/Documents/Allowance Tracker/{child_name}/allowance_config.yaml`
- **Parental Control:** `keyvalue.db` → needs to become `~/Documents/Allowance Tracker/{child_name}/parental_control_attempts.csv`
- **Active Child:** `keyvalue.db` → needs to become `~/Documents/Allowance Tracker/global_config.yaml`

### Existing CSV Infrastructure:
✅ **CSV repositories already exist and are fully functional:**
- `csv/child_repository.rs` - Ready to use
- `csv/allowance_repository.rs` - Ready to use
- `csv/parental_control_repository.rs` - Ready to use
- `csv/global_config_repository.rs` - Handles active child tracking

## Action Plan Summary

The investigation reveals we have a **clean migration path**:

1. **✅ CSV infrastructure is complete** - All needed CSV repositories exist
2. **✅ File moves are straightforward** - Clear boundaries between SQLite and CSV code
3. **✅ Service changes are minimal** - Just constructor parameter changes
4. **✅ No data loss risk** - Database will remain until we confirm CSV works

## Phase 2: File Reorganization Tasks

### Step 1: Create sqlite directory structure
- Create `storage/sqlite/` directory
- Create `storage/sqlite/repositories/` directory

### Step 2: Move SQLite files
- Move `storage/connection.rs` → `storage/sqlite/connection.rs`
- Move `storage/db.rs` → `storage/sqlite/db.rs`
- Move entire `storage/repositories/` → `storage/sqlite/repositories/`

### Step 3: Update imports
- Update all files importing from moved locations
- Update `storage/mod.rs` to remove SQLite exports
- Add SQLite module exports in appropriate location

### Step 4: Service constructor changes
- Update `ChildService::new()` to accept `CsvConnection`
- Update `AllowanceService::new()` to accept `CsvConnection`
- Update `ParentalControlService::new()` to accept `CsvConnection`

### Step 5: Backend initialization
- Change `backend/mod.rs` to use CSV repositories for all services
- Remove `DbConnection` initialization

### Step 6: Test and verify
- Ensure app starts without errors
- Verify clean slate (no data from database)
- Test CSV functionality
- Delete `keyvalue.db` when confirmed working

## Files to Update (Import Changes)

### Domain Layer:
- `src-tauri/src/backend/domain/child_service.rs`
- `src-tauri/src/backend/domain/allowance_service.rs`
- `src-tauri/src/backend/domain/parental_control_service.rs`
- `src-tauri/src/backend/domain/balance_service.rs` (tests)
- `src-tauri/src/backend/domain/transaction_service.rs` (tests)

### IO Layer:
- `src-tauri/src/backend/io/rest/allowance_apis.rs` (tests)
- `src-tauri/src/backend/io/rest/transaction_apis.rs` (tests)
- `src-tauri/src/backend/io/rest/money_management_apis.rs` (tests)

### Backend:
- `src-tauri/src/backend/mod.rs`

### Storage:
- `src-tauri/src/backend/storage/mod.rs`

---

**Status:** MIGRATION COMPLETE! 🎉  
**Phase:** All Phases Complete  
**Target:** Pure CSV architecture with all data in Documents folder ✅

---

## Phase 2 Results: SUCCESS! ✅

### File Reorganization Complete:
- ✅ Created `storage/sqlite/` directory structure
- ✅ Moved all SQLite files to `storage/sqlite/`
- ✅ Updated all import statements
- ✅ Converted all domain services to use CSV repositories

### Architecture Switch Complete:
- ✅ `ChildService` now uses CSV storage
- ✅ `AllowanceService` now uses CSV storage  
- ✅ `ParentalControlService` now uses CSV storage
- ✅ `TransactionService` was already using CSV storage
- ✅ `BalanceService` was already using CSV storage

### Backend Initialization:
- ✅ Removed SQLite database initialization
- ✅ All services now use single CSV connection
- ✅ Data directory: `~/Documents/Allowance Tracker`

### Compilation Status:
- ✅ All domain service errors resolved
- ✅ All import errors resolved
- ⚠️ 2 minor errors remain in SQLite code (preserved for future use)

## Phase 3: Testing & Cleanup Complete! ✅

- ✅ **App startup verified** - No crashes, loads successfully
- ✅ **Documents folder structure created** - `~/Documents/Allowance Tracker/`
- ✅ **CSV data isolated** - No data loaded from old SQLite database
- ✅ **Clean slate confirmed** - Transactions.csv has headers only
- ✅ **SQLite compilation errors fixed** - Code preserved for future use
- ✅ **Old database removed** - `keyvalue.db` deleted as requested

### Final Verification:
- **Before Migration:** Children loaded from SQLite database (`keyvalue.db`)
- **After Migration:** Children loaded from CSV files in Documents folder only
- **Result:** ✅ **SUCCESSFUL MIGRATION** - App completely ignores old SQLite data!

---

## **🎉 MISSION ACCOMPLISHED! 🎉**

**The hybrid architecture has been successfully converted to pure CSV architecture!**

All data now resides exclusively in `~/Documents/Allowance Tracker/` as requested. 