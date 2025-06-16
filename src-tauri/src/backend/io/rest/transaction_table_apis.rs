use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::get,
    Router,
};
use log::{error, info};
use serde::Deserialize;

use crate::backend::AppState;
use shared::{TransactionListRequest, TransactionTableResponse};

// Query parameters for transaction table API
#[derive(Debug, Deserialize)]
pub struct TransactionTableQuery {
    pub limit: Option<u32>,
    pub after: Option<String>,
}

/// Create a router for transaction table related APIs
pub fn router() -> Router<AppState> {
    Router::new().route("/table", get(get_transaction_table))
}

/// Get formatted transaction table data
async fn get_transaction_table(
    State(state): State<AppState>,
    Query(query): Query<TransactionTableQuery>,
) -> impl IntoResponse {
    info!("GET /api/transactions/table - query: {:?}", query);

    let request = TransactionListRequest {
        after: query.after.clone(),
        limit: query.limit,
        start_date: None,
        end_date: None,
    };

    match state.transaction_service.list_transactions(request).await {
        Ok(transactions_response) => {
            let formatted_transactions = state
                .transaction_table_service
                .format_transactions_for_table(&transactions_response.transactions);

            let table_response = TransactionTableResponse {
                formatted_transactions,
                pagination: transactions_response.pagination,
            };

            (StatusCode::OK, Json(table_response)).into_response()
        }
        Err(e) => {
            error!("Failed to get transaction table data: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Error getting transaction table",
            )
                .into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{domain::Transaction, io::rest::tests::setup_test_handlers};
    use axum::http::Request;
    use axum_test::TestServer;
    use serde_json::Value;
    use shared::Pagination;

    #[tokio::test]
    async fn test_get_transaction_table_handler() {
        let app_state = setup_test_handlers().await;
        let server = TestServer::new(router().with_state(app_state.clone())).unwrap();

        // Create a transaction to be fetched
        let _ = app_state
            .transaction_service
            .create_transaction(shared::CreateTransactionRequest {
                description: "Test".to_string(),
                amount: 10.0,
                date: Some("2025-06-16".to_string()),
                child_id: None,
            })
            .await;

        let response = server.get("/table?limit=5").await;
        assert_eq!(response.status_code(), StatusCode::OK);
        let body: Value = response.json();
        assert_eq!(body["formatted_transactions"].as_array().unwrap().len(), 1);
        assert_eq!(
            body["formatted_transactions"][0]["description"],
            "Test"
        );
    }
} 