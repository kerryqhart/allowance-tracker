use yew::prelude::*;
use super::settings_menu::SettingsMenu;

#[derive(Properties, PartialEq)]
pub struct HeaderProps {
    pub current_balance: f64,
    pub on_toggle_delete_mode: Callback<()>,
}

#[function_component(Header)]
pub fn header(props: &HeaderProps) -> Html {
    html! {
        <header class="header">
            <div class="container">
                <h1>{"My Allowance Tracker"}</h1>
                <div class="header-right">
                    <div class="balance-display">
                        <span class="balance-label">{"Current Balance:"}</span>
                        <span class="balance-amount">{format!("${:.2}", props.current_balance)}</span>
                    </div>
                    <SettingsMenu on_toggle_delete_mode={props.on_toggle_delete_mode.clone()} />
                </div>
            </div>
        </header>
    }
} 