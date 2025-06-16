//! # REST API for Money Management
//!
//! Endpoints for adding and spending money.

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
};
use log::{info, error};

use crate::backend::AppState;
use shared::{
    AddMoneyRequest, AddMoneyResponse, SpendMoneyRequest, SpendMoneyResponse,
};

/// Add money (create a positive transaction)
pub async fn add_money(
    State(state): State<AppState>,
    Json(request): Json<AddMoneyRequest>,
) -> impl IntoResponse {
    info!("POST /api/money/add - request: {:?}", request);

    // First validate the request
    let validation = state.money_management_service
        .validate_add_money_form(&request.description, &request.amount.to_string());

    if !validation.is_valid {
        let error_message = state.money_management_service
            .get_first_error_message(&validation.errors)
            .unwrap_or_else(|| "Invalid input".to_string());
        return (StatusCode::BAD_REQUEST, error_message).into_response();
    }

    // Convert to CreateTransactionRequest
    let create_request = state.money_management_service
        .to_create_transaction_request(request);

    // Create the transaction
    match state.transaction_service.create_transaction(create_request).await {
        Ok(transaction) => {
            let success_message = state.money_management_service
                .generate_success_message(transaction.amount);
            
            let formatted_amount = state.money_management_service
                .format_positive_amount(transaction.amount);

            let response = AddMoneyResponse {
                transaction_id: transaction.id,
                success_message,
                new_balance: transaction.balance,
                formatted_amount,
            };

            (StatusCode::CREATED, Json(response)).into_response()
        }
        Err(e) => {
            error!("Failed to add money: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to add money").into_response()
        }
    }
}

/// Spend money (create a negative transaction)
pub async fn spend_money(
    State(state): State<AppState>,
    Json(request): Json<SpendMoneyRequest>,
) -> impl IntoResponse {
    info!("POST /api/money/spend - request: {:?}", request);

    // First validate the request
    let validation = state.money_management_service
        .validate_spend_money_form(&request.description, &request.amount.to_string());

    if !validation.is_valid {
        let error_message = state.money_management_service
            .get_first_error_message(&validation.errors)
            .unwrap_or_else(|| "Invalid input".to_string());
        return (StatusCode::BAD_REQUEST, error_message).into_response();
    }

    // Convert to CreateTransactionRequest (this will make the amount negative)
    let create_request = state.money_management_service
        .spend_to_create_transaction_request(request.clone());

    // Create the transaction
    match state.transaction_service.create_transaction(create_request).await {
        Ok(transaction) => {
            let success_message = state.money_management_service
                .generate_spend_success_message(request.amount);
            
            let formatted_amount = state.money_management_service
                .format_negative_amount(request.amount);

            let response = SpendMoneyResponse {
                transaction_id: transaction.id,
                success_message,
                new_balance: transaction.balance,
                formatted_amount,
            };

            (StatusCode::CREATED, Json(response)).into_response()
        }
        Err(e) => {
            error!("Failed to spend money: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to record spending").into_response()
        }
    }
} 