use yew::prelude::*;
use shared::CalendarFocusDate;
use crate::services::api::ApiClient;
use wasm_bindgen_futures::spawn_local;

#[function_component(SimpleCalendar)]
pub fn simple_calendar() -> Html {
    // State for current month/year from backend
    let calendar_state = use_state(|| Option::<CalendarFocusDate>::None);
    let is_loading = use_state(|| true);
    let error_message = use_state(|| Option::<String>::None);
    
    // Debug counter to verify callbacks are working
    let click_count = use_state(|| 0u32);

    // API client
    let api_client = use_memo((), |_| ApiClient::new());

    // Load initial focus date from backend
    {
        let calendar_state = calendar_state.clone();
        let is_loading = is_loading.clone();
        let error_message = error_message.clone();
        let api_client = api_client.clone();
        
        use_effect_with((), move |_| {
            spawn_local(async move {
                match (*api_client).get_focus_date().await {
                    Ok(focus_date) => {
                        gloo::console::log!(&format!("Loaded focus date from backend: {}/{}", focus_date.month, focus_date.year));
                        calendar_state.set(Some(focus_date));
                        error_message.set(None);
                    }
                    Err(e) => {
                        gloo::console::error!(&format!("Failed to load focus date: {}", e));
                        error_message.set(Some(format!("Failed to load calendar state: {}", e)));
                    }
                }
                is_loading.set(false);
            });
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
        let click_count = click_count.clone();
        let error_message = error_message.clone();
        let api_client = api_client.clone();
        
        Callback::from(move |_: MouseEvent| {
            let calendar_state = calendar_state.clone();
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
                        calendar_state.set(Some(response.focus_date));
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
        let click_count = click_count.clone();
        let error_message = error_message.clone();
        let api_client = api_client.clone();
        
        Callback::from(move |_: MouseEvent| {
            let calendar_state = calendar_state.clone();
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
                        calendar_state.set(Some(response.focus_date));
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
        let mut a = (14 - month) / 12;
        let y = year - a;
        let m = month + 12 * a - 2;
        let day = (1 + y + y / 4 - y / 100 + y / 400 + (31 * m) / 12) % 7;
        day
    };

    // Generate calendar days for the given month/year
    let generate_calendar_days = |month: u32, year: u32| -> Vec<Html> {
        let mut days = Vec::new();
        let days_in_current_month = days_in_month(month, year);
        let first_day = first_day_of_week(month, year);
        
        // Add empty cells for days before the first of the month
        for _ in 0..first_day {
            days.push(html! {
                <div class="calendar-day empty"></div>
            });
        }
        
        // Add days of the current month
        for day in 1..=days_in_current_month {
            days.push(html! {
                <div class="calendar-day">
                    <div class="day-header">
                        <div class="day-number-container">
                            <div class="day-number">{day}</div>
                        </div>
                        <div class="day-balance-subtle">
                            {"$0"}
                        </div>
                    </div>
                    <div class="day-transactions">
                        // Empty for now - just showing the basic structure
                    </div>
                </div>
            });
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
                        <h2 class="calendar-title">{"Loading..."}</h2>
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
                        <h2 class="calendar-title">{"Error"}</h2>
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
            let calendar_days = generate_calendar_days(focus_date.month, focus_date.year);
            
            html! {
                <div class="calendar-card">
                    <div class="calendar-header-container">
                        <div class="calendar-header">
                            <button class="calendar-nav-button" onclick={on_previous} title="Previous Month">
                                <i class="fas fa-chevron-left"></i>
                            </button>
                            <h2 class="calendar-title">
                                {format!("🆕 {} {}", month_name(focus_date.month), focus_date.year)}
                            </h2>
                            <button class="calendar-nav-button" onclick={on_next} title="Next Month">
                                <i class="fas fa-chevron-right"></i>
                            </button>
                        </div>
                        
                        <div style="text-align: center; margin-top: 0.5rem; font-size: 0.9rem; color: #666;">
                            {format!("Debug: {} total clicks | Backend: {}/{}", *click_count, focus_date.month, focus_date.year)}
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
                        {for calendar_days}
                    </div>
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
                            <h2 class="calendar-title">{"🆕 Simple Calendar"}</h2>
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