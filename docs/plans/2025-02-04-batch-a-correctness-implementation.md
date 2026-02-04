# Batch A: Correctness Fixes Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Fix 4 correctness issues that cause silent failures or incorrect behavior.

**Architecture:** Targeted fixes to timezone handling, error recovery, and path validation. No architectural changes.

**Tech Stack:** Rust, chrono

---

## Task 1: Fix Timezone Handling (2.1)

**Files:**
- Modify: `backend/domain/transaction_service.rs:80-84`
- Modify: `backend/storage/csv/transaction_repository.rs:86-92`

**Step 1: Fix transaction_service.rs**

In `backend/domain/transaction_service.rs`, replace lines 80-84:

```rust
        let transaction_date = command.date.unwrap_or_else(|| {
            // Create current time in Eastern timezone if no date provided
            let eastern_offset = chrono::FixedOffset::west_opt(5 * 3600).unwrap(); // EST (UTC-5)
            chrono::Utc::now().with_timezone(&eastern_offset)
        });
```

With:

```rust
        let transaction_date = command.date.unwrap_or_else(|| {
            // Use system local timezone (handles DST automatically)
            chrono::Local::now().fixed_offset()
        });
```

**Step 2: Fix transaction_repository.rs**

In `backend/storage/csv/transaction_repository.rs`, replace lines 86-92:

```rust
        if let Ok(naive_date) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
            // Convert to beginning of day in Eastern Time
            let naive_datetime = naive_date.and_hms_opt(0, 0, 0).unwrap();
            let eastern_offset = FixedOffset::west_opt(5 * 3600).unwrap(); // EST (UTC-5)

            if let Some(dt) = naive_datetime.and_local_timezone(eastern_offset).single() {
                return Ok(dt);
            }
        }
```

With:

```rust
        if let Ok(naive_date) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
            // Convert to beginning of day in local timezone (handles DST automatically)
            let naive_datetime = naive_date.and_hms_opt(0, 0, 0).unwrap();
            if let Some(local_dt) = naive_datetime.and_local_timezone(chrono::Local).single() {
                return Ok(local_dt.fixed_offset());
            }
        }
```

**Step 3: Run cargo check**

Run: `cargo check`
Expected: Compiles successfully

**Step 4: Run cargo test**

Run: `cargo test`
Expected: All tests pass (191 tests)

**Step 5: Commit**

```bash
git add backend/domain/transaction_service.rs backend/storage/csv/transaction_repository.rs
git commit -m "fix: use local timezone instead of hardcoded EST

Replaced hardcoded EST (UTC-5) with chrono::Local which automatically
handles daylight saving time. Previously, timestamps were wrong during
EDT months (March-November)."
```

---

## Task 2: Fix lock().unwrap() Panic Risk (2.3)

**Files:**
- Modify: `backend/storage/csv/connection.rs:75,139,145,171,500,549`
- Modify: `backend/domain/calendar.rs:451,463`

**Step 1: Fix connection.rs (6 occurrences)**

In `backend/storage/csv/connection.rs`, replace all 6 occurrences of:

```rust
.lock().unwrap()
```

With:

```rust
.lock().unwrap_or_else(|e| e.into_inner())
```

The lines to change are: 75, 139, 145, 171, 500, 549

**Step 2: Fix calendar.rs (2 occurrences)**

In `backend/domain/calendar.rs`, replace both occurrences at lines 451 and 463:

```rust
.lock().unwrap()
```

With:

```rust
.lock().unwrap_or_else(|e| e.into_inner())
```

**Step 3: Run cargo check**

Run: `cargo check`
Expected: Compiles successfully

**Step 4: Run cargo test**

Run: `cargo test`
Expected: All tests pass

**Step 5: Commit**

```bash
git add backend/storage/csv/connection.rs backend/domain/calendar.rs
git commit -m "fix: recover from poisoned mutex locks instead of panicking

Changed .lock().unwrap() to .lock().unwrap_or_else(|e| e.into_inner())
at 8 locations. This recovers the data from a poisoned lock instead of
crashing. In a single-user desktop app, poisoned lock data is almost
certainly still valid."
```

---

## Task 3: Fix unwrap_or_default Silent Failures (2.2)

**Files:**
- Modify: `backend/storage/csv/transaction_repository.rs:279,340,345,358,372`

**Step 1: Understand the context**

The problematic `unwrap_or_default()` calls are in methods that silently return empty/zero values when they should either propagate errors or skip malformed records.

**Step 2: Fix get_transaction (line 279)**

In `backend/storage/csv/transaction_repository.rs`, the `get_transaction` method at line 277-282:

```rust
        Ok(self
            .read_transactions(&child_name)
            .unwrap_or_default()
            .into_iter()
            .find(|t| t.id == transaction_id))
```

Replace with:

```rust
        let transactions = self.read_transactions(&child_name)?;
        Ok(transactions.into_iter().find(|t| t.id == transaction_id))
```

**Step 3: Fix update_transaction (lines 340, 345)**

In `update_transaction` method, replace lines 338-346:

```rust
        let mut transactions = self
            .read_transactions_by_id(&transaction.child_id)
            .unwrap_or_default();

        if let Some(index) = transactions.iter().position(|t| t.id == transaction.id) {
            transactions[index] = transaction.clone();
            self.write_transactions_by_id(&transaction.child_id, &transactions)
                .unwrap_or_default();
        }
```

With:

```rust
        let mut transactions = self.read_transactions_by_id(&transaction.child_id)?;

        if let Some(index) = transactions.iter().position(|t| t.id == transaction.id) {
            transactions[index] = transaction.clone();
            self.write_transactions_by_id(&transaction.child_id, &transactions)?;
        }
```

**Step 4: Fix delete_transaction (line 358)**

In `delete_transaction` method, replace lines 356-359:

```rust
        if transactions.len() < original_len {
            self.write_transactions_by_id(child_id, &transactions)
                .unwrap_or_default();
            Ok(true)
```

With:

```rust
        if transactions.len() < original_len {
            self.write_transactions_by_id(child_id, &transactions)?;
            Ok(true)
```

**Step 5: Fix delete_transactions (line 372)**

In `delete_transactions` method, replace lines 370-373:

```rust
        transactions.retain(|t| !transaction_ids.contains(&t.id));
        self.write_transactions(&child_name, &transactions)
            .unwrap_or_default();
        Ok((initial_len - transactions.len()) as u32)
```

With:

```rust
        transactions.retain(|t| !transaction_ids.contains(&t.id));
        self.write_transactions(&child_name, &transactions)?;
        Ok((initial_len - transactions.len()) as u32)
```

**Step 6: Fix update_transaction_balances (line 456)**

In `update_transaction_balances` method, replace lines 454-457:

```rust
            if needs_write {
                // Use internal method to avoid git commits during balance recalculation
                self.write_transactions_by_id_internal(&child_id, &transactions)
                    .unwrap_or_default();
            }
```

With:

```rust
            if needs_write {
                // Use internal method to avoid git commits during balance recalculation
                self.write_transactions_by_id_internal(&child_id, &transactions)?;
            }
```

**Step 7: Run cargo check**

Run: `cargo check`
Expected: Compiles successfully

**Step 8: Run cargo test**

Run: `cargo test`
Expected: All tests pass

**Step 9: Commit**

```bash
git add backend/storage/csv/transaction_repository.rs
git commit -m "fix: propagate errors instead of using unwrap_or_default

Changed 5 occurrences of .unwrap_or_default() to proper error propagation
with ?. This ensures write failures and read errors are reported to callers
instead of silently returning empty/default values."
```

---

## Task 4: Add Path Traversal Protection (2.4)

**Files:**
- Modify: `backend/domain/export_service.rs:175,248-280,294-310`

**Step 1: Update sanitize_path to return Result**

In `backend/domain/export_service.rs`, replace the `sanitize_path` method (lines 247-280):

```rust
    /// Basic path sanitization to handle common user input issues
    fn sanitize_path(&self, path: &str) -> String {
```

With:

```rust
    /// Basic path sanitization with traversal protection
    fn sanitize_path(&self, path: &str) -> Result<String, String> {
        let mut cleaned = path.trim().to_string();

        // Remove surrounding quotes (single or double)
        if (cleaned.starts_with('"') && cleaned.ends_with('"')) ||
           (cleaned.starts_with('\'') && cleaned.ends_with('\'')) {
            cleaned = cleaned[1..cleaned.len()-1].to_string();
        }

        // Trim again after quote removal
        cleaned = cleaned.trim().to_string();

        // Reject path traversal attempts
        if cleaned.contains("..") {
            return Err("Path cannot contain '..' (path traversal not allowed)".to_string());
        }

        // Handle escaped spaces (common on some systems)
        cleaned = cleaned.replace("\\ ", " ");

        // Remove any trailing slashes/backslashes
        while cleaned.ends_with('/') || cleaned.ends_with('\\') {
            cleaned.pop();
        }

        // Handle tilde expansion for home directory
        if cleaned.starts_with('~') {
            if let Some(home) = dirs::home_dir() {
                if cleaned == "~" {
                    cleaned = home.to_string_lossy().to_string();
                } else if cleaned.starts_with("~/") || cleaned.starts_with("~\\") {
                    cleaned = home.join(&cleaned[2..]).to_string_lossy().to_string();
                }
            }
        }

        Ok(cleaned)
    }
```

**Step 2: Update the caller to handle Result**

In `export_transactions_to_path` method, replace lines 173-176:

```rust
            Some(custom_path) if !custom_path.trim().is_empty() => {
                // Basic path sanitization: remove quotes, trim whitespace, handle common issues
                let cleaned_path = self.sanitize_path(&custom_path);
                std::path::PathBuf::from(cleaned_path)
            }
```

With:

```rust
            Some(custom_path) if !custom_path.trim().is_empty() => {
                // Basic path sanitization with traversal protection
                match self.sanitize_path(&custom_path) {
                    Ok(cleaned_path) => std::path::PathBuf::from(cleaned_path),
                    Err(e) => {
                        return Ok(ExportToPathResponse {
                            success: false,
                            message: format!("Invalid export path: {}", e),
                            file_path: String::new(),
                            transaction_count: 0,
                            child_name: String::new(),
                        });
                    }
                }
            }
```

**Step 3: Update tests**

In `backend/domain/export_service.rs`, replace the test function (starting around line 294):

```rust
    #[test]
    fn test_sanitize_path() {
        let service = ExportService::new();

        // Test quote removal and tilde expansion
        let home_dir = dirs::home_dir().unwrap().to_string_lossy().to_string();
        let expected_documents = std::path::PathBuf::from(&home_dir).join("Documents").to_string_lossy().to_string();

        assert_eq!(service.sanitize_path("\"~/Documents\""), expected_documents);
        assert_eq!(service.sanitize_path("'~/Documents'"), expected_documents);

        // Test whitespace trimming
        assert_eq!(service.sanitize_path("  /path/to/dir  "), "/path/to/dir");
        assert_eq!(service.sanitize_path("/path\\ to\\ dir"), "/path to dir");

        // Test trailing slash removal
        assert_eq!(service.sanitize_path("/path/to/dir/"), "/path/to/dir");
        assert_eq!(service.sanitize_path("/path/to/dir\\"), "/path/to/dir");
    }
```

With:

```rust
    #[test]
    fn test_sanitize_path() {
        let service = ExportService::new();

        // Test quote removal and tilde expansion
        let home_dir = dirs::home_dir().unwrap().to_string_lossy().to_string();
        let expected_documents = std::path::PathBuf::from(&home_dir).join("Documents").to_string_lossy().to_string();

        assert_eq!(service.sanitize_path("\"~/Documents\"").unwrap(), expected_documents);
        assert_eq!(service.sanitize_path("'~/Documents'").unwrap(), expected_documents);

        // Test whitespace trimming
        assert_eq!(service.sanitize_path("  /path/to/dir  ").unwrap(), "/path/to/dir");
        assert_eq!(service.sanitize_path("/path\\ to\\ dir").unwrap(), "/path to dir");

        // Test trailing slash removal
        assert_eq!(service.sanitize_path("/path/to/dir/").unwrap(), "/path/to/dir");
        assert_eq!(service.sanitize_path("/path/to/dir\\").unwrap(), "/path/to/dir");
    }

    #[test]
    fn test_sanitize_path_rejects_traversal() {
        let service = ExportService::new();

        // Test path traversal rejection
        assert!(service.sanitize_path("../etc/passwd").is_err());
        assert!(service.sanitize_path("/path/../etc/passwd").is_err());
        assert!(service.sanitize_path("~/Documents/..").is_err());
    }
```

**Step 4: Run cargo check**

Run: `cargo check`
Expected: Compiles successfully

**Step 5: Run cargo test**

Run: `cargo test`
Expected: All tests pass (now 192 tests with the new test)

**Step 6: Commit**

```bash
git add backend/domain/export_service.rs
git commit -m "fix: add path traversal protection to export path sanitization

sanitize_path now returns Result and rejects paths containing '..'.
This prevents accidental or malicious writes outside intended directories."
```

---

## Final Verification

**Step 1: Run full test suite**

Run: `cargo test`
Expected: 192 tests pass, 0 failures

**Step 2: Run clippy**

Run: `cargo clippy -- -D warnings 2>&1 | head -20`
Expected: No new warnings

**Step 3: Verify commits**

Run: `git log --oneline -5`
Expected: 4 commits for the 4 tasks

---

## Summary

| Task | Fix | Files |
|------|-----|-------|
| 1 | Use local timezone instead of hardcoded EST | 2 |
| 2 | Recover from poisoned mutex locks | 2 |
| 3 | Propagate errors instead of unwrap_or_default | 1 |
| 4 | Add path traversal protection | 1 |

**Total: 4 tasks, 5 files modified**
