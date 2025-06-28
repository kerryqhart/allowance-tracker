use yew::prelude::*;
use web_sys::MouseEvent;
use super::challenge_modal::ChallengeModal;
use super::create_child_modal::CreateChildModal;
use super::allowance_config_modal::AllowanceConfigModal;
use super::profile_modal::ProfileModal;
use super::export_modal::ExportModal;
use crate::services::api::ApiClient;
use shared::Child;

#[derive(Properties, PartialEq)]
pub struct SettingsMenuProps {
    pub on_toggle_delete_mode: Callback<()>,
    pub api_client: ApiClient,
    pub active_child: Option<Child>,
}

#[function_component(SettingsMenu)]
pub fn settings_menu(props: &SettingsMenuProps) -> Html {
    let is_open = use_state(|| false);
    let is_authenticated = use_state(|| false);
    let show_challenge = use_state(|| false);
    let show_create_child = use_state(|| false);
    let show_allowance_config = use_state(|| false);
    let show_profile_modal = use_state(|| false);
    let show_export_modal = use_state(|| false);
    
    let toggle_menu = {
        let is_open = is_open.clone();
        let is_authenticated = is_authenticated.clone();
        let show_challenge = show_challenge.clone();
        Callback::from(move |_: MouseEvent| {
            if *is_authenticated {
                // If already authenticated, just toggle menu
                is_open.set(!*is_open);
            } else {
                // If not authenticated, show challenge modal
                show_challenge.set(true);
            }
        })
    };
    
    let close_menu = {
        let is_open = is_open.clone();
        Callback::from(move |_: MouseEvent| {
            is_open.set(false);
        })
    };
    
    // Close menu when clicking outside
    let on_backdrop_click = {
        let is_open = is_open.clone();
        Callback::from(move |e: MouseEvent| {
            e.stop_propagation();
            is_open.set(false);
        })
    };
    
    let on_menu_click = Callback::from(|e: MouseEvent| {
        e.stop_propagation();
    });

    // Challenge modal callbacks
    let on_challenge_success = {
        let is_authenticated = is_authenticated.clone();
        let show_challenge = show_challenge.clone();
        let is_open = is_open.clone();
        Callback::from(move |_| {
            is_authenticated.set(true);
            show_challenge.set(false);
            is_open.set(true); // Open the settings menu after successful authentication
        })
    };

    let on_challenge_close = {
        let show_challenge = show_challenge.clone();
        Callback::from(move |_| {
            show_challenge.set(false);
        })
    };

    // Create child modal callbacks
    let on_create_child_click = {
        let close_menu = close_menu.clone();
        let show_create_child = show_create_child.clone();
        Callback::from(move |_: MouseEvent| {
            close_menu.emit(MouseEvent::new("click").unwrap());
            show_create_child.set(true);
        })
    };

    let on_create_child_success = {
        let show_create_child = show_create_child.clone();
        Callback::from(move |_| {
            show_create_child.set(false);
            // TODO: Show success message or refresh children list
        })
    };

    let on_create_child_close = {
        let show_create_child = show_create_child.clone();
        Callback::from(move |_| {
            show_create_child.set(false);
        })
    };

    // Allowance config modal callbacks
    let on_allowance_config_click = {
        let close_menu = close_menu.clone();
        let show_allowance_config = show_allowance_config.clone();
        Callback::from(move |_: MouseEvent| {
            close_menu.emit(MouseEvent::new("click").unwrap());
            show_allowance_config.set(true);
        })
    };

    let on_allowance_config_success = {
        let show_allowance_config = show_allowance_config.clone();
        Callback::from(move |_| {
            show_allowance_config.set(false);
            // TODO: Show success message or refresh data
        })
    };

    let on_allowance_config_close = {
        let show_allowance_config = show_allowance_config.clone();
        Callback::from(move |_| {
            show_allowance_config.set(false);
        })
    };

    // Export data functionality - show modal
    let on_export_data_click = {
        let close_menu = close_menu.clone();
        let show_export_modal = show_export_modal.clone();
        
        Callback::from(move |_: MouseEvent| {
            close_menu.emit(MouseEvent::new("click").unwrap());
            show_export_modal.set(true);
        })
    };

    // Export modal callbacks
    let on_export_success = {
        let show_export_modal = show_export_modal.clone();
        Callback::from(move |_| {
            show_export_modal.set(false);
            // TODO: Show success notification
        })
    };

    let on_export_close = {
        let show_export_modal = show_export_modal.clone();
        Callback::from(move |_| {
            show_export_modal.set(false);
        })
    };

    // Profile modal callbacks
    let on_profile_close = {
        let show_profile_modal = show_profile_modal.clone();
        Callback::from(move |_| {
            show_profile_modal.set(false);
        })
    };

    html! {
        <div class="settings-menu">
            <button 
                class="settings-button" 
                onclick={toggle_menu}
                aria-label="Settings menu"
            >
                {"⚙"}
            </button>
            
            if *is_open {
                <>
                    <div class="settings-backdrop" onclick={on_backdrop_click}></div>
                    <div class="settings-dropdown" onclick={on_menu_click}>
                        <div class="settings-item" onclick={{
                            let close_menu = close_menu.clone();
                            let show_profile_modal = show_profile_modal.clone();
                            Callback::from(move |_: MouseEvent| {
                                close_menu.emit(MouseEvent::new("click").unwrap());
                                show_profile_modal.set(true);
                            })
                        }}>
                            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                                <path d="M12 12c2.21 0 4-1.79 4-4s-1.79-4-4-4-4 1.79-4 4 1.79 4 4 4zm0 2c-2.67 0-8 1.34-8 4v2h16v-2c0-2.66-5.33-4-8-4z" fill="currentColor"/>
                            </svg>
                            <span>{"Profile"}</span>
                        </div>
                        
                        <div class="settings-item" onclick={on_create_child_click}>
                            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                                <path d="M12 12c2.21 0 4-1.79 4-4s-1.79-4-4-4-4 1.79-4 4 1.79 4 4 4zm0 2c-2.67 0-8 1.34-8 4v2h16v-2c0-2.66-5.33-4-8-4z" fill="currentColor"/>
                                <circle cx="19" cy="5" r="4" fill="currentColor"/>
                                <path d="M17 5h4M19 3v4" stroke="white" stroke-width="1.5"/>
                            </svg>
                            <span>{"Create child"}</span>
                        </div>
                        
                        <div class="settings-item" onclick={on_allowance_config_click}>
                            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                                <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm1.41 16.09V20h-2.67v-1.93c-1.71-.36-3.16-1.46-3.27-3.4h1.96c.1 1.05.82 1.87 2.65 1.87 1.96 0 2.4-.98 2.4-1.59 0-.83-.44-1.61-2.67-2.14-2.48-.6-4.18-1.62-4.18-3.67 0-1.72 1.39-2.84 3.11-3.21V4h2.67v1.95c1.86.45 2.79 1.86 2.85 3.39H14.3c-.05-1.11-.64-1.87-2.22-1.87-1.5 0-2.4.68-2.4 1.64 0 .84.65 1.39 2.67 1.91s4.18 1.39 4.18 3.91c-.01 1.83-1.38 2.83-3.12 3.16z" fill="currentColor"/>
                            </svg>
                            <span>{"Configure allowance"}</span>
                        </div>
                        
                        <div class="settings-item" onclick={{
                            let close_menu = close_menu.clone();
                            let on_toggle_delete_mode = props.on_toggle_delete_mode.clone();
                            Callback::from(move |_: MouseEvent| {
                                close_menu.emit(MouseEvent::new("click").unwrap());
                                on_toggle_delete_mode.emit(());
                            })
                        }}>
                            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                                <path d="M6 19c0 1.1.9 2 2 2h8c1.1 0 2-.9 2-2V7H6v12zM19 4h-3.5l-1-1h-5l-1 1H5v2h14V4z" fill="currentColor"/>
                            </svg>
                            <span>{"Delete transactions"}</span>
                        </div>
                        
                        <div class="settings-item" onclick={on_export_data_click}>
                            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                                <path d="M14 2H6c-1.1 0-1.99.9-1.99 2L4 20c0 1.1.89 2 2 2h10c1.1 0 2-.9 2-2V8l-6-6zm4 18H6V4h7v5h5v11z" fill="currentColor"/>
                            </svg>
                            <span>{"Export data"}</span>
                        </div>
                    </div>
                </>
            }
            
            <ChallengeModal 
                is_open={*show_challenge}
                on_success={on_challenge_success}
                on_close={on_challenge_close}
            />
            
            <CreateChildModal 
                is_open={*show_create_child}
                on_success={on_create_child_success}
                on_close={on_create_child_close}
            />
            
            <AllowanceConfigModal 
                is_open={*show_allowance_config}
                on_success={on_allowance_config_success}
                on_close={on_allowance_config_close}
            />
            
            <ProfileModal 
                is_open={*show_profile_modal}
                on_close={on_profile_close}
                active_child={props.active_child.clone()}
            />
            
            <ExportModal 
                is_open={*show_export_modal}
                on_success={on_export_success}
                on_close={on_export_close}
                api_client={props.api_client.clone()}
                active_child={props.active_child.clone()}
            />
        </div>
    }
} 