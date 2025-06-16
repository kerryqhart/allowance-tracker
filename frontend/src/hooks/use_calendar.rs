use yew::prelude::*;
use shared::CalendarMonth;
use wasm_bindgen_futures::spawn_local;
use web_sys::MouseEvent;
use crate::services::api::ApiClient;

#[derive(Clone)]
pub struct CalendarState {
    pub current_month: u32,
    pub current_year: u32,
    pub calendar_data: Option<CalendarMonth>,
}

pub struct UseCalendarResult {
    pub state: CalendarState,
    pub actions: UseCalendarActions,
}

#[derive(Clone)]
pub struct UseCalendarActions {
    pub prev_month: Callback<MouseEvent>,
    pub next_month: Callback<MouseEvent>,
    pub refresh_calendar: Callback<()>,
}

#[hook]
pub fn use_calendar(api_client: &ApiClient) -> UseCalendarResult {
    let current_month = use_state(|| 6u32); // June
    let current_year = use_state(|| 2025u32);
    let calendar_data = use_state(|| Option::<CalendarMonth>::None);

    // Refresh calendar callback
    let refresh_calendar = {
        let api_client = api_client.clone();
        let calendar_data = calendar_data.clone();
        let current_month = current_month.clone();
        let current_year = current_year.clone();
        
        use_callback((), move |_, _| {
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
                        gloo::console::error!("Failed to fetch calendar data:", e);
                    }
                }
            });
        })
    };

    // Navigation callbacks
    let prev_month = {
        let current_month = current_month.clone();
        let current_year = current_year.clone();
        use_callback((), move |_: MouseEvent, _| {
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
        use_callback((), move |_: MouseEvent, _| {
            if *current_month == 12 {
                current_month.set(1);
                current_year.set(*current_year + 1);
            } else {
                current_month.set(*current_month + 1);
            }
        })
    };

    // Auto-refresh calendar when month/year changes
    use_effect_with((current_month.clone(), current_year.clone()), {
        let refresh_calendar = refresh_calendar.clone();
        move |_| {
            refresh_calendar.emit(());
            || ()
        }
    });

    let state = CalendarState {
        current_month: *current_month,
        current_year: *current_year,
        calendar_data: (*calendar_data).clone(),
    };

    let actions = UseCalendarActions {
        prev_month,
        next_month,
        refresh_calendar,
    };

    UseCalendarResult { state, actions }
} 