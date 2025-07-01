use yew::prelude::*;
use wasm_bindgen_futures::spawn_local;

use hooks::{
    use_transactions::use_transactions,
    use_active_child::use_active_child,
    use_allowance::use_allowance,
    use_periodic_refresh::use_periodic_refresh_simple,
    use_periodic_refresh::use_periodic_refresh_staggered,
    use_interaction_detector::use_interaction_detector_simple,
    use_interaction_detector::use_interaction_detector_debug,
    use_interaction_detector::use_interaction_detector_targeted,
};

mod services;
mod components;
mod hooks;
use services::{
    api::ApiClient,
    logging::Logger,
};
use components::{
    simple_calendar::SimpleCalendar,
    transactions::TransactionViewContainer,
    forms::{
        add_money_form::AddMoneyForm,
        spend_money_form::SpendMoneyForm,
    },
    header::Header,
    goal_card::GoalCard,
};



#[function_component(App)]
fn app() -> Html {
    // Create the API client instance
    let api_client = use_memo((), |_| ApiClient::new());
    
    // Use custom hooks for data management
    let active_child = use_active_child(&api_client);
    let transactions = use_transactions(&api_client, active_child.state.child_change_trigger);
    let allowance = use_allowance();
    
    // Calendar refresh trigger - increment when transactions change
    let calendar_refresh_trigger = use_state(|| 0u32);
    
    // Interaction detector for pausing periodic refresh during user input
    // Option 1: Use the improved version that excludes input fields
    // let interaction_detector = use_interaction_detector_simple();
    
    // Option 2: Use the targeted version that only monitors specific elements (RECOMMENDED)
    let interaction_detector = use_interaction_detector_targeted(None);
    
    let refresh_all_data = {
        let refresh_transactions = transactions.actions.refresh_transactions.clone();
        let calendar_refresh_trigger = calendar_refresh_trigger.clone();
        Callback::from(move |_: ()| {
            gloo::console::log!("Refreshing all data (transactions and calendar)");
            refresh_transactions.emit(());
            calendar_refresh_trigger.set(*calendar_refresh_trigger + 1);
        })
    };
    
    // Periodic refresh setup - following the implementation plan
    
    // APPROACH 1: Original prop-based refresh (EXISTING)
    let refresh_calendar_periodic = {
        let calendar_refresh_trigger = calendar_refresh_trigger.clone();
        use_callback((), move |_, _| {
            // Generate a unique value without reading current state
            let new_value = (js_sys::Date::now() as u64) as u32; // Use full timestamp precision
            // Logger::debug_with_component("periodic-refresh", &format!("🔄 Calendar refresh triggered - setting trigger to: {}", new_value));
            calendar_refresh_trigger.set(new_value);
        })
    };
    
    // APPROACH 2: Direct callback refresh (REMOVED - prop approach now works)
    
    let refresh_transactions_periodic = {
        let refresh_transactions = transactions.actions.refresh_transactions.clone();
        Callback::from(move |_: ()| {
            // Logger::debug_with_component("periodic-refresh", "🔄 Periodic transactions refresh triggered");
            refresh_transactions.emit(());
        })
    };
    
    // Set up periodic refreshing with staggered timing - WITH IMPROVED INTERACTION DETECTION
    // Calendar: Every 10 minutes for day changes and allowance updates
    // Transactions: Every 10 minutes, offset by 5 minutes for load balancing
    
    // APPROACH 1: Calendar prop-based refresh (EXISTING)
    let calendar_refresh_result = use_periodic_refresh_simple(
        refresh_calendar_periodic,
        interaction_detector.is_active, // ENABLED - now works without blocking input
    );
          // Logger::debug_with_component("periodic-refresh", &format!("PROP: Calendar refresh hook initialized - running: {}, paused: {}", calendar_refresh_result.is_running, interaction_detector.is_active));
    
    // APPROACH 2: Calendar direct callback refresh (REMOVED - prop approach now works)
    
    // Transactions: Every 10 minutes, offset by 5 minutes
    let transactions_refresh_result = use_periodic_refresh_staggered(
        refresh_transactions_periodic,
        interaction_detector.is_active, // ENABLED - now works without blocking input
        300000, // 5 minute stagger (300 seconds)
    );
          // Logger::debug_with_component("periodic-refresh", &format!("Transactions refresh hook initialized - running: {}", transactions_refresh_result.is_running));
    
    // Connection status for parent info
    let backend_connected = use_state(|| false);
    let backend_endpoint = use_state(|| String::from("Checking..."));
    
    // Delete mode state
    let delete_mode = use_state(|| false);
    let selected_transactions = use_state(|| Vec::<String>::new());
    
    // Delete mode callbacks
    let toggle_delete_mode = {
        let delete_mode = delete_mode.clone();
        let selected_transactions = selected_transactions.clone();
        Callback::from(move |_| {
            let new_mode = !*delete_mode;
            delete_mode.set(new_mode);
            if !new_mode {
                selected_transactions.set(Vec::new()); // Clear selections when exiting delete mode
            }
        })
    };
    
    let toggle_transaction_selection = {
        let selected_transactions = selected_transactions.clone();
        Callback::from(move |transaction_id: String| {
            let mut current_selections = (*selected_transactions).clone();
            if let Some(index) = current_selections.iter().position(|id| id == &transaction_id) {
                current_selections.remove(index);
            } else {
                current_selections.push(transaction_id);
            }
            selected_transactions.set(current_selections);
        })
    };
    
    let delete_selected_transactions = {
        let selected_transactions = selected_transactions.clone();
        let delete_mode = delete_mode.clone();
        let api_client = api_client.clone();
        let refresh_transactions = transactions.actions.refresh_transactions.clone();
        let calendar_refresh_trigger = calendar_refresh_trigger.clone();
        
        Callback::from(move |_: ()| {
            let selected_ids = (*selected_transactions).clone();
            if selected_ids.is_empty() {
                return;
            }
            
            let api_client = api_client.clone();
            let selected_transactions = selected_transactions.clone();
            let delete_mode = delete_mode.clone();
            let refresh_transactions = refresh_transactions.clone();
            let calendar_refresh_trigger = calendar_refresh_trigger.clone();
            
            spawn_local(async move {
                let request = shared::DeleteTransactionsRequest {
                    transaction_ids: selected_ids.clone(),
                };
                
                match api_client.delete_transactions(request).await {
                    Ok(response) => {
                        gloo::console::log!("Delete successful:", &response.success_message);
                        if !response.not_found_ids.is_empty() {
                            gloo::console::warn!("Some transactions not found:", format!("{:?}", response.not_found_ids));
                        }
                        
                        // Clear selections and exit delete mode
                        selected_transactions.set(Vec::new());
                        delete_mode.set(false);
                        
                        // Refresh data to show updated state
                        refresh_transactions.emit(());
                        calendar_refresh_trigger.set(*calendar_refresh_trigger + 1);
                    }
                    Err(e) => {
                        gloo::console::error!("Failed to delete transactions:", &e);
                        // TODO: Show user-friendly error message
                    }
                }
            });
        })
    };
    




    // Load initial data
    use_effect_with((), {
        let api_client = api_client.clone();
        let refresh_transactions = transactions.actions.refresh_transactions.clone();
        let backend_connected = backend_connected.clone();
        let backend_endpoint = backend_endpoint.clone();
        
        move |_| {
            let api_client = api_client.clone();
            let refresh_transactions = refresh_transactions.clone();
            let backend_connected = backend_connected.clone();
            let backend_endpoint = backend_endpoint.clone();
            
            spawn_local(async move {
                // Test backend connection using API client
                match api_client.test_connection().await {
                    Ok(_) => {
                        // Successfully connected to backend
                        backend_connected.set(true);
                        backend_endpoint.set("localhost:3000".to_string());
                        
                        // Log successful frontend startup and backend connection
                        Logger::info_with_component("frontend-startup", "Frontend initialized and successfully connected to backend API");
                        
                        // Load all data
                        refresh_transactions.emit(());
                    },
                    Err(e) => {
                        // Failed to connect to backend
                        backend_connected.set(false);
                        backend_endpoint.set("Connection failed".to_string());
                        gloo::console::error!("Failed to connect to backend:", e);
                    }
                }
            });
            
            || ()
        }
    });

    html! {
        <>
            <Header 
                current_balance={transactions.state.current_balance} 
                on_toggle_delete_mode={toggle_delete_mode.clone()}
                api_client={(*api_client).clone()}
                active_child={active_child.state.active_child.clone()}
                child_loading={active_child.state.loading}
                active_child_actions={active_child.actions.clone()}
            />



            <main class="main">
                <div class="container">
                    // Calendar with transaction chips and allowance indicators
                    <SimpleCalendar 
                        refresh_trigger={*calendar_refresh_trigger}
                        delete_mode={*delete_mode}
                        selected_transactions={(*selected_transactions).clone()}
                        on_toggle_transaction_selection={toggle_transaction_selection.clone()}
                        on_delete_selected={delete_selected_transactions.clone()}
                        // on_refresh prop removed - using prop-based refresh only
                    />
                    
                    // Legacy calendar section removed - now using SimpleCalendar above

                    <TransactionViewContainer 
                        transactions={transactions.state.formatted_transactions.clone()} 
                        loading={transactions.state.loading}
                    />

                    <div class="money-management-container">
                        <SpendMoneyForm 
                            spend_description={transactions.state.spend_description.clone()}
                            spend_amount={transactions.state.spend_amount.clone()}
                            selected_spend_date={transactions.state.selected_spend_date.clone()}
                            recording_spending={transactions.state.creating_spend_transaction}
                            form_error={transactions.state.spend_form_error.clone()}
                            form_success={transactions.state.spend_form_success}
                            validation_suggestions={transactions.state.spend_validation_suggestions.clone()}
                            on_spend_description_change={transactions.actions.on_spend_description_change.clone()}
                            on_spend_amount_change={transactions.actions.on_spend_amount_change.clone()}
                            on_spend_date_change={transactions.actions.on_spend_date_change.clone()}
                            on_spend_submit={Callback::from(|_| {})} // Dummy callback - using FormData instead
                            on_debug={Callback::from(|_: String| {})} // Dummy callback
                            on_refresh={refresh_all_data.clone()}
                        />

                        <AddMoneyForm 
                            description={transactions.state.description.clone()}
                            amount={transactions.state.amount.clone()}
                            selected_date={transactions.state.selected_date.clone()}
                            creating_transaction={transactions.state.creating_transaction}
                            form_error={transactions.state.form_error.clone()}
                            form_success={transactions.state.form_success}
                            validation_suggestions={transactions.state.validation_suggestions.clone()}
                            on_description_change={transactions.actions.on_description_change.clone()}
                            on_amount_change={transactions.actions.on_amount_change.clone()}
                            on_date_change={transactions.actions.on_date_change.clone()}
                            on_submit={Callback::from(|_| {})} // Dummy callback - using FormData instead
                            on_debug={Callback::from(|_: String| {})} // Dummy callback
                            on_refresh={refresh_all_data.clone()}
                        />


                    </div>
                    
                    // Goal card at the end as requested
                    <GoalCard 
                        api_client={(*api_client).clone()}
                        on_refresh={refresh_all_data.clone()}
                    />
                </div>
            </main>
            
            <div class="connection-status">
                {if *backend_connected {
                    format!("Connected to {}", *backend_endpoint)
                } else {
                    (*backend_endpoint).clone()
                }}
            </div>
        </>
    }
}





fn main() {
    // Log frontend application startup
    Logger::info_with_component("frontend-main", "Frontend application starting up...");
    
    yew::Renderer::<App>::new().render();
}
