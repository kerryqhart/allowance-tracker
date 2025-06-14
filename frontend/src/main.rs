use yew::prelude::*;
use std::collections::HashMap;
use gloo::net::http::Request;
use shared::{Transaction, TransactionListResponse};
use wasm_bindgen_futures::spawn_local;

// Helper function to parse month name to number
fn month_name_to_number(month: &str) -> u32 {
    match month {
        "January" => 1, "February" => 2, "March" => 3, "April" => 4,
        "May" => 5, "June" => 6, "July" => 7, "August" => 8,
        "September" => 9, "October" => 10, "November" => 11, "December" => 12,
        _ => 1,
    }
}

// Helper function to get month name from number
fn number_to_month_name(month: u32) -> &'static str {
    match month {
        1 => "January", 2 => "February", 3 => "March", 4 => "April",
        5 => "May", 6 => "June", 7 => "July", 8 => "August",
        9 => "September", 10 => "October", 11 => "November", 12 => "December",
        _ => "January",
    }
}

// Helper function to get days in month
fn days_in_month(month: u32, year: u32) -> u32 {
    match month {
        2 => if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) { 29 } else { 28 },
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

// Helper function to get first day of month (0 = Sunday, 1 = Monday, etc.)
fn first_day_of_month(month: u32, year: u32) -> u32 {
    // Simple calculation for demo - in real app would use proper date library
    let days_since_epoch = (year - 1970) * 365 + (year - 1969) / 4 - (year - 1901) / 100 + (year - 1601) / 400;
    let days_in_months = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    let mut total_days = days_since_epoch + days_in_months[(month - 1) as usize];
    
    // Add leap day if current year is leap and month > February
    if month > 2 && year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) {
        total_days += 1;
    }
    
    (total_days + 4) % 7 // January 1, 1970 was a Thursday (4)
}

#[function_component(App)]
fn app() -> Html {
    let current_month = use_state(|| 6u32); // June
    let current_year = use_state(|| 2025u32);
    let transactions = use_state(|| Vec::<Transaction>::new());
    let all_transactions = use_state(|| Vec::<Transaction>::new()); // For calendar view
    let loading = use_state(|| true);
    let current_balance = use_state(|| 0.0f64);

    // Load recent transactions for table view
    use_effect_with((), {
        let transactions = transactions.clone();
        let loading = loading.clone();
        let current_balance = current_balance.clone();
        
        move |_| {
            spawn_local(async move {
                loading.set(true);
                
                match Request::get("/api/transactions?limit=10").send().await {
                    Ok(response) => {
                        match response.json::<TransactionListResponse>().await {
                            Ok(data) => {
                                // Set current balance from most recent transaction
                                if let Some(first_tx) = data.transactions.first() {
                                    current_balance.set(first_tx.balance);
                                }
                                transactions.set(data.transactions);
                            },
                            Err(e) => {
                                gloo::console::error!("Failed to parse transactions:", e.to_string());
                            }
                        }
                    },
                    Err(e) => {
                        gloo::console::error!("Failed to fetch transactions:", e.to_string());
                    }
                }
                
                loading.set(false);
            });
            
            || ()
        }
    });

    // Load all transactions for calendar view
    use_effect_with((), {
        let all_transactions = all_transactions.clone();
        
        move |_| {
            spawn_local(async move {
                match Request::get("/api/transactions?limit=100").send().await {
                    Ok(response) => {
                        match response.json::<TransactionListResponse>().await {
                            Ok(data) => {
                                all_transactions.set(data.transactions);
                            },
                            Err(e) => {
                                gloo::console::error!("Failed to parse all transactions:", e.to_string());
                            }
                        }
                    },
                    Err(e) => {
                        gloo::console::error!("Failed to fetch all transactions:", e.to_string());
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
                                            {for transactions.iter().take(10).map(|transaction| {
                                                let amount_class = if transaction.amount >= 0.0 {
                                                    "amount positive"
                                                } else {
                                                    "amount negative"
                                                };
                                                
                                                // Format date nicely
                                                let formatted_date = format_date(&transaction.date);
                                                
                                                html! {
                                                    <tr>
                                                        <td class="date">{formatted_date}</td>
                                                        <td class="description">{&transaction.description}</td>
                                                        <td class={amount_class}>
                                                            {if transaction.amount >= 0.0 {
                                                                format!("+${:.2}", transaction.amount)
                                                            } else {
                                                                format!("-${:.2}", transaction.amount.abs())
                                                            }}
                                                        </td>
                                                        <td class="balance">{format!("${:.2}", transaction.balance)}</td>
                                                    </tr>
                                                }
                                            })}
                                        </tbody>
                                    </table>
                                </div>
                            }
                        }}
                    </section>

                    <section class="calendar-section">
                        <div class="calendar-header">
                            <button class="calendar-nav-btn" onclick={prev_month}>{"‹"}</button>
                            <h2 class="calendar-title">
                                {format!("{} {}", number_to_month_name(*current_month), *current_year)}
                            </h2>
                            <button class="calendar-nav-btn" onclick={next_month}>{"›"}</button>
                        </div>
                        
                        <Calendar 
                            month={*current_month}
                            year={*current_year}
                            transactions={(*all_transactions).clone()}
                        />
                    </section>

                    <section class="actions-section">
                        <h2>{"Add New Transaction"}</h2>
                        <div class="action-buttons">
                            <button class="btn btn-primary">{"Record Allowance"}</button>
                            <button class="btn btn-secondary">{"Record Spending"}</button>
                            <button class="btn btn-accent">{"Record Gift/Income"}</button>
                        </div>
                    </section>
                </div>
            </main>
        </>
    }
}

// Helper function to format RFC 3339 date to human readable format
fn format_date(rfc3339_date: &str) -> String {
    // Parse the RFC 3339 date and format it nicely
    // For now, simple extraction - in a real app would use a proper date library
    if let Some(date_part) = rfc3339_date.split('T').next() {
        if let Ok(parts) = date_part.split('-').collect::<Vec<_>>().try_into() {
            let [year, month, day]: [&str; 3] = parts;
            if let (Ok(y), Ok(m), Ok(d)) = (year.parse::<u32>(), month.parse::<u32>(), day.parse::<u32>()) {
                return format!("{} {}, {}", number_to_month_name(m), d, y);
            }
        }
    }
    // Fallback to original string
    rfc3339_date.to_string()
}

#[derive(Properties, PartialEq)]
struct CalendarProps {
    month: u32,
    year: u32,
    transactions: Vec<Transaction>,
}

#[function_component(Calendar)]
fn calendar(props: &CalendarProps) -> Html {
    let month = props.month;
    let year = props.year;
    
    // Group transactions by day for the current month
    let mut transactions_by_day: HashMap<u32, Vec<&Transaction>> = HashMap::new();
    
    for transaction in &props.transactions {
        // Parse RFC 3339 date (e.g., "2025-06-13T09:00:00-04:00")
        if let Some(date_part) = transaction.date.split('T').next() {
            let parts: Vec<&str> = date_part.split('-').collect();
            if parts.len() == 3 {
                if let (Ok(year_part), Ok(month_part), Ok(day_part)) = (
                    parts[0].parse::<u32>(),
                    parts[1].parse::<u32>(),
                    parts[2].parse::<u32>()
                ) {
                    if month_part == month && year_part == year {
                        transactions_by_day.entry(day_part).or_insert_with(Vec::new).push(transaction);
                    }
                }
            }
        }
    }
    
    // Calculate running balance for every day in the month
    let days_in_current_month = days_in_month(month, year);
    let mut daily_balances: HashMap<u32, f64> = HashMap::new();
    
    // Sort all transactions by date to get proper chronological order
    let mut sorted_transactions = props.transactions.clone();
    sorted_transactions.sort_by(|a, b| {
        // Parse RFC 3339 dates and compare (reverse chronological, so newer first)
        let parse_date = |date_str: &str| -> (u32, u32, u32) {
            if let Some(date_part) = date_str.split('T').next() {
                let parts: Vec<&str> = date_part.split('-').collect();
                if parts.len() == 3 {
                    if let (Ok(year), Ok(month), Ok(day)) = (
                        parts[0].parse::<u32>(),
                        parts[1].parse::<u32>(),
                        parts[2].parse::<u32>()
                    ) {
                        return (year, month, day);
                    }
                }
            }
            (0, 0, 0)
        };
        
        let (year_a, month_a, day_a) = parse_date(&a.date);
        let (year_b, month_b, day_b) = parse_date(&b.date);
        
        // Compare in reverse chronological order (newest first)
        (year_b, month_b, day_b).cmp(&(year_a, month_a, day_a))
    });
    
    // Find the balance at the end of the previous month (or start of this month)
    let mut current_balance = 0.0;
    let mut found_starting_balance = false;
    
    for transaction in &sorted_transactions {
        let parts: Vec<&str> = transaction.date.split(", ").collect();
        if parts.len() == 2 {
            let year_part = parts[1].parse::<u32>().unwrap_or(0);
            let month_day_parts: Vec<&str> = parts[0].split(' ').collect();
            if month_day_parts.len() == 2 {
                let month_part = month_name_to_number(month_day_parts[0]);
                let day_part = month_day_parts[1].parse::<u32>().unwrap_or(0);
                
                if year_part == year && month_part == month {
                    // This is a transaction in our target month
                    if !found_starting_balance {
                        // First transaction of the month - work backwards to get starting balance
                        current_balance = transaction.balance - transaction.amount;
                        found_starting_balance = true;
                    }
                    break;
                }
            }
        }
    }
    
    // Now calculate balance for each day
    for day in 1..=days_in_current_month {
        // Check if there are transactions on this day
        if let Some(day_transactions) = transactions_by_day.get(&day) {
            // Add up all transactions for this day
            let daily_change: f64 = day_transactions.iter().map(|t| t.amount).sum();
            current_balance += daily_change;
        }
        daily_balances.insert(day, current_balance);
    }
    
    let first_day = first_day_of_month(month, year);
    
    // Create calendar grid
    let mut calendar_days = Vec::new();
    
    // Add empty cells for days before the first day of month
    for _ in 0..first_day {
        calendar_days.push(html! {
            <div class="calendar-day empty"></div>
        });
    }
    
    // Add days of the month
    for day in 1..=days_in_current_month {
        let day_transactions = transactions_by_day.get(&day).cloned().unwrap_or_default();
        let day_balance = daily_balances.get(&day).copied().unwrap_or(0.0);
        
        calendar_days.push(html! {
            <div class="calendar-day">
                <div class="day-header">
                    <div class="day-number">{day}</div>
                    <div class="day-balance-subtle">
                        {format!("${:.0}", day_balance)}
                    </div>
                </div>
                
                <div class="day-transactions">
                    {for day_transactions.iter().map(|transaction| {
                        let chip_class = if transaction.amount >= 0.0 {
                            "transaction-chip positive"
                        } else {
                            "transaction-chip negative"
                        };
                        
                        html! {
                            <div class={chip_class} title={transaction.description.clone()}>
                                {if transaction.amount >= 0.0 {
                                    format!("+${:.0}", transaction.amount)
                                } else {
                                    format!("-${:.0}", transaction.amount.abs())
                                }}
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
