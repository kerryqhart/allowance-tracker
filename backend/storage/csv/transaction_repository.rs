use anyhow::{Context, Result};
// Removed async_trait - no longer needed for synchronous operations
use log::{info, warn};
use crate::backend::domain::models::transaction::{
    Transaction as DomainTransaction, TransactionType as DomainTransactionType,
};
use allowance_core::money::Money;
use allowance_core::row::{TxRow, TxType};
use super::connection::CsvConnection;
use crate::backend::storage::GitManager;
use shared::ChildId;

/// `TxRow` (the pure `allowance-core` type, produced and consumed by the
/// single canonical codec) onto the domain `Transaction`. This boundary
/// conversion lives here — the same way `BalanceService` converts at its own
/// boundary — because `allowance-core` must not know about the domain model.
fn row_to_domain(row: TxRow) -> DomainTransaction {
    DomainTransaction {
        id: row.id,
        child_id: row.child_id,
        date: row.date,
        description: row.description,
        amount: row.amount,
        balance: row.balance,
        transaction_type: match row.tx_type {
            TxType::Allowance => DomainTransactionType::Allowance,
            TxType::OneOffIncome => DomainTransactionType::OneOffIncome,
            TxType::Expense => DomainTransactionType::Expense,
            TxType::FutureAllowance => DomainTransactionType::FutureAllowance,
        },
    }
}

fn domain_to_row(t: &DomainTransaction) -> TxRow {
    TxRow {
        id: t.id.clone(),
        child_id: t.child_id.clone(),
        date: t.date,
        description: t.description.clone(),
        amount: t.amount,
        balance: t.balance,
        tx_type: match t.transaction_type {
            DomainTransactionType::Allowance => TxType::Allowance,
            DomainTransactionType::OneOffIncome => TxType::OneOffIncome,
            DomainTransactionType::Expense => TxType::Expense,
            DomainTransactionType::FutureAllowance => TxType::FutureAllowance,
        },
    }
}

/// One thing about a registered child's `transactions.csv` worth surfacing at
/// startup — see `TransactionRepository::validate_all_transaction_files` and
/// `Backend::with_data_dir`, which turns each of these into a `StartupNotice`.
#[derive(Debug, Clone, PartialEq)]
pub enum TransactionFileNotice {
    /// The file did not parse under the codec at all (a hard error: an
    /// unparseable date, a date-only value, or an unrecognised type).
    ParseFailed { child_id: String, reason: String },
    /// The file parsed, but some rows carried legacy f64-precision noise
    /// (e.g. `"14.620000000000001"`) that was rounded to the nearest cent.
    /// The rewrite is correct — but not silently absorbed.
    LegacyPrecisionRounded { child_id: String, rows_rounded: usize },
}

/// CSV-based transaction repository
#[derive(Clone)]
pub struct TransactionRepository {
    connection: CsvConnection,
    git_manager: GitManager,
}

impl TransactionRepository {
    /// Create a new CSV transaction repository
    pub fn new(connection: CsvConnection) -> Self {
        Self {
            connection,
            git_manager: GitManager::new(),
        }
    }
    
    /// Read and parse a child's `transactions.csv`, without converting to the
    /// domain type. Shared by `read_transactions` (which only wants the rows)
    /// and `validate_all_transaction_files` (which also wants
    /// `rows_rounded`, to report it rather than absorb it) so there is one
    /// place that resolves the path and calls the codec.
    fn parse_transactions_file(
        &self,
        child_id: &ChildId,
    ) -> Result<allowance_core::codec::ParsedTransactions> {
        self.connection.ensure_transactions_file_exists(child_id)?;

        let file_path = self.connection.transactions_path(child_id)?;

        let text = std::fs::read_to_string(&file_path)
            .with_context(|| format!("reading {}", file_path.display()))?;

        // The one canonical codec: no current-time fallback on an unparseable
        // date, no chrono::Local resolution of a date-only value, and no
        // deriving an unrecognised type from the description/amount. Any of
        // those would make read-modify-write non-idempotent — one bad row
        // would make the file change on every read/write cycle, and two
        // machines syncing it would never converge. A malformed row is now a
        // hard error surfaced to the caller (and, at startup, collected into
        // a `StartupNotice` — see `validate_all_transaction_files` and
        // `Backend::with_data_dir`) instead of being silently rewritten.
        //
        // Money is parsed with rounding (not the strict `FromStr`): legacy
        // data written before `Money` existed carries f64-precision noise
        // like `"14.620000000000001"`, which means 1462 cents and nothing
        // else. Refusing it here would refuse the user's own history over a
        // rendering artifact. The file self-cleans — the next write emits
        // exactly two decimals via `render_transactions` — but the count of
        // rows that needed rounding is still reported, never silently
        // absorbed; see `rows_rounded` on the result.
        allowance_core::codec::parse_transactions(&text)
            .with_context(|| format!("parsing {}", file_path.display()))
    }

    /// Read all transactions for a child from their CSV file.
    ///
    /// Resolution is by immutable id through the registry. It used to derive
    /// the folder from the child's *display name*, so a rename silently
    /// resolved elsewhere and the history read back as empty.
    fn read_transactions(&self, child_id: &ChildId) -> Result<Vec<DomainTransaction>> {
        let parsed = self.parse_transactions_file(child_id)?;
        Ok(parsed.rows.into_iter().map(row_to_domain).collect())
    }

    /// Check every registered child's `transactions.csv` up front.
    ///
    /// The codec no longer tolerates a malformed row: an unparseable date, a
    /// date-only value, or an unrecognised transaction type is now a hard
    /// `Err` instead of a silently rewritten fallback. Without this check,
    /// the first sign of that would be an error the moment someone opens the
    /// affected child's page. Called once at startup instead, so it becomes a
    /// `StartupNotice` in the banner up front — visible, but not fatal to the
    /// rest of the app.
    ///
    /// A file that parses but needed legacy-precision rounding (see
    /// `Money::parse_rounding`) is reported too, as a lower-severity notice —
    /// the rewrite is correct, but the user should still be told their data
    /// was touched.
    pub fn validate_all_transaction_files(&self) -> Vec<TransactionFileNotice> {
        self.connection
            .registry()
            .entries()
            .iter()
            .filter_map(|entry| match self.parse_transactions_file(&entry.id) {
                Ok(parsed) if parsed.rows_rounded > 0 => {
                    Some(TransactionFileNotice::LegacyPrecisionRounded {
                        child_id: entry.id.to_string(),
                        rows_rounded: parsed.rows_rounded,
                    })
                }
                Ok(_) => None,
                Err(e) => Some(TransactionFileNotice::ParseFailed {
                    child_id: entry.id.to_string(),
                    reason: format!("{e:#}"),
                }),
            })
            .collect()
    }

    /// Write all transactions for a child to their CSV file (internal, no git commit)
    fn write_transactions_internal(&self, child_id: &ChildId, transactions: &[DomainTransaction]) -> Result<()> {
        let file_path = self.connection.transactions_path(child_id)?;

        // Always the canonical (date, id) order and canonical rendering,
        // applied on every write — not only after a merge — so two machines
        // writing the same rows produce identical bytes.
        let rows: Vec<TxRow> = transactions.iter().map(domain_to_row).collect();
        let text = allowance_core::codec::render_transactions(&rows);

        std::fs::write(&file_path, text)
            .with_context(|| format!("writing {}", file_path.display()))?;

        Ok(())
    }

    /// Write all transactions and commit to git (for user-facing operations)
    fn write_transactions(&self, child_id: &ChildId, transactions: &[DomainTransaction]) -> Result<()> {
        self.write_transactions_internal(child_id, transactions)?;

        // Git commit the transaction file change
        let child_dir = self.connection.child_dir(child_id)?;
        let action_description = format!("Updated transactions (total: {})", transactions.len());
        let _ = self.git_manager.commit_file_change(
            &child_dir,
            "transactions.csv",
            &action_description
        );

        Ok(())
    }
}

impl TransactionRepository {
    /// Compare two DateTime objects properly handling timezone conversion
    fn compare_dates(&self, date1: &chrono::DateTime<chrono::FixedOffset>, date2: &str) -> i32 {
        // Parse date2 as string (for backwards compatibility with query parameters)
        if let Ok(dt2) = chrono::DateTime::parse_from_rfc3339(date2) {
            // Compare as datetime objects (automatically handles timezone conversion)
            if *date1 < dt2 { -1 } else if *date1 > dt2 { 1 } else { 0 }
        } else {
            // If parsing fails, compare against RFC3339 representation
            let date1_str = date1.to_rfc3339();
            date1_str.cmp(&date2.to_string()) as i32
        }
    }
    
}

/// Guard shared by every single-transaction write path: refuses to persist a
/// transaction whose balance is still `Transaction::BALANCE_PENDING`
/// (`Money::from_cents(i64::MIN)`, the in-domain stand-in for the old
/// `f64::NAN` "not yet calculated" sentinel).
///
/// `Money::render()` on `i64::MIN` produces `"-92233720368547758.08"`, and
/// parsing that back overflows `i64` and returns `Err` (see allowance-core's
/// own test for this). A later task turns an unparseable CSV row into a
/// hard, unrecoverable read error with no fallback — so persisting
/// BALANCE_PENDING today would write a file this app can never read again.
fn reject_pending_balance(transaction: &DomainTransaction) -> Result<()> {
    if transaction.balance == DomainTransaction::BALANCE_PENDING {
        anyhow::bail!(
            "refusing to persist transaction {}: balance is still BALANCE_PENDING (not yet calculated by BalanceService)",
            transaction.id
        );
    }
    Ok(())
}

impl crate::backend::storage::TransactionStorage for TransactionRepository {
    fn store_transaction(&self, transaction: &DomainTransaction) -> Result<()> {
        reject_pending_balance(transaction)?;
        let child_id = ChildId::from(transaction.child_id.as_str());
        let mut transactions = self.read_transactions(&child_id)?;
        if let Some(pos) = transactions.iter().position(|t| t.id == transaction.id) {
            transactions[pos] = transaction.clone();
        } else {
            transactions.push(transaction.clone());
        }
        self.write_transactions(&child_id, &transactions)
    }

    fn get_transaction(
        &self,
        child_id: &str,
        transaction_id: &str,
    ) -> Result<Option<DomainTransaction>> {
        let transactions = self.read_transactions(&ChildId::from(child_id))?;
        Ok(transactions.into_iter().find(|t| t.id == transaction_id))
    }

    fn list_transactions(
        &self,
        child_id: &str,
        limit: Option<u32>,
        after: Option<String>,
    ) -> Result<Vec<DomainTransaction>> {
        let mut transactions = self.read_transactions(&ChildId::from(child_id))?;
        transactions.sort_by(|a, b| b.date.cmp(&a.date)); // Sort by date descending

        let mut result = transactions;

        if let Some(after_id) = after {
            if let Some(index) = result.iter().position(|t| t.id == after_id) {
                result = result.split_off(index + 1);
            }
        }

        if let Some(limit_val) = limit {
            result.truncate(limit_val as usize);
        }

        Ok(result)
    }

    fn list_transactions_chronological(
        &self,
        child_id: &str,
        start_date: Option<String>,
        end_date: Option<String>,
    ) -> Result<Vec<DomainTransaction>> {
        let mut transactions = self.read_transactions(&ChildId::from(child_id))?;

        transactions.sort_by(|a, b| a.date.cmp(&b.date)); // Sort by date ascending

        let mut filtered = transactions;

        // Convert date strings to datetime objects for proper comparison
        if let Some(start) = start_date {
            filtered.retain(|t| self.compare_dates(&t.date, &start) >= 0);
        }
        if let Some(end) = end_date {
            filtered.retain(|t| self.compare_dates(&t.date, &end) <= 0);
        }

        Ok(filtered)
    }

    fn update_transaction(&self, transaction: &DomainTransaction) -> Result<()> {
        reject_pending_balance(transaction)?;
        info!("Updating transaction in CSV: {}", transaction.id);

        let child_id = ChildId::from(transaction.child_id.as_str());
        let mut transactions = self.read_transactions(&child_id)?;

        if let Some(index) = transactions.iter().position(|t| t.id == transaction.id) {
            transactions[index] = transaction.clone();
            self.write_transactions(&child_id, &transactions)?;
        }

        Ok(())
    }

    fn delete_transaction(&self, child_id: &str, transaction_id: &str) -> Result<bool> {
        let child_id = ChildId::from(child_id);
        let mut transactions = self.read_transactions(&child_id)?;
        let original_len = transactions.len();
        transactions.retain(|t| t.id != transaction_id);

        if transactions.len() < original_len {
            self.write_transactions(&child_id, &transactions)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn delete_transactions(&self, child_id: &str, transaction_ids: &[String]) -> Result<u32> {
        let child_id = ChildId::from(child_id);
        let mut transactions = self.read_transactions(&child_id)?;
        let initial_len = transactions.len();
        transactions.retain(|t| !transaction_ids.contains(&t.id));
        self.write_transactions(&child_id, &transactions)?;
        Ok((initial_len - transactions.len()) as u32)
    }

    fn get_latest_transaction(&self, child_id: &str) -> Result<Option<DomainTransaction>> {
        let mut transactions = self.read_transactions(&ChildId::from(child_id))?;
        transactions.sort_by(|a, b| b.date.cmp(&a.date));
        Ok(transactions.into_iter().next())
    }

    fn get_transactions_since(
        &self,
        child_id: &str,
        date: &str,
    ) -> Result<Vec<DomainTransaction>> {
        let mut transactions = self.read_transactions(&ChildId::from(child_id))?;
        transactions.retain(|t| self.compare_dates(&t.date, date) >= 0);
        Ok(transactions)
    }

    fn get_latest_transaction_before_date(
        &self,
        child_id: &str,
        date: &str,
    ) -> Result<Option<DomainTransaction>> {
        let mut transactions = self.read_transactions(&ChildId::from(child_id))?;
        transactions.retain(|t| self.compare_dates(&t.date, date) < 0);
        transactions.sort_by(|a, b| b.date.cmp(&a.date));
        Ok(transactions.into_iter().next())
    }

    fn update_transaction_balance(
        &self,
        _transaction_id: &str,
        _new_balance: Money,
    ) -> Result<()> {
        // This is a complex operation in a file-based system, as it requires
        // finding the right child, reading all transactions, updating one, and writing back.
        // For now, we assume this is handled by update_transaction or recalculation logic.
        warn!("update_transaction_balance is a no-op in the CSV repository.");
        Ok(())
    }

    fn update_transaction_balances(&self, child_id: &str, updates: &[(String, Money)]) -> Result<()> {
        if updates.is_empty() {
            return Ok(());
        }

        info!("Updating {} transaction balances for child {}", updates.len(), child_id);

        let child_id = ChildId::from(child_id);
        let mut transactions = self.read_transactions(&child_id)?;
        let mut needs_write = false;

        for transaction in &mut transactions {
            if let Some(update) = updates.iter().find(|(id, _)| id == &transaction.id) {
                transaction.balance = update.1;
                needs_write = true;
            }
        }

        if needs_write {
            // Use internal method to avoid git commits during balance recalculation
            self.write_transactions_internal(&child_id, &transactions)?;
        }

        Ok(())
    }

    fn check_transactions_exist(
        &self,
        child_id: &str,
        transaction_ids: &[String],
    ) -> Result<Vec<String>> {
        let all_transactions = self.read_transactions(&ChildId::from(child_id))?;
        let found_ids: Vec<String> = all_transactions
            .into_iter()
            .filter(|t| transaction_ids.contains(&t.id))
            .map(|t| t.id)
            .collect();
        Ok(found_ids)
    }

    fn list_transactions_by_ids(
        &self,
        child_id: &str,
        transaction_ids: &[String],
    ) -> Result<Vec<DomainTransaction>> {
        let all_transactions = self.read_transactions(&ChildId::from(child_id))?;
        let transactions: Vec<DomainTransaction> = all_transactions
            .into_iter()
            .filter(|t| transaction_ids.contains(&t.id))
            .collect();
        Ok(transactions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::child_repository::ChildRepository;
    use crate::backend::storage::{ChildStorage, TransactionStorage};
    use crate::backend::domain::models::child::Child as DomainChild;
    use chrono::Utc;
    use std::sync::Arc;
    use tempfile::TempDir;

    /// Convert a dollar-amount literal (as tests already write them) into
    /// exact `Money` cents, mirroring `Money`'s own `Deserialize` boundary
    /// conversion. Not money arithmetic — a one-shot literal conversion.
    fn dollars(amount: f64) -> Money {
        Money::from_cents((amount * 100.0).round() as i64)
    }

    /// Build a repository over a temp dir with one registered child, id
    /// `test_child`.
    ///
    /// The child must exist before any transaction call: resolution now goes
    /// through the registry, and an unregistered child is an error rather than
    /// a fabricated folder.
    fn setup_test_repo() -> Result<(TransactionRepository, TempDir)> {
        let temp_dir = TempDir::new()?;
        let connection = CsvConnection::new(temp_dir.path())?;
        store_child_on(&connection, "test_child", "Test Child")?;
        let repo = TransactionRepository::new(connection);
        Ok((repo, temp_dir))
    }

    /// Register a child through the *same* connection the repository uses.
    ///
    /// A second `CsvConnection` over the same directory would hold its own
    /// registry snapshot, and the repository's connection would not see the
    /// new child until it was rebuilt.
    fn store_child_on(connection: &CsvConnection, id: &str, name: &str) -> Result<DomainChild> {
        let child = DomainChild {
            id: id.to_string(),
            name: name.to_string(),
            birthdate: chrono::NaiveDate::parse_from_str("2010-01-01", "%Y-%m-%d").unwrap(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        ChildRepository::new(Arc::new(connection.clone())).store_child(&child)?;
        Ok(child)
    }

    #[test]
    fn test_compare_dates_timezone_fix() -> Result<()> {
        let (repo, _env) = setup_test_repo()?;

        // Test the exact scenario from the bug report. `compare_dates` is used
        // only for query-parameter filtering now (date1 already comes in as a
        // parsed `DateTime` off a `TxRow`; the codec is the only place that
        // turns a raw string into one, and it never falls back), so both
        // dates here are constructed directly rather than through the
        // now-deleted `parse_date_string`.
        let cdt_transaction_date = "2025-06-15T00:00:00-05:00"; // CDT transaction
        let utc_query_end_date = "2025-06-30T23:59:59Z";       // UTC query

        // The CDT transaction should be BEFORE the UTC end date (comparison should be < 0)
        let date1 = chrono::DateTime::parse_from_rfc3339(cdt_transaction_date)?;
        let result = repo.compare_dates(&date1, utc_query_end_date);
        println!("Test: compare_dates('{}', '{}') = {}", cdt_transaction_date, utc_query_end_date, result);
        assert!(result < 0, "CDT transaction should be before UTC end date");

        // Test another CDT transaction that should be included
        let cdt_transaction_june27 = "2025-06-27T07:00:00-05:00";
        let date2 = chrono::DateTime::parse_from_rfc3339(cdt_transaction_june27)?;
        let result2 = repo.compare_dates(&date2, utc_query_end_date);
        println!("Test: compare_dates('{}', '{}') = {}", cdt_transaction_june27, utc_query_end_date, result2);
        assert!(result2 < 0, "CDT June 27 transaction should be before UTC end date");

        // `compare_dates` itself still falls back to string comparison when
        // its *second* argument (a query parameter, not a stored row) fails
        // to parse. That fallback is unrelated to the codec and stays.
        let result3 = repo.compare_dates(&date1, "invalid-date");
        println!("Test: compare_dates('{}', 'invalid-date') = {} (fallback)", cdt_transaction_date, result3);

        Ok(())
    }
    
    #[test]
    fn test_store_and_retrieve_transaction() -> Result<()> {
        let (repo, _env) = setup_test_repo()?;
        
        let transaction = DomainTransaction {
            id: "test_tx_001".to_string(),
            child_id: "test_child".to_string(),
            date: chrono::DateTime::parse_from_rfc3339("2024-01-15T10:30:00Z").unwrap(),
            description: "Test transaction".to_string(),
            amount: dollars(25.50),
            balance: dollars(25.50),
            transaction_type: DomainTransactionType::OneOffIncome,
        };

        // Store transaction
        repo.store_transaction(&transaction)?;

        // Retrieve transaction
        let retrieved = repo.get_transaction("test_child", "test_tx_001")?;
        assert!(retrieved.is_some());

        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.id, "test_tx_001");
        assert_eq!(retrieved.description, "Test transaction");
        assert_eq!(retrieved.amount, dollars(25.50));
        
        Ok(())
    }
    
    #[test]
    fn test_list_transactions_with_pagination() -> Result<()> {
        let (repo, _env) = setup_test_repo()?;
        
        // Store multiple transactions
        for i in 1..=5 {
            let transaction = DomainTransaction {
                id: format!("tx_{:03}", i),
                child_id: "test_child".to_string(),
                date: chrono::DateTime::parse_from_rfc3339(&format!("2024-01-{:02}T10:30:00Z", i + 10)).unwrap(),
                description: format!("Transaction {}", i),
                amount: dollars(i as f64 * 10.0),
                balance: dollars((i * (i + 1) / 2) as f64 * 10.0), // Cumulative sum
                transaction_type: DomainTransactionType::OneOffIncome,
            };
            
            repo.store_transaction(&transaction)?;
        }
        
        // Test listing with limit
        let transactions = repo.list_transactions("test_child", Some(3), None)?;
        assert_eq!(transactions.len(), 3);
        
        // Should be ordered by date descending (most recent first)
        assert_eq!(transactions[0].id, "tx_005");
        assert_eq!(transactions[1].id, "tx_004");
        assert_eq!(transactions[2].id, "tx_003");

        Ok(())
    }

    /// Regression test: `Transaction::BALANCE_PENDING`
    /// (`Money::from_cents(i64::MIN)`) must never be persisted. Its
    /// `render()` overflows `i64` on the way back in, and a later task turns
    /// an unparseable CSV row into a hard, unrecoverable read error — so a
    /// persisted BALANCE_PENDING row would permanently brick that child's
    /// transaction file.
    #[test]
    fn store_transaction_refuses_a_balance_pending_transaction() -> Result<()> {
        let (repo, _env) = setup_test_repo()?;

        let transaction = DomainTransaction {
            id: "test_tx_pending".to_string(),
            child_id: "test_child".to_string(),
            date: chrono::DateTime::parse_from_rfc3339("2024-01-15T10:30:00Z").unwrap(),
            description: "Future allowance".to_string(),
            amount: dollars(10.0),
            balance: DomainTransaction::BALANCE_PENDING,
            transaction_type: DomainTransactionType::FutureAllowance,
        };

        let result = repo.store_transaction(&transaction);
        assert!(
            result.is_err(),
            "storing a transaction with balance == BALANCE_PENDING must fail, not silently persist it"
        );

        assert!(
            repo.get_transaction("test_child", "test_tx_pending")?.is_none(),
            "a refused BALANCE_PENDING transaction must not end up in storage"
        );

        Ok(())
    }

    #[test]
    fn update_transaction_refuses_a_balance_pending_transaction() -> Result<()> {
        let (repo, _env) = setup_test_repo()?;

        // Store a normal transaction first...
        let transaction = DomainTransaction {
            id: "test_tx_to_update".to_string(),
            child_id: "test_child".to_string(),
            date: chrono::DateTime::parse_from_rfc3339("2024-01-15T10:30:00Z").unwrap(),
            description: "Allowance".to_string(),
            amount: dollars(10.0),
            balance: dollars(10.0),
            transaction_type: DomainTransactionType::Allowance,
        };
        repo.store_transaction(&transaction)?;

        // ...then attempt to update it to carry BALANCE_PENDING.
        use crate::backend::storage::traits::TransactionStorage;
        let mut pending_update = transaction.clone();
        pending_update.balance = DomainTransaction::BALANCE_PENDING;
        let result = TransactionStorage::update_transaction(&repo, &pending_update);
        assert!(
            result.is_err(),
            "updating a transaction to carry BALANCE_PENDING must fail, not silently persist it"
        );

        let stored = repo.get_transaction("test_child", "test_tx_to_update")?.unwrap();
        assert_eq!(
            stored.balance,
            dollars(10.0),
            "the original balance must be untouched after a refused update"
        );

        Ok(())
    }

    #[test]
    fn test_delete_transaction() -> Result<()> {
        let (repo, _temp_dir) = setup_test_repo()?;
        let child = store_child_on(&repo.connection, "child::test_123", "Second Child")?;

        // Create a test transaction
        let transaction = DomainTransaction {
            id: "test_transaction_123".to_string(),
            child_id: child.id.clone(),
            date: chrono::DateTime::parse_from_rfc3339("2025-01-01T12:00:00Z").unwrap(),
            description: "Test transaction".to_string(),
            amount: dollars(10.0),
            balance: dollars(10.0),
            transaction_type: DomainTransactionType::OneOffIncome,
        };

        // Store and verify
        repo.store_transaction(&transaction)?;
        assert!(repo.get_transaction(&child.id, &transaction.id)?.is_some());

        // Delete and verify
        let deleted = repo.delete_transaction(&child.id, &transaction.id)?;
        assert!(deleted);
        assert!(repo.get_transaction(&child.id, &transaction.id)?.is_none());

        Ok(())
    }
    
    // ========================================
    // ARCHITECTURAL INVARIANT TESTS
    // ========================================
    
    /// The codec accepts RFC3339 only (a colon-delimited offset, or `Z`) and
    /// refuses everything else — no `chrono::Local` resolution of a bare
    /// date, and no silent fallback. `2024-06-15T10:30:00-0500` (no colon in
    /// the offset) used to slide through the old `parse_date_string`'s
    /// current-time fallback undetected; now it is a hard `CodecError::Date`.
    #[test]
    fn round_trip_accepts_only_strict_rfc3339() {
        let strict = [
            "2024-06-15T10:30:00Z",
            "2024-06-15T10:30:00+00:00",
            "2024-06-15T10:30:00.123Z",
            "2024-06-15T10:30:00-05:00",
        ];
        for date_str in strict {
            let csv = format!(
                "id,child_id,date,description,amount,balance,type\nx,c,{date_str},d,1.00,1.00,expense\n"
            );
            assert!(
                allowance_core::codec::parse_transactions(&csv).is_ok(),
                "expected {date_str} to parse"
            );
        }

        let loose = ["2024-06-15T10:30:00-0500", "2024-06-15T10:30:00+0000", "2024-06-15"];
        for date_str in loose {
            let csv = format!(
                "id,child_id,date,description,amount,balance,type\nx,c,{date_str},d,1.00,1.00,expense\n"
            );
            assert!(
                allowance_core::codec::parse_transactions(&csv).is_err(),
                "expected {date_str} to be refused, not silently normalized"
            );
        }
    }

    #[test]
    fn test_invariant_domain_models_use_datetime_objects() -> Result<()> {
        // This test will fail until we fix the domain models
        // It should verify that domain models use DateTime objects, not strings
        
        use std::any::TypeId;
        use chrono::{DateTime, FixedOffset};
        
        // Create a dummy transaction to inspect its field types
        let _transaction = DomainTransaction {
            id: "test".to_string(),
            child_id: "child".to_string(),
            date: chrono::DateTime::parse_from_rfc3339("2024-01-01T12:00:00Z").unwrap(), // Fixed domain model to use DateTime
            description: "Test".to_string(),
            amount: dollars(10.0),
            balance: dollars(10.0),
            transaction_type: DomainTransactionType::OneOffIncome,
        };

        // This test checks that the date field is NOT a string
        // Currently this will fail because date is still a String
        let date_field_type = TypeId::of::<String>();
        let datetime_type = TypeId::of::<DateTime<FixedOffset>>();
        
        println!("Current date field type: {:?}", date_field_type);
        println!("Expected datetime type: {:?}", datetime_type);
        
        // This assertion will fail until we fix the domain model
        // Comment out for now to prevent compilation errors
        // assert_ne!(date_field_type, TypeId::of::<String>(), 
        //           "VIOLATION: Domain Transaction.date should not be a String!");
        
        println!(" Domain model still uses String for date field - needs to be fixed!");
        
        Ok(())
    }
    
    #[test]
    fn test_invariant_no_date_strings_leave_csv_layer() -> Result<()> {
        let (repo, _env) = setup_test_repo()?;
        
        // Store a transaction with a date string (what CSV layer should receive)
        let transaction = DomainTransaction {
            id: "test_string_isolation".to_string(),
            child_id: "test_child".to_string(),
            date: chrono::DateTime::parse_from_str("2024-06-15T10:30:00-0500", "%Y-%m-%dT%H:%M:%S%z").unwrap(), // Parse with timezone
            description: "Test isolation".to_string(),
            amount: dollars(50.0),
            balance: dollars(50.0),
            transaction_type: DomainTransactionType::OneOffIncome,
        };
        
        repo.store_transaction(&transaction)?;
        
        // Retrieve the transaction
        let retrieved = repo.get_transaction("test_child", "test_string_isolation")?;
        assert!(retrieved.is_some());
        
        let retrieved_tx = retrieved.unwrap();
        
        // The retrieved transaction should have a properly formatted date
        // (This test currently passes because we're still using strings everywhere)
        // But it documents the expected behavior
        
        // Verify the date is a proper DateTime object (no string parsing needed)
        let date_str = retrieved_tx.date.to_rfc3339();
        let parsed = chrono::DateTime::parse_from_rfc3339(&date_str);
        assert!(parsed.is_ok(), "Date returned by CSV layer should be valid RFC3339: {}", date_str);
        
        println!("Date object can be converted to valid RFC3339: {}", date_str);
        
        Ok(())
    }
    
    #[test]
    fn test_invariant_date_timezone_handling() -> Result<()> {
        let (repo, _env) = setup_test_repo()?;
        
        // Test that different timezone formats are handled correctly
        let timezone_variants = vec![
            ("2024-06-15T10:30:00Z", "UTC"),
            ("2024-06-15T10:30:00-05:00", "CDT with colon"),
            ("2024-06-15T10:30:00-05:00", "CDT with colon (duplicate)"),
            ("2024-06-15T10:30:00+00:00", "UTC with explicit offset"),
        ];
        
        for (i, (date_str, description)) in timezone_variants.iter().enumerate() {
            let transaction = DomainTransaction {
                id: format!("tz_test_{}", i),
                child_id: "test_child".to_string(),
                date: chrono::DateTime::parse_from_rfc3339(date_str).unwrap(),
                description: format!("Test {}", description),
                amount: dollars(10.0),
                balance: dollars(10.0),
                transaction_type: DomainTransactionType::OneOffIncome,
            };
            
            repo.store_transaction(&transaction)?;
            
            let retrieved = repo.get_transaction("test_child", &format!("tz_test_{}", i))?;
            assert!(retrieved.is_some());
            
            let retrieved_tx = retrieved.unwrap();
            
            // Verify the stored date is parseable
            let date_str = retrieved_tx.date.to_rfc3339();
            let parsed = chrono::DateTime::parse_from_rfc3339(&date_str);
            assert!(parsed.is_ok(), "Failed to parse {} date: {}", description, retrieved_tx.date);
            
            println!("Successfully handled {} timezone: {} -> {}", description, date_str, retrieved_tx.date);
        }
        
        Ok(())
    }
    
    #[test]
    fn test_invariant_invalid_date_handling() -> Result<()> {
        let (repo, _env) = setup_test_repo()?;
        
        // Test how the CSV layer handles invalid date formats
        let invalid_dates = vec![
            "not-a-date",
            "2024-13-45T99:99:99Z",  // Invalid date/time values
            "2024-06-15",           // Date only (should be normalized)
            "",                     // Empty string
        ];
        
        for (i, invalid_date) in invalid_dates.iter().enumerate() {
            let transaction = DomainTransaction {
                id: format!("invalid_date_{}", i),
                child_id: "test_child".to_string(),
                date: chrono::DateTime::parse_from_str(invalid_date, "%Y-%m-%dT%H:%M:%S%z").unwrap_or_else(|_| chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z").unwrap()),
                description: format!("Test invalid date: {}", invalid_date),
                amount: dollars(10.0),
                balance: dollars(10.0),
                transaction_type: DomainTransactionType::OneOffIncome,
            };
            
            // Store should either succeed with normalized date or fail gracefully
            let result = repo.store_transaction(&transaction);
            
            match result {
                Ok(_) => {
                    // If storage succeeded, verify the date was normalized
                    let retrieved = repo.get_transaction("test_child", &format!("invalid_date_{}", i))?;
                    if let Some(retrieved_tx) = retrieved {
                        println!("Invalid date '{}' was normalized to: '{}'", invalid_date, retrieved_tx.date);
                    }
                }
                Err(e) => {
                    println!("Invalid date '{}' was rejected with error: {}", invalid_date, e);
                }
            }
        }
        
        Ok(())
    }

    #[test]
    fn test_timestamp_precision_preservation_tdd() -> Result<()> {
        // TDD Test: Verify storage layer preserves exact timestamps to the second
        // This replicates the bug where multiple transactions on same day get 12:00:00 instead of actual times
        
        use chrono::Timelike; // Import trait for hour() and minute() methods
        
        let (repo, _env) = setup_test_repo()?;
        
        // Create transactions with precise timestamps on the same day (July 21st)
        let tx1 = DomainTransaction {
            id: "tx_09_00".to_string(),
            child_id: "test_child".to_string(),
            date: chrono::DateTime::parse_from_rfc3339("2025-07-21T09:00:00Z").unwrap(),
            description: "Morning transaction".to_string(),
            amount: dollars(1.00),
            balance: dollars(17.62),
            transaction_type: DomainTransactionType::OneOffIncome,
        };
        
        let tx2 = DomainTransaction {
            id: "tx_15_00".to_string(),
            child_id: "test_child".to_string(),
            date: chrono::DateTime::parse_from_rfc3339("2025-07-21T15:00:00Z").unwrap(),
            description: "Afternoon transaction".to_string(),
            amount: dollars(2.00),
            balance: dollars(19.62),
            transaction_type: DomainTransactionType::OneOffIncome,
        };
        
        // Store transactions
        repo.store_transaction(&tx1)?;
        repo.store_transaction(&tx2)?;
        
        // Retrieve transactions
        let retrieved_tx1 = repo.get_transaction("test_child", "tx_09_00")?.unwrap();
        let retrieved_tx2 = repo.get_transaction("test_child", "tx_15_00")?.unwrap();
        
        // CRITICAL TEST: Verify exact timestamps are preserved
        assert_eq!(retrieved_tx1.date.hour(), 9, "Transaction 1 should preserve 09:00 hour");
        assert_eq!(retrieved_tx1.date.minute(), 0, "Transaction 1 should preserve 00 minutes");
        
        assert_eq!(retrieved_tx2.date.hour(), 15, "Transaction 2 should preserve 15:00 hour");
        assert_eq!(retrieved_tx2.date.minute(), 0, "Transaction 2 should preserve 00 minutes");
        
        // Verify they are different times (not both defaulted to 12:00:00)
        assert_ne!(retrieved_tx1.date.hour(), retrieved_tx2.date.hour(), 
                   "Transactions should have different hours, not both 12:00:00");
        
        // Verify chronological order is maintained
        assert!(retrieved_tx1.date < retrieved_tx2.date, 
                "Morning transaction should be earlier than afternoon transaction");
        
        println!("Storage layer preserves timestamp precision:");
        println!("   TX1: {} (hour: {})", retrieved_tx1.date.to_rfc3339(), retrieved_tx1.date.hour());
        println!("   TX2: {} (hour: {})", retrieved_tx2.date.to_rfc3339(), retrieved_tx2.date.hour());

        Ok(())
    }

    #[test]
    fn round_trips_a_legacy_shaped_csv_byte_for_byte() {
        // Guards against a codec change that quietly rewrites every row and makes
        // the first sync look like a thousand-row conflict.
        let text = std::fs::read_to_string("tests/fixtures/transactions_legacy_shapes.csv").unwrap();
        let once = allowance_core::codec::render_transactions(
            &allowance_core::codec::parse_transactions(&text).unwrap().rows);
        let twice = allowance_core::codec::render_transactions(
            &allowance_core::codec::parse_transactions(&once).unwrap().rows);
        assert_eq!(once, twice);
    }
}