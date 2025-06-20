use axum::{extract::State, http::StatusCode, response::Json, routing::post, Router};
use log::{error, info};
use serde_json::Value;

use crate::backend::AppState;
use shared::{ParentalControlRequest, ParentalControlResponse};

/// Create the parental control API router
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/validate", post(validate_parental_control_answer))
}

/// Validate a parental control answer
#[axum::debug_handler]
pub async fn validate_parental_control_answer(
    State(app_state): State<AppState>,
    Json(request): Json<ParentalControlRequest>,
) -> Result<Json<ParentalControlResponse>, (StatusCode, Json<Value>)> {
    info!("POST /api/parental-control/validate - request: {:?}", request);

    // Basic input validation
    if request.answer.trim().is_empty() {
        let error_response = serde_json::json!({
            "error": "Answer cannot be empty",
            "code": "INVALID_INPUT"
        });
        return Err((StatusCode::BAD_REQUEST, Json(error_response)));
    }

    // Validate the answer using the domain service
    match app_state.parental_control_service.validate_answer(request).await {
        Ok(response) => {
            info!("Parental control validation result: success={}", response.success);
            Ok(Json(response))
        }
        Err(e) => {
            error!("Failed to validate parental control answer: {}", e);
            let error_response = serde_json::json!({
                "error": "Internal server error during validation",
                "code": "VALIDATION_ERROR"
            });
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(error_response)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{
        storage::DbConnection,
        domain::{TransactionService, CalendarService, TransactionTableService, MoneyManagementService, child_service::ChildService, ParentalControlService},
    };
    use axum::{
        body::Body,
        http::{Method, Request, StatusCode},
    };
    use serde_json::json;
    use std::sync::Arc;
    use tower::util::ServiceExt; // for `oneshot`

    async fn setup_test_app() -> Router {
        let db = Arc::new(DbConnection::init_test().await.expect("Failed to create test database"));
        
        let allowance_service = crate::backend::domain::AllowanceService::new(db.clone());
        let balance_service = crate::backend::domain::BalanceService::new(db.clone());
        
        let app_state = AppState {
            transaction_service: TransactionService::new(db.clone()),
            calendar_service: CalendarService::new(),
            transaction_table_service: TransactionTableService::new(),
            money_management_service: MoneyManagementService::new(),
            child_service: ChildService::new(db.clone()),
            parental_control_service: ParentalControlService::new(db),
            allowance_service,
            balance_service,
        };

        router().with_state(app_state)
    }

    #[tokio::test]
    async fn test_validate_correct_answer() {
        let app = setup_test_app().await;

        let request_body = json!({
            "answer": "ice cold"
        });

        let request = Request::builder()
            .method(Method::POST)
            .uri("/validate")
            .header("content-type", "application/json")
            .body(Body::from(request_body.to_string()))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let response_json: ParentalControlResponse = serde_json::from_slice(&body).unwrap();

        assert!(response_json.success);
        assert!(response_json.message.contains("Access granted"));
    }

    #[tokio::test]
    async fn test_validate_incorrect_answer() {
        let app = setup_test_app().await;

        let request_body = json!({
            "answer": "wrong answer"
        });

        let request = Request::builder()
            .method(Method::POST)
            .uri("/validate")
            .header("content-type", "application/json")
            .body(Body::from(request_body.to_string()))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let response_json: ParentalControlResponse = serde_json::from_slice(&body).unwrap();

        assert!(!response_json.success);
        assert!(response_json.message.contains("Incorrect answer"));
    }

    #[tokio::test]
    async fn test_validate_case_insensitive() {
        let app = setup_test_app().await;

        let test_cases = vec!["ICE COLD", "Ice Cold", "ice cold"];

        for answer in test_cases {
            let request_body = json!({
                "answer": answer
            });

            let request = Request::builder()
                .method(Method::POST)
                .uri("/validate")
                .header("content-type", "application/json")
                .body(Body::from(request_body.to_string()))
                .unwrap();

            let response = app.clone().oneshot(request).await.unwrap();

            assert_eq!(response.status(), StatusCode::OK);

            let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let response_json: ParentalControlResponse = serde_json::from_slice(&body).unwrap();

            assert!(response_json.success, "Answer '{}' should be accepted", answer);
        }
    }

    #[tokio::test]
    async fn test_validate_empty_answer() {
        let app = setup_test_app().await;

        let request_body = json!({
            "answer": ""
        });

        let request = Request::builder()
            .method(Method::POST)
            .uri("/validate")
            .header("content-type", "application/json")
            .body(Body::from(request_body.to_string()))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let error_json: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(error_json["error"], "Answer cannot be empty");
        assert_eq!(error_json["code"], "INVALID_INPUT");
    }

    #[tokio::test]
    async fn test_validate_whitespace_only_answer() {
        let app = setup_test_app().await;

        let request_body = json!({
            "answer": "   "
        });

        let request = Request::builder()
            .method(Method::POST)
            .uri("/validate")
            .header("content-type", "application/json")
            .body(Body::from(request_body.to_string()))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let error_json: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(error_json["error"], "Answer cannot be empty");
        assert_eq!(error_json["code"], "INVALID_INPUT");
    }

    #[tokio::test]
    async fn test_validate_invalid_json() {
        let app = setup_test_app().await;

        let request = Request::builder()
            .method(Method::POST)
            .uri("/validate")
            .header("content-type", "application/json")
            .body(Body::from("invalid json"))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        // Axum automatically returns 400 for invalid JSON
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
} 