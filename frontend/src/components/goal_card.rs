use yew::prelude::*;
use wasm_bindgen_futures::spawn_local;
use web_sys::HtmlInputElement;
use shared::{CreateGoalRequest, Goal, GoalCalculation};
use crate::services::{api::ApiClient, logging::Logger};

/// Format an ISO date string to a kid-friendly format like "June 27, 2025"
fn format_kid_friendly_date(iso_date: &str) -> String {
    // Parse the ISO date (e.g., "2025-06-27T12:00:00.000Z")
    if let Some(date_part) = iso_date.split('T').next() {
        if let Ok(parts) = date_part.split('-').collect::<Vec<_>>().try_into() as Result<[&str; 3], _> {
            let [year, month, day] = parts;
            if let (Ok(year_num), Ok(month_num), Ok(day_num)) = (year.parse::<u32>(), month.parse::<u32>(), day.parse::<u32>()) {
                let month_name = match month_num {
                    1 => "January",
                    2 => "February", 
                    3 => "March",
                    4 => "April",
                    5 => "May",
                    6 => "June",
                    7 => "July",
                    8 => "August",
                    9 => "September",
                    10 => "October",
                    11 => "November",
                    12 => "December",
                    _ => return iso_date.to_string(), // fallback
                };
                return format!("{} {}, {}", month_name, day_num, year_num);
            }
        }
    }
    // Fallback to original if parsing fails
    iso_date.to_string()
}

#[derive(Properties, PartialEq)]
pub struct GoalCardProps {
    pub api_client: ApiClient,
    pub on_refresh: Callback<()>,
}

#[function_component(GoalCard)]
pub fn goal_card(props: &GoalCardProps) -> Html {
            // Logger::debug_with_component("goal-card", "🎯 Goal card render function called");
    
    let current_goal = use_state(|| Option::<Goal>::None);
    let goal_calculation = use_state(|| Option::<GoalCalculation>::None);
    let loading = use_state(|| false);
    let error_message = use_state(|| Option::<String>::None);
    let success_message = use_state(|| Option::<String>::None);
    
    // Form state for creating new goal
    let goal_description = use_state(|| String::new());
    let goal_amount = use_state(|| String::new());
    let creating_goal = use_state(|| false);

    // Load current goal on component mount
    let load_goal = {
        let api_client = props.api_client.clone();
        let current_goal = current_goal.clone();
        let goal_calculation = goal_calculation.clone();
        let loading = loading.clone();
        let error_message = error_message.clone();
        
        Callback::from(move |_: ()| {
            let api_client = api_client.clone();
            let current_goal = current_goal.clone();
            let goal_calculation = goal_calculation.clone();
            let loading = loading.clone();
            let error_message = error_message.clone();
            
            spawn_local(async move {
                loading.set(true);
                error_message.set(None);
                
                match api_client.get_current_goal().await {
                    Ok(response) => {
                        Logger::info_with_component("goal-card", &format!("🎯 Goal API response: goal={:?}, calculation={:?}", response.goal.is_some(), response.calculation.is_some()));
                        current_goal.set(response.goal.clone());
                        goal_calculation.set(response.calculation);
                        
                        // Debug logging for state transitions
                        if response.goal.is_none() {
                            Logger::info_with_component("goal-card", "🎯 No current goal found - should show create goal form");
                        } else {
                            Logger::info_with_component("goal-card", "🎯 Current goal found - showing goal details");
                        }
                    }
                    Err(e) => {
                        Logger::error_with_component("goal-card", &format!("Failed to load goal: {}", e));
                        error_message.set(Some(format!("Failed to load goal: {}", e)));
                    }
                }
                
                loading.set(false);
            });
        })
    };

    // Load goal on component mount
    use_effect_with((), {
        let load_goal = load_goal.clone();
        move |_| {
            Logger::info_with_component("goal-card", "🎯 Goal card component mounted and loading goal data");
            load_goal.emit(());
            || ()
        }
    });

    let on_description_change = {
        let goal_description = goal_description.clone();
        Callback::from(move |e: InputEvent| {
            let input: HtmlInputElement = e.target_unchecked_into();
            goal_description.set(input.value());
        })
    };

    let on_amount_change = {
        let goal_amount = goal_amount.clone();
        Callback::from(move |e: InputEvent| {
            let input: HtmlInputElement = e.target_unchecked_into();
            goal_amount.set(input.value());
        })
    };

    let on_create_goal = {
        let api_client = props.api_client.clone();
        let goal_description = goal_description.clone();
        let goal_amount = goal_amount.clone();
        let creating_goal = creating_goal.clone();
        let error_message = error_message.clone();
        let success_message = success_message.clone();
        let load_goal = load_goal.clone();
        let on_refresh = props.on_refresh.clone();
        
        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            
            let description = (*goal_description).clone();
            let amount_str = (*goal_amount).clone();
            
            if description.trim().is_empty() {
                error_message.set(Some("Goal description is required".to_string()));
                return;
            }
            
            let target_amount = match amount_str.parse::<f64>() {
                Ok(amount) if amount > 0.0 => amount,
                _ => {
                    error_message.set(Some("Please enter a valid positive amount".to_string()));
                    return;
                }
            };
            
            let api_client = api_client.clone();
            let creating_goal = creating_goal.clone();
            let error_message = error_message.clone();
            let success_message = success_message.clone();
            let goal_description = goal_description.clone();
            let goal_amount = goal_amount.clone();
            let load_goal = load_goal.clone();
            let on_refresh = on_refresh.clone();
            
            spawn_local(async move {
                creating_goal.set(true);
                error_message.set(None);
                success_message.set(None);
                
                let request = CreateGoalRequest {
                    child_id: None, // Use active child
                    description,
                    target_amount,
                };
                
                match api_client.create_goal(request).await {
                    Ok(response) => {
                        Logger::info_with_component("goal-card", &format!("Goal created: {}", response.success_message));
                        success_message.set(Some(response.success_message));
                        goal_description.set(String::new());
                        goal_amount.set(String::new());
                        load_goal.emit(());
                        on_refresh.emit(());
                    }
                    Err(e) => {
                        Logger::error_with_component("goal-card", &format!("Failed to create goal: {}", e));
                        error_message.set(Some(e));
                    }
                }
                
                creating_goal.set(false);
            });
        })
    };

    let on_cancel_goal = {
        let api_client = props.api_client.clone();
        let loading = loading.clone();
        let error_message = error_message.clone();
        let success_message = success_message.clone();
        let load_goal = load_goal.clone();
        let on_refresh = props.on_refresh.clone();
        
        Callback::from(move |_: MouseEvent| {
            Logger::info_with_component("goal-card", "🎯 CANCEL: Cancel Goal button clicked - starting cancellation process");
            
            let api_client = api_client.clone();
            let loading = loading.clone();
            let error_message = error_message.clone();
            let success_message = success_message.clone();
            let load_goal = load_goal.clone();
            let on_refresh = on_refresh.clone();
            
            spawn_local(async move {
                loading.set(true);
                error_message.set(None);
                success_message.set(None);
                
                Logger::info_with_component("goal-card", "🎯 CANCEL: Making API call to cancel goal");
                match api_client.cancel_goal().await {
                    Ok(response) => {
                        Logger::info_with_component("goal-card", &format!("🎯 CANCEL: API success - {}", response.success_message));
                        success_message.set(Some(response.success_message));
                        
                        // Reload goal data to trigger state transition
                        Logger::info_with_component("goal-card", "🎯 CANCEL: Reloading goal data after cancellation");
                        load_goal.emit(());
                        on_refresh.emit(());
                        
                        // Clear success message after 3 seconds to show the create goal form cleanly
                        let success_message_clear = success_message.clone();
                        spawn_local(async move {
                            gloo::timers::future::TimeoutFuture::new(3000).await;
                            success_message_clear.set(None);
                        });
                    }
                    Err(e) => {
                        Logger::error_with_component("goal-card", &format!("🎯 CANCEL: API error - {}", e));
                        error_message.set(Some(e));
                    }
                }
                
                loading.set(false);
            });
        })
    };

    html! {
        <section class="goal-section">
            <h2>{"My Savings Goal"}</h2>
            
            {if let Some(error) = error_message.as_ref() {
                html! {
                    <div class="form-message error">
                        {error}
                    </div>
                }
            } else { html! {} }}
            
            {if let Some(success) = success_message.as_ref() {
                html! {
                    <div class="form-message success">
                        {success}
                    </div>
                }
            } else { html! {} }}
            
            {if *loading {
                html! {
                    <div class="goal-loading">
                        {"Loading goal..."}
                    </div>
                }
            } else if let Some(goal) = current_goal.as_ref() {
                // Show current goal
                // Logger::debug_with_component("goal-card", "🎯 RENDER: Showing current goal display");
                html! {
                    <div class="current-goal">
                        <div class="goal-header">
                            <h3 class="goal-title">{&goal.description}</h3>
                            <div class="goal-target">
                                <span class="goal-amount">{format!("${:.2}", goal.target_amount)}</span>
                            </div>
                        </div>
                        
                        {if let Some(calc) = goal_calculation.as_ref() {
                            html! {
                                <div class="goal-progress">
                                    <div class="progress-info">
                                        <div class="progress-item">
                                            <span class="progress-label">{"Current Balance:"}</span>
                                            <span class="progress-value positive">{format!("${:.2}", calc.current_balance)}</span>
                                        </div>
                                        <div class="progress-item">
                                            <span class="progress-label">{"Amount Needed:"}</span>
                                            <span class="progress-value">{format!("${:.2}", calc.amount_needed)}</span>
                                        </div>
                                        {if calc.is_achievable {
                                            html! {
                                                <>
                                                    <div class="progress-item">
                                                        <span class="progress-label">{"Allowances Needed:"}</span>
                                                        <span class="progress-value">{calc.allowances_needed}</span>
                                                    </div>
                                                    {if let Some(completion_date) = &calc.projected_completion_date {
                                                        html! {
                                                            <div class="progress-item">
                                                                <span class="progress-label">{"Projected Date:"}</span>
                                                                <span class="progress-value completion-date">{format_kid_friendly_date(completion_date)}</span>
                                                            </div>
                                                        }
                                                    } else { html! {} }}
                                                </>
                                            }
                                        } else {
                                            html! {
                                                <div class="progress-item">
                                                    <span class="progress-label warning">{"Not achievable with current allowance"}</span>
                                                </div>
                                            }
                                        }}
                                    </div>
                                    
                                    <div class="progress-bar-container">
                                        <div class="progress-bar">
                                            <div 
                                                class="progress-fill"
                                                style={format!("width: {}%", 
                                                    (calc.current_balance / goal.target_amount * 100.0).min(100.0)
                                                )}
                                            ></div>
                                        </div>
                                        <div class="progress-text">
                                            {format!("{:.1}% complete", 
                                                (calc.current_balance / goal.target_amount * 100.0).min(100.0)
                                            )}
                                        </div>
                                    </div>
                                </div>
                            }
                        } else { html! {} }}
                        
                        <div class="goal-actions">
                            <button 
                                class="btn btn-secondary goal-cancel-btn" 
                                onclick={on_cancel_goal}
                                disabled={*loading}
                            >
                                {"Cancel Goal"}
                            </button>
                        </div>
                    </div>
                }
            } else {
                // Show create goal form
                // Logger::debug_with_component("goal-card", "🎯 RENDER: Showing create goal form");
                html! {
                    <form class="goal-form" onsubmit={on_create_goal}>
                        <div class="form-fields-group">
                            <div class="form-group">
                                <label for="goal-description">{"What are you saving for?"}</label>
                                <input 
                                    type="text" 
                                    id="goal-description"
                                    placeholder="e.g., New bike, Video game, Toy..."
                                    value={(*goal_description).clone()}
                                    oninput={on_description_change}
                                    disabled={*creating_goal}
                                />
                            </div>
                            
                            <div class="form-group">
                                <label for="goal-amount">{"How much do you need?"}</label>
                                <div class="amount-input-wrapper">
                                    <input 
                                        type="number" 
                                        id="goal-amount"
                                        placeholder="25.00"
                                        step="0.01"
                                        min="0.01"
                                        value={(*goal_amount).clone()}
                                        oninput={on_amount_change}
                                        disabled={*creating_goal}
                                    />
                                </div>
                            </div>
                        </div>
                        
                        <div class="form-button-group">
                            <button 
                                type="submit" 
                                class="btn btn-primary goal-create-btn"
                                disabled={*creating_goal}
                            >
                                {if *creating_goal {
                                    "Creating Goal..."
                                } else {
                                    "⊕ Create Goal"
                                }}
                            </button>
                        </div>
                    </form>
                }
            }}
        </section>
    }
} 