use yew::prelude::*;
use web_sys::{HtmlInputElement, MouseEvent};
use wasm_bindgen_futures::spawn_local;
use crate::services::api::ApiClient;
use shared::{Child, ExportToPathRequest};

#[derive(Properties, PartialEq)]
pub struct ExportModalProps {
    pub is_open: bool,
    pub on_success: Callback<()>,
    pub on_close: Callback<()>,
    pub api_client: ApiClient,
    pub active_child: Option<Child>,
}

#[function_component(ExportModal)]
pub fn export_modal(props: &ExportModalProps) -> Html {
    let custom_path = use_state(|| String::new());
    let is_exporting = use_state(|| false);
    let error_message = use_state(|| Option::<String>::None);
    let success_message = use_state(|| Option::<String>::None);

    // Reset state when modal opens
    use_effect_with(props.is_open, {
        let custom_path = custom_path.clone();
        let error_message = error_message.clone();
        let success_message = success_message.clone();
        
        move |is_open| {
            if *is_open {
                custom_path.set(String::new());
                error_message.set(None);
                success_message.set(None);
            }
            || ()
        }
    });

    let on_path_change = {
        let custom_path = custom_path.clone();
        Callback::from(move |e: Event| {
            let input: HtmlInputElement = e.target_unchecked_into();
            custom_path.set(input.value());
        })
    };

    let on_export = {
        let api_client = props.api_client.clone();
        let active_child = props.active_child.clone();
        let custom_path = custom_path.clone();
        let is_exporting = is_exporting.clone();
        let error_message = error_message.clone();
        let success_message = success_message.clone();
        let on_success = props.on_success.clone();
        
        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            
            // Check if we have an active child
            if active_child.is_none() {
                error_message.set(Some("No active child selected for export".to_string()));
                return;
            }
            
            // Set loading state
            is_exporting.set(true);
            error_message.set(None);
            success_message.set(None);
            
            let api_client = api_client.clone();
            let custom_path_value = (*custom_path).clone();
            let is_exporting = is_exporting.clone();
            let error_message = error_message.clone();
            let success_message = success_message.clone();
            let on_success = on_success.clone();
            
            spawn_local(async move {
                crate::services::logging::Logger::debug_with_component("export-modal", "Starting export to path...");
                
                // Create export request with optional custom path
                let export_request = ExportToPathRequest {
                    child_id: None, // Use active child
                    custom_path: if custom_path_value.trim().is_empty() { 
                        None 
                    } else { 
                        Some(custom_path_value.trim().to_string()) 
                    },
                };
                
                match api_client.export_to_path(export_request).await {
                    Ok(response) => {
                        if response.success {
                            crate::services::logging::Logger::debug_with_component("export-modal", &format!(
                                "Successfully exported {} transactions for {} to: {}",
                                response.transaction_count,
                                response.child_name,
                                response.file_path
                            ));
                            
                            success_message.set(Some(format!(
                                "Successfully exported {} transactions to {}",
                                response.transaction_count,
                                response.file_path
                            )));
                            
                            // Close modal after a brief delay
                            gloo::timers::callback::Timeout::new(2000, move || {
                                on_success.emit(());
                            }).forget();
                        } else {
                            crate::services::logging::Logger::debug_with_component("export-modal", &format!("Export failed: {}", response.message));
                            error_message.set(Some(response.message));
                        }
                    }
                    Err(e) => {
                        crate::services::logging::Logger::debug_with_component("export-modal", &format!("Export API failed: {}", e));
                        error_message.set(Some(format!("Export failed: {}", e)));
                    }
                }
                
                is_exporting.set(false);
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
        <div class="allowance-config-modal-backdrop" onclick={on_backdrop_click}>
            <div class="allowance-config-modal" onclick={on_modal_click}>
                <div class="allowance-config-modal-content">
                    <h3 class="allowance-config-title">
                        <svg width="24" height="24" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                            <path d="M14 2H6c-1.1 0-1.99.9-1.99 2L4 20c0 1.1.89 2 2 2h10c1.1 0 2-.9 2-2V8l-6-6zm4 18H6V4h7v5h5v11z" fill="currentColor"/>
                        </svg>
                        {" Export Transaction Data"}
                    </h3>
                    
                    {if let Some(error) = (*error_message).clone() {
                        html! {
                            <div class="allowance-config-error">
                                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                                    <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10s10-4.48 10-10S17.52 2 12 2zm1 15h-2v-2h2v2zm0-4h-2V7h2v6z" fill="currentColor"/>
                                </svg>
                                {error}
                            </div>
                        }
                    } else {
                        html! {}
                    }}
                    
                    {if let Some(success) = (*success_message).clone() {
                        html! {
                            <div class="allowance-config-success">
                                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                                    <path d="M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41z" fill="currentColor"/>
                                </svg>
                                {success}
                            </div>
                        }
                    } else {
                        html! {}
                    }}
                    
                    <form class="allowance-config-form" onsubmit={on_export}>
                        <div class="export-info">
                            <p>{"Export transaction data as a CSV file. By default, files are saved to your Documents folder."}</p>
                        </div>
                        
                        <div class="form-group">
                            <label for="export-path" class="form-label">
                                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                                    <path d="M10 4H4c-1.11 0-2 .89-2 2v12c0 1.11.89 2 2 2h16c1.11 0 2-.89 2-2V8c0-1.11-.89-2-2-2h-8l-2-2z" fill="currentColor"/>
                                </svg>
                                {"Custom Directory (optional)"}
                            </label>
                            <input 
                                id="export-path"
                                type="text" 
                                class="allowance-config-input"
                                placeholder="Leave empty to use Documents folder"
                                value={(*custom_path).clone()}
                                onchange={on_path_change}
                                disabled={*is_exporting}
                                autofocus=true
                            />
                            <small class="form-text">{"Specify a custom directory path, or leave empty to use your Documents folder"}</small>
                        </div>
                        
                        <div class="export-examples">
                            <div class="examples-title">{"Example paths:"}</div>
                            <div class="example-paths">
                                <code>{"/Users/yourname/Desktop"}</code>
                                <code>{"~/Desktop"}</code>
                                <code>{"C:\\Users\\yourname\\Desktop"}</code>
                            </div>
                        </div>
                        
                        <div class="allowance-config-buttons">
                            <button 
                                type="submit" 
                                class="btn btn-primary"
                                disabled={*is_exporting}
                            >
                                {if *is_exporting {
                                    html! {
                                        <>
                                            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg" class="spinner">
                                                <circle cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4" fill="none" opacity="0.25"/>
                                                <path d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z" fill="currentColor"/>
                                            </svg>
                                            {"Exporting..."}
                                        </>
                                    }
                                } else {
                                    html! {
                                        <>
                                            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                                                <path d="M19 9h-4V3H9v6H5l7 7 7-7zM5 18v2h14v-2H5z" fill="currentColor"/>
                                            </svg>
                                            {"Export CSV"}
                                        </>
                                    }
                                }}
                            </button>
                            <button 
                                type="button" 
                                class="btn btn-secondary"
                                onclick={on_cancel}
                                disabled={*is_exporting}
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