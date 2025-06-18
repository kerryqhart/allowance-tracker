use yew::prelude::*;
use web_sys::MouseEvent;
use super::challenge_modal::ChallengeModal;
use super::create_child_modal::CreateChildModal;

#[derive(Properties, PartialEq)]
pub struct SettingsMenuProps {
    pub on_toggle_delete_mode: Callback<()>,
}

#[function_component(SettingsMenu)]
pub fn settings_menu(props: &SettingsMenuProps) -> Html {
    let is_open = use_state(|| false);
    let is_authenticated = use_state(|| false);
    let show_challenge = use_state(|| false);
    let show_create_child = use_state(|| false);
    
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
                        <div class="settings-item" onclick={close_menu.clone()}>
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
                        
                        <div class="settings-item" onclick={close_menu.clone()}>
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
        </div>
    }
} 