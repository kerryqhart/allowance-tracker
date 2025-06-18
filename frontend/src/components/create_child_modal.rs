use yew::prelude::*;
use web_sys::{HtmlInputElement, MouseEvent};
use wasm_bindgen_futures::spawn_local;
use crate::services::api::ApiClient;
use shared::CreateChildRequest;

#[derive(Properties, PartialEq)]
pub struct CreateChildModalProps {
    pub is_open: bool,
    pub on_success: Callback<()>,
    pub on_close: Callback<()>,
}

#[function_component(CreateChildModal)]
pub fn create_child_modal(props: &CreateChildModalProps) -> Html {
    let child_name = use_state(|| String::new());
    let child_birthdate = use_state(|| String::new());
    let is_submitting = use_state(|| false);
    let error_message = use_state(|| Option::<String>::None);
    let api_client = ApiClient::new();

    // Reset state when modal opens
    use_effect_with(props.is_open, {
        let child_name = child_name.clone();
        let child_birthdate = child_birthdate.clone();
        let is_submitting = is_submitting.clone();
        let error_message = error_message.clone();
        move |is_open| {
            if *is_open {
                child_name.set(String::new());
                child_birthdate.set(String::new());
                is_submitting.set(false);
                error_message.set(None);
            }
            || ()
        }
    });

    let on_name_change = {
        let child_name = child_name.clone();
        Callback::from(move |e: Event| {
            let input: HtmlInputElement = e.target_unchecked_into();
            child_name.set(input.value());
        })
    };

    let on_birthdate_change = {
        let child_birthdate = child_birthdate.clone();
        Callback::from(move |e: Event| {
            let input: HtmlInputElement = e.target_unchecked_into();
            child_birthdate.set(input.value());
        })
    };

    let on_submit = {
        let child_name = child_name.clone();
        let child_birthdate = child_birthdate.clone();
        let is_submitting = is_submitting.clone();
        let error_message = error_message.clone();
        let on_success = props.on_success.clone();
        let api_client = api_client.clone();
        
        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            
            let name = (*child_name).clone().trim().to_string();
            let birthdate = (*child_birthdate).clone().trim().to_string();
            
            // Basic validation
            if name.is_empty() {
                error_message.set(Some("Please enter the child's name".to_string()));
                return;
            }
            
            if birthdate.is_empty() {
                error_message.set(Some("Please enter the child's birthdate".to_string()));
                return;
            }
            
            // Validate date format (YYYY-MM-DD)
            if !birthdate.contains('-') || birthdate.len() != 10 {
                error_message.set(Some("Please enter birthdate in YYYY-MM-DD format".to_string()));
                return;
            }
            
            // Set loading state
            is_submitting.set(true);
            error_message.set(None);
            
            // Make API call
            let child_name = child_name.clone();
            let child_birthdate = child_birthdate.clone();
            let is_submitting = is_submitting.clone();
            let error_message = error_message.clone();
            let on_success = on_success.clone();
            let api_client = api_client.clone();
            
            spawn_local(async move {
                let request = CreateChildRequest {
                    name,
                    birthdate,
                };
                
                match api_client.create_child(request).await {
                    Ok(_response) => {
                        // Success! Clear form and close modal
                        child_name.set(String::new());
                        child_birthdate.set(String::new());
                        is_submitting.set(false);
                        on_success.emit(());
                    }
                    Err(e) => {
                        is_submitting.set(false);
                        error_message.set(Some(format!("Failed to create child: {}", e)));
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

    html! {
        <div class="create-child-modal-backdrop" onclick={on_backdrop_click}>
            <div class="create-child-modal" onclick={on_modal_click}>
                <div class="create-child-modal-content">
                    <h3 class="create-child-title">{"👶 Create Child Profile"}</h3>
                    
                    {if let Some(error) = (*error_message).clone() {
                        html! {
                            <div class="create-child-error">
                                {error}
                            </div>
                        }
                    } else {
                        html! {}
                    }}
                    
                    <form class="create-child-form" onsubmit={on_submit}>
                        <div class="form-group">
                            <label for="child-name">{"Child's Name"}</label>
                            <input
                                id="child-name"
                                type="text"
                                class="create-child-input"
                                placeholder="Enter child's name"
                                value={(*child_name).clone()}
                                onchange={on_name_change}
                                disabled={*is_submitting}
                                autofocus=true
                            />
                        </div>
                        
                        <div class="form-group">
                            <label for="child-birthdate">{"Birthdate"}</label>
                            <input
                                id="child-birthdate"
                                type="date"
                                class="create-child-input"
                                value={(*child_birthdate).clone()}
                                onchange={on_birthdate_change}
                                disabled={*is_submitting}
                            />
                        </div>
                        
                        <div class="create-child-buttons">
                            <button 
                                type="submit" 
                                class="btn btn-primary"
                                disabled={*is_submitting}
                            >
                                {if *is_submitting {
                                    "Creating..."
                                } else {
                                    "Create Child"
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
                </div>
            </div>
        </div>
    }
} 