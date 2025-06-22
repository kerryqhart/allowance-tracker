use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use log::{error, info, warn};
use serde::Deserialize;

use crate::backend::AppState;
use shared::{
    CalendarMonth, CalendarFocusDate, UpdateCalendarFocusRequest, UpdateCalendarFocusResponse,
    CurrentDateResponse, TransactionListRequest, TransactionListResponse, PaginationInfo,
};
use chrono::{NaiveDate, NaiveTime};

// Query parameters for calendar month API
#[derive(Debug, Deserialize)]
pub struct CalendarMonthQuery {
    pub month: u32,
    pub year: u32,
}

/// Create a router for calendar related APIs
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/month", get(get_calendar_month))
        .route("/current-date", get(get_current_date))
        .route("/focus-date", get(get_focus_date).post(set_focus_date))
        .route("/focus-date/previous", post(navigate_previous_month))
        .route("/focus-date/next", post(navigate_next_month))
}

/// Get calendar month data with transactions
async fn get_calendar_month(
    State(state): State<AppState>,
    Query(query): Query<CalendarMonthQuery>,
) -> impl IntoResponse {
    info!("GET /api/calendar/month - query: {:?}", query);

    let request = CalendarMonthQuery {
        month: query.month,
        year: query.year,
    };

    // Calculate start and end dates for the requested month using chrono
    let start_of_month = match NaiveDate::from_ymd_opt(request.year as i32, request.month, 1) {
        Some(date) => date,
        None => return (StatusCode::BAD_REQUEST, "Invalid month/year").into_response(),
    };
    
    // Get the last day of the month by going to next month and subtracting 1 day
    let next_month = if request.month == 12 {
        NaiveDate::from_ymd_opt(request.year as i32 + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(request.year as i32, request.month + 1, 1)
    };
    
    let end_of_month = match next_month {
        Some(date) => date.pred_opt().unwrap(),
        None => return (StatusCode::BAD_REQUEST, "Invalid month/year").into_response(),
    };
    
    let start_date = start_of_month.and_time(NaiveTime::from_hms_opt(0, 0, 0).unwrap())
        .format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let end_date = end_of_month.and_time(NaiveTime::from_hms_opt(23, 59, 59).unwrap())
        .format("%Y-%m-%dT%H:%M:%SZ").to_string();

    // Get transactions for this month (including future allowances)
    let transaction_request = TransactionListRequest {
        after: None,
        limit: Some(1000),
        start_date: Some(start_date),
        end_date: Some(end_date),
    };

    let transaction_response = match state.transaction_service.list_transactions(transaction_request).await {
        Ok(response) => response,
        Err(e) => {
            error!("Failed to get transactions for calendar: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Error getting transactions").into_response();
        }
    };

    let mut all_transactions = transaction_response.transactions;

    // Also get the most recent transaction from before this month for starting balance calculation
    // This is critical for proper balance calculation on days without transactions
    let previous_month_end = if request.month == 1 {
        format!("{:04}-12-31T23:59:59Z", request.year - 1)
    } else {
        let prev_month_days = state.calendar_service.days_in_month(request.month - 1, request.year);
        format!("{:04}-{:02}-{:02}T23:59:59Z", request.year, request.month - 1, prev_month_days)
    };

    // Get the most recent transaction before this month (limit to 1, no date range start)
    let previous_transaction_request = TransactionListRequest {
        after: None,
        limit: Some(1),
        start_date: None, // No start date - get from all time
        end_date: Some(previous_month_end), // Only transactions before this month
    };

    let previous_transaction_response = match state.transaction_service.list_transactions(previous_transaction_request).await {
        Ok(response) => response,
        Err(e) => {
            warn!("Failed to get previous transactions for calendar starting balance: {}", e);
            // Continue without previous transactions - starting balance will be 0
            TransactionListResponse {
                transactions: vec![],
                pagination: PaginationInfo {
                    has_more: false,
                    next_cursor: None,
                },
            }
        }
    };

    // Add the previous transaction if found (for balance calculation)
    if !previous_transaction_response.transactions.is_empty() {
        let prev_transaction = &previous_transaction_response.transactions[0];
        
        // Check for duplicates before adding - prevent same transaction being added twice
        let is_duplicate = all_transactions.iter().any(|t| t.id == prev_transaction.id);
        
        if !is_duplicate {
            info!("🗓️ STARTING BALANCE: Adding previous transaction for {}/{}: {} on {} with balance ${:.2}", 
                  request.month, request.year, prev_transaction.id, prev_transaction.date, prev_transaction.balance);
            all_transactions.push(prev_transaction.clone());
        } else {
            info!("🗓️ STARTING BALANCE: Previous transaction {} already in month data, skipping duplicate", 
                  prev_transaction.id);
        }
    } else {
        info!("🗓️ STARTING BALANCE: No previous transactions found before {}/{}, starting balance will be $0.00", 
              request.month, request.year);
    }

    let calendar_month = state.calendar_service.generate_calendar_month(
        request.month,
        request.year,
        all_transactions,
    );
    (StatusCode::OK, Json(calendar_month)).into_response()
}

/// Get current date information from the backend
async fn get_current_date(State(state): State<AppState>) -> impl IntoResponse {
    info!("GET /api/calendar/current-date");

    let current_date = state.calendar_service.get_current_date();
    (StatusCode::OK, Json(current_date)).into_response()
}

/// Get the current focus date for calendar navigation
async fn get_focus_date(State(state): State<AppState>) -> impl IntoResponse {
    info!("GET /api/calendar/focus-date");

    let focus_date = state.calendar_service.get_focus_date();
    (StatusCode::OK, Json(focus_date)).into_response()
}

/// Set the focus date for calendar navigation
async fn set_focus_date(
    State(state): State<AppState>,
    Json(request): Json<UpdateCalendarFocusRequest>,
) -> impl IntoResponse {
    info!("POST /api/calendar/focus-date - request: {:?}", request);

    match state.calendar_service.set_focus_date(request.month, request.year) {
        Ok(focus_date) => {
            let response = shared::UpdateCalendarFocusResponse {
                focus_date,
                success_message: format!("Calendar focus set to {} {}", 
                    state.calendar_service.month_name(request.month), request.year),
            };
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => {
            error!("Failed to set focus date: {}", e);
            (StatusCode::BAD_REQUEST, e).into_response()
        }
    }
}

/// Navigate to the previous month
async fn navigate_previous_month(State(state): State<AppState>) -> impl IntoResponse {
    info!("POST /api/calendar/focus-date/previous");

    let focus_date = state.calendar_service.navigate_previous_month();
    let response = shared::UpdateCalendarFocusResponse {
        focus_date: focus_date.clone(),
        success_message: format!("Navigated to {} {}", 
            state.calendar_service.month_name(focus_date.month), focus_date.year),
    };
    (StatusCode::OK, Json(response)).into_response()
}

/// Navigate to the next month
async fn navigate_next_month(State(state): State<AppState>) -> impl IntoResponse {
    info!("POST /api/calendar/focus-date/next");

    let focus_date = state.calendar_service.navigate_next_month();
    let response = shared::UpdateCalendarFocusResponse {
        focus_date: focus_date.clone(),
        success_message: format!("Navigated to {} {}", 
            state.calendar_service.month_name(focus_date.month), focus_date.year),
    };
    (StatusCode::OK, Json(response)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{initialize_backend, create_router};
    use axum::body::Body;
    use axum::http::{Request, Method};
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_get_focus_date() -> Result<(), Box<dyn std::error::Error>> {
        let app_state = initialize_backend().await?;
        let app = create_router(app_state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/calendar/focus-date")
                    .method(Method::GET)
                    .body(Body::empty())?,
            )
            .await?;

        assert_eq!(response.status(), StatusCode::OK);
        
        // Parse the response body
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
        let focus_date: shared::CalendarFocusDate = serde_json::from_slice(&body)?;
        
        // Verify it's a valid date
        assert!(focus_date.month >= 1 && focus_date.month <= 12);
        assert!(focus_date.year >= 2025);
        
        Ok(())
    }

    #[tokio::test]
    async fn test_set_focus_date() -> Result<(), Box<dyn std::error::Error>> {
        let app_state = initialize_backend().await?;
        let app = create_router(app_state);

        let request_body = shared::UpdateCalendarFocusRequest {
            month: 6,
            year: 2025,
        };

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/calendar/focus-date")
                    .method(Method::POST)
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&request_body)?))?,
            )
            .await?;

        assert_eq!(response.status(), StatusCode::OK);
        
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
        let response: shared::UpdateCalendarFocusResponse = serde_json::from_slice(&body)?;
        
        assert_eq!(response.focus_date.month, 6);
        assert_eq!(response.focus_date.year, 2025);
        assert!(response.success_message.contains("June 2025"));
        
        Ok(())
    }

    #[tokio::test]
    async fn test_navigate_previous_month() -> Result<(), Box<dyn std::error::Error>> {
        let app_state = initialize_backend().await?;
        let app = create_router(app_state.clone());

        // First set to June 2025
        let request_body = shared::UpdateCalendarFocusRequest {
            month: 6,
            year: 2025,
        };
        app_state.calendar_service.set_focus_date(request_body.month, request_body.year)?;

        // Then navigate to previous month
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/calendar/focus-date/previous")
                    .method(Method::POST)
                    .body(Body::empty())?,
            )
            .await?;

        assert_eq!(response.status(), StatusCode::OK);
        
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
        let response: shared::UpdateCalendarFocusResponse = serde_json::from_slice(&body)?;
        
        assert_eq!(response.focus_date.month, 5); // May
        assert_eq!(response.focus_date.year, 2025);
        assert!(response.success_message.contains("May 2025"));
        
        Ok(())
    }

    #[tokio::test]
    async fn test_navigate_next_month() -> Result<(), Box<dyn std::error::Error>> {
        let app_state = initialize_backend().await?;
        let app = create_router(app_state.clone());

        // First set to June 2025
        let request_body = shared::UpdateCalendarFocusRequest {
            month: 6,
            year: 2025,
        };
        app_state.calendar_service.set_focus_date(request_body.month, request_body.year)?;

        // Then navigate to next month
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/calendar/focus-date/next")
                    .method(Method::POST)
                    .body(Body::empty())?,
            )
            .await?;

        assert_eq!(response.status(), StatusCode::OK);
        
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
        let response: shared::UpdateCalendarFocusResponse = serde_json::from_slice(&body)?;
        
        assert_eq!(response.focus_date.month, 7); // July
        assert_eq!(response.focus_date.year, 2025);
        assert!(response.success_message.contains("July 2025"));
        
        Ok(())
    }

    #[tokio::test]
    async fn test_set_focus_date_invalid_month() -> Result<(), Box<dyn std::error::Error>> {
        let app_state = initialize_backend().await?;
        let app = create_router(app_state);

        let request_body = shared::UpdateCalendarFocusRequest {
            month: 13, // Invalid month
            year: 2025,
        };

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/calendar/focus-date")
                    .method(Method::POST)
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&request_body)?))?,
            )
            .await?;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        
        Ok(())
    }
} 