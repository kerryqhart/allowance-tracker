//! # App State Module
//!
//! This module defines the central application state structure and initialization logic
//! for the allowance tracker app.
//!
//! ## Key Types:
//! - `MainTab` - Enum defining available tabs (Calendar, Table)
//! - `AllowanceTrackerApp` - Main application state struct
//!
//! ## Key Functions:
//! - `new()` - Initialize new app instance with backend connection
//! - `clear_messages()` - Clear success/error messages
//!
//! ## Purpose:
//! This module serves as the central state management for the entire application,
//! using a modular architecture with separate state modules for different concerns.
//!
//! ## State Management:
//! The AllowanceTrackerApp struct composes multiple focused state modules:
//! - CoreAppState: Backend, child, balance, tab
//! - UIState: Loading, messages
//! - CalendarState: Calendar navigation, transactions
//! - ModalState: Modal visibility and flow
//! - FormState: Form inputs and validation
//! - InteractionState: User selections, dropdowns

use log::{info, warn};
use chrono::{Datelike, TimeZone};
use shared::*;
use crate::backend::Backend;
use crate::backend::domain::{SyncCommand, SyncThreadHandle};
use crate::backend::domain::sync_notifier::sync_channel;
use crate::backend::domain::sync_persistence::{SyncState, RetryQueue, sync_state_path, retry_queue_path};
use crate::backend::storage::HttpRemoteClient;
use std::sync::Arc;

// Import all state modules
use crate::ui::state::*;

// Re-export types from state modules to avoid duplication
pub use crate::ui::state::{MainTab, OverlayType, ParentalControlStage, ProtectedAction, TransactionType};
pub use crate::ui::state::form_state::{MoneyTransactionModalConfig, MoneyTransactionFormState};

/// Main application struct for the egui allowance tracker
///
/// This uses a modular architecture with focused state modules for maintainability.
pub struct AllowanceTrackerApp {
    // Modular state architecture
    pub core: CoreAppState,           // Backend, child, balance, tab
    pub ui: UIState,                  // Loading, messages
    pub calendar: CalendarState,      // Calendar navigation, overlays
    pub modal: ModalState,            // All modal states
    pub form: FormState,              // Form validation, inputs
    pub interaction: InteractionState, // User selections, dropdowns
    pub table: TableState,            // Transaction table pagination
    pub chart: ChartState,            // Chart visualization and time periods
    pub goal: GoalUiState,            // Goal management and progress tracking
    pub settings: crate::ui::components::settings::SettingsState, // Settings modals and forms
    pub sync: SyncUiState,            // Sync status and conflict management

    // Sync thread handles — None until Task 6 wires them at startup
    /// Sender to control the sync thread (PollNow, Shutdown). None when sync is disabled.
    pub sync_command_tx: Option<std::sync::mpsc::Sender<SyncCommand>>,
    /// Handle to the sync background thread. None when sync is disabled.
    pub sync_thread: Option<SyncThreadHandle>,
    /// Tracks whether the window was focused on the previous frame, for edge-detection.
    /// Initialised to `true` so the first frame doesn't spuriously trigger a `PollNow`
    /// — Task 6 can do an explicit startup poll if one is desired.
    pub was_focused: bool,
}

impl AllowanceTrackerApp {
    /// Create a new AllowanceTrackerApp with modular architecture
    pub fn new(cc: &eframe::CreationContext<'_>) -> Result<Self, anyhow::Error> {
        info!("Initializing AllowanceTrackerApp with modular architecture");
        
        // Setup custom fonts including Chalkboard
        crate::ui::setup_custom_fonts(&cc.egui_ctx);
        
        // Install image loaders for background support
        egui_extras::install_image_loaders(&cc.egui_ctx);
        
        // Create SyncNotifier before Backend so the notifier is wired in from the start.
        let (sync_notifier, event_rx) = sync_channel();
        let backend = crate::backend::Backend::new(Some(sync_notifier))?;

        // Check for pending allowances on app startup
        match backend.transaction_service.as_ref().check_and_issue_pending_allowances() {
            Ok(count) => {
                if count > 0 {
                    info!("Issued {} pending allowances on app startup", count);
                } else {
                    info!("No pending allowances found on app startup");
                }
            }
            Err(e) => {
                warn!("Failed to check pending allowances on startup: {}", e);
            }
        }
        
        let now = chrono::Local::now();
        let _current_month = now.month();
        let _current_year = now.year();

        // ── Sync startup wiring ───────────────────────────────────────────────
        //
        // Load persisted sync state from the data directory.  If the file
        // doesn't exist yet the types default to "disabled / no URL", so the
        // app starts normally with sync off.
        //
        // To enable sync manually (until a settings UI exists), create the
        // file `sync_state.yaml` in ~/Documents/Allowance Tracker:
        //
        //   enabled: true
        //   remote_url: "https://<your-api-gateway>.execute-api.<region>.amazonaws.com/internal"
        //   watermarks: {}
        let data_dir = backend.data_dir.clone();
        let sync_state = SyncState::load(&sync_state_path(&data_dir)).unwrap_or_default();
        let retry_queue = RetryQueue::load(&retry_queue_path(&data_dir)).unwrap_or_default();

        let (sync, sync_command_tx, sync_thread) =
            if sync_state.enabled {
                if let Some(ref url) = sync_state.remote_url {
                    info!("Sync enabled — connecting to remote: {}", url);
                    let remote: Arc<dyn crate::backend::storage::RemoteStorage> =
                        Arc::new(HttpRemoteClient::new(url.clone()));
                    let (message_tx, message_rx) = std::sync::mpsc::channel();
                    let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<SyncCommand>();

                    let handle = SyncThreadHandle::spawn(
                        remote,
                        event_rx,
                        cmd_rx,
                        message_tx,
                        sync_state.watermarks.clone(),
                        retry_queue.events.clone(),
                        data_dir.clone(),
                    );

                    (SyncUiState::with_receiver(message_rx), Some(cmd_tx), Some(handle))
                } else {
                    info!("Sync config has enabled=true but no remote_url — sync disabled");
                    // event_rx is dropped here; the SyncNotifier in Backend will log
                    // a warning on the first send but writes will not fail.
                    (SyncUiState::new(), None, None)
                }
            } else {
                info!("Sync disabled (no sync_state.yaml or enabled=false)");
                // Discard the event receiver — notifier sends are best-effort no-ops.
                drop(event_rx);
                (SyncUiState::new(), None, None)
            };
        // ─────────────────────────────────────────────────────────────────────

        // Initialize modular state components
        let core = CoreAppState::new(backend);
        let ui = UIState::new();
        let calendar = CalendarState::new(); // Uses current date
        let modal = ModalState::new();
        let form = FormState::new();
        let interaction = InteractionState::new();
        let table = TableState::new();
        let chart = ChartState::new();
        let goal = GoalUiState::new();
        let settings = crate::ui::components::settings::SettingsState::new();

        Ok(Self {
            // Modular state
            core,
            ui,
            calendar,
            modal,
            form,
            interaction,
            table,
            chart,
            goal,
            settings,
            sync,
            sync_command_tx,
            sync_thread,
            was_focused: true,
        })
    }

    // TEMPORARY: Getter methods for backward compatibility
    pub fn backend(&self) -> &Backend {
        &self.core.backend
    }
    
    /// Get current child directly from backend service (the source of truth)
    /// This replaces the cached current_child() method to avoid inconsistencies
    pub fn get_current_child_from_backend(&self) -> Option<shared::Child> {
        match self.backend().child_service.get_active_child() {
            Ok(result) => {
                // Only log once per actual change, not every frame (commented out to reduce noise)
                // log::info!("GET_CURRENT_CHILD_BACKEND: Raw result: {:?}", 
                //     result.active_child.child.as_ref().map(|c| (&c.id, &c.name)));
                
                result.active_child.child.map(|domain_child| {
                    crate::ui::mappers::to_dto(domain_child)
                })
            },
            Err(e) => {
                log::warn!("GET_CURRENT_CHILD_BACKEND: Failed to get current child from backend: {}", e);
                None
            }
        }
    }
    
    /// DEPRECATED: Get current child from cached state 
    /// Use get_current_child_from_backend() instead to ensure consistency
    #[deprecated(note = "Use get_current_child_from_backend() instead")]
    pub fn current_child(&self) -> &Option<Child> {
        &self.core.current_child
    }
    
    pub fn current_balance(&self) -> f64 {
        self.core.current_balance
    }
    
    pub fn current_tab(&self) -> MainTab {
        self.core.current_tab
    }
    
    // TEMPORARY: Setter methods for state synchronization
    pub fn set_current_tab(&mut self, tab: MainTab) {
        self.core.current_tab = tab;
    }
    
    pub fn set_loading(&mut self, loading: bool) {
        self.ui.loading = loading;
    }

    /// Start parental control challenge for a specific action
    pub fn start_parental_control_challenge(&mut self, action: crate::ui::state::modal_state::ProtectedAction) {
        use crate::ui::state::modal_state::ParentalControlStage;
        
        info!("Starting parental control challenge for: {:?}", action);
        self.modal.pending_protected_action = Some(action);
        self.modal.parental_control_stage = ParentalControlStage::Question1;
        self.modal.parental_control_input.clear();
        self.modal.parental_control_error = None;
        self.modal.parental_control_loading = false;
        self.modal.show_parental_control_modal = true;
    }

    /// Cancel parental control challenge
    pub fn cancel_parental_control_challenge(&mut self) {
        info!("Cancelling parental control challenge");
        self.modal.show_parental_control_modal = false;
        self.modal.pending_protected_action = None;
        self.modal.parental_control_stage = ParentalControlStage::Question1;
        self.modal.parental_control_input.clear();
        self.modal.parental_control_error = None;
        self.modal.parental_control_loading = false;
    }

    /// Reset parental control state to question 1
    pub fn reset_parental_control(&mut self) {
        self.modal.parental_control_stage = ParentalControlStage::Question1;
        self.modal.parental_control_input.clear();
        self.modal.parental_control_error = None;
        self.modal.parental_control_loading = false;
    }

    /// Advance to question 2
    pub fn advance_to_question_2(&mut self) {
        self.modal.parental_control_stage = ParentalControlStage::Question2;
        self.modal.parental_control_input.clear();
        self.modal.parental_control_error = None;
    }

    /// Mark parental control as authenticated and proceed to question stage
    pub fn mark_parental_control_authenticated(&mut self) {
        self.modal.parental_control_stage = ParentalControlStage::Question1;
        self.modal.parental_control_input.clear();
        self.modal.parental_control_error = None;
        self.modal.parental_control_loading = false;
        
        // Sync compatibility fields
        // self.selected_month = self.calendar.selected_month;
        // self.selected_year = self.calendar.selected_year;
        // self.calendar_loading = self.calendar.calendar_loading;
        
        // Reload calendar data for the new month
        self.load_calendar_data();
        info!("Navigated to previous month: {}/{}", self.calendar.selected_month, self.calendar.selected_year);
    }

    /// Navigate to the next month
    pub fn navigate_to_next_month(&mut self) {
        self.calendar.navigate_to_next_month();
        
        // Sync compatibility fields
        // self.selected_month = self.calendar.selected_month;
        // self.selected_year = self.calendar.selected_year;
        // self.calendar_loading = self.calendar.calendar_loading;
        
        // Reload calendar data for the new month
        self.load_calendar_data();
        info!("Navigated to next month: {}/{}", self.calendar.selected_month, self.calendar.selected_year);
    }

    /// Get the current month name as a string
    pub fn get_current_month_name(&self) -> String {
        self.calendar.get_current_month_name()
    }

    /// Clear any error or success messages
    pub fn clear_messages(&mut self) {
        self.ui.clear_messages();
        
        // Sync compatibility fields
        // self.error_message = self.ui.error_message.clone(); // Removed
        // self.success_message = self.ui.success_message.clone(); // Removed
    }

    /// Submit answer for parental control validation
    pub fn submit_parental_control_answer(&mut self) {
        // Validate input
        if self.modal.parental_control_input.trim().is_empty() {
            self.modal.parental_control_error = Some("Please enter an answer".to_string());
            return;
        }
        
        // Set loading state and clear errors
        self.modal.parental_control_loading = true;
        self.modal.parental_control_error = None;
        
        // Create command for backend validation
        let command = crate::backend::domain::commands::parental_control::ValidateParentalControlCommand {
            answer: self.modal.parental_control_input.clone(),
        };
        
        // Call backend service
        match self.backend().parental_control_service.validate_answer(command) {
            Ok(result) => {
                self.modal.parental_control_loading = false;
                
                if result.success {
                    info!("Parental control authentication successful");
                    self.modal.parental_control_stage = ParentalControlStage::Authenticated;
                    
                    // Execute the pending action
                    info!("PARENTAL_CONTROL_SUCCESS: Checking for pending actions...");
                    info!("pending_protected_action = {:?}", self.modal.pending_protected_action);
                    info!("pending_settings_action = {:?}", self.modal.pending_settings_action);
                    
                    if let Some(action) = self.modal.pending_protected_action {
                        info!("EXECUTING protected action: {:?}", action);
                        self.execute_protected_action(action);
                    } else {
                        log::warn!("WARNING: No pending protected action found after successful parental control!");
                    }
                    
                    // Close modal after brief success display
                    self.modal.show_parental_control_modal = false;
                    // Access granted feedback removed
                } else {
                    info!("Parental control validation failed");
                    self.modal.parental_control_error = Some(result.message);
                    self.modal.parental_control_input.clear();
                }
            }
            Err(e) => {
                self.modal.parental_control_loading = false;
                log::error!("🚨 Parental control validation error: {}", e);
                self.modal.parental_control_error = Some("Validation failed. Please try again.".to_string());
            }
        }
    }
    
    /// Execute the action after successful authentication
    fn execute_protected_action(&mut self, action: crate::ui::state::modal_state::ProtectedAction) {
        use crate::ui::state::modal_state::ProtectedAction;
        
        info!("EXECUTE_PROTECTED_ACTION called with: {:?}", action);
        
        match action {
            ProtectedAction::DeleteTransactions => {
                info!("Executing delete transactions action");
                self.enter_transaction_selection_mode();
            }
            ProtectedAction::AccessSettings => {
                info!("EXECUTING SETTINGS ACCESS ACTION!");
                info!("Checking for pending_settings_action...");
                if let Some(settings_action) = self.modal.pending_settings_action {
                    info!("Found pending settings action: {:?}", settings_action);
                    info!("CALLING execute_settings_action...");
                    self.execute_settings_action(settings_action);
                } else {
                    log::warn!("🚨 AccessSettings action triggered but no pending settings action found");
                }
            }
        }
        
        self.modal.pending_protected_action = None;
        self.modal.pending_settings_action = None; // Clear both actions
    }
    
    /// Execute specific settings menu action after parental control authentication
    fn execute_settings_action(&mut self, action: crate::ui::state::modal_state::SettingsAction) {
        use crate::ui::state::modal_state::SettingsAction;
        
        info!("⚙️ EXECUTE_SETTINGS_ACTION CALLED!");
        info!("⚙️ Settings action received: {:?}", action);
        info!("⚙️ About to enter match statement...");
        
        match action {
            SettingsAction::ShowProfile => {
                // Extract child data before mutations to avoid borrow conflicts
                let child_data = if let Some(child) = self.get_current_child_from_backend() {
                    Some((
                        child.id.clone(),
                        child.name.clone(),
                        child.birthdate,
                        child.created_at,
                        child.updated_at,
                    ))
                } else {
                    None
                };
                
                if let Some((id, name, birthdate, created_at, updated_at)) = child_data {
                    let domain_child = crate::backend::domain::models::child::Child {
                        id: id.clone(),
                        name: name.clone(),
                        birthdate,
                        created_at,
                        updated_at,
                    };
                    self.settings.profile_form.populate_from_child(&domain_child);
                    self.settings.show_profile_modal = true;
                    info!("👤 Profile modal opened for child: {}", name);
                } else {
                    log::warn!("🚨 No active child found for profile action");
                    self.ui.error_message = Some("No child selected. Please select a child first.".to_string());
                }
            }
            SettingsAction::CreateChild => {
                info!("👶 Create child action - opening modal");
                self.settings.show_create_child_modal = true;
                self.settings.create_child_form.clear(); // Reset form state
            }
            SettingsAction::ConfigureAllowance => {
                info!("🚨 CONFIGURE_ALLOWANCE_ACTION_TRIGGERED! Opening modal...");
                if self.get_current_child_from_backend().is_some() {
                    info!("🚨 Setting show_allowance_config_modal = true");
                    self.settings.show_allowance_config_modal = true;
                    info!("🚨 Modal flag set, now loading config...");
                    self.load_allowance_config_for_modal(); // Load existing config
                    info!("🚨 Config loaded, modal should be visible");
                } else {
                    info!("🚨 ERROR: No child selected for allowance config");
                    self.ui.error_message = Some("No child selected. Please select a child first.".to_string());
                }
            }
            SettingsAction::DeleteTransactions => {
                info!("Delete transactions action - entering selection mode");
                self.enter_transaction_selection_mode();
            }
            SettingsAction::ExportData => {
                info!("📤 Export data action - opening modal");
                self.settings.show_export_modal = true;
                self.settings.export_form.clear(); // Reset form state
                
                // Update preview immediately
                let child_name = self.get_current_child_from_backend().as_ref().map(|c| c.name.clone());
                let child_name_ref = child_name.as_deref();
                self.settings.export_form.update_preview(child_name_ref);
            }
            SettingsAction::DataDirectory => {
                info!("Data directory action - opening modal");
                self.settings.show_data_directory_modal = true;

                // Clear form state when opening modal
                self.settings.data_directory_form.clear();
            }
            SettingsAction::InitialSync => {
                self.open_backfill_modal();
            }
        }
    }
    
    // ====================
    // TRANSACTION SELECTION METHODS
    // ====================
    
    /// Enter transaction selection mode for deletion
    pub fn enter_transaction_selection_mode(&mut self) {
        info!("Entering transaction selection mode");
        self.interaction.transaction_selection_mode = true;
        self.interaction.selected_transaction_ids.clear();
        
        // TEMPORARY: Sync compatibility fields
        // self.transaction_selection_mode = true;
        // self.selected_transaction_ids.clear();
        
        // Transaction selection mode feedback removed
    }
    
    /// Exit transaction selection mode without deleting
    pub fn exit_transaction_selection_mode(&mut self) {
        info!("🚫 Exiting transaction selection mode");
        self.interaction.transaction_selection_mode = false;
        self.interaction.selected_transaction_ids.clear();
        
        // TEMPORARY: Sync compatibility fields
        // self.transaction_selection_mode = false;
        // self.selected_transaction_ids.clear();
        
        self.clear_messages();
    }
    
    /// Toggle selection of a transaction
    pub fn toggle_transaction_selection(&mut self, transaction_id: &str) {
        if self.interaction.selected_transaction_ids.contains(transaction_id) {
            info!("➖ Deselecting transaction: {}", transaction_id);
            self.interaction.selected_transaction_ids.remove(transaction_id);
            // self.selected_transaction_ids.remove(transaction_id); // Sync compatibility field
        } else {
            info!("Selecting transaction: {}", transaction_id);
            self.interaction.selected_transaction_ids.insert(transaction_id.to_string());
            // self.selected_transaction_ids.insert(transaction_id.to_string()); // Sync compatibility field
        }
    }
    
    /// Check if a transaction is selected
    pub fn is_transaction_selected(&self, transaction_id: &str) -> bool {
        self.interaction.selected_transaction_ids.contains(transaction_id)
    }
    
    /// Get count of selected transactions
    pub fn selected_transaction_count(&self) -> usize {
        self.interaction.selected_transaction_ids.len()
    }
    
    /// Clear all selected transactions
    pub fn clear_transaction_selection(&mut self) {
        info!("🧹 Clearing all transaction selections");
        self.interaction.selected_transaction_ids.clear();
        // self.selected_transaction_ids.clear(); // Sync compatibility field
    }
    
    /// Check if any transactions are selected
    pub fn has_selected_transactions(&self) -> bool {
        !self.interaction.selected_transaction_ids.is_empty()
    }
    
    // ====================
    // ADD MONEY FORM VALIDATION METHODS
    // ====================

    /// Validate the add money form and update validation state
    /// Delegates to MoneyManagementService for consistent validation logic
    pub fn validate_add_money_form(&mut self) {
        use crate::backend::domain::money_management::MoneyManagementService;

        self.form.add_money_description_error = None;
        self.form.add_money_amount_error = None;

        let service = MoneyManagementService::new();
        let description = &self.form.add_money_description;
        let amount_input = &self.form.add_money_amount;

        // Use domain service validation
        let validation = service.validate_add_money_form(description, amount_input);

        // Map validation errors to UI state
        for error in &validation.errors {
            use shared::ValidationError;
            match error {
                ValidationError::EmptyDescription |
                ValidationError::DescriptionTooLong(_) => {
                    self.form.add_money_description_error = Some(service.get_error_message(error));
                }
                ValidationError::EmptyAmount => {
                    // Don't show "Amount is required" error immediately - let the grayed button be sufficient
                }
                _ => {
                    self.form.add_money_amount_error = Some(service.get_error_message(error));
                }
            }
        }

        // Update overall validation state
        self.form.add_money_is_valid = self.form.add_money_description_error.is_none()
            && self.form.add_money_amount_error.is_none()
            && !description.trim().is_empty()
            && !amount_input.trim().is_empty();
    }

    /// Clean and parse amount input string
    /// Delegates to MoneyManagementService for consistent parsing logic
    fn clean_and_parse_amount(&self, amount_input: &str) -> Result<f64, String> {
        use crate::backend::domain::money_management::MoneyManagementService;
        MoneyManagementService::new().clean_and_parse_amount(amount_input)
    }
    
    /// Format amount for currency display ($XX.XX)
    pub fn format_currency_amount(&self, amount: f64) -> String {
        format!("${:.2}", amount)
    }
    
    /// Clear add money form and validation state
    pub fn clear_add_money_form(&mut self) {
        self.form.add_money_description.clear();
        self.form.add_money_amount.clear();
        self.form.add_money_description_error = None;
        self.form.add_money_amount_error = None;
        self.form.add_money_is_valid = true;
    }
    
    /// Auto-format amount field as user types (adds $ and proper decimal formatting)
    pub fn auto_format_amount_field(&mut self) {
        let input = self.form.add_money_amount.clone();

        // Only auto-format if the input looks like a valid number
        if let Ok(amount) = self.clean_and_parse_amount(&input) {
            // Only format if the amount is reasonable
            if amount > 0.0 && amount < 1_000_000.0 {
                // Format as $XX.XX but only if user isn't currently typing
                if !input.ends_with('.') && !input.ends_with('0') {
                    self.form.add_money_amount = format!("{:.2}", amount);
                }
            }
        }
    }
    
    // ====================
    // GENERIC MONEY TRANSACTION FORM VALIDATION METHODS
    // ====================

    /// Validate a generic money transaction form and update its validation state
    /// Delegates amount validation to MoneyManagementService for consistency
    pub fn validate_money_transaction_form(&self, form_state: &mut MoneyTransactionFormState, config: &MoneyTransactionModalConfig) {
        use crate::backend::domain::money_management::MoneyManagementService;

        form_state.description_error = None;
        form_state.amount_error = None;

        let service = MoneyManagementService::new();

        // Validate description (use UI config for max length)
        let description = form_state.description.trim();
        if description.is_empty() {
            form_state.description_error = Some("Description is required".to_string());
        } else if description.len() > config.max_description_length {
            form_state.description_error = Some(format!(
                "Description too long ({}/{} characters)",
                description.len(),
                config.max_description_length
            ));
        }

        // Validate amount using domain service
        let amount_input = form_state.amount.trim();
        if !amount_input.is_empty() {
            let validation = service.validate_add_money_form(description, amount_input);
            for error in &validation.errors {
                use shared::ValidationError;
                match error {
                    ValidationError::EmptyDescription |
                    ValidationError::DescriptionTooLong(_) => {
                        // Skip - we handle description validation above with UI config
                    }
                    ValidationError::EmptyAmount => {
                        // Don't show error for empty - let grayed button be sufficient
                    }
                    _ => {
                        form_state.amount_error = Some(service.get_error_message(error));
                    }
                }
            }
        }

        // Update overall validation state
        form_state.is_valid = form_state.description_error.is_none()
            && form_state.amount_error.is_none()
            && !description.is_empty()
            && !amount_input.is_empty();
    }
    
    // ====================
    // BACKEND INTEGRATION METHODS
    // ====================
    
    pub fn submit_income_transaction(&mut self) -> bool {
        use crate::backend::domain::money_management::MoneyManagementService;
        use chrono::Timelike;
        info!("Submitting income transaction - Description: '{}', Amount: '{}'", 
                  self.form.income_form_state.description, self.form.income_form_state.amount);
        let amount = match self.clean_and_parse_amount(&self.form.income_form_state.amount) {
            Ok(amount) => amount,
            Err(error) => {
                log::error!("Failed to parse amount: {}", error);
                self.ui.error_message = Some(format!("Invalid amount: {}", error));
                return false;
            }
        };
        let date_time = self.calendar.selected_day.map(|date| {
            let now = chrono::Local::now();
            let naive_datetime = date.and_hms_opt(now.hour(), now.minute(), now.second()).unwrap();
            let eastern_offset = chrono::FixedOffset::west_opt(5 * 3600).unwrap();
            eastern_offset.from_local_datetime(&naive_datetime).single().unwrap()
        });
        let request = shared::AddMoneyRequest {
            description: self.form.income_form_state.description.trim().to_string(),
            amount,
            date: date_time,
        };
        let money_service = MoneyManagementService::new();
        match money_service.add_money_complete(
            request,
            &self.backend().child_service,
            &self.backend().transaction_service,
            &self.backend().goal_service,
        ) {
            Ok(response) => {
                info!("Income transaction successful: {}", response.success_message);
                self.core.current_balance = response.new_balance;
                self.load_calendar_data();
                true
            }
            Err(error) => {
                log::error!("Income transaction failed: {}", error);
                self.ui.error_message = Some(format!("Failed to add money: {}", error));
                false
            }
        }
    }

    pub fn submit_expense_transaction(&mut self) -> bool {
        use crate::backend::domain::money_management::MoneyManagementService;
        use chrono::Timelike;
        info!("💸 Submitting expense transaction - Description: '{}', Amount: '{}'", 
                  self.form.expense_form_state.description, self.form.expense_form_state.amount);
        let amount = match self.clean_and_parse_amount(&self.form.expense_form_state.amount) {
            Ok(amount) => amount,
            Err(error) => {
                log::error!("Failed to parse amount: {}", error);
                self.ui.error_message = Some(format!("Invalid amount: {}", error));
                return false;
            }
        };
        let date_time = self.calendar.selected_day.map(|date| {
            let now = chrono::Local::now();
            let naive_datetime = date.and_hms_opt(now.hour(), now.minute(), now.second()).unwrap();
            let eastern_offset = chrono::FixedOffset::west_opt(5 * 3600).unwrap();
            eastern_offset.from_local_datetime(&naive_datetime).single().unwrap()
        });
        let request = shared::SpendMoneyRequest {
            description: self.form.expense_form_state.description.trim().to_string(),
            amount,
            date: date_time,
        };
        let money_service = MoneyManagementService::new();
        match money_service.spend_money_complete(
            request,
            &self.backend().child_service,
            &self.backend().transaction_service,
            &self.backend().goal_service,
        ) {
            Ok(response) => {
                info!("Expense transaction successful: {}", response.success_message);
                self.core.current_balance = response.new_balance;
                self.load_calendar_data();
                true
            }
            Err(error) => {
                log::error!("Expense transaction failed: {}", error);
                self.ui.error_message = Some(format!("Failed to spend money: {}", error));
                false
            }
        }
    }
}

impl Drop for AllowanceTrackerApp {
    fn drop(&mut self) {
        // Shut down the sync thread cleanly when the app closes.
        // SyncThreadHandle's own Drop is a no-op after this because
        // shutdown() uses Option::take on its internal join handle.
        if let Some(ref mut thread) = self.sync_thread {
            thread.shutdown();
        }
    }
}
