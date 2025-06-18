use yew::prelude::*;
use web_sys::{HtmlInputElement, HtmlSelectElement, MouseEvent};
use wasm_bindgen_futures::spawn_local;
use crate::services::api::ApiClient;
use shared::{AllowanceConfig, GetAllowanceConfigRequest, UpdateAllowanceConfigRequest};

#[derive(Properties, PartialEq)]
pub struct AllowanceConfigModalProps {
    pub is_open: bool,
    pub on_success: Callback<()>,
    pub on_close: Callback<()>,
}

#[function_component(AllowanceConfigModal)]
pub fn allowance_config_modal(props: &AllowanceConfigModalProps) -> Html {
    let amount = use_state(|| 5.0); // Default to $5
    let day_of_week = use_state(|| 0u8); // Default to Sunday
    let is_active = use_state(|| true);
    let is_submitting = use_state(|| false);
    let is_loading = use_state(|| false);
    let error_message = use_state(|| Option::<String>::None);
    let success_message = use_state(|| Option::<String>::None);
    let existing_config = use_state(|| Option::<AllowanceConfig>::None);
    let api_client = ApiClient::new();

    // Load existing allowance config when modal opens
    use_effect_with(props.is_open, {
        let is_loading = is_loading.clone();
        let error_message = error_message.clone();
        let success_message = success_message.clone();
        let amount = amount.clone();
        let day_of_week = day_of_week.clone();
        let is_active = is_active.clone();
        let existing_config = existing_config.clone();
        let api_client = api_client.clone();
        
        move |is_open| {
            if *is_open {
                // Reset state
                error_message.set(None);
                success_message.set(None);
                is_loading.set(true);
                
                // Load existing config
                spawn_local(async move {
                    let request = GetAllowanceConfigRequest { child_id: None };
                    match api_client.get_allowance_config(request).await {
                        Ok(response) => {
                            if let Some(config) = response.allowance_config {
                                // Load existing values
                                amount.set(config.amount);
                                day_of_week.set(config.day_of_week);
                                is_active.set(config.is_active);
                                existing_config.set(Some(config));
                            } else {
                                // No existing config, use defaults
                                amount.set(5.0);
                                day_of_week.set(0);
                                is_active.set(true);
                                existing_config.set(None);
                            }
                            is_loading.set(false);
                        }
                        Err(e) => {
                            error_message.set(Some(format!("Failed to load allowance config: {}", e)));
                            is_loading.set(false);
                        }
                    }
                });
            }
            || ()
        }
    });

    let on_amount_change = {
        let amount = amount.clone();
        Callback::from(move |e: Event| {
            let input: HtmlInputElement = e.target_unchecked_into();
            if let Ok(value) = input.value().parse::<f64>() {
                amount.set(value);
            }
        })
    };

    let on_day_change = {
        let day_of_week = day_of_week.clone();
        Callback::from(move |e: Event| {
            let select: HtmlSelectElement = e.target_unchecked_into();
            if let Ok(value) = select.value().parse::<u8>() {
                day_of_week.set(value);
            }
        })
    };

    let on_active_change = {
        let is_active = is_active.clone();
        Callback::from(move |e: Event| {
            let input: HtmlInputElement = e.target_unchecked_into();
            is_active.set(input.checked());
        })
    };

    let on_submit = {
        let amount = amount.clone();
        let day_of_week = day_of_week.clone();
        let is_active = is_active.clone();
        let is_submitting = is_submitting.clone();
        let error_message = error_message.clone();
        let success_message = success_message.clone();
        let on_success = props.on_success.clone();
        let api_client = api_client.clone();
        
        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            
            let amount_value = *amount;
            let day_value = *day_of_week;
            let active_value = *is_active;
            
            // Basic validation
            if amount_value < 0.0 {
                error_message.set(Some("Amount must be non-negative".to_string()));
                return;
            }
            
            if amount_value > 1000000.0 {
                error_message.set(Some("Amount must be less than $1,000,000".to_string()));
                return;
            }
            
            if day_value > 6 {
                error_message.set(Some("Invalid day of week".to_string()));
                return;
            }
            
            // Set loading state
            is_submitting.set(true);
            error_message.set(None);
            success_message.set(None);
            
            // Make API call
            let amount = amount.clone();
            let day_of_week = day_of_week.clone();
            let is_active = is_active.clone();
            let is_submitting = is_submitting.clone();
            let error_message = error_message.clone();
            let success_message = success_message.clone();
            let on_success = on_success.clone();
            let api_client = api_client.clone();
            
            spawn_local(async move {
                let request = UpdateAllowanceConfigRequest {
                    child_id: None, // Use active child
                    amount: amount_value,
                    day_of_week: day_value,
                    is_active: active_value,
                };
                
                match api_client.update_allowance_config(request).await {
                    Ok(response) => {
                        // Success!
                        is_submitting.set(false);
                        success_message.set(Some(response.success_message));
                        
                        // Close modal after a brief delay
                        gloo::timers::callback::Timeout::new(1500, move || {
                            on_success.emit(());
                        }).forget();
                    }
                    Err(e) => {
                        is_submitting.set(false);
                        error_message.set(Some(format!("Failed to save allowance config: {}", e)));
                    }
                }
            });
        })
    };

    let on_backdrop_click = {
        let on_close = props.on_close.clone();
        Callback::from(move |e: MouseEvent| {
            e.stop_propagation();
            on_close.emit(());
        })
    };

    let on_modal_click = Callback::from(|e: MouseEvent| {
        e.stop_propagation();
    });

    let on_cancel = {
        let on_close = props.on_close.clone();
        Callback::from(move |_: MouseEvent| {
            on_close.emit(());
        })
    };

    if !props.is_open {
        return html! {};
    }

    let day_names = [
        (0, "Sunday"),
        (1, "Monday"),
        (2, "Tuesday"),
        (3, "Wednesday"),
        (4, "Thursday"),
        (5, "Friday"),
        (6, "Saturday"),
    ];

    html! {
        <div class="allowance-config-modal-backdrop" onclick={on_backdrop_click}>
            <div class="allowance-config-modal" onclick={on_modal_click}>
                <div class="allowance-config-modal-content">
                    <h3 class="allowance-config-title">{"💰 Configure Weekly Allowance"}</h3>
                    
                    if *is_loading {
                        <div class="allowance-config-loading">
                            {"Loading current settings..."}
                        </div>
                    } else {
                        <>
                            {if let Some(error) = (*error_message).clone() {
                                html! {
                                    <div class="allowance-config-error">
                                        {error}
                                    </div>
                                }
                            } else {
                                html! {}
                            }}
                            
                            {if let Some(success) = (*success_message).clone() {
                                html! {
                                    <div class="allowance-config-success">
                                        {success}
                                    </div>
                                }
                            } else {
                                html! {}
                            }}
                            
                            <form class="allowance-config-form" onsubmit={on_submit}>
                                <div class="form-group">
                                    <label for="allowance-amount">{"Weekly Allowance Amount"}</label>
                                    <div class="input-group">
                                        <span class="input-group-text">{"$"}</span>
                                        <input
                                            id="allowance-amount"
                                            type="number"
                                            class="allowance-config-input"
                                            placeholder="5.00"
                                            value={format!("{:.2}", *amount)}
                                            onchange={on_amount_change}
                                            disabled={*is_submitting}
                                            min="0"
                                            max="1000000"
                                            step="0.01"
                                            autofocus=true
                                        />
                                    </div>
                                    <small class="form-text">{"Amount to give each week (between $0 and $1,000,000)"}</small>
                                </div>
                                
                                <div class="form-group">
                                    <label for="allowance-day">{"Day of Week"}</label>
                                    <select
                                        id="allowance-day"
                                        class="allowance-config-select"
                                        value={(*day_of_week).to_string()}
                                        onchange={on_day_change}
                                        disabled={*is_submitting}
                                    >
                                        {for day_names.iter().map(|(value, name)| {
                                            html! {
                                                <option value={value.to_string()} selected={*value == *day_of_week}>
                                                    {name}
                                                </option>
                                            }
                                        })}
                                    </select>
                                    <small class="form-text">{"Day of the week to give the allowance"}</small>
                                </div>
                                
                                <div class="form-group">
                                    <label class="checkbox-label">
                                        <input
                                            type="checkbox"
                                            checked={*is_active}
                                            onchange={on_active_change}
                                            disabled={*is_submitting}
                                        />
                                        <span class="checkmark"></span>
                                        {"Enable weekly allowance"}
                                    </label>
                                    <small class="form-text">{"Uncheck to temporarily disable the allowance"}</small>
                                </div>
                                
                                <div class="allowance-config-buttons">
                                    <button 
                                        type="submit" 
                                        class="btn btn-primary"
                                        disabled={*is_submitting || *is_loading}
                                    >
                                        {if *is_submitting {
                                            "Saving..."
                                        } else if existing_config.is_some() {
                                            "Update Allowance"
                                        } else {
                                            "Create Allowance"
                                        }}
                                    </button>
                                    <button 
                                        type="button" 
                                        class="btn btn-secondary"
                                        onclick={on_cancel}
                                        disabled={*is_submitting}
                                    >
                                        {"Cancel"}
                                    </button>
                                </div>
                            </form>
                        </>
                    }
                </div>
            </div>
        </div>
    }
} 