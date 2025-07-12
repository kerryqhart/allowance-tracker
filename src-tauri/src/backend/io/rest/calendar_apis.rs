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
    CurrentDateResponse, PaginationInfo, Transaction,
};

use crate::backend::domain::commands::transactions::TransactionListQuery;
use crate::backend::io::rest::mappers::transaction_mapper::TransactionMapper;
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
    Query(request): Query<CalendarMonthQuery>,
) -> impl IntoResponse {
    info!("🗓️ GET /api/calendar - request: {:?}", request);

    // Use the new orchestration method from calendar service
    match state.calendar_service.get_calendar_month_with_transactions(
        request.month,
        request.year,
        &state.transaction_service,
    ).await {
        Ok(calendar_month) => {
            info!("✅ Calendar generated with {} days", calendar_month.days.len());
            let total_transaction_count: usize = calendar_month.days.iter()
                .map(|day| day.transactions.len())
                .sum();
            info!("🗓️ Total transactions in calendar days: {}", total_transaction_count);
            
            (StatusCode::OK, Json(calendar_month)).into_response()
        }
        Err(e) => {
            error!("❌ Failed to get calendar data: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Error getting calendar data").into_response()
        }
    }
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