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
//!
//! ## Design Principles
//!
//! - **Domain-Driven Design**: Models real-world allowance transaction concepts
//! - **Storage Agnostic**: Works with any storage implementation via repositories
//! - **Async First**: All operations are asynchronous for better performance
//! - **Error Handling**: Comprehensive error handling with detailed messages
//! - **Testability**: Pure business logic with comprehensive test coverage


use crate::backend::storage::{DbConnection, TransactionRepository};
use crate::backend::domain::child_service::ChildService;
use crate::backend::domain::allowance_service::AllowanceService;
use shared::{Transaction, TransactionType, TransactionListRequest, TransactionListResponse, PaginationInfo, CreateTransactionRequest, DeleteTransactionsRequest, DeleteTransactionsResponse};
use anyhow::Result;
use tracing::info;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use chrono::{NaiveDate, Local, Datelike};

#[derive(Clone)]
pub struct TransactionService {
    transaction_repository: TransactionRepository,
    child_service: ChildService,
    allowance_service: AllowanceService,
}

impl TransactionService {
    pub fn new(db: Arc<DbConnection>) -> Self {
        let transaction_repository = TransactionRepository::new((*db).clone());
        let child_service = ChildService::new(db.clone());
        let allowance_service = AllowanceService::new(db);
        Self { transaction_repository, child_service, allowance_service }
    }

    /// Create a new transaction
    pub async fn create_transaction(&self, request: CreateTransactionRequest) -> Result<Transaction> {
        info!("Creating transaction: {:?}", request);

        // Validate description length
        if request.description.is_empty() || request.description.len() > 256 {
            return Err(anyhow::anyhow!("Description must be between 1 and 256 characters"));
        }

        // Get the active child
        let active_child_response = self.child_service.get_active_child().await?;
        let active_child = active_child_response.active_child
            .ok_or_else(|| anyhow::anyhow!("No active child found"))?;

        // Get current balance from latest transaction for this child
        let current_balance = match self.transaction_repository.get_latest_transaction(&active_child.id).await? {
            Some(latest) => latest.balance,
            None => 0.0, // First transaction starts at 0
        };

        // Calculate new balance
        let new_balance = current_balance + request.amount;

        // Generate transaction ID and date
        let now_millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_millis() as u64;
        
        let transaction_id = Transaction::generate_id(request.amount, now_millis);
        
        // Use provided date or generate current RFC 3339 timestamp in Eastern Time
        let date = match request.date {
            Some(date) => date,
            None => {
                // Generate RFC 3339 formatted timestamp in Eastern Time (assuming EDT for now, UTC-4)
                // In a production app, you'd want to detect the actual timezone or let the user configure it
                let now = SystemTime::now();
                let utc_datetime = time::OffsetDateTime::from(now);
                
                // Convert to Eastern Time (assuming EDT for now, UTC-4)
                // In a production app, you'd want to detect the actual timezone or let the user configure it
                let eastern_offset = time::UtcOffset::from_hms(-4, 0, 0)?;
                let eastern_datetime = utc_datetime.to_offset(eastern_offset);
                
                eastern_datetime.format(&time::format_description::well_known::Rfc3339)?
            }
        };

        let transaction = Transaction {
            id: transaction_id,
            child_id: active_child.id,
            date,
            description: request.description,
            amount: request.amount,
            balance: new_balance,
            transaction_type: if request.amount >= 0.0 { TransactionType::Income } else { TransactionType::Expense },
        };

        // Store in database
        self.transaction_repository.store_transaction(&transaction).await?;

        info!("Created transaction: {} with balance: {:.2}", transaction.id, new_balance);
        Ok(transaction)
    }

    /// List transactions with pagination and optional date filtering
    pub async fn list_transactions(&self, request: TransactionListRequest) -> Result<TransactionListResponse> {
        info!("Listing transactions with request: {:?}", request);

        // Get the active child
        let active_child_response = self.child_service.get_active_child().await?;
        let active_child = active_child_response.active_child
            .ok_or_else(|| anyhow::anyhow!("No active child found"))?;

        // Set default limit if not provided (max 100)
        let limit = request.limit.unwrap_or(20).min(100);
        
        // Query one extra record to determine if there are more results
        let query_limit = limit + 1;

        // Get transactions from database for the active child
        let mut db_transactions = self.transaction_repository.list_transactions(&active_child.id, query_limit, request.after.as_deref()).await?;

        // Generate future allowance transactions for the requested date range if specified
        let future_allowances = if let (Some(start_date_str), Some(end_date_str)) = (&request.start_date, &request.end_date) {
            // Parse the date strings to NaiveDate
            match (chrono::DateTime::parse_from_rfc3339(start_date_str), chrono::DateTime::parse_from_rfc3339(end_date_str)) {
                (Ok(start_dt), Ok(end_dt)) => {
                    let start_date = start_dt.date_naive();
                    let end_date = end_dt.date_naive();
                    
                    info!("🔍 TRANSACTION SERVICE: Generating future allowances for date range {} to {}", start_date, end_date);
                    
                    self.allowance_service
                        .generate_future_allowance_transactions(&active_child.id, start_date, end_date)
                        .await?
                }
                _ => {
                    info!("Failed to parse start_date: {} or end_date: {}, skipping future allowances", start_date_str, end_date_str);
                    Vec::new()
                }
            }
        } else {
            // No date range specified, don't generate future allowances
            info!("No start_date/end_date specified, skipping future allowances");
            Vec::new()
        };

        // Combine database transactions with future allowances
        db_transactions.extend(future_allowances);

        // Sort all transactions by date (newest first)
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
        info!("🔍 TRANSACTION DEBUG: Returning {} transactions (including future allowances), has_more: {}", response.transactions.len(), has_more);
        for (i, transaction) in response.transactions.iter().enumerate() {
            info!("🔍 Transaction {}: id={}, date={}, description={}, amount={}, type={:?}", 
                  i + 1, transaction.id, transaction.date, transaction.description, transaction.amount, transaction.transaction_type);
        }
        
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
            deleted_count,
            success_message,
            not_found_ids,
        })
    }




}

#[cfg(test)]
impl TransactionService {
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
    use shared::Child;

    async fn create_test_service() -> TransactionService {
        let db = Arc::new(DbConnection::init_test().await.expect("Failed to init test DB"));
        let service = TransactionService::new(db.clone());
        
        // Create a test child and set as active using the child repository directly
        // This ensures we get the specific ID we want for consistent testing
        let test_child = Child {
            id: "test_child_123".to_string(),
            name: "Test Child".to_string(),
            birthdate: "2015-01-01".to_string(),
            created_at: "2025-06-18T00:00:00-04:00".to_string(),
            updated_at: "2025-06-18T00:00:00-04:00".to_string(),
        };
        
        // Use the repository directly for testing to ensure consistent IDs
        use crate::backend::storage::repositories::ChildRepository;
        let child_repo = ChildRepository::new((*db).clone());
        child_repo.store_child(&test_child).await.expect("Failed to store test child");
        child_repo.set_active_child(&test_child.id).await.expect("Failed to set active child");
        
        service
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
        
        // Create some transactions for the test child
        let request1 = CreateTransactionRequest {
            description: "June allowance".to_string(),
            amount: 10.0,
            date: Some("2025-06-01T09:00:00-04:00".to_string()),
        };
        let tx1 = service.create_transaction(request1).await.unwrap();
        
        let request2 = CreateTransactionRequest {
            description: "Spent on candy".to_string(),
            amount: -3.0,
            date: Some("2025-06-15T12:00:00-04:00".to_string()),
        };
        let tx2 = service.create_transaction(request2).await.unwrap();
        
        // List transactions (this is what the calendar API would call)
        let list_request = TransactionListRequest {
            after: None,
            limit: Some(1000), // Calendar requests many transactions
            start_date: None,
            end_date: None,
        };
        
        let response = service.list_transactions(list_request).await.unwrap();
        
        // Verify all transactions belong to the test child
        assert!(!response.transactions.is_empty());
        for transaction in &response.transactions {
            assert_eq!(transaction.child_id, "test_child_123");
        }
        
        // Verify our created transactions are included
        let tx_ids: Vec<String> = response.transactions.iter().map(|t| t.id.clone()).collect();
        assert!(tx_ids.contains(&tx1.id));
        assert!(tx_ids.contains(&tx2.id));
        
        // Test calendar service integration
        use crate::backend::domain::CalendarService;
        let calendar_service = CalendarService::new();
        
        let calendar_month = calendar_service.generate_calendar_month(
            6, // June
            2025,
            response.transactions,
            None, // No allowance config for this test
        );
        
        // Verify calendar was generated correctly
        assert_eq!(calendar_month.month, 6);
        assert_eq!(calendar_month.year, 2025);
        assert!(!calendar_month.days.is_empty());
        
        // Find day 1 and day 15 to verify transactions are placed correctly
        let day_1 = calendar_month.days.iter().find(|d| d.day == 1 && !d.is_empty);
        let day_15 = calendar_month.days.iter().find(|d| d.day == 15 && !d.is_empty);
        
        if let Some(d1) = day_1 {
            // Should have at least one transaction on June 1st
            assert!(!d1.transactions.is_empty());
            // All transactions should belong to our test child
            for tx in &d1.transactions {
                assert_eq!(tx.child_id, "test_child_123");
            }
        }
        
        if let Some(d15) = day_15 {
            // Should have at least one transaction on June 15th
            assert!(!d15.transactions.is_empty());
            // All transactions should belong to our test child
            for tx in &d15.transactions {
                assert_eq!(tx.child_id, "test_child_123");
            }
        }
        
        println!("✅ Calendar API integration with child scoping works correctly!");
    }
}
