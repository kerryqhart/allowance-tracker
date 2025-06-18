use yew::prelude::*;
use web_sys::MouseEvent;
use wasm_bindgen_futures::spawn_local;
use shared::Child;
use crate::services::api::ApiClient;
use crate::hooks::use_active_child::UseActiveChildActions;

#[derive(Properties, PartialEq)]
pub struct ChildSelectorMenuProps {
    pub api_client: ApiClient,
    pub active_child: Option<Child>,
    pub child_loading: bool,
    pub active_child_actions: UseActiveChildActions,
}

#[function_component(ChildSelectorMenu)]
pub fn child_selector_menu(props: &ChildSelectorMenuProps) -> Html {
    let is_open = use_state(|| false);
    let all_children = use_state(|| Vec::<Child>::new());
    let loading = use_state(|| false);
    
    let toggle_menu = {
        let is_open = is_open.clone();
        let api_client = props.api_client.clone();
        let all_children = all_children.clone();
        let loading = loading.clone();
        
        Callback::from(move |_: MouseEvent| {
            let current_open = *is_open;
            
            if !current_open {
                // Opening the dropdown - refresh children list
                let api_client = api_client.clone();
                let all_children = all_children.clone();
                let loading = loading.clone();
                
                spawn_local(async move {
                    loading.set(true);
                    
                    match api_client.list_children().await {
                        Ok(children_response) => {
                            all_children.set(children_response.children);
                        }
                        Err(e) => {
                            gloo::console::error!("Failed to load children list:", e);
                        }
                    }
                    
                    loading.set(false);
                });
            }
            
            is_open.set(!current_open);
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

    // Handle child selection
    let on_select_child = {
        let set_active_child = props.active_child_actions.set_active_child.clone();
        let close_menu = close_menu.clone();
        
        Callback::from(move |child_id: String| {
            set_active_child.emit(child_id);
            close_menu.emit(MouseEvent::new("click").unwrap());
        })
    };

    // Get the display letter for the current active child
    let display_letter = props.active_child.as_ref()
        .map(|child| child.name.chars().next().unwrap_or('?').to_uppercase().to_string())
        .unwrap_or_else(|| "?".to_string());

    html! {
        <div class="child-selector-menu">
            <button 
                class="child-selector-button" 
                onclick={toggle_menu}
                aria-label="Select child"
                disabled={props.child_loading || *loading}
            >
                {if props.child_loading || *loading {
                    html! { <div class="child-selector-spinner"></div> }
                } else {
                    html! { <span class="child-selector-letter">{display_letter}</span> }
                }}
            </button>
            
            if *is_open {
                <>
                    <div class="child-selector-backdrop" onclick={on_backdrop_click}></div>
                    <div class="child-selector-dropdown" onclick={on_menu_click}>
                        {
                            if *loading {
                                html! {
                                    <div class="child-selector-item loading">
                                        <div class="child-selector-spinner"></div>
                                        <span>{"Loading children..."}</span>
                                    </div>
                                }
                            } else if all_children.is_empty() {
                                html! {
                                    <div class="child-selector-item no-children">
                                        <span>{"No children found"}</span>
                                    </div>
                                }
                            } else {
                                all_children.iter().map(|child| {
                                    let child_id = child.id.clone();
                                    let child_name = child.name.clone();
                                    let is_active = props.active_child.as_ref()
                                        .map(|active| active.id == child.id)
                                        .unwrap_or(false);
                                    
                                    let on_click = {
                                        let on_select = on_select_child.clone();
                                        Callback::from(move |_: MouseEvent| {
                                            on_select.emit(child_id.clone());
                                        })
                                    };
                                    
                                    let child_letter = child.name.chars().next().unwrap_or('?').to_uppercase().to_string();
                                    
                                    html! {
                                        <div 
                                            class={classes!("child-selector-item", is_active.then(|| "active"))} 
                                            onclick={on_click}
                                        >
                                            <div class="child-avatar">
                                                {child_letter}
                                            </div>
                                            <span class="child-name">{child_name}</span>
                                            {if is_active {
                                                html! { <span class="child-active-indicator">{"✓"}</span> }
                                            } else {
                                                html! {}
                                            }}
                                        </div>
                                    }
                                }).collect::<Html>()
                            }
                        }
                    </div>
                </>
            }
        </div>
    }
} 