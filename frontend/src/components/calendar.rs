use yew::prelude::*;
use shared::{CalendarMonth, AllowanceConfig, CurrentDateResponse};
use crate::services::date_utils::{format_calendar_date, month_name};
use web_sys::console;

#[derive(Properties, Clone, PartialEq)]
pub struct CalendarProps {
    pub calendar_data: CalendarMonth,
    pub delete_mode: bool,
    pub selected_transactions: Vec<String>,
    pub on_toggle_transaction_selection: Callback<String>,
    pub on_delete_selected: Callback<()>,
    pub allowance_config: Option<AllowanceConfig>,
    pub on_previous_month: Callback<()>,
    pub on_next_month: Callback<()>,
    pub current_date: Option<CurrentDateResponse>,
}

#[function_component(Calendar)]
pub fn calendar(props: &CalendarProps) -> Html {
    let flash_color = use_state(|| "#4f46e5".to_string());
    
    // Flash effect for navigation test indicator
    use_effect_with((), {
        let flash_color = flash_color.clone();
        move |_| {
            let flash_color = flash_color.clone();
            gloo::timers::callback::Timeout::new(100, move || {
                flash_color.set("#10b981".to_string()); // Green flash
                let flash_color = flash_color.clone();
                gloo::timers::callback::Timeout::new(300, move || {
                    flash_color.set("#4f46e5".to_string()); // Back to blue
                }).forget();
            }).forget();
            || ()
        }
    });

    let calendar_data = &props.calendar_data;
    
    // State for test glyph flash color and debugging counters
    let prev_click_count = use_state(|| 0u32);
    let next_click_count = use_state(|| 0u32);
    let last_update_time = use_state(|| "never".to_string());
    
    let on_previous_month_click = {
        let on_previous_month = props.on_previous_month.clone();
        let flash_color = flash_color.clone();
        let prev_click_count = prev_click_count.clone();
        let last_update_time = last_update_time.clone();
        Callback::from(move |_| {
            let new_count = *prev_click_count + 1;
            prev_click_count.set(new_count);
            last_update_time.set(format!("PREV-{}", new_count));
            
            console::log_1(&format!("Left arrow clicked! Count: {}", new_count).into());
            flash_color.set("red".to_string());
            
            // Reset flash color after 300ms
            let flash_color_reset = flash_color.clone();
            wasm_bindgen_futures::spawn_local(async move {
                gloo::timers::future::sleep(std::time::Duration::from_millis(300)).await;
                flash_color_reset.set("green".to_string());
            });
            
            on_previous_month.emit(());
        })
    };

    let on_next_month_click = {
        let on_next_month = props.on_next_month.clone();
        let flash_color = flash_color.clone();
        let next_click_count = next_click_count.clone();
        let last_update_time = last_update_time.clone();
        Callback::from(move |_| {
            let new_count = *next_click_count + 1;
            next_click_count.set(new_count);
            last_update_time.set(format!("NEXT-{}", new_count));
            
            console::log_1(&format!("Right arrow clicked! Count: {}", new_count).into());
            flash_color.set("blue".to_string());
            
            // Reset flash color after 300ms
            let flash_color_reset = flash_color.clone();
            wasm_bindgen_futures::spawn_local(async move {
                gloo::timers::future::sleep(std::time::Duration::from_millis(300)).await;
                flash_color_reset.set("green".to_string());
            });
            
            on_next_month.emit(());
        })
    };
    
    // Helper function to determine if a day should show allowance indicator
    let is_allowance_day = |day: u32| -> bool {
        if let Some(config) = &props.allowance_config {
            if !config.is_active {
                return false;
            }
            
            // Calculate day of week for this day in the calendar
            let days_from_first = day - 1; // 0-based index from first day of month
            let day_of_week = (calendar_data.first_day_of_week + days_from_first) % 7;
            
            // Check if this day matches the configured allowance day of week
            if day_of_week as u8 != config.day_of_week {
                return false;
            }
            
            // Only show allowance indicator for future dates if we have current date info
            if let Some(current_date) = &props.current_date {
                // Compare the calendar day with current date
                if calendar_data.year > current_date.year {
                    return true; // Future year
                } else if calendar_data.year == current_date.year {
                    if calendar_data.month > current_date.month {
                        return true; // Future month in current year
                    } else if calendar_data.month == current_date.month {
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
        // Check if this is the current day using backend date info
        let is_today = if let Some(current_date) = &props.current_date {
            calendar_data.year == current_date.year 
                && calendar_data.month == current_date.month 
                && day_data.day == current_date.day
        } else {
            false // If we don't have current date info, no day is marked as today
        };
        
        let day_class = if props.delete_mode {
            if is_today {
                "calendar-day delete-mode today"
            } else {
                "calendar-day delete-mode"
            }
        } else {
            if is_today {
                "calendar-day today"
            } else {
                "calendar-day"
            }
        };
        
        calendar_days.push(html! {
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
                    // Show allowance chip if this is an allowance day
                    {if is_allowance_day(day_data.day) {
                        // Get the allowance amount from config
                        let allowance_amount = props.allowance_config.as_ref()
                            .map(|config| config.amount)
                            .unwrap_or(0.0);
                        
                        html! {
                            <div class="allowance-chip" title={format!("Weekly allowance: ${:.2}", allowance_amount)}>
                                <span class="transaction-amount">
                                    {format!("+${:.0}", allowance_amount)}
                                </span>
                            </div>
                        }
                    } else {
                        html! {}
                    }}
                    
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
                                
                                // Show checkbox in delete mode
                                {if props.delete_mode {
                                    let transaction_id = transaction.id.clone();
                                    let is_selected = props.selected_transactions.contains(&transaction_id);
                                    let on_toggle = props.on_toggle_transaction_selection.clone();
                                    
                                    html! {
                                        <input 
                                            type="checkbox" 
                                            class="transaction-checkbox"
                                            checked={is_selected}
                                            onchange={{
                                                let transaction_id = transaction_id.clone();
                                                Callback::from(move |_| {
                                                    on_toggle.emit(transaction_id.clone());
                                                })
                                            }}
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
                                
                                // Custom tooltip div that will be shown on hover (only if not in delete mode)
                                {if !props.delete_mode {
                                    html! {
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
                                    }
                                } else {
                                    html! {}
                                }}
                            </div>
                        }
                    })}
                </div>
            </div>
        });
    }
    
    html! {
        <div class="calendar-card" style="position: relative;">
            // Test glyph positioned in whitespace between left arrow and title
            <div style="position: absolute; top: 10px; left: 35%; transform: translateX(-50%); z-index: 1000; display: flex; flex-direction: column; align-items: center; gap: 4px;">
                <div style="display: flex; align-items: center; gap: 8px;">
                    <div class="test-glyph" style={format!("background-color: {}; width: 24px; height: 24px; border-radius: 50%; transition: background-color 0.3s ease; display: flex; align-items: center; justify-content: center; color: white; font-weight: bold; font-size: 14px; border: 2px solid white; box-shadow: 0 2px 4px rgba(0,0,0,0.2);", *flash_color)} title="Navigation Test Indicator">
                        {"●"}
                    </div>
                    <div style="background-color: rgba(0,0,0,0.8); color: white; padding: 4px 8px; border-radius: 4px; font-size: 12px; font-family: monospace; white-space: nowrap;" title="Current Calendar State">
                        {format!("{}/{}", calendar_data.month, calendar_data.year)}
                    </div>
                </div>
                <div style="background-color: rgba(0,0,0,0.8); color: white; padding: 2px 6px; border-radius: 4px; font-size: 10px; font-family: monospace; white-space: nowrap;" title="Click Debug Info">
                    {format!("←{} →{} | {}", *prev_click_count, *next_click_count, *last_update_time)}
                </div>
            </div>
            
            <div class="calendar-header-container">
                <div class="calendar-header">
                    <button class="calendar-nav-button" onclick={on_previous_month_click} title="Previous Month">
                        <i class="fas fa-chevron-left"></i>
                    </button>
                    <h2 class="calendar-title">
                        {
                            if props.delete_mode {
                                format!("🗑️ Delete Mode - {} {}", month_name(calendar_data.month), calendar_data.year)
                            } else {
                                format!("{} {}", month_name(calendar_data.month), calendar_data.year)
                            }
                        }
                    </h2>
                    <button class="calendar-nav-button" onclick={on_next_month_click} title="Next Month">
                        <i class="fas fa-chevron-right"></i>
                    </button>
                </div>
                 { if props.delete_mode {
                    html! {
                        <div class="delete-mode-controls">
                            <span class="selection-count">{format!("{} selected", props.selected_transactions.len())}</span>
                            <button 
                                class="delete-button" 
                                onclick={
                                    let on_delete_selected = props.on_delete_selected.clone();
                                    Callback::from(move |_: MouseEvent| {
                                        on_delete_selected.emit(());
                                    })
                                }
                                disabled={props.selected_transactions.is_empty()}
                            >
                                {"Delete Selected"}
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
        </div>
    }
} 