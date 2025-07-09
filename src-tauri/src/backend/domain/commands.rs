// src-tauri/src/backend/domain/commands.rs

//! Domain-level command and query types
//! These structs are used by services inside the domain layer and are **not**
//! exposed over the public API. The REST (or Tauri) layer is responsible for
//! mapping the public DTOs defined in the `shared` crate to these internal
//! types.

pub mod transactions {
    use super::super::models::transaction::Transaction as DomainTransaction;

    /// Input for creating a new transaction.
    #[derive(Debug, Clone)]
    pub struct CreateTransactionCommand {
        pub description: String,
        pub amount: f64,
        pub date: Option<String>,
    }

    /// Query parameters for listing transactions.
    #[derive(Debug, Clone, Default)]
    pub struct TransactionListQuery {
        pub after: Option<String>,
        pub limit: Option<u32>,
        pub start_date: Option<String>,
        pub end_date: Option<String>,
    }

    /// Command for deleting multiple transactions.
    #[derive(Debug, Clone)]
    pub struct DeleteTransactionsCommand {
        pub transaction_ids: Vec<String>,
    }

    /// Generic pagination info returned by list queries.
    #[derive(Debug, Clone)]
    pub struct PaginationInfo {
        pub has_more: bool,
        pub next_cursor: Option<String>,
    }

    /// Result of listing transactions.
    #[derive(Debug, Clone)]
    pub struct TransactionListResult {
        pub transactions: Vec<DomainTransaction>,
        pub pagination: PaginationInfo,
    }

    /// Result of deleting transactions.
    #[derive(Debug, Clone)]
    pub struct DeleteTransactionsResult {
        pub deleted_count: usize,
        pub not_found_ids: Vec<String>,
        pub success_message: String,
    }
} 