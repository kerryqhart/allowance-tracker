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

    // Render loading state
    if *is_loading {
        return html! {
            <div style="background: linear-gradient(135deg, #667eea 0%, #764ba2 100%); color: white; padding: 1rem; border-radius: 8px; margin-bottom: 1rem;">
                <h3 style="margin: 0 0 1rem 0; text-align: center;">{"🆕 Simple Calendar Test (Backend State)"}</h3>
                <div style="text-align: center; padding: 2rem;">
                    {"Loading calendar state from backend..."}
                </div>
            </div>
        };
    }

    // Render error state
    if let Some(error) = error_message.as_ref() {
        return html! {
            <div style="background: linear-gradient(135deg, #dc2626 0%, #b91c1c 100%); color: white; padding: 1rem; border-radius: 8px; margin-bottom: 1rem;">
                <h3 style="margin: 0 0 1rem 0; text-align: center;">{"🆕 Simple Calendar Test (Backend State)"}</h3>
                <div style="text-align: center; padding: 1rem;">
                    <div style="font-weight: bold; margin-bottom: 0.5rem;">{"Error:"}</div>
                    <div>{error}</div>
                </div>
            </div>
        };
    }

    // Render calendar with backend state
    match calendar_state.as_ref() {
        Some(focus_date) => {
            html! {
                <div style="background: linear-gradient(135deg, #667eea 0%, #764ba2 100%); color: white; padding: 1rem; border-radius: 8px; margin-bottom: 1rem;">
                    <h3 style="margin: 0 0 1rem 0; text-align: center;">{"🆕 Simple Calendar Test (Backend State)"}</h3>
                    
                    <div style="display: flex; align-items: center; justify-content: center; gap: 1rem;">
                        <button 
                            onclick={on_previous}
                            style="background: rgba(255,255,255,0.2); border: 1px solid rgba(255,255,255,0.3); color: white; border-radius: 50%; width: 40px; height: 40px; cursor: pointer; display: flex; align-items: center; justify-content: center; font-size: 1.2rem;"
                            title="Previous Month"
                        >
                            {"◀"}
                        </button>
                        
                        <div style="font-size: 1.5rem; font-weight: bold; min-width: 200px; text-align: center;">
                            {format!("{} {}", month_name(focus_date.month), focus_date.year)}
                        </div>
                        
                        <button 
                            onclick={on_next}
                            style="background: rgba(255,255,255,0.2); border: 1px solid rgba(255,255,255,0.3); color: white; border-radius: 50%; width: 40px; height: 40px; cursor: pointer; display: flex; align-items: center; justify-content: center; font-size: 1.2rem;"
                            title="Next Month"
                        >
                            {"▶"}
                        </button>
                    </div>
                    
                    <div style="text-align: center; margin-top: 0.5rem; font-size: 0.9rem; opacity: 0.8;">
                        {format!("Debug: {} total clicks | Backend: {}/{}", *click_count, focus_date.month, focus_date.year)}
                    </div>
                </div>
            }
        }
        None => {
            html! {
                <div style="background: linear-gradient(135deg, #f59e0b 0%, #d97706 100%); color: white; padding: 1rem; border-radius: 8px; margin-bottom: 1rem;">
                    <h3 style="margin: 0 0 1rem 0; text-align: center;">{"🆕 Simple Calendar Test (Backend State)"}</h3>
                    <div style="text-align: center; padding: 1rem;">
                        {"No calendar state available"}
                    </div>
                </div>
            }
        }
    }
} 