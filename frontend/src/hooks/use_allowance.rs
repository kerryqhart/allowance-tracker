use yew::prelude::*;
use wasm_bindgen_futures::spawn_local;
use shared::{AllowanceConfig, GetAllowanceConfigRequest};
use crate::services::api::ApiClient;

#[derive(Clone, PartialEq)]
pub struct AllowanceState {
    pub config: Option<AllowanceConfig>,
    pub loading: bool,
    pub error: Option<String>,
}

impl Default for AllowanceState {
    fn default() -> Self {
        Self {
            config: None,
            loading: false,
            error: None,
        }
    }
}

/// Hook for managing allowance configuration
#[hook]
pub fn use_allowance() -> UseStateHandle<AllowanceState> {
    let allowance_state = use_state(AllowanceState::default);
    let api_client = ApiClient::new();

    // Fetch allowance config on mount
    use_effect_with((), {
        let allowance_state = allowance_state.clone();
        let api_client = api_client.clone();
        
        move |_| {
            allowance_state.set(AllowanceState {
                config: None,
                loading: true,
                error: None,
            });

            spawn_local(async move {
                let request = GetAllowanceConfigRequest { child_id: None };
                match api_client.get_allowance_config(request).await {
                    Ok(response) => {
                        allowance_state.set(AllowanceState {
                            config: response.allowance_config,
                            loading: false,
                            error: None,
                        });
                    }
                    Err(e) => {
                        allowance_state.set(AllowanceState {
                            config: None,
                            loading: false,
                            error: Some(format!("Failed to fetch allowance config: {}", e)),
                        });
                    }
                }
            });

            || ()
        }
    });

    allowance_state
} 