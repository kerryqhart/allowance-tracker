use yew::prelude::*;
use shared::Child;
use wasm_bindgen_futures::spawn_local;
use crate::services::api::ApiClient;

#[derive(Clone)]
pub struct ActiveChildState {
    pub active_child: Option<Child>,
    pub loading: bool,
    pub child_change_trigger: u32, // Increments when child changes to trigger refreshes
}

pub struct UseActiveChildResult {
    pub state: ActiveChildState,
    pub actions: UseActiveChildActions,
}

#[derive(Clone, PartialEq)]
pub struct UseActiveChildActions {
    pub refresh_active_child: Callback<()>,
    pub set_active_child: Callback<String>,
}

#[hook]
pub fn use_active_child(api_client: &ApiClient) -> UseActiveChildResult {
    let active_child = use_state(|| Option::<Child>::None);
    let loading = use_state(|| false);
    let child_change_trigger = use_state(|| 0u32);

    // Refresh active child callback
    let refresh_active_child = {
        let api_client = api_client.clone();
        let active_child = active_child.clone();
        let loading = loading.clone();
        
        use_callback((), move |_, _| {
            let api_client = api_client.clone();
            let active_child = active_child.clone();
            let loading = loading.clone();
            
            spawn_local(async move {
                loading.set(true);
                
                match api_client.get_active_child().await {
                    Ok(response) => {
                        active_child.set(response.active_child);
                    }
                    Err(e) => {
                        gloo::console::error!("Failed to get active child:", e);
                    }
                }
                
                loading.set(false);
            });
        })
    };

    // Set active child callback
    let set_active_child = {
        let api_client = api_client.clone();
        let active_child = active_child.clone();
        let loading = loading.clone();
        let child_change_trigger = child_change_trigger.clone();
        
        use_callback((), move |child_id: String, _| {
            let api_client = api_client.clone();
            let active_child = active_child.clone();
            let loading = loading.clone();
            let child_change_trigger = child_change_trigger.clone();
            
            spawn_local(async move {
                loading.set(true);
                
                match api_client.set_active_child(child_id).await {
                    Ok(response) => {
                        active_child.set(Some(response.active_child));
                        // Increment trigger to notify other hooks to refresh
                        child_change_trigger.set(*child_change_trigger + 1);
                    }
                    Err(e) => {
                        gloo::console::error!("Failed to set active child:", e);
                    }
                }
                
                loading.set(false);
            });
        })
    };

    // Load initial active child
    use_effect_with((), {
        let refresh_active_child = refresh_active_child.clone();
        move |_| {
            refresh_active_child.emit(());
            || ()
        }
    });

    let state = ActiveChildState {
        active_child: (*active_child).clone(),
        loading: *loading,
        child_change_trigger: *child_change_trigger,
    };

    let actions = UseActiveChildActions {
        refresh_active_child,
        set_active_child,
    };

    UseActiveChildResult { state, actions }
} 