// Import backend module for real data integration
pub mod backend;

use shared::{
    ActiveChildResponse, AddMoneyRequest, AddMoneyResponse, CalendarMonth,
    CancelGoalRequest, CancelGoalResponse, ChildListResponse, ChildResponse, CreateChildRequest,
    CreateGoalRequest, CreateGoalResponse, CurrentDateResponse,
    DeleteTransactionsRequest, DeleteTransactionsResponse, GetDataDirectoryResponse,
    GetAllowanceConfigRequest, GetAllowanceConfigResponse, GetCurrentGoalRequest,
    GetCurrentGoalResponse, LogEntry, ParentalControlRequest, ParentalControlResponse,
    RelocateDataDirectoryRequest, RelocateDataDirectoryResponse, RevertDataDirectoryRequest,
    RevertDataDirectoryResponse, SetActiveChildRequest,
    SetActiveChildResponse, SpendMoneyRequest, SpendMoneyResponse, TransactionListRequest,
    TransactionTableResponse, UpdateAllowanceConfigRequest, UpdateAllowanceConfigResponse,
};

use crate::backend::domain::commands::transactions::{DeleteTransactionsCommand, TransactionListQuery};
use crate::backend::io::rest::mappers::transaction_mapper::TransactionMapper;
use backend::{
    io::rest::mappers::child_mapper::ChildMapper,
    initialize_backend, AppState,
};
use log::{error, info};
use axum::serve;
use tokio::net::TcpListener;
use std::net::SocketAddr;
use tauri::Manager;
use tauri_plugin_log::Target;

// Real Tauri commands that use backend services
#[tauri::command]
async fn get_calendar_month(
    app_state: tauri::State<'_, AppState>,
    month: u32,
    year: u32,
) -> Result<CalendarMonth, String> {
    info!(
        "🗓️ Getting calendar for month {} year {} with real data",
        month, year
    );

    let transaction_query = TransactionListQuery {
        after: None,
        limit: Some(10000),
        start_date: None,
        end_date: Some(format!(
            "{:04}-{:02}-{:02}T23:59:59Z",
            year,
            month,
            app_state.calendar_service.days_in_month(month, year)
        )),
    };

    let transaction_result = app_state
        .transaction_service
        .list_transactions(transaction_query)
        .await
        .map_err(|e| format!("Failed to load calendar data: {}", e))?;

    let dto_transactions: Vec<shared::Transaction> = transaction_result
        .transactions
        .into_iter()
        .map(TransactionMapper::to_dto)
        .collect();

    let calendar_month = app_state.calendar_service.generate_calendar_month(
        month,
        year,
        dto_transactions,
    );

    info!(
        "✅ Calendar generated with {} days",
        calendar_month.days.len()
    );
    Ok(calendar_month)
}

#[tauri::command]
async fn get_current_date(app_state: tauri::State<'_, AppState>) -> Result<CurrentDateResponse, String> {
    let current_date = app_state.calendar_service.get_current_date();
    Ok(current_date)
}

#[tauri::command]
async fn get_allowance_config(
    app_state: tauri::State<'_, AppState>,
) -> Result<GetAllowanceConfigResponse, String> {
    info!("📋 Getting allowance config with real data");
    let request = GetAllowanceConfigRequest { child_id: None };
    app_state
        .allowance_service
        .get_allowance_config(request)
        .await
        .map_err(|e| format!("Failed to get allowance config: {}", e))
}

#[tauri::command]
async fn update_allowance_config(
    app_state: tauri::State<'_, AppState>,
    amount: f64,
    day_of_week: u8,
    is_active: bool,
) -> Result<UpdateAllowanceConfigResponse, String> {
    info!(
        "💰 Updating allowance config: ${:.2} on day {} (active: {})",
        amount, day_of_week, is_active
    );
    let request = UpdateAllowanceConfigRequest {
        child_id: None,
        amount,
        day_of_week,
        is_active,
    };
    app_state
        .allowance_service
        .update_allowance_config(request)
        .await
        .map_err(|e| format!("Failed to update allowance config: {}", e))
}

#[tauri::command]
async fn get_current_goal(
    app_state: tauri::State<'_, AppState>,
) -> Result<GetCurrentGoalResponse, String> {
    info!("🎯 Getting current goal with real data");
    let request = GetCurrentGoalRequest { child_id: None };
    app_state
        .goal_service
        .get_current_goal(request)
        .await
        .map_err(|e| format!("Failed to get current goal: {}", e))
}

#[tauri::command]
async fn create_goal(
    app_state: tauri::State<'_, AppState>,
    amount: f64,
    description: String,
) -> Result<CreateGoalResponse, String> {
    info!(
        "🎯 CREATE_GOAL COMMAND CALLED: '{}' for ${:.2}",
        description, amount
    );
    let request = CreateGoalRequest {
        child_id: None,
        description,
        target_amount: amount,
    };
    app_state
        .goal_service
        .create_goal(request)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn cancel_goal(app_state: tauri::State<'_, AppState>) -> Result<CancelGoalResponse, String> {
    info!("🎯 Cancelling current goal");
    let request = CancelGoalRequest { child_id: None };
    app_state
        .goal_service
        .cancel_goal(request)
        .await
        .map_err(|e| format!("Failed to cancel goal: {}", e))
}

#[tauri::command]
async fn get_transactions(
    app_state: tauri::State<'_, AppState>,
    limit: Option<u32>,
) -> Result<TransactionTableResponse, String> {
    info!("📄 Getting transactions table");
    let domain_query = TransactionListQuery {
        after: None,
        limit,
        start_date: None,
        end_date: None,
    };

    let result = app_state
        .transaction_service
        .list_transactions(domain_query)
        .await
        .map_err(|e| e.to_string())?;

    let dto_transactions: Vec<shared::Transaction> = result
        .transactions
        .into_iter()
        .map(TransactionMapper::to_dto)
        .collect();

    let formatted_transactions = app_state
        .transaction_table_service
        .format_transactions_for_table(&dto_transactions);

    Ok(TransactionTableResponse {
        formatted_transactions,
        pagination: shared::PaginationInfo {
            has_more: result.pagination.has_more,
            next_cursor: result.pagination.next_cursor,
        },
    })
}

#[tauri::command]
async fn get_active_child(
    app_state: tauri::State<'_, AppState>,
) -> Result<ActiveChildResponse, String> {
    info!("👶 Getting active child with real data");
    let active_child_domain = app_state
        .child_service
        .get_active_child()
        .await
        .map_err(|e| e.to_string())?;
    let response_dto = ChildMapper::to_active_child_dto(active_child_domain);
    info!(
        "✅ Active child loaded: {:?}",
        response_dto.active_child.as_ref().map(|c| &c.name)
    );
    Ok(response_dto)
}

#[tauri::command]
async fn has_children(app_state: tauri::State<'_, AppState>) -> Result<bool, String> {
    info!("👶 Checking for children with real data");
    match app_state.child_service.list_children().await {
        Ok(children) => Ok(!children.is_empty()),
        Err(e) => {
            error!("❌ Failed to list children: {}", e);
            Err(format!("Failed to list children: {}", e))
        }
    }
}

#[tauri::command]
async fn list_children(
    app_state: tauri::State<'_, AppState>,
) -> Result<ChildListResponse, String> {
    info!("👶 Listing children with real data");
    let children_response = app_state
        .child_service
        .list_children()
        .await
        .map_err(|e| format!("Failed to list children: {}", e))?;
    let response_dto = ChildMapper::to_child_list_dto(children_response);
    Ok(response_dto)
}

#[tauri::command]
async fn set_active_child(
    app_state: tauri::State<'_, AppState>,
    request: SetActiveChildRequest,
) -> Result<SetActiveChildResponse, String> {
    info!("👶 Setting active child with real data: {}", request.child_id);
    let response = app_state
        .child_service
        .set_active_child(&request.child_id)
        .await
        .map_err(|e| format!("Failed to set active child: {}", e))?;
    let response_dto = ChildMapper::to_set_active_child_dto(response);
    Ok(response_dto)
}

#[tauri::command]
async fn create_child(
    app_state: tauri::State<'_, AppState>,
    request: CreateChildRequest,
) -> Result<ChildResponse, String> {
    info!("👶 Creating child with real data: {}", request.name);
    let response = app_state
        .child_service
        .create_child(request)
        .await
        .map_err(|e| format!("Failed to create child: {}", e))?;
    let response_dto =
        ChildMapper::to_child_response_dto(response, "Child created successfully");
    Ok(response_dto)
}

#[tauri::command]
async fn add_money(
    app_state: tauri::State<'_, AppState>,
    request: AddMoneyRequest,
) -> Result<AddMoneyResponse, String> {
    info!("💸 Adding money with real data: {:?}", request);
    let create_command = app_state
        .money_management_service
        .to_create_transaction_command(request);
    let domain_transaction = app_state
        .transaction_service
        .create_transaction(create_command)
        .await
        .map_err(|e| e.to_string())?;
    
    Ok(AddMoneyResponse {
        transaction_id: domain_transaction.id,
        new_balance: domain_transaction.balance,
        success_message: "Money added successfully!".to_string(),
        formatted_amount: format!("${:.2}", domain_transaction.amount),
    })
}

#[tauri::command]
async fn spend_money(
    app_state: tauri::State<'_, AppState>,
    request: SpendMoneyRequest,
) -> Result<SpendMoneyResponse, String> {
    info!("💸 Spending money with real data: {:?}", request);
    let create_command = app_state
        .money_management_service
        .spend_to_create_transaction_command(request);
    let domain_transaction = app_state
        .transaction_service
        .create_transaction(create_command)
        .await
        .map_err(|e| e.to_string())?;
    
    Ok(SpendMoneyResponse {
        transaction_id: domain_transaction.id,
        new_balance: domain_transaction.balance,
        success_message: "Money spent successfully!".to_string(),
        formatted_amount: format!("-${:.2}", domain_transaction.amount.abs()),
    })
}

#[tauri::command]
async fn delete_transactions(
    app_state: tauri::State<'_, AppState>,
    request: DeleteTransactionsRequest,
) -> Result<DeleteTransactionsResponse, String> {
    info!("🗑️ Deleting transactions: {:?}", request.transaction_ids);
    let cmd = DeleteTransactionsCommand {
        transaction_ids: request.transaction_ids,
    };
    let domain_result = app_state
        .transaction_service
        .delete_transactions(cmd)
        .await
        .map_err(|e| e.to_string())?;
    
    Ok(DeleteTransactionsResponse {
        deleted_count: domain_result.deleted_count,
        success_message: domain_result.success_message,
        not_found_ids: domain_result.not_found_ids,
    })
}

#[tauri::command]
async fn validate_parental_control(
    app_state: tauri::State<'_, AppState>,
    request: ParentalControlRequest,
) -> Result<ParentalControlResponse, String> {
    info!("🔒 Validating parental control");
    app_state
        .parental_control_service
        .validate_answer(request)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_data_directory(
    app_state: tauri::State<'_, AppState>,
) -> Result<GetDataDirectoryResponse, String> {
    info!("📂 Getting data directory");
    app_state
        .data_directory_service
        .get_current_directory(None)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn relocate_data_directory(
    app_state: tauri::State<'_, AppState>,
    request: RelocateDataDirectoryRequest,
) -> Result<RelocateDataDirectoryResponse, String> {
    info!("🚚 Relocating data directory to: {:?}", request.new_path);
    app_state
        .data_directory_service
        .relocate_directory(request)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn revert_data_directory(
    app_state: tauri::State<'_, AppState>,
    request: RevertDataDirectoryRequest,
) -> Result<RevertDataDirectoryResponse, String> {
    info!("⏪ Reverting data directory");
    app_state
        .data_directory_service
        .revert_directory(request)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn log_message(_app_state: tauri::State<'_, AppState>, log_entry: LogEntry) -> Result<(), String> {
    // This is a placeholder. The real logging service should be implemented.
    info!("[FRONTEND][{}] {}", log_entry.level, log_entry.message);
    Ok(())
}

#[tauri::command]
fn test_only_vec_strings(transaction_ids: Vec<String>) -> Result<String, String> {
    info!(
        "Received test_only_vec_strings command with: {:?}",
        transaction_ids
    );
    Ok(format!("Successfully received test_only_vec_strings"))
}

#[tauri::command]
fn test_number_and_vec_strings(
    num: i32,
    transaction_ids: Vec<String>,
) -> Result<String, String> {
    Ok(format!(
        "Received number {} and {} transaction IDs.",
        num,
        transaction_ids.len()
    ))
}

pub fn run() {
    // Initialize the backend services before starting Tauri
    let app_state = tauri::async_runtime::block_on(async {
        initialize_backend()
            .await
            .expect("Failed to initialize backend")
    });

    // Start the embedded Axum REST API server so the Yew frontend can
    // continue to use HTTP endpoints (http://localhost:3000) during the
    // transition phase to full native bindings.
    {
        let router = backend::create_router(app_state.clone());
        // Launch the server in a background task so it doesn't block the UI.
        tauri::async_runtime::spawn(async move {
            let addr: SocketAddr = "0.0.0.0:3000".parse().expect("Invalid bind address");
            info!("🌐 Starting embedded Axum REST API server at {}", addr);
            match TcpListener::bind(addr).await {
                Ok(listener) => {
                    if let Err(e) = serve(listener, router).await {
                        error!("Axum server error: {}", e);
                    }
                }
                Err(e) => error!("Failed to bind Axum listener: {}", e),
            }
        });
    }

    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .targets([
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::LogDir {
                        file_name: Some("allowance-tracker.log".to_string()),
                    }),
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout),
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Webview),
                ])
                .level(log::LevelFilter::Info)
                .build()
        )
        .plugin(tauri_plugin_dialog::init())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            // Real commands
            get_calendar_month,
            get_current_date,
            get_transactions,
            get_active_child,
            has_children,
            list_children,
            set_active_child,
            create_child,
            add_money,
            spend_money,
            delete_transactions,
            validate_parental_control,
            get_allowance_config,
            update_allowance_config,
            get_current_goal,
            create_goal,
            cancel_goal,
            get_data_directory,
            relocate_data_directory,
            revert_data_directory,
            log_message,
            // Test-only commands
            test_only_vec_strings,
            test_number_and_vec_strings,
        ])
        .setup(|app| {
            info!("🚀 Application setup complete, window should be visible.");
            let window = app.get_webview_window("main").unwrap();
            // window.open_devtools();
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use std::path::PathBuf;

    async fn setup_test_app_state() -> AppState {
        let temp_dir = tempdir().unwrap();
        initialize_backend(temp_dir.path().to_path_buf())
            .expect("Failed to initialize backend for test")
    }

    #[tokio::test]
    async fn test_full_flow() {
        let app_state = setup_test_app_state().await;
        let app_state_tauri = tauri::State::from(app_state);

        // 1. Create a child
        let create_child_req = CreateChildRequest {
            name: "Test Child".to_string(),
            birthdate: "2015-01-01".to_string(),
        };
        let create_child_res = create_child(app_state_tauri.clone(), create_child_req)
            .await
            .unwrap();
        let child_id = create_child_res.child.id;

        // 2. Set active child
        let set_active_req = SetActiveChildRequest { child_id: child_id.clone() };
        set_active_child(app_state_tauri.clone(), set_active_req)
            .await
            .unwrap();

        // 3. Add money
        let add_money_req = AddMoneyRequest {
            amount: 100.0,
            description: "Initial deposit".to_string(),
            date: None,
        };
        add_money(app_state_tauri.clone(), add_money_req)
            .await
            .unwrap();

        // 4. Check transactions
        let transactions = get_transactions(app_state_tauri.clone(), None).await.unwrap();
        assert_eq!(transactions.formatted_transactions.len(), 1);
        assert_eq!(transactions.formatted_transactions[0].amount, 100.0);
    }
}