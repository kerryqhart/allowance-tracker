use yew::prelude::*;
use gloo::net::http::Request;
use shared::{AddMoneyRequest, AddMoneyResponse, MoneyFormValidation, CalendarMonth, TransactionTableResponse, FormattedTransaction, AmountType};
use wasm_bindgen_futures::spawn_local;
use web_sys::HtmlInputElement;

// Helper function to get month name from number
fn month_name(month: u32) -> &'static str {
    match month {
        1 => "January", 2 => "February", 3 => "March", 4 => "April",
        5 => "May", 6 => "June", 7 => "July", 8 => "August",
        9 => "September", 10 => "October", 11 => "November", 12 => "December",
        _ => "January",
    }
}

// Simple helper function for calendar tooltips (calendar uses raw transactions)
fn format_calendar_date(rfc3339_date: &str) -> String {
    if let Some(date_part) = rfc3339_date.split('T').next() {
        if let Ok(parts) = date_part.split('-').collect::<Vec<_>>().try_into() {
            let [year, month, day]: [&str; 3] = parts;
            if let (Ok(y), Ok(m), Ok(d)) = (year.parse::<u32>(), month.parse::<u32>(), day.parse::<u32>()) {
                return format!("{} {}, {}", month_name(m), d, y);
            }
        }
    }
    rfc3339_date.to_string()
}





#[function_component(App)]
fn app() -> Html {
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
    
    // Connection status for parent info
    let backend_connected = use_state(|| false);
    let backend_endpoint = use_state(|| String::from("Checking..."));

    // Function to refresh transaction data
    let refresh_transactions = {
        let formatted_transactions = formatted_transactions.clone();
        let calendar_data = calendar_data.clone();
        let current_balance = current_balance.clone();
        let loading = loading.clone();
        let current_month = current_month.clone();
        let current_year = current_year.clone();
        
        Callback::from(move |_| {
            let formatted_transactions = formatted_transactions.clone();
            let calendar_data = calendar_data.clone();
            let current_balance = current_balance.clone();
            let loading = loading.clone();
            let current_month = current_month.clone();
            let current_year = current_year.clone();
            
            spawn_local(async move {
                loading.set(true);
                
                // Refresh formatted transaction table using backend API
                if let Ok(response) = Request::get("http://localhost:3000/api/transactions/table?limit=10").send().await {
                    if let Ok(data) = response.json::<TransactionTableResponse>().await {
                        // Set current balance from most recent transaction
                        if let Some(first_tx) = data.formatted_transactions.first() {
                            current_balance.set(first_tx.raw_balance);
                        }
                        formatted_transactions.set(data.formatted_transactions);
                    } else {
                        gloo::console::error!("Failed to parse formatted transactions:", response.text().await.unwrap_or_default());
                    }
                } else {
                    gloo::console::error!("Failed to fetch formatted transactions");
                }
                
                // Refresh calendar data using backend API
                let calendar_url = format!("http://localhost:3000/api/calendar/month?month={}&year={}", *current_month, *current_year);
                if let Ok(response) = Request::get(&calendar_url).send().await {
                    if let Ok(data) = response.json::<CalendarMonth>().await {
                        calendar_data.set(Some(data));
                    } else {
                        gloo::console::error!("Failed to parse calendar data:", response.text().await.unwrap_or_default());
                    }
                } else {
                    gloo::console::error!("Failed to fetch calendar data");
                }
                
                loading.set(false);
            });
        })
    };

    // Add money callback - simplified to use backend validation and processing
    let add_money = {
        let description = description.clone();
        let amount = amount.clone();
        let creating_transaction = creating_transaction.clone();
        let form_error = form_error.clone();
        let form_success = form_success.clone();
        let refresh_transactions = refresh_transactions.clone();
        
        Callback::from(move |_| {
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
                
                match Request::post("http://localhost:3000/api/money/add")
                    .json(&request)
                    .unwrap()
                    .send()
                    .await
                {
                    Ok(response) => {
                        if response.ok() {
                            // Success! Parse response and use backend success message
                            match response.json::<AddMoneyResponse>().await {
                                Ok(add_response) => {
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
                                Err(e) => {
                                    form_error.set(Some(format!("Failed to parse response: {}", e)));
                                }
                            }
                        } else {
                            // Use backend error message
                            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
                            form_error.set(Some(error_text));
                        }
                    }
                    Err(e) => {
                        form_error.set(Some(format!("Network error: {}", e)));
                    }
                }
                
                creating_transaction.set(false);
            });
        })
    };

    // Validation function using backend API
    let validate_form = {
        let description = description.clone();
        let amount = amount.clone();
        let form_error = form_error.clone();
        let validation_suggestions = validation_suggestions.clone();
        
        Callback::from(move |_| {
            let description = description.clone();
            let amount = amount.clone();
            let form_error = form_error.clone();
            let validation_suggestions = validation_suggestions.clone();
            
            spawn_local(async move {
                #[derive(serde::Serialize)]
                struct ValidateRequest {
                    description: String,
                    amount_input: String,
                }
                
                let request = ValidateRequest {
                    description: (*description).clone(),
                    amount_input: (*amount).clone(),
                };
                
                match Request::post("http://localhost:3000/api/money/validate")
                    .json(&request)
                    .unwrap()
                    .send()
                    .await
                {
                    Ok(response) => {
                        if let Ok(validation) = response.json::<MoneyFormValidation>().await {
                            if validation.is_valid {
                                form_error.set(None);
                                validation_suggestions.set(Vec::new());
                            } else {
                                // Use first error message from backend
                                if let Some(first_error) = validation.errors.first() {
                                    // Map backend error to frontend message (temporary, backend should provide messages)
                                    let error_message = match first_error {
                                        _ => format!("Validation error: {:?}", first_error),
                                    };
                                    form_error.set(Some(error_message));
                                }
                                validation_suggestions.set(validation.suggestions);
                            }
                        }
                    }
                    Err(e) => {
                        gloo::console::warn!("Validation request failed:", e.to_string());
                    }
                }
            });
        })
    };

    // Form input handlers with validation
    let on_description_change = {
        let description = description.clone();
        let validate_form = validate_form.clone();
        Callback::from(move |e: Event| {
            let input: HtmlInputElement = e.target_unchecked_into();
            description.set(input.value());
            // Trigger validation after short delay
            let validate_form = validate_form.clone();
            spawn_local(async move {
                gloo::timers::future::TimeoutFuture::new(500).await;
                validate_form.emit(());
            });
        })
    };

    let on_amount_change = {
        let amount = amount.clone();
        let validate_form = validate_form.clone();
        Callback::from(move |e: Event| {
            let input: HtmlInputElement = e.target_unchecked_into();
            amount.set(input.value());
            // Trigger validation after short delay
            let validate_form = validate_form.clone();
            spawn_local(async move {
                gloo::timers::future::TimeoutFuture::new(500).await;
                validate_form.emit(());
            });
        })
    };

    // Load initial data
    use_effect_with((), {
        let refresh_transactions = refresh_transactions.clone();
        let backend_connected = backend_connected.clone();
        let backend_endpoint = backend_endpoint.clone();
        
        move |_| {
            spawn_local(async move {
                // Test backend connection
                match Request::get("http://localhost:3000/api/transactions/table?limit=1").send().await {
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
                        gloo::console::error!("Failed to connect to backend:", e.to_string());
                    }
                }
            });
            
            || ()
        }
    });

    // Reload calendar when month/year changes
    use_effect_with((current_month.clone(), current_year.clone()), {
        let calendar_data = calendar_data.clone();
        
        move |(month, year)| {
            let calendar_data = calendar_data.clone();
            let month = **month;
            let year = **year;
            
            spawn_local(async move {
                let calendar_url = format!("http://localhost:3000/api/calendar/month?month={}&year={}", month, year);
                if let Ok(response) = Request::get(&calendar_url).send().await {
                    if let Ok(data) = response.json::<CalendarMonth>().await {
                        calendar_data.set(Some(data));
                    } else {
                        gloo::console::error!("Failed to parse calendar data:", response.text().await.unwrap_or_default());
                    }
                } else {
                    gloo::console::error!("Failed to fetch calendar data");
                }
            });
            
            || ()
        }
    });

    // Navigation callbacks
    let prev_month = {
        let current_month = current_month.clone();
        let current_year = current_year.clone();
        Callback::from(move |_| {
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
        Callback::from(move |_| {
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

                    <section class="transactions-section">
                        <h2>{"Recent Transactions"}</h2>
                        
                        {if *loading {
                            html! { <div class="loading">{"Loading transactions..."}</div> }
                        } else {
                            html! {
                                <div class="table-container">
                                    <table class="transactions-table">
                                        <thead>
                                            <tr>
                                                <th>{"Date"}</th>
                                                <th>{"Description"}</th>
                                                <th>{"Amount"}</th>
                                                <th>{"Balance"}</th>
                                            </tr>
                                        </thead>
                                        <tbody>
                                            {for formatted_transactions.iter().map(|transaction| {
                                                // Use backend-provided CSS class based on amount type
                                                let amount_class = match transaction.amount_type {
                                                    AmountType::Positive => "amount positive",
                                                    AmountType::Negative => "amount negative",
                                                    AmountType::Zero => "amount zero",
                                                };
                                                
                                                html! {
                                                    <tr>
                                                        <td class="date">{&transaction.formatted_date}</td>
                                                        <td class="description">{&transaction.description}</td>
                                                        <td class={amount_class}>
                                                            {&transaction.formatted_amount}
                                                        </td>
                                                        <td class="balance">{&transaction.formatted_balance}</td>
                                                    </tr>
                                                }
                                            })}
                                        </tbody>
                                    </table>
                                </div>
                            }
                        }}
                    </section>

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



#[derive(Properties, PartialEq)]
struct CalendarProps {
    calendar_data: CalendarMonth,
}

#[function_component(Calendar)]
fn calendar(props: &CalendarProps) -> Html {
    let calendar_data = &props.calendar_data;
    
    // Create calendar grid using backend-provided data
    let mut calendar_days = Vec::new();
    
    // Add empty cells for days before the first day of month
    for _ in 0..calendar_data.first_day_of_week {
        calendar_days.push(html! {
            <div class="calendar-day empty"></div>
        });
    }
    
    // Add days of the month using backend-provided day data
    for day_data in &calendar_data.days {
        calendar_days.push(html! {
            <div class="calendar-day">
                <div class="day-header">
                    <div class="day-number">{day_data.day}</div>
                    <div class="day-balance-subtle">
                        {format!("${:.0}", day_data.balance)}
                    </div>
                </div>
                
                <div class="day-transactions">
                    {for day_data.transactions.iter().map(|transaction| {
                        let chip_class = if transaction.amount >= 0.0 {
                            "transaction-chip positive"
                        } else {
                            "transaction-chip negative"
                        };
                        
                        // Create detailed tooltip content
                        let tooltip_content = format!(
                            "💰 {}\n💵 Amount: ${:.2}\n📅 Date: {}\n💳 Balance: ${:.2}",
                            transaction.description,
                            transaction.amount,
                            format_calendar_date(&transaction.date),
                            transaction.balance
                        );
                        
                        html! {
                            <div class={format!("{} transaction-tooltip", chip_class)} 
                                 title={tooltip_content}
                                 data-description={transaction.description.clone()}
                                 data-amount={format!("{:.2}", transaction.amount)}
                                 data-date={format_calendar_date(&transaction.date)}
                                 data-balance={format!("{:.2}", transaction.balance)}>
                                {if transaction.amount >= 0.0 {
                                    format!("+${:.0}", transaction.amount)
                                } else {
                                    format!("-${:.0}", transaction.amount.abs())
                                }}
                                
                                // Custom tooltip div that will be shown on hover
                                <div class="custom-tooltip">
                                    <div class="tooltip-header">
                                        <strong>{&transaction.description}</strong>
                                    </div>
                                    <div class="tooltip-body">
                                        <div class="tooltip-row">
                                            <span class="tooltip-label">{"💵 Amount:"}</span>
                                            <span class={if transaction.amount >= 0.0 { "tooltip-value positive" } else { "tooltip-value negative" }}>
                                                {format!("${:.2}", transaction.amount)}
                                            </span>
                                        </div>
                                        <div class="tooltip-row">
                                            <span class="tooltip-label">{"📅 Date:"}</span>
                                            <span class="tooltip-value">{format_calendar_date(&transaction.date)}</span>
                                        </div>
                                        <div class="tooltip-row">
                                            <span class="tooltip-label">{"💳 Balance:"}</span>
                                            <span class="tooltip-value">{format!("${:.2}", transaction.balance)}</span>
                                        </div>
                                    </div>
                                </div>
                            </div>
                        }
                    })}
                </div>
            </div>
        });
    }
    
    html! {
        <div class="calendar">
            <div class="calendar-weekdays">
                <div class="weekday">{"Sun"}</div>
                <div class="weekday">{"Mon"}</div>
                <div class="weekday">{"Tue"}</div>
                <div class="weekday">{"Wed"}</div>
                <div class="weekday">{"Thu"}</div>
                <div class="weekday">{"Fri"}</div>
                <div class="weekday">{"Sat"}</div>
            </div>
            <div class="calendar-grid">
                {for calendar_days}
            </div>
        </div>
    }
}

fn main() {
    yew::Renderer::<App>::new().render();
}
