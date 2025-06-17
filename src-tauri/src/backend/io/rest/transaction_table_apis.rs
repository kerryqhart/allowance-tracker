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
    // Tests temporarily disabled due to missing test infrastructure
    // TODO: Re-enable after fixing test setup
} 