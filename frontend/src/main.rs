use yew::prelude::*;
use wasm_bindgen_futures::spawn_local;
use hooks::{
    use_transactions::use_transactions,
    use_calendar::use_calendar,
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
    let transactions = use_transactions(&api_client);
    let calendar = use_calendar(&api_client);
    
    // Connection status for parent info
    let backend_connected = use_state(|| false);
    let backend_endpoint = use_state(|| String::from("Checking..."));



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
            <Header current_balance={transactions.state.current_balance} />

            <main class="main">
                <div class="container">
                    <section class="calendar-section">
                        <div class="calendar-header">
                            <button class="calendar-nav-btn" onclick={calendar.actions.prev_month}>{"‹"}</button>
                            <h2 class="calendar-title">
                                {if let Some(cal_data) = calendar.state.calendar_data.as_ref() {
                                    format!("{} {}", month_name(cal_data.month), cal_data.year)
                                } else {
                                    format!("Loading...")
                                }}
                            </h2>
                            <button class="calendar-nav-btn" onclick={calendar.actions.next_month}>{"›"}</button>
                        </div>
                        
                        {if let Some(cal_data) = calendar.state.calendar_data.as_ref() {
                            html! { <Calendar calendar_data={cal_data.clone()} /> }
                        } else {
                            html! { <div class="loading">{"Loading calendar..."}</div> }
                        }}
                    </section>

                    <TransactionTable 
                        transactions={transactions.state.formatted_transactions.clone()} 
                        loading={transactions.state.loading}
                    />

                    <div class="money-management-container">
                        <AddMoneyForm 
                            description={transactions.state.description.clone()}
                            amount={transactions.state.amount.clone()}
                            creating_transaction={transactions.state.creating_transaction}
                            form_error={transactions.state.form_error.clone()}
                            form_success={transactions.state.form_success}
                            validation_suggestions={transactions.state.validation_suggestions.clone()}
                            on_description_change={transactions.actions.on_description_change}
                            on_amount_change={transactions.actions.on_amount_change}
                            on_submit={transactions.actions.add_money}
                        />

                        <SpendMoneyForm 
                            description={transactions.state.spend_description.clone()}
                            amount={transactions.state.spend_amount.clone()}
                            creating_transaction={transactions.state.creating_spend_transaction}
                            form_error={transactions.state.spend_form_error.clone()}
                            form_success={transactions.state.spend_form_success}
                            validation_suggestions={transactions.state.spend_validation_suggestions.clone()}
                            on_description_change={transactions.actions.on_spend_description_change}
                            on_amount_change={transactions.actions.on_spend_amount_change}
                            on_submit={transactions.actions.spend_money}
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
