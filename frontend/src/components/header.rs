use yew::prelude::*;
use super::settings_menu::SettingsMenu;
use super::child_selector_menu::ChildSelectorMenu;
use crate::services::api::ApiClient;
use crate::hooks::use_active_child::UseActiveChildActions;
use shared::Child;

#[derive(Properties, PartialEq)]
pub struct HeaderProps {
    pub current_balance: f64,
    pub on_toggle_delete_mode: Callback<()>,
    pub api_client: ApiClient,
    pub active_child: Option<Child>,
    pub child_loading: bool,
    pub active_child_actions: UseActiveChildActions,
}

#[function_component(Header)]
pub fn header(props: &HeaderProps) -> Html {
    html! {
        <header class="header">
            <div class="container">
                <h1>
                    {if props.child_loading {
                        "Loading...".to_string()
                    } else if let Some(child) = &props.active_child {
                        // Extract first name (everything before the first space)
                        let first_name = child.name.split_whitespace()
                            .next()
                            .unwrap_or(&child.name);
                        format!("{}'s Allowance Tracker", first_name)
                    } else {
                        "My Allowance Tracker".to_string()
                    }}
                </h1>
                <div class="header-right">
                    <div class="balance-display">
                        <span class="balance-label">
                            {if props.child_loading {
                                "Loading...".to_string()
                            } else if let Some(child) = &props.active_child {
                                format!("{}'s Balance:", child.name)
                            } else {
                                "Current Balance:".to_string()
                            }}
                        </span>
                        <span class="balance-amount">{format!("${:.2}", props.current_balance)}</span>
                    </div>
                    <div class="header-menus">
                        <ChildSelectorMenu 
                            api_client={props.api_client.clone()} 
                            active_child={props.active_child.clone()}
                            child_loading={props.child_loading}
                            active_child_actions={props.active_child_actions.clone()}
                        />
                        <SettingsMenu on_toggle_delete_mode={props.on_toggle_delete_mode.clone()} />
                    </div>
                </div>
            </div>
        </header>
    }
} 