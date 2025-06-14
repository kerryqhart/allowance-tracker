use crate::db::DbConnection;
use shared::{Transaction, TransactionListRequest, TransactionListResponse, PaginationInfo};
use anyhow::Result;
use tracing::info;

#[derive(Clone)]
pub struct TransactionService {
    db: DbConnection,
}

impl TransactionService {
    pub fn new(db: DbConnection) -> Self {
        Self { db }
    }

    /// List transactions with pagination and optional date filtering
    pub async fn list_transactions(&self, request: TransactionListRequest) -> Result<TransactionListResponse> {
        info!("Listing transactions with request: {:?}", request);

        // Set default limit if not provided (max 100)
        let limit = request.limit.unwrap_or(20).min(100);
        
        // Query one extra record to determine if there are more results
        let query_limit = limit + 1;

        // For now, return mock data until we implement the database layer
        let mock_transactions = self.generate_mock_transactions();
        
        // Apply cursor filtering
        let filtered_transactions = if let Some(after_cursor) = &request.after {
            self.apply_cursor_filter(mock_transactions, after_cursor)?
        } else {
            mock_transactions
        };

        // Apply date range filtering
        let date_filtered = self.apply_date_filter(filtered_transactions, &request)?;

        // Apply limit and determine pagination
        let mut results: Vec<Transaction> = date_filtered.into_iter().take(query_limit as usize).collect();
        
        let has_more = results.len() > limit as usize;
        if has_more {
            results.pop(); // Remove the extra record we queried
        }

        let next_cursor = if has_more {
            results.last().map(|t| t.id.clone())
        } else {
            None
        };

        let response = TransactionListResponse {
            transactions: results,
            pagination: PaginationInfo {
                has_more,
                next_cursor,
            },
        };

        info!("Returning {} transactions, has_more: {}", response.transactions.len(), has_more);
        Ok(response)
    }

    /// Apply cursor-based filtering (transactions after the given cursor)
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

    /// Apply date range filtering
    fn apply_date_filter(&self, transactions: Vec<Transaction>, request: &TransactionListRequest) -> Result<Vec<Transaction>> {
        // For now, just return all transactions since we don't have date parsing yet
        // In a real implementation, we would parse start_date and end_date and filter accordingly
        if request.start_date.is_some() || request.end_date.is_some() {
            info!("Date filtering requested but not yet implemented");
        }
        Ok(transactions)
    }

    /// Generate mock transaction data for testing
    fn generate_mock_transactions(&self) -> Vec<Transaction> {
        use std::time::{SystemTime, UNIX_EPOCH};
        
        let base_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        vec![
            // June 2025 transactions (most recent first)
            Transaction {
                id: Transaction::generate_id(10.0, base_time - 86400000), // June 13, 2025
                date: "2025-06-13T09:00:00-04:00".to_string(),
                description: "Weekly allowance".to_string(),
                amount: 10.0,
                balance: 55.0,
            },
            Transaction {
                id: Transaction::generate_id(15.0, base_time - 259200000), // June 10, 2025  
                date: "2025-06-10T15:30:00-04:00".to_string(),
                description: "Gift from Grandma".to_string(),
                amount: 15.0,
                balance: 45.0,
            },
            Transaction {
                id: Transaction::generate_id(-12.0, base_time - 432000000), // June 8, 2025
                date: "2025-06-08T14:20:15-04:00".to_string(),
                description: "Bought new toy".to_string(),
                amount: -12.0,
                balance: 30.0,
            },
            Transaction {
                id: Transaction::generate_id(10.0, base_time - 604800000), // June 6, 2025
                date: "2025-06-06T09:00:00-04:00".to_string(),
                description: "Weekly allowance".to_string(),
                amount: 10.0,
                balance: 42.0,
            },
            Transaction {
                id: Transaction::generate_id(-5.0, base_time - 777600000), // June 4, 2025
                date: "2025-06-04T16:45:30-04:00".to_string(),
                description: "Movie ticket".to_string(),
                amount: -5.0,
                balance: 32.0,
            },
            Transaction {
                id: Transaction::generate_id(10.0, base_time - 1209600000), // May 30, 2025
                date: "2025-05-30T09:00:00-04:00".to_string(),
                description: "Weekly allowance".to_string(),
                amount: 10.0,
                balance: 37.0,
            },
            Transaction {
                id: Transaction::generate_id(-3.0, base_time - 1382400000), // May 28, 2025
                date: "2025-05-28T13:15:22-04:00".to_string(),
                description: "Ice cream treat".to_string(),
                amount: -3.0,
                balance: 27.0,
            },
            Transaction {
                id: Transaction::generate_id(10.0, base_time - 1814400000), // May 23, 2025
                date: "2025-05-23T09:00:00-04:00".to_string(),
                description: "Weekly allowance".to_string(),
                amount: 10.0,
                balance: 30.0,
            },
            Transaction {
                id: Transaction::generate_id(-8.0, base_time - 2073600000), // May 20, 2025
                date: "2025-05-20T11:30:45-04:00".to_string(),
                description: "Comic book".to_string(),
                amount: -8.0,
                balance: 20.0,
            },
            Transaction {
                id: Transaction::generate_id(10.0, base_time - 2419200000), // May 16, 2025
                date: "2025-05-16T09:00:00-04:00".to_string(),
                description: "Weekly allowance".to_string(),
                amount: 10.0,
                balance: 28.0,
            },
            Transaction {
                id: Transaction::generate_id(20.0, base_time - 2592000000), // May 14, 2025
                date: "2025-05-14T10:00:00-04:00".to_string(),
                description: "Birthday money from Uncle Bob".to_string(),
                amount: 20.0,
                balance: 18.0,
            },
            Transaction {
                id: Transaction::generate_id(10.0, base_time - 3024000000), // May 9, 2025
                date: "2025-05-09T09:00:00-04:00".to_string(),
                description: "Weekly allowance".to_string(),
                amount: 10.0,
                balance: -2.0, // Negative balance from previous spending
            },
            Transaction {
                id: Transaction::generate_id(-15.0, base_time - 3196800000), // May 7, 2025
                date: "2025-05-07T14:22:10-04:00".to_string(),
                description: "Art supplies".to_string(),
                amount: -15.0,
                balance: -12.0,
            },
            Transaction {
                id: Transaction::generate_id(10.0, base_time - 3628800000), // May 2, 2025
                date: "2025-05-02T09:00:00-04:00".to_string(),
                description: "Weekly allowance".to_string(),
                amount: 10.0,
                balance: 3.0,
            },
        ]
    }
}

// Keep the old ValueStore for backwards compatibility during transition
#[derive(Clone)]
pub struct ValueStore {
    db: DbConnection,
}

impl ValueStore {
    pub fn new(db: DbConnection) -> Self {
        Self { db }
    }

    pub async fn get_value(&self, key: &str) -> Result<Option<String>> {
        info!("Getting value for key: {}", key);
        self.db.get_value(key).await
    }

    pub async fn put_value(&self, key: &str, value: &str) -> Result<()> {
        info!("Putting value for key: {}", key);
        self.db.put_value(key, value).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn create_test_service() -> TransactionService {
        let db = DbConnection::init().await.expect("Failed to init test DB");
        TransactionService::new(db)
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
        let mock_transactions = service.generate_mock_transactions();
        
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
        let transactions = service.generate_mock_transactions();
        
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
        
        // Test limit too high (should be capped at 100)
        let request = TransactionListRequest {
            after: None,
            limit: Some(200),
            start_date: None,
            end_date: None,
        };

        let response = service.list_transactions(request).await.unwrap();
        
        // Should be capped at reasonable limit
        assert!(response.transactions.len() <= 100);
    }
}
