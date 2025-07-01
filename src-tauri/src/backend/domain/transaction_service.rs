//! Transaction service domain logic for the allowance tracker.
//!
//! This module contains the core business logic for transaction management,
//! including CRUD operations, balance calculations, pagination, and data persistence.
//! It serves as the primary service layer for all transaction-related operations.
//!
//! ## Key Responsibilities
//!
//! - **Transaction CRUD**: Creating, reading, updating, and deleting transactions
//! - **Balance Management**: Calculating running balances from transaction history
//! - **Data Persistence**: Interfacing with storage layer for transaction data
//! - **Pagination**: Cursor-based pagination for efficient data retrieval
//! - **Business Rules**: Enforcing transaction validation and business constraints
//! - **Backdated Transactions**: Supporting historical transaction insertion with balance recalculation

//! - **Date Filtering**: Supporting date range queries for transaction lists
//!
//! ## Core Components
//!
//! - **TransactionService**: Main service orchestrating all transaction operations
//! - **Transaction**: Core domain entity representing allowance transactions
//! - **TransactionListRequest**: Query parameters for transaction retrieval
//! - **TransactionListResponse**: Paginated response with transaction data
//! - **CreateTransactionRequest**: Input data for creating new transactions
//!
//! ## Business Rules
//!
//! - Transactions must have non-empty descriptions (1-256 characters)
//! - Each transaction updates the running balance automatically
//! - Transactions are ordered chronologically (newest first)
//! - Unique transaction IDs are generated based on amount and timestamp
//! - Balance calculations consider all historical transactions
//! - Pagination uses cursor-based approach for consistent results
//! - Backdated transactions trigger balance recalculation for subsequent transactions
//!
//! ## Design Principles
//!
//! - **Domain-Driven Design**: Models real-world allowance transaction concepts
//! - **Storage Agnostic**: Works with any storage implementation via repositories
//! - **Async First**: All operations are asynchronous for better performance
//! - **Error Handling**: Comprehensive error handling with detailed messages
//! - **Testability**: Pure business logic with comprehensive test coverage


use anyhow::Result;
use log::{info, error};
use std::sync::Arc;
use shared::{
    CreateTransactionRequest, DeleteTransactionsRequest, DeleteTransactionsResponse,
    PaginationInfo, Transaction, TransactionListRequest, TransactionListResponse,
    TransactionType,
};
use crate::backend::storage::{Connection, TransactionStorage};
use crate::backend::domain::{child_service::ChildService, AllowanceService, BalanceService};
use std::time::{SystemTime, UNIX_EPOCH};
use chrono::{NaiveDate, Local, Datelike, FixedOffset, DateTime, ParseError, TimeZone};
use time;

#[derive(Clone)]
pub struct TransactionService<C: Connection> {
    transaction_repository: C::TransactionRepository,
    child_service: ChildService,
    allowance_service: AllowanceService,
    balance_service: BalanceService<C>,
}

impl<C: Connection> TransactionService<C> {
    pub fn new(connection: Arc<C>, child_service: ChildService, allowance_service: AllowanceService, balance_service: BalanceService<C>) -> Self {
        let transaction_repository = connection.create_transaction_repository();
        Self { transaction_repository, child_service, allowance_service, balance_service }
    }

    /// Create a new transaction with support for backdated transactions
    pub async fn create_transaction(&self, request: CreateTransactionRequest) -> Result<Transaction> {
        info!("🚀 TransactionService::create_transaction called with: {:?}", request);

        // Validate description length
        if request.description.is_empty() || request.description.len() > 256 {
            return Err(anyhow::anyhow!("Description must be between 1 and 256 characters"));
        }

        // Get the active child
        let active_child_response = self.child_service.get_active_child().await?;
        let active_child = active_child_response.active_child
            .ok_or_else(|| anyhow::anyhow!("No active child found"))?;

        info!("✅ Active child for transaction: {}", active_child.id);

        // Generate transaction ID and determine final date
        let now_millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_millis() as u64;
        
        let transaction_id = Transaction::generate_id(request.amount, now_millis);
        
        // Use provided date or generate current RFC 3339 timestamp in Eastern Time
        let transaction_date = match request.date {
            Some(date) => date,
            None => {
                // Generate RFC 3339 formatted timestamp in Eastern Time (assuming EST/EDT, UTC-5/-4)
                let now = SystemTime::now();
                let utc_datetime = time::OffsetDateTime::from(now);
                
                // Convert to Eastern Time (assuming EST for now, UTC-5)
                let eastern_offset = time::UtcOffset::from_hms(-5, 0, 0)?;
                let eastern_datetime = utc_datetime.to_offset(eastern_offset);
                
                eastern_datetime.format(&time::format_description::well_known::Rfc3339)?
            }
        };

        info!("📅 Transaction date: {}", transaction_date);

        // Calculate the correct balance for this transaction based on its date
        let transaction_balance = self.balance_service
            .calculate_balance_for_new_transaction(&active_child.id, &transaction_date, request.amount)
            .await?;

        info!("💰 Calculated balance: {:.2}", transaction_balance);

        let transaction = Transaction {
            id: transaction_id,
            child_id: active_child.id.clone(),
            date: transaction_date.clone(),
            description: request.description,
            amount: request.amount,
            balance: transaction_balance,
            transaction_type: if request.amount >= 0.0 { TransactionType::Income } else { TransactionType::Expense },
        };

        // Store the transaction in database
        info!("💾 Storing transaction in database...");
        self.transaction_repository.store_transaction(&transaction).await?;

        info!("✅ Transaction stored: {} with balance: {:.2}", transaction.id, transaction_balance);

        // Check if this is a backdated transaction that requires balance recalculation
        if self.balance_service.requires_balance_recalculation(&active_child.id, &transaction_date).await? {
            info!("📅 Backdated transaction detected, triggering balance recalculation from {}", transaction_date);
            
            // Recalculate balances for all transactions from this date forward
            let updated_count = self.balance_service
                .recalculate_balances_from_date(&active_child.id, &transaction_date)
                .await?;
            
            info!("✅ Recalculated {} transaction balances due to backdated transaction", updated_count);
        } else {
            info!("📅 Transaction is current, no balance recalculation needed");
        }

        Ok(transaction)
    }

    /// Check for and issue any pending allowances for the active child
    /// This should be called before returning transaction lists to ensure allowances are up to date
    pub async fn check_and_issue_pending_allowances(&self) -> Result<u32> {
        info!("🎯 Checking for pending allowances to issue");

        // Get the active child
        let active_child_response = self.child_service.get_active_child().await?;
        let active_child = match active_child_response.active_child {
            Some(child) => child,
            None => {
                info!("No active child found, skipping allowance check");
                return Ok(0);
            }
        };

        // Check for pending allowances in the last 7 days (to catch missed allowances)
        let current_date = Local::now().date_naive();
        let check_from_date = current_date - chrono::Duration::days(7);
        
        let pending_allowances = self.allowance_service
            .get_pending_allowance_dates(&active_child.id, check_from_date, current_date)
            .await?;

        let mut issued_count = 0;

        // Issue each pending allowance
        for (allowance_date, amount) in pending_allowances {
            match self.create_allowance_transaction(&active_child.id, allowance_date, amount).await {
                Ok(transaction) => {
                    info!("✅ Issued allowance: {} for ${:.2} on {}", 
                          transaction.id, amount, allowance_date);
                    issued_count += 1;
                }
                Err(e) => {
                    // Log error but continue with other allowances
                    error!("❌ Failed to issue allowance for {} on {}: {}", 
                           active_child.id, allowance_date, e);
                }
            }
        }

        if issued_count > 0 {
            info!("🎉 Successfully issued {} automatic allowances", issued_count);
        } else {
            info!("✓ No pending allowances to issue");
        }

        Ok(issued_count)
    }

    /// Create an allowance transaction for a specific date
    async fn create_allowance_transaction(&self, child_id: &str, date: NaiveDate, amount: f64) -> Result<Transaction> {
        info!("Creating allowance transaction for child {} on {} for ${:.2}", child_id, date, amount);

        // Generate transaction ID and RFC 3339 timestamp for the allowance date (12:00 PM)
        let now_millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_millis() as u64;
        
        let transaction_id = Transaction::generate_id(amount, now_millis);
        
        // Create RFC 3339 timestamp for 12:00 PM on the allowance date in Eastern Time
        let allowance_datetime = date.and_hms_opt(12, 0, 0).unwrap();
        let utc_datetime = allowance_datetime.and_utc();
        
        // Convert to Eastern Time (assuming EST for now, UTC-5)
        let eastern_offset = FixedOffset::west_opt(5 * 3600).unwrap(); // UTC-5
        let eastern_datetime = utc_datetime.with_timezone(&eastern_offset);
        let transaction_date = eastern_datetime.to_rfc3339();

        info!("📅 Allowance transaction date: {}", transaction_date);

        // Calculate the correct balance for this transaction based on its date
        let transaction_balance = self.balance_service
            .calculate_balance_for_new_transaction(child_id, &transaction_date, amount)
            .await?;

        info!("💰 Calculated allowance balance: {:.2}", transaction_balance);

        let transaction = Transaction {
            id: transaction_id,
            child_id: child_id.to_string(),
            date: transaction_date.clone(),
            description: "Weekly allowance".to_string(),
            amount,
            balance: transaction_balance,
            transaction_type: TransactionType::Income,
        };

        // Store the transaction in database
        info!("💾 Storing allowance transaction in database...");
        self.transaction_repository.store_transaction(&transaction).await?;

        info!("✅ Allowance transaction stored: {} with balance: {:.2}", transaction.id, transaction_balance);

        // Check if this is a backdated transaction that requires balance recalculation
        if self.balance_service.requires_balance_recalculation(child_id, &transaction_date).await? {
            info!("📅 Backdated allowance detected, triggering balance recalculation from {}", transaction_date);
            
            // Recalculate balances for all transactions from this date forward
            let updated_count = self.balance_service
                .recalculate_balances_from_date(child_id, &transaction_date)
                .await?;
            
            info!("✅ Recalculated {} transaction balances due to backdated allowance", updated_count);
        } else {
            info!("📅 Allowance transaction is current, no balance recalculation needed");
        }

        Ok(transaction)
    }

    /// List transactions with pagination and optional date filtering
    pub async fn list_transactions(&self, request: TransactionListRequest) -> Result<TransactionListResponse> {
        info!("Listing transactions with request: {:?}", request);

        // Check and issue any pending allowances before listing transactions
        // This ensures allowances are automatically issued when the app is used
        match self.check_and_issue_pending_allowances().await {
            Ok(issued_count) => {
                if issued_count > 0 {
                    info!("🎯 Issued {} pending allowances before listing transactions", issued_count);
                }
            }
            Err(e) => {
                // Log the error but don't fail the transaction listing
                error!("⚠️ Failed to check/issue pending allowances: {}. Continuing with transaction listing.", e);
            }
        }

        // Get the active child
        let active_child_response = self.child_service.get_active_child().await?;
        let active_child = active_child_response.active_child
            .ok_or_else(|| anyhow::anyhow!("No active child found"))?;

        // Set default limit if not provided (max 100)
        let limit = request.limit.unwrap_or(20).min(100);
        
        // Query one extra record to determine if there are more results
        let query_limit = limit + 1;

        // Get transactions from database for the active child
        let mut db_transactions = self.transaction_repository.list_transactions(&active_child.id, Some(query_limit), request.after).await?;

        // Generate future allowance transactions for the requested date range if specified
        let mut future_allowances = if let Some(end_date_str) = &request.end_date {
            // Parse the end date
            match chrono::DateTime::parse_from_rfc3339(end_date_str) {
                Ok(end_dt) => {
                    let end_date = end_dt.date_naive();
                    
                    // Use start_date if provided, otherwise use current date (today)
                    let start_date = if let Some(start_date_str) = &request.start_date {
                        match chrono::DateTime::parse_from_rfc3339(start_date_str) {
                            Ok(start_dt) => start_dt.date_naive(),
                            Err(_) => {
                                info!("Failed to parse start_date: {}, using current date", start_date_str);
                                chrono::Local::now().date_naive()
                            }
                        }
                    } else {
                        // No start_date specified, use current date for comprehensive historical fetching
                        chrono::Local::now().date_naive()
                    };
                    
                    info!("🔍 TRANSACTION SERVICE: Generating future allowances for date range {} to {}", start_date, end_date);
                    
                    self.allowance_service
                        .generate_future_allowance_transactions(&active_child.id, start_date, end_date)
                        .await?
                }
                Err(_) => {
                    info!("Failed to parse end_date: {}, skipping future allowances", end_date_str);
                    Vec::new()
                }
            }
        } else {
            // No end_date specified, don't generate future allowances
            info!("No end_date specified, skipping future allowances");
            Vec::new()
        };

        // Calculate proper balances for future allowances
        if !future_allowances.is_empty() {
            // Get the current balance from the latest real transaction
            let current_balance = match self.transaction_repository.get_latest_transaction(&active_child.id).await? {
                Some(latest) => latest.balance,
                None => 0.0,
            };

            // Sort future allowances by date (chronological order)
            future_allowances.sort_by(|a, b| a.date.cmp(&b.date));

            // Calculate projected balances for future allowances
            let mut running_balance = current_balance;
            for future_allowance in &mut future_allowances {
                running_balance += future_allowance.amount;
                future_allowance.balance = running_balance;
                info!("🔍 FUTURE BALANCE: Set future allowance {} balance to ${:.2}", 
                     future_allowance.id, running_balance);
            }
        }

        // Combine database transactions with future allowances
        db_transactions.extend(future_allowances);

        // Sort all transactions by date (newest first) - now simple string sorting works since all dates are RFC 3339
        db_transactions.sort_by(|a, b| b.date.cmp(&a.date));

        // Apply pagination to the combined list
        let has_more = db_transactions.len() > limit as usize;
        if has_more {
            db_transactions.truncate(limit as usize);
        }

        let next_cursor = if has_more {
            db_transactions.last().map(|t| t.id.clone())
        } else {
            None
        };

        let response = TransactionListResponse {
            transactions: db_transactions,
            pagination: PaginationInfo {
                has_more,
                next_cursor,
            },
        };

        // Debug log the transactions being returned
                // debug!("🔍 TRANSACTION DEBUG: Returning {} transactions (including future allowances), has_more: {}", response.transactions.len(), has_more);
        // for (i, transaction) in response.transactions.iter().enumerate() {
        //     debug!("🔍 Transaction {}: id={}, date={}, description={}, amount={}, type={:?}",
        //         i + 1, transaction.id, transaction.date, transaction.description, transaction.amount, transaction.transaction_type);
        // }
        
        Ok(response)
    }

    /// Delete multiple transactions
    pub async fn delete_transactions(&self, request: DeleteTransactionsRequest) -> Result<DeleteTransactionsResponse> {
        info!("Deleting {} transactions: {:?}", request.transaction_ids.len(), request.transaction_ids);

        if request.transaction_ids.is_empty() {
            return Ok(DeleteTransactionsResponse {
                deleted_count: 0,
                success_message: "No transactions to delete".to_string(),
                not_found_ids: vec![],
            });
        }

        // Get the active child
        let active_child_response = self.child_service.get_active_child().await?;
        let active_child = active_child_response.active_child
            .ok_or_else(|| anyhow::anyhow!("No active child found"))?;

        // Check which transactions actually exist for this child
        let existing_ids = self.transaction_repository.check_transactions_exist(&active_child.id, &request.transaction_ids).await?;
        let not_found_ids: Vec<String> = request.transaction_ids
            .iter()
            .filter(|id| !existing_ids.contains(id))
            .cloned()
            .collect();

        // Delete the existing transactions for this child
        let deleted_count = if !existing_ids.is_empty() {
            self.transaction_repository.delete_transactions(&active_child.id, &existing_ids).await?
        } else {
            0
        };

        let success_message = match deleted_count {
            0 => "No transactions were deleted".to_string(),
            1 => "1 transaction deleted successfully".to_string(),
            n => format!("{} transactions deleted successfully", n),
        };

        info!("Deleted {} transactions, {} not found", deleted_count, not_found_ids.len());

        Ok(DeleteTransactionsResponse {
            deleted_count: deleted_count as usize,
            success_message,
            not_found_ids,
        })
    }
}

#[cfg(test)]
impl<C: Connection> TransactionService<C> {
    /// Generate mock transaction data for testing
    fn generate_mock_transactions(&self, child_id: &str) -> Vec<Transaction> {
        use std::time::{SystemTime, UNIX_EPOCH};
        
        let base_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        vec![
            // June 2025 transactions (most recent first)
            Transaction {
                id: Transaction::generate_id(10.0, base_time - 86400000), // June 13, 2025
                child_id: child_id.to_string(),
                date: "2025-06-13T09:00:00-04:00".to_string(),
                description: "Weekly allowance".to_string(),
                amount: 10.0,
                balance: 55.0,
                transaction_type: TransactionType::Income,
            },
            Transaction {
                id: Transaction::generate_id(15.0, base_time - 259200000), // June 10, 2025  
                child_id: child_id.to_string(),
                date: "2025-06-10T15:30:00-04:00".to_string(),
                description: "Gift from Grandma".to_string(),
                amount: 15.0,
                balance: 45.0,
                transaction_type: TransactionType::Income,
            },
            Transaction {
                id: Transaction::generate_id(-12.0, base_time - 432000000), // June 8, 2025
                child_id: child_id.to_string(),
                date: "2025-06-08T14:20:15-04:00".to_string(),
                description: "Bought new toy".to_string(),
                amount: -12.0,
                balance: 30.0,
                transaction_type: TransactionType::Expense,
            },
            Transaction {
                id: Transaction::generate_id(10.0, base_time - 604800000), // June 6, 2025
                child_id: child_id.to_string(),
                date: "2025-06-06T09:00:00-04:00".to_string(),
                description: "Weekly allowance".to_string(),
                amount: 10.0,
                balance: 42.0,
                transaction_type: TransactionType::Income,
            },
            Transaction {
                id: Transaction::generate_id(-5.0, base_time - 777600000), // June 4, 2025
                child_id: child_id.to_string(),
                date: "2025-06-04T16:45:30-04:00".to_string(),
                description: "Movie ticket".to_string(),
                amount: -5.0,
                balance: 32.0,
                transaction_type: TransactionType::Expense,
            },
            Transaction {
                id: Transaction::generate_id(10.0, base_time - 1209600000), // May 30, 2025
                child_id: child_id.to_string(),
                date: "2025-05-30T09:00:00-04:00".to_string(),
                description: "Weekly allowance".to_string(),
                amount: 10.0,
                balance: 37.0,
                transaction_type: TransactionType::Income,
            },
            Transaction {
                id: Transaction::generate_id(-3.0, base_time - 1382400000), // May 28, 2025
                child_id: child_id.to_string(),
                date: "2025-05-28T13:15:22-04:00".to_string(),
                description: "Ice cream treat".to_string(),
                amount: -3.0,
                balance: 27.0,
                transaction_type: TransactionType::Expense,
            },
            Transaction {
                id: Transaction::generate_id(10.0, base_time - 1814400000), // May 23, 2025
                child_id: child_id.to_string(),
                date: "2025-05-23T09:00:00-04:00".to_string(),
                description: "Weekly allowance".to_string(),
                amount: 10.0,
                balance: 30.0,
                transaction_type: TransactionType::Income,
            },
            Transaction {
                id: Transaction::generate_id(-8.0, base_time - 2073600000), // May 20, 2025
                child_id: child_id.to_string(),
                date: "2025-05-20T11:30:45-04:00".to_string(),
                description: "Comic book".to_string(),
                amount: -8.0,
                balance: 20.0,
                transaction_type: TransactionType::Expense,
            },
            Transaction {
                id: Transaction::generate_id(10.0, base_time - 2419200000), // May 16, 2025
                child_id: child_id.to_string(),
                date: "2025-05-16T09:00:00-04:00".to_string(),
                description: "Weekly allowance".to_string(),
                amount: 10.0,
                balance: 28.0,
                transaction_type: TransactionType::Income,
            },
            Transaction {
                id: Transaction::generate_id(20.0, base_time - 2592000000), // May 14, 2025
                child_id: child_id.to_string(),
                date: "2025-05-14T10:00:00-04:00".to_string(),
                description: "Birthday money from Uncle Bob".to_string(),
                amount: 20.0,
                balance: 18.0,
                transaction_type: TransactionType::Income,
            },
            Transaction {
                id: Transaction::generate_id(10.0, base_time - 3024000000), // May 9, 2025
                child_id: child_id.to_string(),
                date: "2025-05-09T09:00:00-04:00".to_string(),
                description: "Weekly allowance".to_string(),
                amount: 10.0,
                balance: -2.0, // Negative balance from previous spending
                transaction_type: TransactionType::Income,
            },
            Transaction {
                id: Transaction::generate_id(-15.0, base_time - 3196800000), // May 7, 2025
                child_id: child_id.to_string(),
                date: "2025-05-07T14:22:10-04:00".to_string(),
                description: "Art supplies".to_string(),
                amount: -15.0,
                balance: -12.0,
                transaction_type: TransactionType::Expense,
            },
            Transaction {
                id: Transaction::generate_id(10.0, base_time - 3628800000), // May 2, 2025
                child_id: child_id.to_string(),
                date: "2025-05-02T09:00:00-04:00".to_string(),
                description: "Weekly allowance".to_string(),
                amount: 10.0,
                balance: 3.0,
                transaction_type: TransactionType::Income,
            },
        ]
    }

    /// Apply cursor-based filtering (transactions after the given cursor) - for testing
    fn apply_cursor_filter(&self, transactions: Vec<Transaction>, after_cursor: &str) -> Result<Vec<Transaction>> {
        // Parse the cursor timestamp for comparison
        let (_, cursor_timestamp) = Transaction::parse_id(after_cursor)
            .map_err(|e| anyhow::anyhow!("Invalid cursor format: {}", e))?;

        // Filter transactions that come after the cursor timestamp
        let filtered: Vec<Transaction> = transactions
            .into_iter()
            .filter(|tx| {
                if let Ok(tx_timestamp) = tx.extract_timestamp() {
                    tx_timestamp < cursor_timestamp // Reverse chronological order (newest first)
                } else {
                    false
                }
            })
            .collect();

        Ok(filtered)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use crate::backend::storage::csv::CsvConnection;
    use crate::backend::{ChildService, AllowanceService, BalanceService};

    async fn create_test_service() -> TransactionService<CsvConnection> {
        let db = Arc::new(CsvConnection::new_default().expect("Failed to init test DB"));
        
        // Create required service dependencies
        let child_service = ChildService::new(db.clone());
        let allowance_service = AllowanceService::new(db.clone());
        let balance_service = BalanceService::new(db.clone());
        
        TransactionService::new(db, child_service, allowance_service, balance_service)
    }

    #[tokio::test]
    async fn test_list_transactions_default() {
        let service = create_test_service().await;
        
        let request = TransactionListRequest {
            after: None,
            limit: None,
            start_date: None,
            end_date: None,
        };

        let response = service.list_transactions(request).await.unwrap();
        
        // Should return default limit (20) or fewer
        assert!(response.transactions.len() <= 20);
        
        // Should be sorted in reverse chronological order (newest first)
        for i in 1..response.transactions.len() {
            let prev_timestamp = response.transactions[i - 1].extract_timestamp().unwrap();
            let curr_timestamp = response.transactions[i].extract_timestamp().unwrap();
            assert!(prev_timestamp > curr_timestamp, "Transactions should be in reverse chronological order");
        }
    }

    #[tokio::test]
    async fn test_list_transactions_with_limit() {
        let service = create_test_service().await;
        
        let request = TransactionListRequest {
            after: None,
            limit: Some(2),
            start_date: None,
            end_date: None,
        };

        let response = service.list_transactions(request).await.unwrap();
        
        // Should respect the limit
        assert!(response.transactions.len() <= 2);
    }

    #[tokio::test]
    async fn test_list_transactions_with_cursor() {
        let service = create_test_service().await;
        
        // Create some test transactions to work with
        for i in 1..=5 {
            let request = CreateTransactionRequest {
                description: format!("Test transaction {}", i),
                amount: 10.0 * i as f64,
                date: None,
            };
            service.create_transaction(request).await.unwrap();
            
            // Small delay to ensure different timestamps
            tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
        }
        
        // First, get initial transactions
        let first_request = TransactionListRequest {
            after: None,
            limit: Some(2),
            start_date: None,
            end_date: None,
        };

        let first_response = service.list_transactions(first_request).await.unwrap();
        assert!(!first_response.transactions.is_empty());

        // Use the last transaction as cursor for next page
        let cursor = &first_response.transactions.last().unwrap().id;
        
        let second_request = TransactionListRequest {
            after: Some(cursor.clone()),
            limit: Some(2),
            start_date: None,
            end_date: None,
        };

        let second_response = service.list_transactions(second_request).await.unwrap();
        
        // Should not include the cursor transaction itself
        for tx in &second_response.transactions {
            assert_ne!(tx.id, *cursor);
        }
    }

    #[tokio::test]
    async fn test_cursor_filter() {
        let service = create_test_service().await;
        let mock_transactions = service.generate_mock_transactions("test_child_123");
        
        // Use the second transaction as cursor
        let cursor = &mock_transactions[1].id;
        
        let filtered = service.apply_cursor_filter(mock_transactions.clone(), cursor).unwrap();
        
        // Should only include transactions after the cursor
        let cursor_timestamp = Transaction::parse_id(cursor).unwrap().1;
        
        for tx in &filtered {
            let tx_timestamp = tx.extract_timestamp().unwrap();
            assert!(tx_timestamp < cursor_timestamp, "All returned transactions should be after cursor");
        }
    }

    #[tokio::test]
    async fn test_transaction_id_generation_consistency() {
        let service = create_test_service().await;
        let transactions = service.generate_mock_transactions("test_child_123");
        
        for tx in &transactions {
            // Should be able to parse the generated ID
            let (tx_type, timestamp) = Transaction::parse_id(&tx.id).unwrap();
            
            // Type should match the amount
            if tx.amount < 0.0 {
                assert_eq!(tx_type, "expense");
            } else {
                assert_eq!(tx_type, "income");
            }
            
            // Should be able to extract timestamp
            assert_eq!(tx.extract_timestamp().unwrap(), timestamp);
        }
    }

    #[tokio::test]
    async fn test_pagination_info() {
        let service = create_test_service().await;
        
        // Request with limit smaller than available data
        let request = TransactionListRequest {
            after: None,
            limit: Some(2),
            start_date: None,
            end_date: None,
        };

        let response = service.list_transactions(request).await.unwrap();
        
        if response.transactions.len() == 2 {
            // If we got the full limit, there might be more
            if response.pagination.has_more {
                assert!(response.pagination.next_cursor.is_some());
            }
        }
    }

    #[tokio::test]
    async fn test_limit_bounds() {
        let service = create_test_service().await;

        // Test with very high limit (should be capped at 100)
        let request = TransactionListRequest {
            after: None,
            limit: Some(1000),
            start_date: None,
            end_date: None,
        };

        let response = service.list_transactions(request).await.unwrap();
        // Should cap at 100 items max
        assert!(response.transactions.len() <= 100);
    }

    #[tokio::test]
    async fn test_create_transaction_basic() {
        let service = create_test_service().await;

        let request = CreateTransactionRequest {
            description: "Test allowance".to_string(),
            amount: 10.0,
            date: None, // Will use current timestamp
        };

        let transaction = service.create_transaction(request).await.unwrap();

        assert_eq!(transaction.description, "Test allowance");
        assert_eq!(transaction.amount, 10.0);
        assert_eq!(transaction.balance, 10.0); // First transaction, starting from 0
        assert!(transaction.id.starts_with("transaction::income::"));
        assert!(!transaction.date.is_empty());
    }

    #[tokio::test]
    async fn test_create_transaction_running_balance() {
        let service = create_test_service().await;

        // Create first transaction (income)
        let request1 = CreateTransactionRequest {
            description: "First allowance".to_string(),
            amount: 10.0,
            date: None,
        };
        let tx1 = service.create_transaction(request1).await.unwrap();
        assert_eq!(tx1.balance, 10.0);

        // Create second transaction (expense)
        let request2 = CreateTransactionRequest {
            description: "Buy snack".to_string(),
            amount: -3.0,
            date: None,
        };
        let tx2 = service.create_transaction(request2).await.unwrap();
        assert_eq!(tx2.balance, 7.0); // 10.0 - 3.0

        // Create third transaction (income)
        let request3 = CreateTransactionRequest {
            description: "Second allowance".to_string(),
            amount: 15.0,
            date: None,
        };
        let tx3 = service.create_transaction(request3).await.unwrap();
        assert_eq!(tx3.balance, 22.0); // 7.0 + 15.0
    }

    #[tokio::test]
    async fn test_create_transaction_with_custom_date() {
        let service = create_test_service().await;

        let custom_date = "2025-06-14T10:30:00-04:00".to_string();
        let request = CreateTransactionRequest {
            description: "Custom date transaction".to_string(),
            amount: 5.0,
            date: Some(custom_date.clone()),
        };

        let transaction = service.create_transaction(request).await.unwrap();
        assert_eq!(transaction.date, custom_date);
    }

    #[tokio::test]
    async fn test_create_transaction_validation() {
        let service = create_test_service().await;

        // Test empty description
        let request = CreateTransactionRequest {
            description: "".to_string(),
            amount: 10.0,
            date: None,
        };
        let result = service.create_transaction(request).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Description must be between 1 and 256 characters"));

        // Test too long description
        let long_description = "a".repeat(257);
        let request = CreateTransactionRequest {
            description: long_description,
            amount: 10.0,
            date: None,
        };
        let result = service.create_transaction(request).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Description must be between 1 and 256 characters"));
    }

    #[tokio::test]
    async fn test_create_transaction_negative_balance() {
        let service = create_test_service().await;

        // Create transaction that results in negative balance
        let request = CreateTransactionRequest {
            description: "Expensive purchase".to_string(),
            amount: -25.0,
            date: None,
        };

        let transaction = service.create_transaction(request).await.unwrap();
        assert_eq!(transaction.balance, -25.0); // Starting from 0, goes negative
        assert!(transaction.id.starts_with("transaction::expense::"));
    }

    #[tokio::test]
    async fn test_list_transactions_with_database_data() {
        let service = create_test_service().await;

        // Create some transactions
        let transactions = vec![
            CreateTransactionRequest {
                description: "First transaction".to_string(),
                amount: 10.0,
                date: None,
            },
            CreateTransactionRequest {
                description: "Second transaction".to_string(),
                amount: -5.0,
                date: None,
            },
            CreateTransactionRequest {
                description: "Third transaction".to_string(),
                amount: 20.0,
                date: None,
            },
        ];

        // Store transactions in database
        for req in transactions {
            service.create_transaction(req).await.unwrap();
        }

        // List transactions
        let request = TransactionListRequest {
            after: None,
            limit: Some(10),
            start_date: None,
            end_date: None,
        };

        let response = service.list_transactions(request).await.unwrap();
        assert_eq!(response.transactions.len(), 3);
        
        // Should be in reverse chronological order (newest first)
        assert_eq!(response.transactions[0].description, "Third transaction");
        assert_eq!(response.transactions[1].description, "Second transaction");
        assert_eq!(response.transactions[2].description, "First transaction");

        // Check running balances
        assert_eq!(response.transactions[0].balance, 25.0); // 10 - 5 + 20
        assert_eq!(response.transactions[1].balance, 5.0);  // 10 - 5
        assert_eq!(response.transactions[2].balance, 10.0); // 10
    }

    #[tokio::test]
    async fn test_delete_transactions_basic() {
        let service = create_test_service().await;
        
        // Create some test transactions with different timestamps to avoid ID collisions
        let tx1 = service.create_transaction(CreateTransactionRequest {
            description: "Transaction 1".to_string(),
            amount: 10.0,
            date: Some("2025-01-01T10:00:00-05:00".to_string()),
        }).await.unwrap();
        
        // Small delay to ensure different timestamps
        tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
        
        let tx2 = service.create_transaction(CreateTransactionRequest {
            description: "Transaction 2".to_string(),
            amount: 20.0,
            date: Some("2025-01-01T11:00:00-05:00".to_string()),
        }).await.unwrap();
        
        tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
        
        let tx3 = service.create_transaction(CreateTransactionRequest {
            description: "Transaction 3".to_string(),
            amount: -5.0,
            date: Some("2025-01-01T12:00:00-05:00".to_string()),
        }).await.unwrap();
        
        // Verify they exist
        let list_response = service.list_transactions(TransactionListRequest {
            after: None,
            limit: Some(10),
            start_date: None,
            end_date: None,
        }).await.unwrap();
        assert_eq!(list_response.transactions.len(), 3);
        
        // Delete two transactions
        let delete_request = DeleteTransactionsRequest {
            transaction_ids: vec![tx1.id.clone(), tx3.id.clone()],
        };
        
        let delete_response = service.delete_transactions(delete_request).await.unwrap();
        assert_eq!(delete_response.deleted_count, 2);
        assert_eq!(delete_response.success_message, "2 transactions deleted successfully");
        assert!(delete_response.not_found_ids.is_empty());
        
        // Verify only tx2 remains
        let remaining_transactions = service.list_transactions(TransactionListRequest {
            after: None,
            limit: Some(10),
            start_date: None,
            end_date: None,
        }).await.unwrap();
        assert_eq!(remaining_transactions.transactions.len(), 1);
        assert_eq!(remaining_transactions.transactions[0].id, tx2.id);
    }

    #[tokio::test]
    async fn test_delete_transactions_empty_list() {
        let service = create_test_service().await;
        
        let delete_request = DeleteTransactionsRequest {
            transaction_ids: vec![],
        };
        
        let delete_response = service.delete_transactions(delete_request).await.unwrap();
        assert_eq!(delete_response.deleted_count, 0);
        assert_eq!(delete_response.success_message, "No transactions to delete");
        assert!(delete_response.not_found_ids.is_empty());
    }

    #[tokio::test]
    async fn test_delete_transactions_not_found() {
        let service = create_test_service().await;
        
        // Create one transaction with specific date to avoid ID collisions
        let tx1 = service.create_transaction(CreateTransactionRequest {
            description: "Transaction 1".to_string(),
            amount: 10.0,
            date: Some("2025-01-02T10:00:00-05:00".to_string()),
        }).await.unwrap();
        
        // Try to delete mix of existing and non-existing transactions
        let delete_request = DeleteTransactionsRequest {
            transaction_ids: vec![
                tx1.id.clone(),
                "nonexistent1".to_string(),
                "nonexistent2".to_string(),
            ],
        };
        
        let delete_response = service.delete_transactions(delete_request).await.unwrap();
        assert_eq!(delete_response.deleted_count, 1);
        assert_eq!(delete_response.success_message, "1 transaction deleted successfully");
        assert_eq!(delete_response.not_found_ids.len(), 2);
        assert!(delete_response.not_found_ids.contains(&"nonexistent1".to_string()));
        assert!(delete_response.not_found_ids.contains(&"nonexistent2".to_string()));
        
        // Verify the existing transaction was deleted
        let remaining_transactions = service.list_transactions(TransactionListRequest {
            after: None,
            limit: Some(10),
            start_date: None,
            end_date: None,
        }).await.unwrap();
        
        // Since we created one transaction and deleted it, there should be 0 database transactions
        // The service might fall back to mock data if no database transactions exist
        // So we need to check if the specific transaction we created is gone
        let found_deleted_tx = remaining_transactions.transactions.iter()
            .any(|tx| tx.id == tx1.id);
        assert!(!found_deleted_tx, "Deleted transaction should not be found in results");
    }

    #[tokio::test]
    async fn test_delete_transactions_none_found() {
        let service = create_test_service().await;
        
        // Try to delete transactions that don't exist
        let request = DeleteTransactionsRequest {
            transaction_ids: vec!["nonexistent1".to_string(), "nonexistent2".to_string()],
        };

        let response = service.delete_transactions(request).await.unwrap();
        
        assert_eq!(response.deleted_count, 0);
        assert_eq!(response.not_found_ids.len(), 2);
        assert!(response.not_found_ids.contains(&"nonexistent1".to_string()));
        assert!(response.not_found_ids.contains(&"nonexistent2".to_string()));
        assert_eq!(response.success_message, "No transactions were deleted");
    }

    #[tokio::test]
    async fn test_calendar_integration_with_child_scoping() {
        let service = create_test_service().await;
        
        // Test that calendar integration works with child-scoped transactions
        let request = TransactionListRequest {
            after: None,
            limit: Some(10),
            start_date: Some("2025-06-01T00:00:00Z".to_string()),
            end_date: Some("2025-06-30T23:59:59Z".to_string()),
        };

        let response = service.list_transactions(request).await.unwrap();
        
        // Should have mock transactions + future allowances for June
        assert!(!response.transactions.is_empty());
        
        // Verify child scoping - all transactions should have the same child_id
        let first_child_id = &response.transactions[0].child_id;
        for transaction in &response.transactions {
            assert_eq!(&transaction.child_id, first_child_id);
        }
    }

    #[tokio::test]
    async fn test_cross_month_balance_forwarding() {
        let service = create_test_service().await;

        // Simulate getting transactions up to July 31, 2025 (for August starting balance calculation)
        let july_request = TransactionListRequest {
            after: None,
            limit: Some(1000),
            start_date: None, // From beginning of time
            end_date: Some("2025-07-31T23:59:59Z".to_string()), // Up to end of July
        };

        let july_response = service.list_transactions(july_request).await.unwrap();
        
        // Should include both real transactions and future allowances
        assert!(!july_response.transactions.is_empty());
        
        // Find the most recent transaction before August (should be from July)
        let mut sorted_transactions = july_response.transactions.clone();
        sorted_transactions.sort_by(|a, b| b.date.cmp(&a.date)); // Newest first
        
        // Find most recent transaction before August 1, 2025
        let mut august_starting_balance = 0.0;
        for transaction in &sorted_transactions {
            if transaction.date.as_str() < "2025-08-01T00:00:00Z" {
                august_starting_balance = transaction.balance;
                info!("Found starting balance for August: ${:.2} from transaction {} on {}", 
                      august_starting_balance, transaction.id, transaction.date);
                break;
            }
        }
        
        // August should start with July's ending balance (not June's)
        // Since we have future allowances in July, this should be > the last June transaction
        assert!(august_starting_balance > 0.0, "August should have a positive starting balance from July");
        
        // Verify that we have future allowances in the response
        let future_allowances: Vec<_> = july_response.transactions.iter()
            .filter(|t| t.transaction_type == TransactionType::FutureAllowance)
            .collect();
        assert!(!future_allowances.is_empty(), "Should have future allowances for July");
        
        // Verify future allowances have proper balance progression
        let mut july_allowances: Vec<_> = future_allowances.into_iter()
            .filter(|t| t.date.starts_with("2025-07"))
            .collect();
        july_allowances.sort_by(|a, b| a.date.cmp(&b.date));
        
        if july_allowances.len() > 1 {
            for window in july_allowances.windows(2) {
                let earlier = &window[0];
                let later = &window[1];
                assert!(later.balance > earlier.balance, 
                       "Later allowance should have higher balance: {} vs {}", 
                       later.balance, earlier.balance);
            }
        }
    }

    #[tokio::test]
    async fn test_future_allowance_balance_calculation() {
        let service = create_test_service().await;

        // Test that future allowances get proper projected balances
        let future_request = TransactionListRequest {
            after: None,
            limit: Some(1000),
            start_date: None,
            end_date: Some("2025-12-31T23:59:59Z".to_string()), // Far into future
        };

        let response = service.list_transactions(future_request).await.unwrap();
        
        let future_allowances: Vec<_> = response.transactions.iter()
            .filter(|t| t.transaction_type == TransactionType::FutureAllowance)
            .collect();
        
        if !future_allowances.is_empty() {
            // Verify all future allowances have non-zero balances
            for allowance in &future_allowances {
                assert!(allowance.balance > 0.0, 
                       "Future allowance {} should have positive balance, got {}", 
                       allowance.id, allowance.balance);
            }
            
            // Verify balances are progressive (later allowances have higher balances)
            let mut sorted_allowances = future_allowances.clone();
            sorted_allowances.sort_by(|a, b| a.date.cmp(&b.date));
            
            for window in sorted_allowances.windows(2) {
                let earlier = window[0];
                let later = window[1];
                assert!(later.balance >= earlier.balance, 
                       "Later allowance should have >= balance: {} >= {}", 
                       later.balance, earlier.balance);
            }
        }
    }

    #[tokio::test]
    async fn test_historical_fetch_without_start_date() {
        let service = create_test_service().await;

        // Test that omitting start_date still generates future allowances when end_date is present
        let request = TransactionListRequest {
            after: None,
            limit: Some(1000),
            start_date: None, // No start date - should use current date
            end_date: Some("2025-08-31T23:59:59Z".to_string()),
        };

        let response = service.list_transactions(request).await.unwrap();
        
        // Should have generated future allowances despite no start_date
        let future_allowances: Vec<_> = response.transactions.iter()
            .filter(|t| t.transaction_type == TransactionType::FutureAllowance)
            .collect();
        
        assert!(!future_allowances.is_empty(), 
               "Should generate future allowances even without start_date");
        
        // Verify future allowances are within the expected range (current date to end_date)
        let current_date = chrono::Local::now().date_naive();
        for allowance in &future_allowances {
            let allowance_date = allowance.date.split('T').next().unwrap();
            let current_date_str = current_date.format("%Y-%m-%d").to_string();
            assert!(allowance_date >= current_date_str.as_str(),
                   "Future allowance date {} should be >= current date {}", 
                   allowance_date, current_date);
            assert!(allowance_date <= "2025-08-31",
                   "Future allowance date {} should be <= end date", allowance_date);
        }
    }

    #[tokio::test]
    async fn test_multi_month_progression() {
        let service = create_test_service().await;

        // Test progression across multiple months (June -> July -> August)
        let comprehensive_request = TransactionListRequest {
            after: None,
            limit: Some(2000),
            start_date: None,
            end_date: Some("2025-08-31T23:59:59Z".to_string()),
        };

        let response = service.list_transactions(comprehensive_request).await.unwrap();
        
        // Group transactions by month
        let mut june_transactions = Vec::new();
        let mut july_transactions = Vec::new();
        let mut august_transactions = Vec::new();
        
        for transaction in &response.transactions {
            let date = &transaction.date;
            if date.starts_with("2025-06") {
                june_transactions.push(transaction);
            } else if date.starts_with("2025-07") {
                july_transactions.push(transaction);
            } else if date.starts_with("2025-08") {
                august_transactions.push(transaction);
            }
        }
        
        // Should have transactions in all months
        assert!(!june_transactions.is_empty(), "Should have June transactions");
        assert!(!july_transactions.is_empty(), "Should have July transactions (future allowances)");
        assert!(!august_transactions.is_empty(), "Should have August transactions (future allowances)");
        
        // Find ending balance of each month
        let june_end_balance = june_transactions.iter()
            .max_by_key(|t| &t.date)
            .map(|t| t.balance)
            .unwrap_or(0.0);
        
        let july_start_balance = july_transactions.iter()
            .min_by_key(|t| &t.date)
            .map(|t| {
                // For future allowances, the balance shown is AFTER the allowance is added
                // So we need to subtract the amount to get the starting balance
                t.balance - t.amount
            })
            .unwrap_or(0.0);
        
        // July should start where June ended (within allowance amount tolerance)
        let allowance_amount = 5.0; // Expected weekly allowance
        assert!((july_start_balance - june_end_balance).abs() < allowance_amount,
               "July starting balance ({:.2}) should be close to June ending balance ({:.2})",
               july_start_balance, june_end_balance);
        
        info!("✅ Multi-month progression test passed: June end: ${:.2}, July start: ${:.2}", 
              june_end_balance, july_start_balance);
    }

    #[tokio::test]
    async fn test_precise_cross_month_balance_forwarding() {
        let service = create_test_service().await;

        // Test that July calendar correctly starts with June's ending balance
        let june_request = TransactionListRequest {
            after: None,
            limit: Some(1000),
            start_date: None,
            end_date: Some("2025-06-30T23:59:59Z".to_string()),
        };

        let june_response = service.list_transactions(june_request).await.unwrap();
        
        // Find June's ending balance (from latest June transaction)
        let june_transactions: Vec<_> = june_response.transactions.iter()
            .filter(|t| t.date.starts_with("2025-06"))
            .collect();
        
        let june_end_balance = june_transactions.iter()
            .max_by_key(|t| &t.date)
            .map(|t| t.balance)
            .unwrap_or(0.0);
        
        // Now test July starting balance
        let july_request = TransactionListRequest {
            after: None,
            limit: Some(1000),
            start_date: Some("2025-07-01T00:00:00Z".to_string()),
            end_date: Some("2025-07-31T23:59:59Z".to_string()),
        };

        let july_response = service.list_transactions(july_request).await.unwrap();
        
        let july_transactions: Vec<_> = july_response.transactions.iter()
            .filter(|t| t.date.starts_with("2025-07"))
            .collect();
        
        let july_start_balance = july_transactions.iter()
            .min_by_key(|t| &t.date)
            .map(|t| {
                // For future allowances, the balance shown is AFTER the allowance is added
                // So we need to subtract the amount to get the starting balance
                t.balance - t.amount
            })
            .unwrap_or(0.0);
        
        // July should start where June ended (within allowance amount tolerance)
        let allowance_amount = 5.0; // Expected weekly allowance
        assert!((july_start_balance - june_end_balance).abs() < allowance_amount,
               "July starting balance ({:.2}) should be close to June ending balance ({:.2})",
               july_start_balance, june_end_balance);
        
        info!("✅ Multi-month progression test passed: June end: ${:.2}, July start: ${:.2}", 
              june_end_balance, july_start_balance);
    }

    #[tokio::test]
    async fn test_check_and_issue_pending_allowances_no_child() {
        let service = create_test_service().await;
        
        // Test when no active child is set
        let issued_count = service
            .check_and_issue_pending_allowances()
            .await
            .expect("Should handle no active child gracefully");
        
        assert_eq!(issued_count, 0, "Should issue 0 allowances when no active child");
    }

    #[tokio::test]
    async fn test_check_and_issue_pending_allowances_no_config() {
        let service = create_test_service().await;
        
        // Create and set active child
        let child = create_test_child(&service).await;
        let _ = service.child_service.set_active_child(shared::SetActiveChildRequest {
            child_id: child.id.clone(),
        }).await.expect("Failed to set active child");
        
        // Test when no allowance config exists
        let issued_count = service
            .check_and_issue_pending_allowances()
            .await
            .expect("Should handle no allowance config gracefully");
        
        assert_eq!(issued_count, 0, "Should issue 0 allowances when no config exists");
    }

    #[tokio::test]
    async fn test_check_and_issue_pending_allowances_inactive_config() {
        let service = create_test_service().await;
        
        // Create and set active child
        let child = create_test_child(&service).await;
        let _ = service.child_service.set_active_child(shared::SetActiveChildRequest {
            child_id: child.id.clone(),
        }).await.expect("Failed to set active child");
        
        // Create inactive allowance config
        let _ = service.allowance_service.update_allowance_config(shared::UpdateAllowanceConfigRequest {
            child_id: Some(child.id.clone()),
            amount: 10.0,
            day_of_week: 1, // Monday
            is_active: false, // Inactive
        }).await.expect("Failed to create allowance config");
        
        // Test when allowance config is inactive
        let issued_count = service
            .check_and_issue_pending_allowances()
            .await
            .expect("Should handle inactive config gracefully");
        
        assert_eq!(issued_count, 0, "Should issue 0 allowances when config is inactive");
    }

    #[tokio::test]
    async fn test_check_and_issue_pending_allowances_with_pending() {
        let service = create_test_service().await;
        
        // Create and set active child
        let child = create_test_child(&service).await;
        let _ = service.child_service.set_active_child(shared::SetActiveChildRequest {
            child_id: child.id.clone(),
        }).await.expect("Failed to set active child");
        
        // Create active allowance config for today's day of week
        let today = Local::now().date_naive();
        let today_day_of_week = today.weekday().num_days_from_sunday() as u8;
        
        let _ = service.allowance_service.update_allowance_config(shared::UpdateAllowanceConfigRequest {
            child_id: Some(child.id.clone()),
            amount: 7.50,
            day_of_week: today_day_of_week,
            is_active: true,
        }).await.expect("Failed to create allowance config");
        
        // Test that allowance gets issued for today
        let issued_count = service
            .check_and_issue_pending_allowances()
            .await
            .expect("Should issue pending allowances");
        
        assert_eq!(issued_count, 1, "Should issue 1 allowance for today");
        
        // Verify the allowance was actually created
        let transactions = service.list_transactions(TransactionListRequest {
            after: None,
            limit: Some(10),
            start_date: None,
            end_date: None,
        }).await.expect("Failed to list transactions");
        
        let allowance_transactions: Vec<_> = transactions.transactions.iter()
            .filter(|t| t.description.contains("allowance"))
            .collect();
        
        assert_eq!(allowance_transactions.len(), 1, "Should have created 1 allowance transaction");
        assert_eq!(allowance_transactions[0].amount, 7.50, "Allowance amount should match config");
        assert_eq!(allowance_transactions[0].transaction_type, TransactionType::Income, "Allowance should be income type");
    }

    #[tokio::test]
    async fn test_check_and_issue_pending_allowances_no_duplicates() {
        let service = create_test_service().await;
        
        // Create and set active child
        let child = create_test_child(&service).await;
        let _ = service.child_service.set_active_child(shared::SetActiveChildRequest {
            child_id: child.id.clone(),
        }).await.expect("Failed to set active child");
        
        // Create active allowance config for today's day of week
        let today = Local::now().date_naive();
        let today_day_of_week = today.weekday().num_days_from_sunday() as u8;
        
        let _ = service.allowance_service.update_allowance_config(shared::UpdateAllowanceConfigRequest {
            child_id: Some(child.id.clone()),
            amount: 5.0,
            day_of_week: today_day_of_week,
            is_active: true,
        }).await.expect("Failed to create allowance config");
        
        // First call should issue allowance
        let first_issued = service
            .check_and_issue_pending_allowances()
            .await
            .expect("Should issue pending allowances on first call");
        
        assert_eq!(first_issued, 1, "Should issue 1 allowance on first call");
        
        // Second call should not issue duplicate allowance
        let second_issued = service
            .check_and_issue_pending_allowances()
            .await
            .expect("Should not fail on second call");
        
        assert_eq!(second_issued, 0, "Should not issue duplicate allowance on second call");
        
        // Verify only one allowance transaction exists
        let transactions = service.list_transactions(TransactionListRequest {
            after: None,
            limit: Some(10),
            start_date: None,
            end_date: None,
        }).await.expect("Failed to list transactions");
        
        let allowance_transactions: Vec<_> = transactions.transactions.iter()
            .filter(|t| t.description.contains("allowance"))
            .collect();
        
        assert_eq!(allowance_transactions.len(), 1, "Should have exactly 1 allowance transaction, no duplicates");
    }

    #[tokio::test]
    async fn test_create_allowance_transaction() {
        let service = create_test_service().await;
        
        // Create and set active child
        let child = create_test_child(&service).await;
        let _ = service.child_service.set_active_child(shared::SetActiveChildRequest {
            child_id: child.id.clone(),
        }).await.expect("Failed to set active child");
        
        let allowance_date = Local::now().date_naive();
        let allowance_amount = 12.50;
        
        // Create allowance transaction
        let transaction = service
            .create_allowance_transaction(&child.id, allowance_date, allowance_amount)
            .await
            .expect("Should create allowance transaction");
        
        assert_eq!(transaction.child_id, child.id);
        assert_eq!(transaction.amount, allowance_amount);
        assert_eq!(transaction.description, "Weekly allowance");
        assert_eq!(transaction.transaction_type, TransactionType::Income);
        assert!(transaction.date.contains(&allowance_date.format("%Y-%m-%d").to_string()));
        assert!(transaction.balance >= allowance_amount, "Balance should be at least the allowance amount");
        
        // Verify transaction was stored
        let stored_transactions = service.transaction_repository
            .list_transactions(&child.id, None, None)
            .await
            .expect("Failed to list stored transactions");
        
        let stored_allowance = stored_transactions.iter()
            .find(|t| t.id == transaction.id)
            .expect("Allowance transaction should be stored");
        
        assert_eq!(stored_allowance.amount, allowance_amount);
        assert_eq!(stored_allowance.description, "Weekly allowance");
    }

    #[tokio::test]
    async fn test_list_transactions_triggers_allowance_check() {
        let service = create_test_service().await;
        
        // Create and set active child
        let child = create_test_child(&service).await;
        let _ = service.child_service.set_active_child(shared::SetActiveChildRequest {
            child_id: child.id.clone(),
        }).await.expect("Failed to set active child");
        
        // Create active allowance config for today's day of week
        let today = Local::now().date_naive();
        let today_day_of_week = today.weekday().num_days_from_sunday() as u8;
        
        let _ = service.allowance_service.update_allowance_config(shared::UpdateAllowanceConfigRequest {
            child_id: Some(child.id.clone()),
            amount: 8.75,
            day_of_week: today_day_of_week,
            is_active: true,
        }).await.expect("Failed to create allowance config");
        
        // Initially should have no transactions
        let initial_transactions = service.transaction_repository
            .list_transactions(&child.id, None, None)
            .await
            .expect("Failed to list initial transactions");
        
        assert!(initial_transactions.is_empty(), "Should start with no transactions");
        
        // Call list_transactions - this should trigger allowance check and issue allowance
        let response = service.list_transactions(TransactionListRequest {
            after: None,
            limit: Some(10),
            start_date: None,
            end_date: None,
        }).await.expect("Failed to list transactions");
        
        // Should now have the issued allowance
        let allowance_transactions: Vec<_> = response.transactions.iter()
            .filter(|t| t.description.contains("allowance"))
            .collect();
        
        assert_eq!(allowance_transactions.len(), 1, "Should have automatically issued allowance");
        assert_eq!(allowance_transactions[0].amount, 8.75, "Auto-issued allowance should match config");
    }

    // Helper function to create a test child
    async fn create_test_child(service: &TransactionService<CsvConnection>) -> shared::Child {
        let request = shared::CreateChildRequest {
            name: format!("Test Child {}", chrono::Utc::now().timestamp_millis()),
            birthdate: "2015-01-01".to_string(),
        };
        let response = service.child_service.create_child(request).await
            .expect("Failed to create test child");
        response.child
    }
}
