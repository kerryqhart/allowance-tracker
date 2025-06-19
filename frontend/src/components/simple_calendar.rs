use yew::prelude::*;
use shared::{CalendarFocusDate, CurrentDateResponse, AllowanceConfig, GetAllowanceConfigRequest, CalendarMonth};
use crate::services::api::ApiClient;
use wasm_bindgen_futures::spawn_local;

#[derive(Properties, PartialEq)]
pub struct SimpleCalendarProps {
    #[prop_or(0)]
    pub refresh_trigger: u32,
    #[prop_or(false)]
    pub delete_mode: bool,
    #[prop_or_default]
    pub selected_transactions: Vec<String>,
    #[prop_or_default]
    pub on_toggle_transaction_selection: Callback<String>,
    #[prop_or_default]
    pub on_delete_selected: Callback<()>,
}

#[function_component(SimpleCalendar)]
pub fn simple_calendar(props: &SimpleCalendarProps) -> Html {
    // State for current month/year from backend
    let calendar_state = use_state(|| Option::<CalendarFocusDate>::None);
    let current_date = use_state(|| Option::<CurrentDateResponse>::None);
    let allowance_config = use_state(|| Option::<AllowanceConfig>::None);
    let calendar_month_data = use_state(|| Option::<CalendarMonth>::None);
    let is_loading = use_state(|| true);
    let error_message = use_state(|| Option::<String>::None);
    
    // Debug counter to verify callbacks are working
    let click_count = use_state(|| 0u32);

    // API client
    let api_client = use_memo((), |_| ApiClient::new());

    // Helper function to create tooltip text for transaction chips
    let create_transaction_tooltip = |transaction: &shared::Transaction| -> String {
        if transaction.amount >= 0.0 {
            format!("Got money from {}", transaction.description)
        } else {
            format!("Spent on {}", transaction.description)
        }
    };

    // Helper function to refresh calendar month data
    let refresh_calendar_month_data = {
        let calendar_state = calendar_state.clone();
        let calendar_month_data = calendar_month_data.clone();
        let api_client = api_client.clone();
        
        use_callback((calendar_state.clone(),), move |_, (current_calendar_state,)| {
            let calendar_month_data = calendar_month_data.clone();
            let api_client = api_client.clone();
            let current_state = current_calendar_state.clone();
            
            spawn_local(async move {
                if let Some(focus_date) = (*current_state).as_ref() {
                    gloo::console::log!(&format!("Refreshing calendar for {}/{}", focus_date.month, focus_date.year));
                    match (*api_client).get_calendar_month(focus_date.month, focus_date.year).await {
                        Ok(calendar_month) => {
                            gloo::console::log!(&format!("Refreshed calendar month data with {} days and {} total transactions", 
                                calendar_month.days.len(),
                                calendar_month.days.iter().map(|d| d.transactions.len()).sum::<usize>()));
                            calendar_month_data.set(Some(calendar_month));
                        }
                        Err(e) => {
                            gloo::console::warn!(&format!("Failed to refresh calendar month data: {}", e));
                        }
                    }
                } else {
                    gloo::console::warn!("No focus date available for calendar refresh");
                }
            });
        })
    };

    // Load initial focus date, current date, allowance config, and calendar month data from backend
    {
        let calendar_state = calendar_state.clone();
        let current_date = current_date.clone();
        let allowance_config = allowance_config.clone();
        let calendar_month_data = calendar_month_data.clone();
        let is_loading = is_loading.clone();
        let error_message = error_message.clone();
        let api_client = api_client.clone();
        
        use_effect_with((), move |_| {
            spawn_local(async move {
                // Fetch focus date, current date, and allowance config first
                let focus_date_result = (*api_client).get_focus_date().await;
                let current_date_result = (*api_client).get_current_date().await;
                let allowance_result = (*api_client).get_allowance_config(GetAllowanceConfigRequest { child_id: None }).await;
                
                // If we got focus date, also fetch calendar month data
                let calendar_month_result = if let Ok(ref focus_date) = focus_date_result {
                    (*api_client).get_calendar_month(focus_date.month, focus_date.year).await
                } else {
                    Err("No focus date available".to_string())
                };
                
                // Handle all four API results
                match (focus_date_result, current_date_result, allowance_result, calendar_month_result) {
                    (Ok(focus_date), Ok(current_date_response), Ok(allowance_response), Ok(calendar_month)) => {
                        gloo::console::log!(&format!("Loaded focus date: {}/{}, current date: {}/{}/{}, allowance: {:?}, calendar month with {} days", 
                            focus_date.month, focus_date.year,
                            current_date_response.month, current_date_response.day, current_date_response.year,
                            allowance_response.allowance_config,
                            calendar_month.days.len()));
                        calendar_state.set(Some(focus_date));
                        current_date.set(Some(current_date_response));
                        allowance_config.set(allowance_response.allowance_config);
                        calendar_month_data.set(Some(calendar_month));
                        error_message.set(None);
                    }
                    (Ok(focus_date), Ok(current_date_response), Ok(allowance_response), Err(calendar_error)) => {
                        gloo::console::warn!(&format!("Failed to load calendar month data: {}", calendar_error));
                        calendar_state.set(Some(focus_date));
                        current_date.set(Some(current_date_response));
                        allowance_config.set(allowance_response.allowance_config);
                        // Continue without calendar month data - just won't show transaction chips
                        error_message.set(None);
                    }
                    // Handle other combinations with simplified logic - just log and continue
                    _ => {
                        gloo::console::warn!("Some API calls failed, continuing with partial data");
                        error_message.set(Some("Failed to load calendar state".to_string()));
                    }
                }
                is_loading.set(false);
            });
            || ()
        });
    }

    // Effect to refresh calendar month data when refresh_trigger changes
    {
        let refresh_calendar_month_data = refresh_calendar_month_data.clone();
        let refresh_trigger = props.refresh_trigger;
        
        use_effect_with(refresh_trigger, move |trigger| {
            if *trigger > 0 {
                gloo::console::log!(&format!("Calendar refresh triggered: {}", trigger));
                refresh_calendar_month_data.emit(());
            }
            || ()
        });
    }

    // Month names helper
    let month_name = |month: u32| -> &'static str {
        match month {
            1 => "January", 2 => "February", 3 => "March", 4 => "April",
            5 => "May", 6 => "June", 7 => "July", 8 => "August",
            9 => "September", 10 => "October", 11 => "November", 12 => "December",
            _ => "Invalid",
        }
    };

    // Previous month callback
    let on_previous = {
        let calendar_state = calendar_state.clone();
        let calendar_month_data = calendar_month_data.clone();
        let click_count = click_count.clone();
        let error_message = error_message.clone();
        let api_client = api_client.clone();
        
        Callback::from(move |_: MouseEvent| {
            let calendar_state = calendar_state.clone();
            let calendar_month_data = calendar_month_data.clone();
            let click_count = click_count.clone();
            let error_message = error_message.clone();
            let api_client = api_client.clone();
            
            let new_count = *click_count + 1;
            click_count.set(new_count);
            
            gloo::console::log!(&format!("Previous clicked! Count: {}", new_count));
            
            spawn_local(async move {
                match (*api_client).navigate_previous_month().await {
                    Ok(response) => {
                        gloo::console::log!(&format!("Backend response: {}", response.success_message));
                        calendar_state.set(Some(response.focus_date.clone()));
                        
                        // Also fetch calendar month data for the new month
                        match (*api_client).get_calendar_month(response.focus_date.month, response.focus_date.year).await {
                            Ok(calendar_month) => {
                                calendar_month_data.set(Some(calendar_month));
                            }
                            Err(e) => {
                                gloo::console::warn!(&format!("Failed to load calendar month data: {}", e));
                                calendar_month_data.set(None);
                            }
                        }
                        
                        error_message.set(None);
                    }
                    Err(e) => {
                        gloo::console::error!(&format!("Failed to navigate to previous month: {}", e));
                        error_message.set(Some(format!("Navigation failed: {}", e)));
                    }
                }
            });
        })
    };

    // Next month callback
    let on_next = {
        let calendar_state = calendar_state.clone();
        let calendar_month_data = calendar_month_data.clone();
        let click_count = click_count.clone();
        let error_message = error_message.clone();
        let api_client = api_client.clone();
        
        Callback::from(move |_: MouseEvent| {
            let calendar_state = calendar_state.clone();
            let calendar_month_data = calendar_month_data.clone();
            let click_count = click_count.clone();
            let error_message = error_message.clone();
            let api_client = api_client.clone();
            
            let new_count = *click_count + 1;
            click_count.set(new_count);
            
            gloo::console::log!(&format!("Next clicked! Count: {}", new_count));
            
            spawn_local(async move {
                match (*api_client).navigate_next_month().await {
                    Ok(response) => {
                        gloo::console::log!(&format!("Backend response: {}", response.success_message));
                        calendar_state.set(Some(response.focus_date.clone()));
                        
                        // Also fetch calendar month data for the new month
                        match (*api_client).get_calendar_month(response.focus_date.month, response.focus_date.year).await {
                            Ok(calendar_month) => {
                                calendar_month_data.set(Some(calendar_month));
                            }
                            Err(e) => {
                                gloo::console::warn!(&format!("Failed to load calendar month data: {}", e));
                                calendar_month_data.set(None);
                            }
                        }
                        
                        error_message.set(None);
                    }
                    Err(e) => {
                        gloo::console::error!(&format!("Failed to navigate to next month: {}", e));
                        error_message.set(Some(format!("Navigation failed: {}", e)));
                    }
                }
            });
        })
    };

    // Helper function to calculate days in month
    let days_in_month = |month: u32, year: u32| -> u32 {
        match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 => {
                if (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0) {
                    29
                } else {
                    28
                }
            }
            _ => 30,
        }
    };

    // Helper function to get first day of week (0 = Sunday, 1 = Monday, etc.)
    let first_day_of_week = |month: u32, year: u32| -> u32 {
        // Simple calculation for first day of week
        // This is a basic implementation - you might want to use a more robust date library
        let a = (14 - month) / 12;
        let y = year - a;
        let m = month + 12 * a - 2;
        let day = (1 + y + y / 4 - y / 100 + y / 400 + (31 * m) / 12) % 7;
        day
    };

    // Helper function to determine if a day should show allowance chip
    let is_future_allowance_day = |day: u32, month: u32, year: u32, 
                                   current_date_ref: &Option<CurrentDateResponse>, 
                                   allowance_config_ref: &Option<AllowanceConfig>| -> bool {
        // Check if we have allowance config and it's active
        if let Some(config) = allowance_config_ref {
            if !config.is_active {
                return false;
            }
            
            // Calculate day of week for this day (0 = Sunday, 1 = Monday, ..., 6 = Saturday)
            let a = (14 - month) / 12;
            let y = year - a;
            let m = month + 12 * a - 2;
            let day_of_week = (day + y + y / 4 - y / 100 + y / 400 + (31 * m) / 12) % 7;
            
            // Check if this day matches the configured allowance day of week
            if day_of_week as u8 != config.day_of_week {
                return false;
            }
            
            // Only show allowance indicator for future dates if we have current date info
            if let Some(current_date) = current_date_ref {
                // Compare the calendar day with current date
                if year > current_date.year {
                    return true; // Future year
                } else if year == current_date.year {
                    if month > current_date.month {
                        return true; // Future month in current year
                    } else if month == current_date.month {
                        return day > current_date.day; // Future day in current month
                    }
                }
                
                false // Past date
            } else {
                // Fallback: if we don't have current date info, show allowance indicators
                true
            }
        } else {
            false
        }
    };

    // Generate calendar days using backend calendar month data
    let generate_calendar_days = |calendar_month_ref: &Option<CalendarMonth>, current_date_ref: &Option<CurrentDateResponse>, allowance_config_ref: &Option<AllowanceConfig>| -> Vec<Html> {
        let mut days = Vec::new();
        
        if let Some(calendar_month) = calendar_month_ref {
            // Add empty cells for days before the first day of month
            for _ in 0..calendar_month.first_day_of_week {
                days.push(html! {
                    <div class="calendar-day empty"></div>
                });
            }
            
            // Add days of the month using backend-provided day data
            for day_data in &calendar_month.days {
                // Check if this is the current day using backend date info
                let is_today = if let Some(current_date_response) = current_date_ref {
                    calendar_month.year == current_date_response.year 
                        && calendar_month.month == current_date_response.month 
                        && day_data.day == current_date_response.day
                } else {
                    false // If we don't have current date info, no day is marked as today
                };
                
                let day_class = if is_today {
                    "calendar-day today"
                } else {
                    "calendar-day"
                };
                
                // Check if this day should show an allowance chip
                let show_allowance_chip = is_future_allowance_day(day_data.day, calendar_month.month, calendar_month.year, current_date_ref, allowance_config_ref);
                let allowance_amount = allowance_config_ref.as_ref().map(|config| config.amount).unwrap_or(0.0);
                
                // Process transactions for this day (limit to 4, then show "+X more")
                let transactions = &day_data.transactions;
                let display_transactions = if transactions.len() <= 4 {
                    transactions.clone()
                } else {
                    transactions[0..4].to_vec()
                };
                let remaining_count = if transactions.len() > 4 {
                    transactions.len() - 4
                } else {
                    0
                };
                
                days.push(html! {
                    <div class={day_class}>
                        <div class="day-header">
                            <div class="day-number-container">
                                <div class="day-number">{day_data.day}</div>
                            </div>
                            <div class="day-balance-subtle">
                                {format!("${:.0}", day_data.balance)}
                            </div>
                        </div>
                        <div class="day-transactions">
                            // Show allowance chip if this is a future allowance day
                            {if show_allowance_chip {
                                html! {
                                    <div class="transaction-tooltip">
                                        <div class="simple-calendar-allowance-chip">
                                            <span class="transaction-amount">
                                                {format!("+${:.0}", allowance_amount)}
                                            </span>
                                        </div>
                                        <div class="custom-tooltip allowance-border">
                                            <div class="tooltip-header">
                                                {"Upcoming allowance"}
                                            </div>
                                            <div class="tooltip-body">
                                                <div class="tooltip-row">
                                                    <span class="tooltip-label">{"Amount:"}</span>
                                                    <span class="tooltip-value positive">
                                                        {format!("+${:.2}", allowance_amount)}
                                                    </span>
                                                </div>
                                            </div>
                                        </div>
                                    </div>
                                }
                            } else {
                                html! {}
                            }}
                            
                            // Show transaction chips
                            {for display_transactions.iter().map(|transaction| {
                                let chip_class = if transaction.amount >= 0.0 {
                                    "simple-calendar-transaction-chip positive"
                                } else {
                                    "simple-calendar-transaction-chip negative"
                                };
                                
                                let is_selected = props.selected_transactions.contains(&transaction.id);
                                let transaction_id = transaction.id.clone();
                                let on_toggle_selection = props.on_toggle_transaction_selection.clone();
                                let delete_mode = props.delete_mode;
                                
                                let chip_click = Callback::from(move |_: MouseEvent| {
                                    if delete_mode {
                                        on_toggle_selection.emit(transaction_id.clone());
                                    }
                                });
                                
                                let final_chip_class = if is_selected {
                                    format!("{} selected", chip_class)
                                } else {
                                    chip_class.to_string()
                                };
                                
                                html! {
                                    <div class="transaction-tooltip">
                                        <div class={final_chip_class} 
                                             onclick={chip_click}>
                                            {if props.delete_mode {
                                                html! {
                                                    <input 
                                                        type="checkbox" 
                                                        class="transaction-checkbox"
                                                        checked={is_selected}
                                                        readonly=true
                                                    />
                                                }
                                            } else {
                                                html! {}
                                            }}
                                            <span class="transaction-amount">
                                                {if transaction.amount >= 0.0 {
                                                    format!("+${:.0}", transaction.amount)
                                                } else {
                                                    format!("-${:.0}", transaction.amount.abs())
                                                }}
                                            </span>
                                        </div>
                                        <div class={format!("custom-tooltip {}", if transaction.amount >= 0.0 { "income-border" } else { "spending-border" })}>
                                            <div class="tooltip-header">
                                                {create_transaction_tooltip(transaction)}
                                            </div>
                                            <div class="tooltip-body">
                                                <div class="tooltip-row">
                                                    <span class="tooltip-label">{"Amount:"}</span>
                                                    <span class={format!("tooltip-value {}", if transaction.amount >= 0.0 { "positive" } else { "negative" })}>
                                                        {if transaction.amount >= 0.0 {
                                                            format!("+${:.2}", transaction.amount)
                                                        } else {
                                                            format!("-${:.2}", transaction.amount.abs())
                                                        }}
                                                    </span>
                                                </div>
                                            </div>
                                        </div>
                                    </div>
                                }
                            })}
                            
                            // Show "+X more" indicator if there are remaining transactions
                            {if remaining_count > 0 {
                                html! {
                                    <div class="simple-calendar-more-indicator">
                                        {format!("+{} more", remaining_count)}
                                    </div>
                                }
                            } else {
                                html! {}
                            }}
                        </div>
                    </div>
                });
            }
        }
        
        days
    };

    // Render loading state
    if *is_loading {
        return html! {
            <div class="calendar-card">
                <div class="calendar-header-container">
                    <div class="calendar-header">
                        <button class="calendar-nav-button" disabled=true>
                            <i class="fas fa-chevron-left"></i>
                        </button>
                        <h2>{"Loading..."}</h2>
                        <button class="calendar-nav-button" disabled=true>
                            <i class="fas fa-chevron-right"></i>
                        </button>
                    </div>
                </div>
                
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
                    <div style="grid-column: 1 / -1; text-align: center; padding: 2rem; color: #666;">
                        {"Loading calendar state from backend..."}
                    </div>
                </div>
            </div>
        };
    }

    // Render error state
    if let Some(error) = error_message.as_ref() {
        return html! {
            <div class="calendar-card">
                <div class="calendar-header-container">
                    <div class="calendar-header">
                        <button class="calendar-nav-button" disabled=true>
                            <i class="fas fa-chevron-left"></i>
                        </button>
                        <h2>{"Error"}</h2>
                        <button class="calendar-nav-button" disabled=true>
                            <i class="fas fa-chevron-right"></i>
                        </button>
                    </div>
                </div>
                
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
                    <div style="grid-column: 1 / -1; text-align: center; padding: 2rem; color: #dc2626;">
                        <div style="font-weight: bold; margin-bottom: 0.5rem;">{"Error:"}</div>
                        <div>{error}</div>
                    </div>
                </div>
            </div>
        };
    }

    // Render calendar with backend state
    match calendar_state.as_ref() {
        Some(focus_date) => {
            let calendar_days = generate_calendar_days(&*calendar_month_data, &*current_date, &*allowance_config);
            
                html! {
        <div class="calendar-card">
            <section class="calendar-section">
                <div class="calendar-header-container">
                    <div class="calendar-header">
                        <button class="calendar-nav-button" onclick={on_previous} title="Previous Month">
                            <i class="fas fa-chevron-left"></i>
                        </button>
                        <h2>
                            {format!("{} {}", month_name(focus_date.month), focus_date.year)}
                        </h2>
                        <button class="calendar-nav-button" onclick={on_next} title="Next Month">
                            <i class="fas fa-chevron-right"></i>
                        </button>
                    </div>
                    
                    {if props.delete_mode {
                        html! {
                            <div class="delete-mode-controls">
                                <span class="selection-count">
                                    {format!("{} transaction{} selected", 
                                        props.selected_transactions.len(),
                                        if props.selected_transactions.len() == 1 { "" } else { "s" }
                                    )}
                                </span>
                                <button 
                                    class="delete-button"
                                    onclick={{
                                        let on_delete = props.on_delete_selected.clone();
                                        Callback::from(move |_: MouseEvent| {
                                            on_delete.emit(());
                                        })
                                    }}
                                    disabled={props.selected_transactions.is_empty()}
                                    title="Delete selected transactions"
                                >
                                    <i class="fas fa-trash"></i>
                                    {" Delete"}
                                </button>
                            </div>
                        }
                    } else {
                        html! {}
                    }}
                    </div>
                    
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
                </section>
            </div>
            }
        }
        None => {
            html! {
                <div class="calendar-card">
                    <div class="calendar-header-container">
                        <div class="calendar-header">
                            <button class="calendar-nav-button" disabled=true>
                                <i class="fas fa-chevron-left"></i>
                            </button>
                            <h2>{"Simple Calendar"}</h2>
                            <button class="calendar-nav-button" disabled=true>
                                <i class="fas fa-chevron-right"></i>
                            </button>
                        </div>
                    </div>
                    
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
                        <div style="grid-column: 1 / -1; text-align: center; padding: 2rem; color: #f59e0b;">
                            {"No calendar state available"}
                        </div>
                    </div>
                </div>
            }
        }
    }
} 