//! Transaction service domain logic for the allowance tracker.
use crate::backend::{
    domain::{
        allowance_service::AllowanceService,
        balance_service::BalanceService,
        child_service::ChildService,
        email_service::{EmailServiceWrapper, EmailConfig},
        models::{
            child::Child as DomainChild,
            transaction::{Transaction as DomainTransaction, TransactionType as DomainTransactionType},
        },
    },
    storage::csv::{CsvConnection, TransactionRepository},
    storage::traits::TransactionStorage,
};
use crate::backend::domain::commands::transactions::{CreateTransactionCommand, TransactionListQuery, TransactionListResult, DeleteTransactionsCommand, DeleteTransactionsResult, PaginationInfo as DomainPagination, CalendarTransactionsQuery, CalendarTransactionsResult};
use anyhow::{anyhow, Result};
use chrono::{Local, NaiveDate};
use log::{error, info, debug};
use std::sync::Arc;
use shared::sync::{SyncEvent, SyncAction, SyncSource, EntityType};
use crate::backend::domain::SyncNotifier;

use std::time::{SystemTime, UNIX_EPOCH};

pub struct TransactionService {
    transaction_repository: TransactionRepository,
    child_service: ChildService,
    allowance_service: AllowanceService,
    balance_service: BalanceService,
    email_service: Option<EmailServiceWrapper>,
    sync_notifier: Option<SyncNotifier>,
}

impl TransactionService {
    pub fn new(
        connection: Arc<CsvConnection>,
        child_service: ChildService,
        allowance_service: AllowanceService,
        balance_service: BalanceService,
        sync_notifier: Option<SyncNotifier>,
    ) -> Self {
        let transaction_repository = TransactionRepository::new((*connection).clone());
        Self {
            transaction_repository,
            child_service,
            allowance_service,
            balance_service,
            email_service: None,
            sync_notifier,
        }
    }

    pub fn with_email_service(
        connection: Arc<CsvConnection>,
        child_service: ChildService,
        allowance_service: AllowanceService,
        balance_service: BalanceService,
        email_config: EmailConfig,
        sync_notifier: Option<SyncNotifier>,
    ) -> Result<Self> {
        let transaction_repository = TransactionRepository::new((*connection).clone());
        let email_service = EmailServiceWrapper::new(email_config)?;
        Ok(Self {
            transaction_repository,
            child_service,
            allowance_service,
            balance_service,
            email_service: Some(email_service),
            sync_notifier,
        })
    }

    fn notify_sync(&self, entity_type: EntityType, entity_id: &str, child_id: &str, action: SyncAction) {
        if let Some(ref notifier) = self.sync_notifier {
            notifier.notify(SyncEvent::new(
                entity_type,
                entity_id.to_string(),
                child_id.to_string(),
                action,
                SyncSource::Local,
            ));
        }
    }

    pub fn create_transaction_domain(
        &self,
        command: CreateTransactionCommand,
    ) -> Result<DomainTransaction> {
        // Validate description length here (moving logic from DTO layer)
        if command.description.is_empty() || command.description.len() > 256 {
            return Err(anyhow!("Description must be between 1 and 256 characters"));
        }

        let active_child = self.get_active_child()?;
        
        // FIXED: Use DateTime object directly from command (no parsing needed)
        let transaction_date = command.date.unwrap_or_else(|| {
            // Use system local timezone (handles DST automatically)
            chrono::Local::now().fixed_offset()
        });

        let transaction = self.create_transaction_internal(
            &active_child.id,
            transaction_date,
            command.description,
            command.amount,
        )?;

        // Send email notification if email service is configured
        if let Some(email_service) = &self.email_service {
            log::info!("Email service is configured, sending notification for transaction: {}", transaction.id);
            let action = if transaction.amount >= 0.0 { "earned" } else { "spent" };
            let current_balance = self.balance_service.get_current_balance(&active_child.id)?;
            log::info!("Sending email notification: {} ${:.2} for {}", action, transaction.amount.abs(), active_child.name);
            if let Err(e) = email_service.send_transaction_notification(&transaction, &active_child, action, current_balance) {
                error!("Failed to send transaction notification email: {}", e);
            } else {
                log::info!("Email notification sent successfully!");
            }
        } else {
            log::info!("No email service configured, skipping notification");
        }

        Ok(transaction)
    }

    /// Private unified function for creating any transaction
    fn create_transaction_internal(
        &self,
        child_id: &str,
        date: chrono::DateTime<chrono::FixedOffset>,
        description: String,
        amount: f64,
    ) -> Result<DomainTransaction> {
        let now_millis = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64;
        let transaction_id = DomainTransaction::generate_id(amount, now_millis);

        let transaction_balance = self
            .balance_service
            .calculate_balance_for_new_transaction(
                child_id,
                &date.to_rfc3339(),
                amount,
            )?;

        let domain_transaction = DomainTransaction {
            id: transaction_id,
            child_id: child_id.to_string(),
            date,
            description,
            amount,
            balance: transaction_balance,
            transaction_type: if amount >= 0.0 {
                DomainTransactionType::OneOffIncome
            } else {
                DomainTransactionType::Expense
            },
        };

        self.transaction_repository
            .store_transaction(&domain_transaction)?;

        self.notify_sync(EntityType::Transaction, &domain_transaction.id, child_id, SyncAction::Created);

        if self
            .balance_service
            .requires_balance_recalculation(child_id, &date.to_rfc3339())?
        {
            self.balance_service
                .recalculate_balances_from_date(child_id, &date.to_rfc3339())?;
        }

        Ok(domain_transaction)
    }

    pub fn create_transaction(
        &self,
        cmd: CreateTransactionCommand,
    ) -> Result<DomainTransaction> {
        self.create_transaction_domain(cmd)
    }

    pub fn list_transactions_domain(
        &self,
        query: TransactionListQuery,
    ) -> Result<TransactionListResult> {
        let active_child = self.get_active_child()?;

        let limit = query.limit.unwrap_or(20);
        let query_limit = limit + 1;

        // Decide which repository method to use based on date filters
        let mut db_transactions = if query.start_date.is_some() || query.end_date.is_some() {
            // Fetch chronologically within range then reverse so newest first
            let mut txs = self
                .transaction_repository
                .list_transactions_chronological(&active_child.id, query.start_date.clone(), query.end_date.clone())?;
            txs.reverse();
            // Apply cursor & limit manually (after is applied after reversing because IDs unique)
            if let Some(after_id) = query.after.clone() {
                if let Some(idx) = txs.iter().position(|t| t.id == after_id) {
                    txs = txs.into_iter().skip(idx + 1).collect();
                }
            }
            if let Some(lim) = query.limit {
                txs.truncate(lim as usize + 1); // +1 to detect has_more later
            }
            txs
        } else {
            self
                .transaction_repository
                .list_transactions(&active_child.id, Some(query_limit), query.after)?
        };

        db_transactions.sort_by(|a, b| b.date.cmp(&a.date));

        let has_more = db_transactions.len() > limit as usize;
        if has_more {
            db_transactions.truncate(limit as usize);
        }

        let next_cursor = if has_more {
            db_transactions.last().map(|t| t.id.clone())
        } else {
            None
        };

        Ok(TransactionListResult {
            transactions: db_transactions,
            pagination: DomainPagination { has_more, next_cursor },
        })
    }

    pub fn list_transactions(
        &self,
        query: TransactionListQuery,
    ) -> Result<TransactionListResult> {
        self.list_transactions_domain(query)
    }



    /// List transactions for calendar display, including future allowances
    /// This method orchestrates getting historical transactions and generating
    /// future allowances for the specified month
    pub fn list_transactions_for_calendar(
        &self,
        query: CalendarTransactionsQuery,
    ) -> Result<CalendarTransactionsResult> {
        debug!("🗓️ Getting transactions for calendar: month={}, year={}", query.month, query.year);

        // Get the active child
        let active_child = self.get_active_child()?;

        // Calculate days in month for end date
        let days_in_month = match query.month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 => {
                // Check for leap year
                let year = query.year as i32;
                if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) {
                    29
                } else {
                    28
                }
            }
            _ => return Err(anyhow!("Invalid month: {}", query.month)),
        };

        // Get historical transactions up to the end of the requested month
        let end_date = format!("{:04}-{:02}-{:02}T23:59:59Z", query.year, query.month, days_in_month);
        debug!("🗓️ Fetching historical transactions up to: {}", end_date);

        let transaction_query = TransactionListQuery {
            after: None,
            limit: Some(10000), // Get all transactions for calendar
            start_date: None,
            end_date: Some(end_date),
        };

        let transaction_result = self.list_transactions_domain(transaction_query)?;
        let mut all_transactions = transaction_result.transactions;
        
        debug!("🗓️ Found {} historical transactions", all_transactions.len());

        // Generate future allowances for the requested month
        let start_date = match NaiveDate::from_ymd_opt(query.year as i32, query.month, 1) {
            Some(date) => date,
            None => return Err(anyhow!("Invalid date: {}/{}", query.month, query.year)),
        };
        
        let end_date = match NaiveDate::from_ymd_opt(query.year as i32, query.month, days_in_month) {
            Some(date) => date,
            None => return Err(anyhow!("Invalid end date: {}/{}/{}", query.month, days_in_month, query.year)),
        };

        debug!("🗓️ Generating future allowances for child {} from {} to {}", 
              active_child.id, start_date, end_date);

        match self.allowance_service.generate_future_allowance_transactions(&active_child.id, start_date, end_date) {
            Ok(future_allowances) => {
                debug!("🗓️ Generated {} future allowances", future_allowances.len());
                for (i, allowance) in future_allowances.iter().enumerate().take(3) {
                    debug!("🗓️ Future allowance {}: id={}, date={}, amount={}", 
                         i + 1, allowance.id, allowance.date, allowance.amount);
                }
                if future_allowances.len() > 3 {
                    debug!("🗓️ ... and {} more future allowances", future_allowances.len() - 3);
                }
                all_transactions.extend(future_allowances);
            },
            Err(e) => {
                error!("Failed to generate future allowances: {}", e);
                // Continue without future allowances rather than failing
            }
        }

        debug!("🗓️ Total transactions for calendar: {}", all_transactions.len());

        Ok(CalendarTransactionsResult {
            transactions: all_transactions,
        })
    }

    pub fn delete_transactions_domain(
        &self,
        cmd: DeleteTransactionsCommand,
    ) -> Result<DeleteTransactionsResult> {
        let active_child = self.get_active_child()?;
        let existing_ids = self
            .transaction_repository
            .check_transactions_exist(&active_child.id, &cmd.transaction_ids)?;
        let not_found_ids: Vec<String> = cmd
            .transaction_ids
            .iter()
            .filter(|id| !existing_ids.contains(id))
            .cloned()
            .collect();

        // Fetch transactions before deleting them for email notifications
        let transactions_to_delete = if !existing_ids.is_empty() {
            self.transaction_repository
                .list_transactions_by_ids(&active_child.id, &existing_ids)?
        } else {
            Vec::new()
        };

        let deleted_count = if !existing_ids.is_empty() {
            self.transaction_repository
                .delete_transactions(&active_child.id, &existing_ids)?
        } else {
            0
        };

        if deleted_count > 0 {
            self.balance_service
                .recalculate_balances_from_date(&active_child.id, "1970-01-01T00:00:00Z")?;
            for id in &existing_ids {
                self.notify_sync(EntityType::Transaction, id, &active_child.id, SyncAction::Deleted);
            }
        }

        // Send email notifications for deleted transactions
        if let Some(email_service) = &self.email_service {
            let current_balance = self.balance_service.get_current_balance(&active_child.id)?;
            for transaction in &transactions_to_delete {
                if let Err(e) = email_service.send_transaction_deleted_notification(transaction, &active_child, current_balance) {
                    error!("Failed to send transaction deletion notification email: {}", e);
                }
            }
        }

        let success_message = match deleted_count {
            0 => "No transactions were deleted".to_string(),
            1 => "1 transaction deleted successfully".to_string(),
            n => format!("{} transactions deleted successfully", n),
        };

        Ok(DeleteTransactionsResult {
            deleted_count: deleted_count as usize,
            success_message,
            not_found_ids,
        })
    }

    pub fn delete_transactions(
        &self,
        cmd: DeleteTransactionsCommand,
    ) -> Result<DeleteTransactionsResult> {
        self.delete_transactions_domain(cmd)
    }

    /// Check for and issue any pending allowances
    /// This should be called on app startup or on a scheduled basis
    pub fn check_and_issue_pending_allowances(&self) -> Result<u32> {
        debug!("ALLOWANCE DEBUG: check_and_issue_pending_allowances() called");
        if let Ok(active_child) = self.get_active_child() {
            debug!("ALLOWANCE DEBUG: Found active child: {}", active_child.id);
            let current_date = Local::now().naive_local().date();
            let check_from_date = current_date - chrono::Duration::days(90);
            debug!("ALLOWANCE DEBUG: Checking allowances from {} to {}", check_from_date, current_date);

            let pending_allowances = match self.allowance_service.get_pending_allowance_dates(&active_child.id, check_from_date, current_date) {
                Ok(dates) => dates,
                Err(e) => {
                    error!("ALLOWANCE DEBUG: Failed to get pending allowance dates: {}", e);
                    return Ok(0);
                }
            };
            debug!("ALLOWANCE DEBUG: Found {} pending allowances", pending_allowances.len());
            
            let mut issued_count = 0;
            for (allowance_date, amount) in pending_allowances {
                debug!("ALLOWANCE DEBUG: About to create allowance for {} (${:.2})", allowance_date, amount);
                match self
                    .create_allowance_transaction(&active_child.id, allowance_date, amount)
                {
                    Ok(transaction) => {
                        info!(
                            "ALLOWANCE DEBUG: Successfully issued allowance: {} for ${:.2} on {}",
                            transaction.id, amount, allowance_date
                        );
                        issued_count += 1;
                    }
                    Err(e) => {
                        error!(
                            "ALLOWANCE DEBUG: Failed to issue allowance for {} on {}: {}",
                            active_child.id, allowance_date, e
                        );
                    }
                }
            }
            debug!("ALLOWANCE DEBUG: Total allowances issued: {}", issued_count);
            return Ok(issued_count);
        } else {
            debug!("ALLOWANCE DEBUG: No active child found");
        }
        Ok(0)
    }

    fn create_allowance_transaction(
        &self,
        child_id: &str,
        date: NaiveDate,
        amount: f64,
    ) -> Result<DomainTransaction> {
        debug!("ALLOWANCE DEBUG: create_allowance_transaction() called for child {}, date {}, amount ${:.2}", child_id, date, amount);

        // Convert NaiveDate to DateTime at noon Eastern time
        let allowance_datetime = date.and_hms_opt(12, 0, 0).unwrap();
        let utc_datetime = chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(
            allowance_datetime,
            chrono::Utc,
        );
        let eastern_offset = chrono::FixedOffset::west_opt(5 * 3600).unwrap();
        let eastern_datetime = utc_datetime.with_timezone(&eastern_offset);
        debug!("ALLOWANCE DEBUG: Transaction date: {}", eastern_datetime.to_rfc3339());

        let now_millis = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64;
        let transaction_id = DomainTransaction::generate_id(amount, now_millis);

        let transaction_balance = self
            .balance_service
            .calculate_balance_for_new_transaction(
                child_id,
                &eastern_datetime.to_rfc3339(),
                amount,
            )?;

        let domain_transaction = DomainTransaction {
            id: transaction_id,
            child_id: child_id.to_string(),
            date: eastern_datetime,
            description: "Weekly allowance".to_string(),
            amount,
            balance: transaction_balance,
            transaction_type: DomainTransactionType::Allowance,
        };

        self.transaction_repository
            .store_transaction(&domain_transaction)?;

        self.notify_sync(EntityType::Transaction, &domain_transaction.id, child_id, SyncAction::Created);

        if self
            .balance_service
            .requires_balance_recalculation(child_id, &eastern_datetime.to_rfc3339())?
        {
            self.balance_service
                .recalculate_balances_from_date(child_id, &eastern_datetime.to_rfc3339())?;
        }

        debug!("ALLOWANCE DEBUG: create_allowance_transaction() completed for {}", domain_transaction.id);

        Ok(domain_transaction)
    }

    pub fn get_active_child(&self) -> Result<DomainChild> {
        self.child_service
            .get_active_child()?
            .active_child.child
            .ok_or_else(|| anyhow!("No active child found."))
    }

    /// Create a balance service for projected balance calculations
    pub fn create_balance_service(&self) -> &BalanceService {
        &self.balance_service
    }

    /// List all transactions for a specific child_id (used by backfill)
    pub fn list_all_transactions_for_child(
        &self,
        child_id: &str,
    ) -> Result<Vec<crate::backend::domain::models::transaction::Transaction>> {
        self.transaction_repository
            .list_transactions_chronological(child_id, None, None)
    }

    // ====================
    // SYNC SUPPORT METHODS
    // ====================

    /// Read a single transaction by child_id and transaction_id.
    /// Used by the UI thread to respond to ReadEntityRequest from the sync thread.
    pub fn get_transaction_by_id(
        &self,
        child_id: &str,
        transaction_id: &str,
    ) -> Result<Option<DomainTransaction>> {
        self.transaction_repository.get_transaction(child_id, transaction_id)
    }

    /// Upsert a transaction that arrived from the remote (applied by the UI thread).
    ///
    /// **Critical:** does NOT call `notify_sync` — writing a remote-sourced entity
    /// back through the notifier would create a sync loop.
    pub fn upsert_transaction_from_sync(&self, transaction: &DomainTransaction) -> Result<()> {
        self.transaction_repository.store_transaction(transaction)
    }

    /// Delete a transaction by ID without firing the sync notifier.
    /// Used to apply remote deletes locally. Idempotent: a remote delete for a
    /// transaction that doesn't exist locally is logged but not an error.
    pub fn delete_transaction_by_id(&self, child_id: &str, transaction_id: &str) -> Result<()> {
        let removed = self.transaction_repository.delete_transaction(child_id, transaction_id)?;
        if !removed {
            log::debug!("delete_transaction_by_id: {transaction_id} not found for {child_id}; skipping");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{
        domain::{
            models::child::Child as DomainChild,
            commands::child::{CreateChildCommand, SetActiveChildCommand},
        },
        storage::{
            csv::{connection::CsvConnection},
        },
    };
    // Tests now use domain models instead of shared types

    fn create_test_service() -> (TransactionService, Arc<CsvConnection>, tempfile::TempDir) {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let connection = Arc::new(CsvConnection::new(temp_dir.path()).unwrap());
        let child_service = ChildService::new(connection.clone(), None);
        let allowance_service = AllowanceService::new(connection.clone());
        let balance_service = BalanceService::new(connection.clone());
        let transaction_service = TransactionService::new(
            connection.clone(),
            child_service,
            allowance_service,
            balance_service,
            None,
        );
        (transaction_service, connection, temp_dir)
    }

    fn create_test_child(
        child_service: &ChildService,
        child_name: &str,
    ) -> Result<DomainChild> {
        let create_child_command = CreateChildCommand {
            name: child_name.to_string(),
            birthdate: "2015-01-01".to_string(),
        };
        let result = child_service.create_child(create_child_command)?;
        Ok(result.child)
    }

    #[test]
    fn test_create_transaction_basic() {
        let (service, _conn, _temp_dir) = create_test_service();
        let test_child = create_test_child(&service.child_service, "test_child").unwrap();
        let set_active_command = SetActiveChildCommand {
            child_id: test_child.id.clone(),
        };
        service
            .child_service
            .set_active_child(set_active_command)
            
            .unwrap();

        let cmd = CreateTransactionCommand {
            amount: 10.0,
            description: "Test transaction".to_string(),
            date: None,
        };
        let transaction = service.create_transaction(cmd).unwrap();
        assert_eq!(transaction.amount, 10.0);
        assert_eq!(transaction.description, "Test transaction");
        assert_eq!(transaction.balance, 10.0);
        assert_eq!(transaction.transaction_type, DomainTransactionType::OneOffIncome);
    }

    #[test]
    fn test_multiple_list_calls_create_duplicate_allowances() {
        use chrono::Datelike;
        
        let (service, _conn, _temp_dir) = create_test_service();
        let child = create_test_child(&service.child_service, "Test Child").expect("Failed to create test child");

        // Set as active child
        let set_active_cmd = crate::backend::domain::commands::child::SetActiveChildCommand {
            child_id: child.id.clone(),
        };
        service.child_service.set_active_child(set_active_cmd).expect("Failed to set active child");

        // Create allowance config for today
        let today = chrono::Local::now().date_naive();
        let day_of_week = today.weekday().num_days_from_sunday() as u8;

        let allowance_cmd = crate::backend::domain::commands::allowance::UpdateAllowanceConfigCommand {
            child_id: Some(child.id.clone()),
            amount: 10.0,
            day_of_week,
            is_active: true,
            use_age_based_amount: false,
        };
        service.allowance_service.update_allowance_config(allowance_cmd).expect("Failed to create allowance config");

        // First call to list_transactions_domain - should create one allowance
        let query1 = TransactionListQuery {
            after: None,
            limit: Some(10),
            start_date: None,
            end_date: None,
        };
        
        let result1 = service.list_transactions_domain(query1).expect("Failed to list transactions (call 1)");
        println!("First call returned {} transactions", result1.transactions.len());

        // Second call immediately after - should NOT create another allowance
        let query2 = TransactionListQuery {
            after: None,
            limit: Some(10),
            start_date: None,
            end_date: None,
        };
        
        let result2 = service.list_transactions_domain(query2).expect("Failed to list transactions (call 2)");
        println!("Second call returned {} transactions", result2.transactions.len());

        // Count how many allowance transactions were created
        let allowance_transactions: Vec<_> = result2.transactions
            .iter()
            .filter(|tx| tx.description.to_lowercase().contains("allowance"))
            .collect();

        println!("Found {} allowance transactions", allowance_transactions.len());
        
        // Should have NO allowance transactions since we removed automatic allowance creation
        assert_eq!(allowance_transactions.len(), 0, "Should have no allowance transactions since automatic creation was removed");
    }

    #[test]
    fn test_explicit_allowance_check_creates_allowance() {
        use chrono::Datelike;
        
        let (service, _conn, _temp_dir) = create_test_service();
        let child = create_test_child(&service.child_service, "Test Child").expect("Failed to create test child");

        // Set as active child
        let set_active_cmd = crate::backend::domain::commands::child::SetActiveChildCommand {
            child_id: child.id.clone(),
        };
        service.child_service.set_active_child(set_active_cmd).expect("Failed to set active child");

        // Create allowance config for today
        let today = chrono::Local::now().date_naive();
        let day_of_week = today.weekday().num_days_from_sunday() as u8;

        let allowance_cmd = crate::backend::domain::commands::allowance::UpdateAllowanceConfigCommand {
            child_id: Some(child.id.clone()),
            amount: 10.0,
            day_of_week,
            is_active: true,
            use_age_based_amount: false,
        };
        service.allowance_service.update_allowance_config(allowance_cmd).expect("Failed to create allowance config");

        // First, verify no allowance transactions exist
        let query = TransactionListQuery {
            after: None,
            limit: Some(10),
            start_date: None,
            end_date: None,
        };
        
        let result_before = service.list_transactions_domain(query.clone()).expect("Failed to list transactions before allowance check");
        let allowance_count_before: Vec<_> = result_before.transactions
            .iter()
            .filter(|tx| tx.description.to_lowercase().contains("allowance"))
            .collect();
        
        assert_eq!(allowance_count_before.len(), 0, "Should have no allowance transactions before explicit check");

        // Now explicitly check for pending allowances
        let issued_count = service.check_and_issue_pending_allowances().expect("Failed to check pending allowances");
        
        // The method checks the last 7 days, so it might issue multiple allowances if there were missed days
        assert!(issued_count >= 1, "Should have issued at least one allowance");

        // Verify the allowance was created
        let result_after = service.list_transactions_domain(query).expect("Failed to list transactions after allowance check");
        let allowance_count_after: Vec<_> = result_after.transactions
            .iter()
            .filter(|tx| tx.description.to_lowercase().contains("allowance"))
            .collect();
        
        assert!(allowance_count_after.len() >= 1, "Should have at least one allowance transaction after explicit check");
        
        // Verify the allowance details
        if let Some(allowance) = allowance_count_after.first() {
            assert_eq!(allowance.amount, 10.0, "Allowance amount should be $10");
            assert!(allowance.description.to_lowercase().contains("allowance"), "Should be an allowance transaction");
        }
    }
}