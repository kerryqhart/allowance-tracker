use yew::prelude::*;
use web_sys::{HtmlInputElement, MouseEvent};
use wasm_bindgen_futures::spawn_local;
use crate::services::{api::ApiClient, logging::Logger};
use shared::Child;

#[derive(Clone, PartialEq)]
pub enum DataDirectoryModalState {
    Loading,
    Ready { current_path: String },
    Processing,
    Success { message: String },
    Error { message: String },
}

#[derive(Properties, PartialEq)]
pub struct DataDirectoryModalProps {
    pub is_open: bool,
    pub on_close: Callback<()>,
    pub api_client: ApiClient,
    pub active_child: Option<Child>,
}

#[function_component(DataDirectoryModal)]
pub fn data_directory_modal(props: &DataDirectoryModalProps) -> Html {
    let state = use_state(|| DataDirectoryModalState::Loading);
    let new_path = use_state(|| String::new());
    let error_message = use_state(|| Option::<String>::None);

    // Load current directory when modal opens
    {
        let state = state.clone();
        let api_client = props.api_client.clone();
        let active_child = props.active_child.clone();
        let is_open = props.is_open;
        
        use_effect_with(is_open, move |&is_open| {
            if is_open {
                let state = state.clone();
                let api_client = api_client.clone();
                let child_id = active_child.as_ref().map(|child| child.id.clone());
                
                spawn_local(async move {
                    Logger::debug_with_component("DataDirectoryModal", &format!("Loading current data directory for child_id: {:?}", child_id));
                    match api_client.get_current_data_directory(child_id).await {
                        Ok(response) => {
                            state.set(DataDirectoryModalState::Ready {
                                current_path: response.current_path,
                            });
                        }
                        Err(error) => {
                            Logger::debug_with_component("DataDirectoryModal", &format!("Failed to load current directory: {}", error));
                            state.set(DataDirectoryModalState::Error {
                                message: format!("Failed to load current directory: {}", error),
                            });
                        }
                    }
                });
            }
        });
    }

    // Check if a child is selected
    let child_name = props.active_child.as_ref().map(|child| child.name.clone()).unwrap_or_else(|| "No child selected".to_string());
    let has_active_child = props.active_child.is_some();

    // Reset form when modal opens
    use_effect_with(props.is_open, {
        let new_path = new_path.clone();
        let error_message = error_message.clone();
        
        move |is_open| {
            if *is_open {
                new_path.set(String::new());
                error_message.set(None);
            }
            || ()
        }
    });

    // Check if the current path is redirected (not in default location)
    let is_redirected = {
        let state = (*state).clone();
        let active_child = props.active_child.as_ref();
        
        match (state, active_child) {
            (DataDirectoryModalState::Ready { current_path }, Some(child)) => {
                // Default path would be ~/Documents/Allowance Tracker/{child_name}
                let default_base = if cfg!(target_os = "macos") || cfg!(target_os = "linux") {
                    format!("{}/Documents/Allowance Tracker", std::env::var("HOME").unwrap_or_default())
                } else {
                    format!("{}\\Documents\\Allowance Tracker", std::env::var("USERPROFILE").unwrap_or_default())
                };
                let default_path = format!("{}/{}", default_base, child.name);
                current_path != default_path
            },
            _ => false,
        }
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

    let on_close_click = {
        let on_close = props.on_close.clone();
        let state = state.clone();
        Callback::from(move |_: MouseEvent| {
            Logger::debug_with_component("DataDirectoryModal", "Closing data directory modal");
            state.set(DataDirectoryModalState::Loading);
            on_close.emit(());
        })
    };

    let on_path_change = {
        let new_path = new_path.clone();
        Callback::from(move |e: Event| {
            let input: HtmlInputElement = e.target_unchecked_into();
            new_path.set(input.value());
        })
    };

    let on_relocate = {
        let state = state.clone();
        let api_client = props.api_client.clone();
        let active_child = props.active_child.clone();
        let new_path = new_path.clone();
        let error_message = error_message.clone();
        let on_close = props.on_close.clone();
        
        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            
            if active_child.is_none() {
                error_message.set(Some("No active child selected".to_string()));
                return;
            }
            
            let new_path_value = (*new_path).trim().to_string();
            if new_path_value.is_empty() {
                error_message.set(Some("Please specify a new directory path".to_string()));
                return;
            }
            
            let state = state.clone();
            let api_client = api_client.clone();
            let child_id = active_child.as_ref().map(|child| child.id.clone());
            let child_name = active_child.as_ref().map(|child| child.name.clone()).unwrap_or_default();
            let error_message = error_message.clone();
            let on_close = on_close.clone();
            
            spawn_local(async move {
                Logger::debug_with_component("DataDirectoryModal", &format!("Starting data directory relocation for child '{}' to: {}", child_name, new_path_value));
                state.set(DataDirectoryModalState::Processing);
                error_message.set(None);
                
                match api_client.relocate_data_directory(child_id, new_path_value).await {
                    Ok(response) => {
                        if response.success {
                            Logger::debug_with_component("DataDirectoryModal", &format!("Relocation successful: {}", response.message));
                            state.set(DataDirectoryModalState::Success {
                                message: response.message,
                            });
                            
                            // Close modal after a brief delay
                            gloo::timers::callback::Timeout::new(2000, move || {
                                on_close.emit(());
                            }).forget();
                        } else {
                            Logger::debug_with_component("DataDirectoryModal", &format!("Relocation failed: {}", response.message));
                            state.set(DataDirectoryModalState::Error {
                                message: response.message,
                            });
                        }
                    }
                    Err(error) => {
                        Logger::debug_with_component("DataDirectoryModal", &format!("Relocation error: {}", error));
                        state.set(DataDirectoryModalState::Error {
                            message: format!("Failed to relocate directory: {}", error),
                        });
                    }
                }
            });
        })
    };

    let on_revert = {
        let state = state.clone();
        let api_client = props.api_client.clone();
        let active_child = props.active_child.clone();
        let error_message = error_message.clone();
        let on_close = props.on_close.clone();
        
        Callback::from(move |_: MouseEvent| {
            if active_child.is_none() {
                error_message.set(Some("No active child selected".to_string()));
                return;
            }
            
            let state = state.clone();
            let api_client = api_client.clone();
            let child_id = active_child.as_ref().map(|child| child.id.clone());
            let child_name = active_child.as_ref().map(|child| child.name.clone()).unwrap_or_default();
            let error_message = error_message.clone();
            let on_close = on_close.clone();
            
            spawn_local(async move {
                Logger::debug_with_component("DataDirectoryModal", &format!("Starting data directory revert for child '{}'", child_name));
                state.set(DataDirectoryModalState::Processing);
                error_message.set(None);
                
                match api_client.revert_data_directory(child_id).await {
                    Ok(response) => {
                        if response.success {
                            Logger::debug_with_component("DataDirectoryModal", &format!("Revert successful: {}", response.message));
                            state.set(DataDirectoryModalState::Success {
                                message: response.message,
                            });
                            
                            // Close modal after a brief delay
                            gloo::timers::callback::Timeout::new(2000, move || {
                                on_close.emit(());
                            }).forget();
                        } else {
                            Logger::debug_with_component("DataDirectoryModal", &format!("Revert failed: {}", response.message));
                            state.set(DataDirectoryModalState::Error {
                                message: response.message,
                            });
                        }
                    }
                    Err(error) => {
                        Logger::debug_with_component("DataDirectoryModal", &format!("Revert error: {}", error));
                        state.set(DataDirectoryModalState::Error {
                            message: format!("Failed to revert directory: {}", error),
                        });
                    }
                }
            });
        })
    };

    if !props.is_open {
        return html! {};
    }

    html! {
        <div class="data-directory-modal-backdrop" onclick={on_backdrop_click}>
            <div class="data-directory-modal" onclick={on_modal_click}>
                <div class="data-directory-modal-header">
                    <div class="data-directory-modal-title">
                        <svg width="20" height="20" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                            <path d="M10 4H4c-1.11 0-2 .89-2 2v12c0 1.11.89 2 2 2h16c1.11 0 2-.89 2-2V8c0-1.11-.89-2-2-2h-8l-2-2z" fill="currentColor"/>
                        </svg>
                        <span>{format!("Data Directory - {}", child_name)}</span>
                    </div>
                    <button class="modal-close-button" onclick={on_close_click.clone()}>
                        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                            <path d="M19 6.41L17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z" fill="currentColor"/>
                        </svg>
                    </button>
                </div>

                <div class="data-directory-modal-content">
                    {
                        if !has_active_child {
                            html! {
                                <div class="data-directory-error">
                                    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                                        <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10s10-4.48 10-10S17.52 2 12 2zm1 15h-2v-2h2v2zm0-4h-2V7h2v6z" fill="currentColor"/>
                                    </svg>
                                    <p>{"No active child selected. Please select a child first."}</p>
                                </div>
                            }
                        } else {
                            match (*state).clone() {
                                DataDirectoryModalState::Loading => html! {
                                    <div class="data-directory-loading">
                                        <div class="spinner"></div>
                                        <p>{"Loading current data directory..."}</p>
                                    </div>
                                },
                            DataDirectoryModalState::Ready { current_path } => html! {
                                <div class="data-directory-ready">
                                    {if let Some(error) = (*error_message).clone() {
                                        html! {
                                            <div class="data-directory-error">
                                                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                                                    <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10s10-4.48 10-10S17.52 2 12 2zm1 15h-2v-2h2v2zm0-4h-2V7h2v6z" fill="currentColor"/>
                                                </svg>
                                                {error}
                                            </div>
                                        }
                                    } else {
                                        html! {}
                                    }}
                                    
                                    <div class="current-directory-section">
                                        <h3>{"Current Location"}</h3>
                                        <div class="directory-path-display">
                                            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                                                <path d="M10 4H4c-1.11 0-2 .89-2 2v12c0 1.11.89 2 2 2h16c1.11 0 2-.89 2-2V8c0-1.11-.89-2-2-2h-8l-2-2z" fill="currentColor"/>
                                            </svg>
                                            <span class="directory-path">{current_path}</span>
                                        </div>
                                        {if is_redirected {
                                            html! {
                                                <div class="redirect-status">
                                                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                                                        <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10s10-4.48 10-10S17.52 2 12 2zm-2 15l-5-5 1.41-1.41L10 14.17l7.59-7.59L19 8l-9 9z" fill="#10B981"/>
                                                    </svg>
                                                    <span>{"Data has been moved to a custom location"}</span>
                                                </div>
                                            }
                                        } else {
                                            html! {
                                                <div class="redirect-status">
                                                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                                                        <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10s10-4.48 10-10S17.52 2 12 2zm-2 15l-5-5 1.41-1.41L10 14.17l7.59-7.59L19 8l-9 9z" fill="#6B7280"/>
                                                    </svg>
                                                    <span>{"Data is in the default location"}</span>
                                                </div>
                                            }
                                        }}
                                    </div>
                                    
                                    <form class="data-directory-form" onsubmit={on_relocate}>
                                        <div class="relocate-info">
                                            <p>{"Move your allowance tracking data to a new location. All data including transaction history and settings will be safely transferred."}</p>
                                        </div>
                                        
                                        <div class="form-group">
                                            <label for="new-directory-path" class="form-label">
                                                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                                                    <path d="M10 4H4c-1.11 0-2 .89-2 2v12c0 1.11.89 2 2 2h16c1.11 0 2-.89 2-2V8c0-1.11-.89-2-2-2h-8l-2-2z" fill="currentColor"/>
                                                </svg>
                                                {"New Directory Path"}
                                            </label>
                                            <input 
                                                id="new-directory-path"
                                                type="text" 
                                                class="form-input"
                                                placeholder="/Users/username/Documents/MyAllowanceData"
                                                value={(*new_path).clone()}
                                                onchange={on_path_change}
                                            />
                                            <div class="form-help">
                                                {"Enter the full path to where you want to move your data. The directory will be created if it doesn't exist."}
                                            </div>
                                        </div>
                                        
                                        <div class="form-actions">
                                            <button type="button" class="cancel-button" onclick={on_close_click}>
                                                {"Cancel"}
                                            </button>
                                            {if is_redirected {
                                                html! {
                                                    <button type="button" class="revert-button" onclick={on_revert}>
                                                        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                                                            <path d="M12 5V1L7 6l5 5V7c3.31 0 6 2.69 6 6s-2.69 6-6 6-6-2.69-6-6H4c0 4.42 3.58 8 8 8s8-3.58 8-8-3.58-8-8-8z" fill="currentColor"/>
                                                        </svg>
                                                        {"Revert to Default"}
                                                    </button>
                                                }
                                            } else {
                                                html! {}
                                            }}
                                            <button type="submit" class="move-button">
                                                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                                                    <path d="M10 4H4c-1.11 0-2 .89-2 2v12c0 1.11.89 2 2 2h16c1.11 0 2-.89 2-2V8c0-1.11-.89-2-2-2h-8l-2-2z" fill="currentColor"/>
                                                </svg>
                                                {"Move Data"}
                                            </button>
                                        </div>
                                    </form>
                                </div>
                            },
                            DataDirectoryModalState::Processing => html! {
                                <div class="data-directory-processing">
                                    <div class="spinner"></div>
                                    <h3>{"Moving Data Directory..."}</h3>
                                    <p>{"Please wait while your data is being moved to the new location. This may take a few moments."}</p>
                                    <div class="processing-steps">
                                        <div class="step active">{"📂 Copying files..."}</div>
                                        <div class="step">{"✅ Verifying data integrity..."}</div>
                                        <div class="step">{"🔄 Updating configuration..."}</div>
                                    </div>
                                </div>
                            },
                            DataDirectoryModalState::Success { message } => html! {
                                <div class="data-directory-success">
                                    <div class="success-icon">
                                        <svg width="48" height="48" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                                            <circle cx="12" cy="12" r="12" fill="#10B981"/>
                                            <path d="m9 12 2 2 4-4" stroke="white" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
                                        </svg>
                                    </div>
                                    <h3>{"Success!"}</h3>
                                    <p class="success-message">{message}</p>
                                    <p class="closing-note">{"This dialog will close automatically..."}</p>
                                </div>
                            },
                            DataDirectoryModalState::Error { message } => html! {
                                <div class="data-directory-error-state">
                                    <div class="error-icon">
                                        <svg width="48" height="48" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                                            <circle cx="12" cy="12" r="12" fill="#EF4444"/>
                                            <path d="M15 9l-6 6m0-6l6 6" stroke="white" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
                                        </svg>
                                    </div>
                                    <h3>{"Error"}</h3>
                                    <p class="error-message">{message}</p>
                                    <button class="retry-button" onclick={on_close_click}>
                                        {"Close"}
                                    </button>
                                </div>
                            },
                        }
                    }
                }
                </div>
            </div>
        </div>
    }
} 