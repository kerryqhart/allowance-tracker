use yew::prelude::*;
use shared::{AddMoneyRequest, SpendMoneyRequest, CalendarMonth, FormattedTransaction};
use wasm_bindgen_futures::spawn_local;
use web_sys::{HtmlInputElement, MouseEvent};

mod services;
mod components;
use services::{
    date_utils::month_name,
    api::ApiClient,
};
use components::{
    calendar::Calendar,
    transactions::transaction_table::TransactionTable,
};



#[function_component(App)]
fn app() -> Html {
    // API client for backend communication
    let api_client = use_state(|| ApiClient::new());
    
    let current_month = use_state(|| 6u32); // June
    let current_year = use_state(|| 2025u32);
    let formatted_transactions = use_state(|| Vec::<FormattedTransaction>::new()); // Backend-formatted transactions
    let calendar_data = use_state(|| Option::<CalendarMonth>::None); // Backend-provided calendar
    let loading = use_state(|| true);
    let current_balance = use_state(|| 0.0f64);
    
    // Form state for transaction creation
    let description = use_state(|| String::new());
    let amount = use_state(|| String::new());
    let creating_transaction = use_state(|| false);
    let form_error = use_state(|| Option::<String>::None);
    let form_success = use_state(|| false);
    let validation_suggestions = use_state(|| Vec::<String>::new());
    
    // Spend money form states
    let spend_description = use_state(String::new);
    let spend_amount = use_state(String::new);
    let creating_spend_transaction = use_state(|| false);
    let spend_form_error = use_state(|| None::<String>);
    let spend_form_success = use_state(|| false);
    let spend_validation_suggestions = use_state(Vec::<String>::new);
    
    // Connection status for parent info
    let backend_connected = use_state(|| false);
    let backend_endpoint = use_state(|| String::from("Checking..."));

    // Function to refresh transaction data
    let refresh_transactions = {
        let api_client = api_client.clone();
        let formatted_transactions = formatted_transactions.clone();
        let calendar_data = calendar_data.clone();
        let current_balance = current_balance.clone();
        let loading = loading.clone();
        let current_month = current_month.clone();
        let current_year = current_year.clone();
        
        Callback::from(move |_: ()| {
            let api_client = (*api_client).clone();
            let formatted_transactions = formatted_transactions.clone();
            let calendar_data = calendar_data.clone();
            let current_balance = current_balance.clone();
            let loading = loading.clone();
            let current_month = current_month.clone();
            let current_year = current_year.clone();
            
            spawn_local(async move {
                loading.set(true);
                
                // Refresh formatted transaction table using API client
                match api_client.get_transactions(Some(10)).await {
                    Ok(data) => {
                        // Set current balance from most recent transaction
                        if let Some(first_tx) = data.formatted_transactions.first() {
                            current_balance.set(first_tx.raw_balance);
                        }
                        formatted_transactions.set(data.formatted_transactions);
                    }
                    Err(e) => {
                        gloo::console::error!("Failed to fetch transactions:", e);
                    }
                }
                
                // Refresh calendar data using API client
                match api_client.get_calendar_month(*current_month, *current_year).await {
                    Ok(data) => {
                        calendar_data.set(Some(data));
                    }
                    Err(e) => {
                        gloo::console::error!("Failed to fetch calendar data:", e);
                    }
                }
                
                loading.set(false);
            });
        })
    };

    // Add money callback - simplified to use backend validation and processing
    let add_money = {
        let api_client = api_client.clone();
        let description = description.clone();
        let amount = amount.clone();
        let creating_transaction = creating_transaction.clone();
        let form_error = form_error.clone();
        let form_success = form_success.clone();
        let refresh_transactions = refresh_transactions.clone();
        
        Callback::from(move |_: ()| {
            let api_client = (*api_client).clone();
            let description = description.clone();
            let amount = amount.clone();
            let creating_transaction = creating_transaction.clone();
            let form_error = form_error.clone();
            let form_success = form_success.clone();
            let refresh_transactions = refresh_transactions.clone();
            
            spawn_local(async move {
                // Clear previous messages
                form_error.set(None);
                form_success.set(false);
                creating_transaction.set(true);
                
                // Parse amount for the API (let backend handle validation)  
                let amount_value = match (*amount).trim().parse::<f64>() {
                    Ok(val) => val,
                    Err(_) => {
                        // If we can't parse, let backend handle the validation error
                        0.0
                    }
                };
                
                let request = AddMoneyRequest {
                    description: (*description).clone(),
                    amount: amount_value,
                    date: None, // Use current time
                };
                
                match api_client.add_money(request).await {
                    Ok(_response) => {
                        // Clear form and show success
                        description.set(String::new());
                        amount.set(String::new());
                        form_success.set(true);
                        refresh_transactions.emit(());
                        
                        // Clear success message after 3 seconds
                        let form_success_clear = form_success.clone();
                        spawn_local(async move {
                            gloo::timers::future::TimeoutFuture::new(3000).await;
                            form_success_clear.set(false);
                        });
                    }
                    Err(error_message) => {
                        form_error.set(Some(error_message));
                    }
                }
                
                creating_transaction.set(false);
            });
        })
    };

    // Spend money callback 
    let spend_money = {
        let api_client = api_client.clone();
        let spend_description = spend_description.clone();
        let spend_amount = spend_amount.clone();
        let creating_spend_transaction = creating_spend_transaction.clone();
        let spend_form_error = spend_form_error.clone();
        let spend_form_success = spend_form_success.clone();
        let refresh_transactions = refresh_transactions.clone();
        
        Callback::from(move |_: ()| {
            let api_client = (*api_client).clone();
            let spend_description = spend_description.clone();
            let spend_amount = spend_amount.clone();
            let creating_spend_transaction = creating_spend_transaction.clone();
            let spend_form_error = spend_form_error.clone();
            let spend_form_success = spend_form_success.clone();
            let refresh_transactions = refresh_transactions.clone();
            
            spawn_local(async move {
                // Clear previous messages
                spend_form_error.set(None);
                spend_form_success.set(false);
                creating_spend_transaction.set(true);
                
                // Parse amount for the API (let backend handle validation)
                let amount_value = match (*spend_amount).trim().parse::<f64>() {
                    Ok(val) => val,
                    Err(_) => {
                        // If we can't parse, let backend handle the validation error
                        0.0
                    }
                };
                
                let request = SpendMoneyRequest {
                    description: (*spend_description).clone(),
                    amount: amount_value,
                    date: None, // Use current time
                };
                
                match api_client.spend_money(request).await {
                    Ok(_response) => {
                        // Clear form and show success
                        spend_description.set(String::new());
                        spend_amount.set(String::new());
                        spend_form_success.set(true);
                        refresh_transactions.emit(());
                        
                        // Clear success message after 3 seconds
                        let spend_form_success_clear = spend_form_success.clone();
                        spawn_local(async move {
                            gloo::timers::future::TimeoutFuture::new(3000).await;
                            spend_form_success_clear.set(false);
                        });
                    }
                    Err(error_message) => {
                        spend_form_error.set(Some(error_message));
                    }
                }
                
                creating_spend_transaction.set(false);
            });
        })
    };

    // Validation function using backend API


    // Form input handlers without premature validation
    let on_description_change = {
        let description = description.clone();
        let form_error = form_error.clone();
        Callback::from(move |e: Event| {
            let input: HtmlInputElement = e.target_unchecked_into();
            description.set(input.value());
            // Clear any existing error when user starts typing
            form_error.set(None);
        })
    };

    let on_amount_change = {
        let amount = amount.clone();
        let form_error = form_error.clone();
        Callback::from(move |e: Event| {
            let input: HtmlInputElement = e.target_unchecked_into();
            amount.set(input.value());
            // Clear any existing error when user starts typing
            form_error.set(None);
        })
    };

    // Spend form input handlers without premature validation
    let on_spend_description_change = {
        let spend_description = spend_description.clone();
        let spend_form_error = spend_form_error.clone();
        Callback::from(move |e: Event| {
            let input: HtmlInputElement = e.target_unchecked_into();
            spend_description.set(input.value());
            // Clear any existing error when user starts typing
            spend_form_error.set(None);
        })
    };

    let on_spend_amount_change = {
        let spend_amount = spend_amount.clone();
        let spend_form_error = spend_form_error.clone();
        Callback::from(move |e: Event| {
            let input: HtmlInputElement = e.target_unchecked_into();
            spend_amount.set(input.value());
            // Clear any existing error when user starts typing
            spend_form_error.set(None);
        })
    };

    // Load initial data
    use_effect_with((), {
        let api_client = api_client.clone();
        let refresh_transactions = refresh_transactions.clone();
        let backend_connected = backend_connected.clone();
        let backend_endpoint = backend_endpoint.clone();
        
        move |_| {
            let api_client = (*api_client).clone();
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

    // Reload calendar when month/year changes
    use_effect_with((current_month.clone(), current_year.clone()), {
        let api_client = api_client.clone();
        let calendar_data = calendar_data.clone();
        
        move |(month, year)| {
            let api_client = (*api_client).clone();
            let calendar_data = calendar_data.clone();
            let month = **month;
            let year = **year;
            
            spawn_local(async move {
                match api_client.get_calendar_month(month, year).await {
                    Ok(data) => {
                        calendar_data.set(Some(data));
                    }
                    Err(e) => {
                        gloo::console::error!("Failed to fetch calendar data:", e);
                    }
                }
            });
            
            || ()
        }
    });

    // Navigation callbacks
    let prev_month = {
        let current_month = current_month.clone();
        let current_year = current_year.clone();
        Callback::from(move |_: MouseEvent| {
            if *current_month == 1 {
                current_month.set(12);
                current_year.set(*current_year - 1);
            } else {
                current_month.set(*current_month - 1);
            }
        })
    };

    let next_month = {
        let current_month = current_month.clone();
        let current_year = current_year.clone();
        Callback::from(move |_: MouseEvent| {
            if *current_month == 12 {
                current_month.set(1);
                current_year.set(*current_year + 1);
            } else {
                current_month.set(*current_month + 1);
            }
        })
    };

    html! {
        <>
            <header class="header">
                <div class="container">
                    <h1>{"My Allowance Tracker"}</h1>
                    <div class="balance-display">
                        <span class="balance-label">{"Current Balance:"}</span>
                        <span class="balance-amount">{format!("${:.2}", *current_balance)}</span>
                    </div>
                </div>
            </header>

            <main class="main">
                <div class="container">
                    <section class="calendar-section">
                        <div class="calendar-header">
                            <button class="calendar-nav-btn" onclick={prev_month}>{"‹"}</button>
                            <h2 class="calendar-title">
                                {if let Some(cal_data) = calendar_data.as_ref() {
                                    format!("{} {}", month_name(cal_data.month), cal_data.year)
                                } else {
                                    format!("Loading...")
                                }}
                            </h2>
                            <button class="calendar-nav-btn" onclick={next_month}>{"›"}</button>
                        </div>
                        
                        {if let Some(cal_data) = calendar_data.as_ref() {
                            html! { <Calendar calendar_data={cal_data.clone()} /> }
                        } else {
                            html! { <div class="loading">{"Loading calendar..."}</div> }
                        }}
                    </section>

                    <TransactionTable 
                        transactions={(*formatted_transactions).clone()} 
                        loading={*loading}
                    />
                </div>

                <div class="money-management-container">
                    <section class="add-money-section">
                        <h2>{"✨ Add Extra Money"}</h2>
                        
                        {if let Some(error) = (*form_error).as_ref() {
                            html! {
                                <div class="form-message error">
                                    {error}
                                </div>
                            }
                        } else { html! {} }}
                        
                        {if !validation_suggestions.is_empty() {
                            html! {
                                <div class="form-message info">
                                    <strong>{"💡 Suggestions:"}</strong>
                                    <ul>
                                        {for validation_suggestions.iter().map(|suggestion| {
                                            html! { <li>{suggestion}</li> }
                                        })}
                                    </ul>
                                </div>
                            }
                        } else { html! {} }}
                        
                        {if *form_success {
                            html! {
                                <div class="form-message success">
                                    {"🎉 Money added successfully!"}
                                </div>
                            }
                        } else { html! {} }}
                        
                        <form class="add-money-form" onsubmit={
                            let add_money = add_money.clone();
                            Callback::from(move |e: SubmitEvent| {
                                e.prevent_default();
                                add_money.emit(());
                            })
                        }>
                            <div class="form-group">
                                <label for="description">{"What did you get money for?"}</label>
                                <input 
                                    type="text"
                                    id="description"
                                    placeholder="Birthday gift, chores, found money..."
                                    value={(*description).clone()}
                                    onchange={on_description_change}
                                    disabled={*creating_transaction}
                                />
                            </div>
                            
                            <div class="form-group">
                                <label for="amount">{"How much money? (dollars)"}</label>
                                <input 
                                    type="number" 
                                    id="amount"
                                    placeholder="5.00"
                                    step="0.01"
                                    min="0.01"
                                    value={(*amount).clone()}
                                    onchange={on_amount_change}
                                    disabled={*creating_transaction}
                                />
                            </div>
                            
                            <button 
                                type="submit" 
                                class="btn btn-primary add-money-btn"
                                disabled={*creating_transaction}
                            >
                                {if *creating_transaction {
                                    "Adding Money..."
                                } else {
                                    "✨ Add Extra Money"
                                }}
                            </button>
                        </form>
                    </section>

                    <section class="spend-money-section">
                        <h2>{"💸 Spend Money"}</h2>
                        
                        {if let Some(error) = (*spend_form_error).as_ref() {
                            html! {
                                <div class="form-message error">
                                    {error}
                                </div>
                            }
                        } else { html! {} }}
                        
                        {if !spend_validation_suggestions.is_empty() {
                            html! {
                                <div class="form-message info">
                                    <strong>{"💡 Suggestions:"}</strong>
                                    <ul>
                                        {for spend_validation_suggestions.iter().map(|suggestion| {
                                            html! { <li>{suggestion}</li> }
                                        })}
                                    </ul>
                                </div>
                            }
                        } else { html! {} }}
                        
                        {if *spend_form_success {
                            html! {
                                <div class="form-message success">
                                    {"💸 Money spent successfully!"}
                                </div>
                            }
                        } else { html! {} }}
                        
                        <form class="spend-money-form" onsubmit={
                            let spend_money = spend_money.clone();
                            Callback::from(move |e: SubmitEvent| {
                                e.prevent_default();
                                spend_money.emit(());
                            })
                        }>
                            <div class="form-group">
                                <label for="spend-description">{"What did you spend money on?"}</label>
                                <input 
                                    type="text"
                                    id="spend-description"
                                    placeholder="Toy, candy, book, game..."
                                    value={(*spend_description).clone()}
                                    onchange={on_spend_description_change}
                                    disabled={*creating_spend_transaction}
                                />
                            </div>
                            
                            <div class="form-group">
                                <label for="spend-amount">{"How much did you spend? (dollars)"}</label>
                                <input 
                                    type="number" 
                                    id="spend-amount"
                                    placeholder="2.50"
                                    step="0.01"
                                    min="0.01"
                                    value={(*spend_amount).clone()}
                                    onchange={on_spend_amount_change}
                                    disabled={*creating_spend_transaction}
                                />
                            </div>
                            
                            <button 
                                type="submit" 
                                class="btn btn-secondary spend-money-btn"
                                disabled={*creating_spend_transaction}
                            >
                                {if *creating_spend_transaction {
                                    "Recording Spending..."
                                } else {
                                    "💸 Record Spending"
                                }}
                            </button>
                        </form>
                    </section>
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
