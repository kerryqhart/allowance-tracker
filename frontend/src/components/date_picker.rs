use yew::prelude::*;
use web_sys::{window, Element};
use wasm_bindgen::JsCast;
use crate::services::date_utils::*;

#[derive(Properties, PartialEq)]
pub struct DatePickerProps {
    /// Selected date in YYYY-MM-DD format, or None for "today"
    pub selected_date: Option<String>,
    /// Callback when date changes
    pub on_date_change: Callback<Option<String>>,
    /// Whether the date picker is disabled
    pub disabled: bool,
    /// Optional label for the date picker
    pub label: Option<String>,
    /// Optional debug callback
    #[prop_or_default]
    pub on_debug: Option<Callback<String>>,
}

#[function_component(DatePicker)]
pub fn date_picker(props: &DatePickerProps) -> Html {
    let show_calendar = use_state(|| false);
    let calendar_ref = use_node_ref();
    
    // Get current date info
    let current_date = get_current_date();
    let _current_display = get_current_date_display();
    
    // Determine display text and actual date value
    let (display_text, actual_date) = match &props.selected_date {
        Some(date) if is_today(date) => ("Today".to_string(), date.clone()),
        Some(date) => (format_date_for_display(date), date.clone()),
        None => ("Today".to_string(), current_date.clone()),
    };
    
    // Calendar state
    let calendar_month = use_state(|| {
        let now = js_sys::Date::new_0();
        now.get_month() as u32 + 1 // Convert to 1-based
    });
    let calendar_year = use_state(|| {
        let now = js_sys::Date::new_0();
        now.get_full_year() as u32
    });
    
    // Toggle calendar visibility
    let toggle_calendar = {
        let show_calendar = show_calendar.clone();
        let _calendar_ref = calendar_ref.clone();
        let on_debug = props.on_debug.clone();
        Callback::from(move |_: MouseEvent| {
            let was_open = *show_calendar;
            show_calendar.set(!was_open);
            
            // Debug logging
            if let Some(debug_cb) = &on_debug {
                debug_cb.emit(format!("📅 Calendar toggle: {} -> {}", was_open, !was_open));
            }
        })
    };
    
    // Handle date selection
    let on_date_select = {
        let on_date_change = props.on_date_change.clone();
        let show_calendar = show_calendar.clone();
        Callback::from(move |date: String| {
            let new_date = if is_today(&date) {
                None // Use None for "today" to keep it dynamic
            } else {
                Some(date)
            };
            on_date_change.emit(new_date);
            show_calendar.set(false);
        })
    };
    
    // Handle "Today" button click
    let on_today_click = {
        let on_date_change = props.on_date_change.clone();
        let show_calendar = show_calendar.clone();
        Callback::from(move |_: MouseEvent| {
            on_date_change.emit(None);
            show_calendar.set(false);
        })
    };
    
    // Handle clicking outside calendar to close it
    {
        let show_calendar = show_calendar.clone();
        let calendar_ref = calendar_ref.clone();
        use_effect_with(*show_calendar, move |is_open| {
            if !*is_open {
                return;
            }
            
            let _callback = {
                let show_calendar = show_calendar.clone();
                let calendar_ref = calendar_ref.clone();
                gloo::events::EventListener::new(&window().unwrap(), "click", move |e| {
                    if let Some(target) = e.target() {
                        if let Ok(element) = target.dyn_into::<Element>() {
                            if let Some(calendar_element) = calendar_ref.cast::<Element>() {
                                if !calendar_element.contains(Some(&element)) {
                                    show_calendar.set(false);
                                }
                            }
                        }
                    }
                })
            };
            
            // The callback will be dropped automatically when the effect is cleaned up
        });
    }
    
    // Previous/Next month navigation
    let prev_month = {
        let calendar_month = calendar_month.clone();
        let calendar_year = calendar_year.clone();
        Callback::from(move |_: MouseEvent| {
            let current_month = *calendar_month;
            let current_year = *calendar_year;
            
            if current_month == 1 {
                calendar_month.set(12);
                calendar_year.set(current_year - 1);
            } else {
                calendar_month.set(current_month - 1);
            }
        })
    };
    
    let next_month = {
        let calendar_month = calendar_month.clone();
        let calendar_year = calendar_year.clone();
        Callback::from(move |_: MouseEvent| {
            let current_month = *calendar_month;
            let current_year = *calendar_year;
            
            if current_month == 12 {
                calendar_month.set(1);
                calendar_year.set(current_year + 1);
            } else {
                calendar_month.set(current_month + 1);
            }
        })
    };
    
    // Generate calendar days
    let calendar_days = generate_calendar_days(*calendar_year, *calendar_month);
    
    html! {
        <div class="date-picker" ref={calendar_ref.clone()}>
            {if let Some(label) = &props.label {
                html! { <label class="date-picker-label">{label}</label> }
            } else { html! {} }}
            
            <div class="date-picker-input">
                <button 
                    type="button"
                    class="date-display-button"
                    onclick={toggle_calendar}
                    disabled={props.disabled}
                >
                    <span class="date-text">{display_text}</span>
                    <span class="calendar-icon">{"📅"}</span>
                </button>
                
                {if *show_calendar && !props.disabled {
                    html! {
                        <div class="calendar-dropdown">
                            <div class="calendar-header">
                                <button type="button" class="nav-button" onclick={prev_month}>{"‹"}</button>
                                <span class="month-year">{format!("{} {}", 
                                    match *calendar_month {
                                        1 => "January", 2 => "February", 3 => "March", 4 => "April",
                                        5 => "May", 6 => "June", 7 => "July", 8 => "August",
                                        9 => "September", 10 => "October", 11 => "November", 12 => "December",
                                        _ => "January",
                                    }, 
                                    *calendar_year
                                )}</span>
                                <button type="button" class="nav-button" onclick={next_month}>{"›"}</button>
                            </div>
                            
                            <div class="calendar-grid">
                                <div class="weekday-header">
                                    <span>{"Sun"}</span>
                                    <span>{"Mon"}</span>
                                    <span>{"Tue"}</span>
                                    <span>{"Wed"}</span>
                                    <span>{"Thu"}</span>
                                    <span>{"Fri"}</span>
                                    <span>{"Sat"}</span>
                                </div>
                                
                                <div class="calendar-days">
                                    {for calendar_days.iter().map(|day| {
                                        let day_clone = day.clone();
                                        let on_date_select = on_date_select.clone();
                                        let is_selected = day.date_string == actual_date;
                                        let is_today_day = is_today(&day.date_string);
                                        
                                        html! {
                                            <button
                                                type="button"
                                                class={classes!(
                                                    "calendar-day",
                                                    day.is_current_month.then(|| "current-month"),
                                                    (!day.is_current_month).then(|| "other-month"),
                                                    day.is_valid.then(|| "valid"),
                                                    (!day.is_valid).then(|| "invalid"),
                                                    is_selected.then(|| "selected"),
                                                    is_today_day.then(|| "today")
                                                )}
                                                disabled={!day.is_valid}
                                                onclick={
                                                    let date_string = day_clone.date_string.clone();
                                                    Callback::from(move |_: MouseEvent| {
                                                        on_date_select.emit(date_string.clone());
                                                    })
                                                }
                                            >
                                                {day.day}
                                            </button>
                                        }
                                    })}
                                </div>
                            </div>
                            
                            <div class="calendar-footer">
                                <button type="button" class="today-button" onclick={on_today_click}>
                                    {"Today"}
                                </button>
                            </div>
                        </div>
                    }
                } else { html! {} }}
            </div>
        </div>
    }
}

#[derive(Clone, PartialEq)]
struct CalendarDay {
    day: u32,
    date_string: String, // YYYY-MM-DD format
    is_current_month: bool,
    is_valid: bool,
}

fn generate_calendar_days(year: u32, month: u32) -> Vec<CalendarDay> {
    let mut days = Vec::new();
    
    let days_in_current_month = days_in_month(year, month);
    let first_day_of_week = get_first_day_of_month(year, month);
    
    // Add padding days from previous month
    let prev_month = if month == 1 { 12 } else { month - 1 };
    let prev_year = if month == 1 { year - 1 } else { year };
    let days_in_prev_month = days_in_month(prev_year, prev_month);
    
    for i in 0..first_day_of_week {
        let day = days_in_prev_month - first_day_of_week + i + 1;
        let date_string = format!("{:04}-{:02}-{:02}", prev_year, prev_month, day);
        days.push(CalendarDay {
            day,
            date_string: date_string.clone(),
            is_current_month: false,
            is_valid: is_valid_transaction_date(&date_string),
        });
    }
    
    // Add current month days
    for day in 1..=days_in_current_month {
        let date_string = format!("{:04}-{:02}-{:02}", year, month, day);
        days.push(CalendarDay {
            day,
            date_string: date_string.clone(),
            is_current_month: true,
            is_valid: is_valid_transaction_date(&date_string),
        });
    }
    
    // Add padding days from next month to complete the grid (42 days total - 6 weeks)
    let next_month = if month == 12 { 1 } else { month + 1 };
    let next_year = if month == 12 { year + 1 } else { year };
    let remaining_days = 42 - days.len();
    
    for day in 1..=remaining_days {
        let date_string = format!("{:04}-{:02}-{:02}", next_year, next_month, day);
        days.push(CalendarDay {
            day: day as u32,
            date_string: date_string.clone(),
            is_current_month: false,
            is_valid: is_valid_transaction_date(&date_string),
        });
    }
    
    days
} 