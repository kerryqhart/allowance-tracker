use yew::prelude::*;
use wasm_bindgen_futures::spawn_local;

use hooks::{
    use_transactions::use_transactions,
    use_calendar::use_calendar,
    use_active_child::use_active_child,
    use_allowance::use_allowance,
};

mod services;
mod components;
mod hooks;
use services::{
    date_utils::month_name,
    api::ApiClient,
};
use components::{
    calendar::Calendar,
    transactions::transaction_table::TransactionTable,
    forms::{
        add_money_form::AddMoneyForm,
        spend_money_form::SpendMoneyForm,
    },
    header::Header,
};



#[function_component(App)]
fn app() -> Html {
    // Create the API client instance
    let api_client = use_memo((), |_| ApiClient::new());
    
    // Use custom hooks for data management
    let active_child = use_active_child(&api_client);
    let transactions = use_transactions(&api_client, active_child.state.child_change_trigger);
    let calendar = use_calendar(&api_client, active_child.state.child_change_trigger);
    let allowance = use_allowance();
    
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
        let refresh_calendar = calendar.actions.refresh_calendar.clone();
        
        Callback::from(move |_| {
            let selected_ids = (*selected_transactions).clone();
            if selected_ids.is_empty() {
                return;
            }
            
            let api_client = api_client.clone();
            let selected_transactions = selected_transactions.clone();
            let delete_mode = delete_mode.clone();
            let refresh_transactions = refresh_transactions.clone();
            let refresh_calendar = refresh_calendar.clone();
            
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
                        refresh_calendar.emit(());
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
                    <section class="calendar-section">
                        <div class="calendar-header">
                            <button class="calendar-nav-btn" onclick={calendar.actions.prev_month}>{"‹"}</button>
                            <h2 class="calendar-title">
                                {if let Some(cal_data) = calendar.state.calendar_data.as_ref() {
                                    if *delete_mode {
                                        format!("🗑️ Delete Mode - {} {}", month_name(cal_data.month), cal_data.year)
                                    } else {
                                        format!("{} {}", month_name(cal_data.month), cal_data.year)
                                    }
                                } else {
                                    format!("Loading...")
                                }}
                            </h2>
                            <button class="calendar-nav-btn" onclick={calendar.actions.next_month}>{"›"}</button>
                        </div>
                        
                        {if let Some(cal_data) = calendar.state.calendar_data.as_ref() {
                            html! { 
                                <Calendar 
                                    calendar_data={cal_data.clone()} 
                                    delete_mode={*delete_mode}
                                    selected_transactions={(*selected_transactions).clone()}
                                    on_toggle_transaction_selection={toggle_transaction_selection.clone()}
                                    on_delete_selected={delete_selected_transactions.clone()}
                                    allowance_config={allowance.config.clone()}
                                /> 
                            }
                        } else {
                            html! { <div class="loading">{"Loading calendar..."}</div> }
                        }}
                    </section>

                    <TransactionTable 
                        transactions={transactions.state.formatted_transactions.clone()} 
                        loading={transactions.state.loading}
                    />

                    <div class="money-management-container">
                        <SpendMoneyForm 
                            spend_description={transactions.state.spend_description.clone()}
                            spend_amount={transactions.state.spend_amount.clone()}
                            recording_spending={transactions.state.creating_spend_transaction}
                            form_error={transactions.state.spend_form_error.clone()}
                            form_success={transactions.state.spend_form_success}
                            validation_suggestions={transactions.state.spend_validation_suggestions.clone()}
                            on_spend_description_change={transactions.actions.on_spend_description_change.clone()}
                            on_spend_amount_change={transactions.actions.on_spend_amount_change.clone()}
                            on_spend_submit={Callback::from(|_| {})} // Dummy callback - using FormData instead
                            on_debug={Callback::from(|_: String| {})} // Dummy callback
                            on_refresh={transactions.actions.refresh_transactions.clone()}
                        />

                        <AddMoneyForm 
                            description={transactions.state.description.clone()}
                            amount={transactions.state.amount.clone()}
                            creating_transaction={transactions.state.creating_transaction}
                            form_error={transactions.state.form_error.clone()}
                            form_success={transactions.state.form_success}
                            validation_suggestions={transactions.state.validation_suggestions.clone()}
                            on_description_change={transactions.actions.on_description_change.clone()}
                            on_amount_change={transactions.actions.on_amount_change.clone()}
                            on_submit={Callback::from(|_| {})} // Dummy callback - using FormData instead
                            on_debug={Callback::from(|_: String| {})} // Dummy callback
                            on_refresh={transactions.actions.refresh_transactions.clone()}
                        />


                    </div>
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
    yew::Renderer::<App>::new().render();
}
