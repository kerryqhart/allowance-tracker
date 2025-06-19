use yew::prelude::*;
use shared::{CalendarMonth, CurrentDateResponse};
use wasm_bindgen_futures::spawn_local;
use crate::services::api::ApiClient;

#[derive(Clone, Debug, PartialEq)]
pub struct CalendarState {
    pub current_month: u32,
    pub current_year: u32,
    pub calendar_data: Option<CalendarMonth>,
    pub current_date: Option<CurrentDateResponse>,
}

pub struct UseCalendarResult {
    pub state: CalendarState,
    pub actions: UseCalendarActions,
}

#[derive(Clone)]
pub struct UseCalendarActions {
    pub refresh_calendar: Callback<()>,
    pub previous_month: Callback<()>,
    pub next_month: Callback<()>,
}

#[hook]
pub fn use_calendar(api_client: &ApiClient, child_change_trigger: u32) -> UseCalendarResult {
    let current_date = use_state(|| Option::<CurrentDateResponse>::None);
    let current_month = use_state(|| 1u32);
    let current_year = use_state(|| 2025u32);
    let calendar_data = use_state(|| Option::<CalendarMonth>::None);
    let date_loaded = use_state(|| false);

    // Fetch current date from backend on first load
    use_effect_with((), {
        let api_client = api_client.clone();
        let current_date = current_date.clone();
        let current_month = current_month.clone();
        let current_year = current_year.clone();
        let date_loaded = date_loaded.clone();
        
        move |_| {
            spawn_local(async move {
                match api_client.get_current_date().await {
                    Ok(date_response) => {
                        gloo::console::log!("✅ Current date loaded:", format!("{}/{}", date_response.month, date_response.year));
                        current_month.set(date_response.month);
                        current_year.set(date_response.year);
                        current_date.set(Some(date_response));
                        date_loaded.set(true);
                    }
                    Err(e) => {
                        gloo::console::error!("❌ Failed to fetch current date from backend:", e);
                        date_loaded.set(true);
                    }
                }
            });
            || ()
        }
    });

    // Refresh calendar when date is loaded or month/year changes
    use_effect_with((date_loaded.clone(), current_month.clone(), current_year.clone()), {
        let api_client = api_client.clone();
        let calendar_data = calendar_data.clone();
        
        move |(date_is_loaded, month, year)| {
            if **date_is_loaded {
                let api_client = api_client.clone();
                let calendar_data = calendar_data.clone();
                let month = **month;
                let year = **year;
                
                gloo::console::log!("🗓️ Refreshing calendar for:", format!("{}/{}", month, year));
                
                spawn_local(async move {
                    match api_client.get_calendar_month(month, year).await {
                        Ok(data) => {
                            gloo::console::log!("✅ Calendar data loaded for:", format!("{}/{}", month, year));
                            calendar_data.set(Some(data));
                        }
                        Err(e) => {
                            gloo::console::error!("❌ Failed to fetch calendar data:", e);
                        }
                    }
                });
            } else {
                gloo::console::log!("⏳ Waiting for date to load...");
            }
            || ()
        }
    });

    // Refresh when child changes
    use_effect_with(child_change_trigger, {
        let api_client = api_client.clone();
        let calendar_data = calendar_data.clone();
        let current_month = current_month.clone();
        let current_year = current_year.clone();
        let date_loaded = date_loaded.clone();
        
        move |_| {
            if *date_loaded {
                let api_client = api_client.clone();
                let calendar_data = calendar_data.clone();
                let month = *current_month;
                let year = *current_year;
                
                gloo::console::log!("👶 Child changed, refreshing calendar");
                
                spawn_local(async move {
                    match api_client.get_calendar_month(month, year).await {
                        Ok(data) => {
                            calendar_data.set(Some(data));
                        }
                        Err(e) => {
                            gloo::console::error!("❌ Failed to fetch calendar data:", e);
                        }
                    }
                });
            }
            || ()
        }
    });

    let refresh_calendar = {
        let api_client = api_client.clone();
        let calendar_data = calendar_data.clone();
        let current_month = current_month.clone();
        let current_year = current_year.clone();
        
        Callback::from(move |_| {
            let api_client = api_client.clone();
            let calendar_data = calendar_data.clone();
            let month = *current_month;
            let year = *current_year;
            
            spawn_local(async move {
                match api_client.get_calendar_month(month, year).await {
                    Ok(data) => {
                        calendar_data.set(Some(data));
                    }
                    Err(e) => {
                        gloo::console::error!("❌ Failed to fetch calendar data:", e);
                    }
                }
            });
        })
    };

    let previous_month = {
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

    let state = CalendarState {
        current_month: *current_month,
        current_year: *current_year,
        calendar_data: (*calendar_data).clone(),
        current_date: (*current_date).clone(),
    };

    let actions = UseCalendarActions {
        refresh_calendar,
        previous_month,
        next_month,
    };

    UseCalendarResult { state, actions }
} 