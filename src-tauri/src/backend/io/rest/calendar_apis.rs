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
use shared::{CalendarMonthRequest, TransactionListRequest};

// Query parameters for calendar month API
#[derive(Debug, Deserialize)]
pub struct CalendarMonthQuery {
    pub month: u32,
    pub year: u32,
}

/// Create a router for calendar related APIs
pub fn router() -> Router<AppState> {
    Router::new().route("/month", get(get_calendar_month))
}

/// Get calendar month data with transactions
async fn get_calendar_month(
    State(state): State<AppState>,
    Query(query): Query<CalendarMonthQuery>,
) -> impl IntoResponse {
    info!("GET /api/calendar/month - query: {:?}", query);

    let request = CalendarMonthRequest {
        month: query.month,
        year: query.year,
    };

    match state
        .transaction_service
        .list_transactions(TransactionListRequest {
            after: None,
            limit: Some(1000), // Get enough transactions for calendar calculations
            start_date: None,
            end_date: None,
        })
        .await
    {
        Ok(transactions_response) => {
            let calendar_month = state.calendar_service.generate_calendar_month(
                request.month,
                request.year,
                transactions_response.transactions,
            );
            (StatusCode::OK, Json(calendar_month)).into_response()
        }
        Err(e) => {
            error!("Failed to fetch transactions for calendar: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Error generating calendar").into_response()
        }
    }
} 