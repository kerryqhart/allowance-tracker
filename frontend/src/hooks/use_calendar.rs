use yew::prelude::*;
use shared::{CalendarMonth, CurrentDateResponse, CalendarFocusDate};
use wasm_bindgen_futures::spawn_local;
use crate::services::api::ApiClient;

#[derive(Clone, Debug, PartialEq)]
pub struct CalendarState {
    pub current_month: u32,
    pub current_year: u32,
    pub calendar_data: Option<CalendarMonth>,
    pub current_date: Option<CurrentDateResponse>,
    pub loading: bool,
    pub error_message: Option<String>,
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
    let focus_date = use_state(|| Option::<CalendarFocusDate>::None);
    let calendar_data = use_state(|| Option::<CalendarMonth>::None);
    let loading = use_state(|| true);
    let error_message = use_state(|| Option::<String>::None);
    let initial_load_done = use_state(|| false);

    // Load initial data from backend on first load
    use_effect_with((), {
        let api_client = api_client.clone();
        let current_date = current_date.clone();
        let focus_date = focus_date.clone();
        let loading = loading.clone();
        let error_message = error_message.clone();
        let initial_load_done = initial_load_done.clone();
        
        move |_| {
            spawn_local(async move {
                // Load current date and focus date in parallel
                let current_date_future = api_client.get_current_date();
                let focus_date_future = api_client.get_focus_date();
                
                let current_date_result = current_date_future.await;
                let focus_date_result = focus_date_future.await;
                
                match (current_date_result, focus_date_result) {
                    (Ok(current_date_response), Ok(focus_date_response)) => {
                        gloo::console::log!("✅ Loaded current date and focus date from backend");
                        current_date.set(Some(current_date_response));
                        focus_date.set(Some(focus_date_response));
                        error_message.set(None);
                    }
                    (Err(current_date_error), Ok(focus_date_response)) => {
                        gloo::console::warn!("⚠️ Failed to load current date:", &current_date_error);
                        focus_date.set(Some(focus_date_response));
                        error_message.set(Some(format!("Failed to load current date: {}", current_date_error)));
                    }
                    (Ok(current_date_response), Err(focus_date_error)) => {
                        gloo::console::error!("❌ Failed to load focus date:", &focus_date_error);
                        current_date.set(Some(current_date_response));
                        error_message.set(Some(format!("Failed to load calendar focus: {}", focus_date_error)));
                    }
                    (Err(current_date_error), Err(focus_date_error)) => {
                        gloo::console::error!("❌ Failed to load both current date and focus date");
                        error_message.set(Some(format!("Backend error - Current date: {}, Focus date: {}", current_date_error, focus_date_error)));
                    }
                }
                
                initial_load_done.set(true);
                loading.set(false);
            });
            || ()
        }
    });

    // Refresh calendar when focus date changes
    use_effect_with((*initial_load_done, (*focus_date).clone()), {
        let api_client = api_client.clone();
        let calendar_data = calendar_data.clone();
        let error_message = error_message.clone();
        
        move |(is_loaded, focus_date_opt)| {
            if *is_loaded {
                if let Some(focus_date_val) = focus_date_opt {
                    let api_client = api_client.clone();
                    let calendar_data = calendar_data.clone();
                    let error_message = error_message.clone();
                    let month = focus_date_val.month;
                    let year = focus_date_val.year;
                    
                    gloo::console::log!("🗓️ Loading calendar data for backend focus date:", format!("{}/{}", month, year));
                    
                    spawn_local(async move {
                        match api_client.get_calendar_month(month, year).await {
                            Ok(data) => {
                                gloo::console::log!("✅ Calendar data loaded for:", format!("{}/{}", month, year));
                                calendar_data.set(Some(data));
                                error_message.set(None);
                            }
                            Err(e) => {
                                gloo::console::error!("❌ Failed to fetch calendar data:", &e);
                                error_message.set(Some(format!("Failed to load calendar: {}", e)));
                            }
                        }
                    });
                }
            }
            || ()
        }
    });

    // Refresh when child changes
    use_effect_with(child_change_trigger, {
        let api_client = api_client.clone();
        let calendar_data = calendar_data.clone();
        let focus_date = focus_date.clone();
        let initial_load_done = initial_load_done.clone();
        let error_message = error_message.clone();
        
        move |_| {
            if *initial_load_done {
                if let Some(focus_date_val) = focus_date.as_ref() {
                    let api_client = api_client.clone();
                    let calendar_data = calendar_data.clone();
                    let error_message = error_message.clone();
                    let month = focus_date_val.month;
                    let year = focus_date_val.year;
                    
                    gloo::console::log!("👶 Child changed, refreshing calendar");
                    
                    spawn_local(async move {
                        match api_client.get_calendar_month(month, year).await {
                            Ok(data) => {
                                calendar_data.set(Some(data));
                                error_message.set(None);
                            }
                            Err(e) => {
                                gloo::console::error!("❌ Failed to fetch calendar data:", &e);
                                error_message.set(Some(format!("Failed to refresh calendar: {}", e)));
                            }
                        }
                    });
                }
            }
            || ()
        }
    });

    let refresh_calendar = {
        let api_client = api_client.clone();
        let calendar_data = calendar_data.clone();
        let focus_date = focus_date.clone();
        let error_message = error_message.clone();
        
        Callback::from(move |_| {
            if let Some(focus_date_val) = focus_date.as_ref() {
                let api_client = api_client.clone();
                let calendar_data = calendar_data.clone();
                let error_message = error_message.clone();
                let month = focus_date_val.month;
                let year = focus_date_val.year;
                
                gloo::console::log!("🔄 Manual calendar refresh");
                
                spawn_local(async move {
                    match api_client.get_calendar_month(month, year).await {
                        Ok(data) => {
                            calendar_data.set(Some(data));
                            error_message.set(None);
                        }
                        Err(e) => {
                            gloo::console::error!("❌ Failed to fetch calendar data:", &e);
                            error_message.set(Some(format!("Failed to refresh calendar: {}", e)));
                        }
                    }
                });
            }
        })
    };

    let previous_month = {
        let api_client = api_client.clone();
        let focus_date = focus_date.clone();
        let error_message = error_message.clone();
        
        Callback::from(move |_| {
            let api_client = api_client.clone();
            let focus_date = focus_date.clone();
            let error_message = error_message.clone();
            
            gloo::console::log!("◀️ Previous month (backend API)");
            
            spawn_local(async move {
                match api_client.navigate_previous_month().await {
                    Ok(response) => {
                        gloo::console::log!("✅ Backend navigation successful:", response.success_message);
                        focus_date.set(Some(response.focus_date));
                        error_message.set(None);
                    }
                    Err(e) => {
                        gloo::console::error!("❌ Failed to navigate to previous month:", &e);
                        error_message.set(Some(format!("Navigation failed: {}", e)));
                    }
                }
            });
        })
    };

    let next_month = {
        let api_client = api_client.clone();
        let focus_date = focus_date.clone();
        let error_message = error_message.clone();
        
        Callback::from(move |_| {
            let api_client = api_client.clone();
            let focus_date = focus_date.clone();
            let error_message = error_message.clone();
            
            gloo::console::log!("▶️ Next month (backend API)");
            
            spawn_local(async move {
                match api_client.navigate_next_month().await {
                    Ok(response) => {
                        gloo::console::log!("✅ Backend navigation successful:", response.success_message);
                        focus_date.set(Some(response.focus_date));
                        error_message.set(None);
                    }
                    Err(e) => {
                        gloo::console::error!("❌ Failed to navigate to next month:", &e);
                        error_message.set(Some(format!("Navigation failed: {}", e)));
                    }
                }
            });
        })
    };

    let state = CalendarState {
        current_month: focus_date.as_ref().map(|f| f.month).unwrap_or(1),
        current_year: focus_date.as_ref().map(|f| f.year).unwrap_or(2025),
        calendar_data: (*calendar_data).clone(),
        current_date: (*current_date).clone(),
        loading: *loading,
        error_message: (*error_message).clone(),
    };

    let actions = UseCalendarActions {
        refresh_calendar,
        previous_month,
        next_month,
    };

    UseCalendarResult { state, actions }
} 