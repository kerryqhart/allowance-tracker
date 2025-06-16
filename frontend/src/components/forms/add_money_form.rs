use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct AddMoneyFormProps {
    // Form state
    pub description: String,
    pub amount: String,
    pub creating_transaction: bool,
    pub form_error: Option<String>,
    pub form_success: bool,
    pub validation_suggestions: Vec<String>,
    
    // Event handlers
    pub on_description_change: Callback<Event>,
    pub on_amount_change: Callback<Event>,
    pub on_submit: Callback<()>,
    pub on_debug: Callback<String>,
    pub on_refresh: Callback<()>,
}

#[function_component(AddMoneyForm)]
pub fn add_money_form(props: &AddMoneyFormProps) -> Html {
    html! {
        <section class="add-money-section">
            <h2>{"✨ Add Extra Money"}</h2>
            
            {if let Some(error) = props.form_error.as_ref() {
                html! {
                    <div class="form-message error">
                        {error}
                    </div>
                }
            } else { html! {} }}
            
            {if !props.validation_suggestions.is_empty() {
                html! {
                    <div class="form-message info">
                        <strong>{"💡 Suggestions:"}</strong>
                        <ul>
                            {for props.validation_suggestions.iter().map(|suggestion| {
                                html! { <li>{suggestion}</li> }
                            })}
                        </ul>
                    </div>
                }
            } else { html! {} }}
            
            {if props.form_success {
                html! {
                    <div class="form-message success">
                        {"🎉 Money added successfully!"}
                    </div>
                }
            } else { html! {} }}
            
            <form class="add-money-form" onsubmit={
                let on_debug = props.on_debug.clone();
                let on_refresh = props.on_refresh.clone();
                Callback::from(move |e: SubmitEvent| {
                    e.prevent_default();
                    
                    gloo::console::log!("🔥 ADD FORM SUBMIT TRIGGERED!");
                    on_debug.emit("🔥 ADD FORM SUBMITTED! 🔥".to_string());
                    
                    // Get form data directly from the event
                    use web_sys::{HtmlFormElement, FormData};
                    use wasm_bindgen::JsCast;
                    
                    let form: HtmlFormElement = e.target_unchecked_into();
                    let form_data = FormData::new_with_form(&form).expect("form data");
                    
                    let description = form_data.get("description").as_string().unwrap_or_default();
                    let amount_str = form_data.get("amount").as_string().unwrap_or_default();
                    
                    gloo::console::log!("🔥 Form data - desc:", &description);
                    gloo::console::log!("🔥 Form data - amount:", &amount_str);
                    
                    // Parse amount and make API call
                    if !description.trim().is_empty() && !amount_str.trim().is_empty() {
                        if let Ok(amount) = amount_str.parse::<f64>() {
                            if amount > 0.0 {
                                gloo::console::log!("🚀 Making API call with:", &description, amount);
                                on_debug.emit(format!("Making API call: {} ${}", description, amount));
                                
                                // Make the actual API call
                                use crate::services::api::ApiClient;
                                use shared::AddMoneyRequest;
                                let api_client = ApiClient::new();
                                
                                let request = AddMoneyRequest {
                                    description: description.clone(),
                                    amount,
                                    date: None,
                                };
                                
                                let on_refresh = on_refresh.clone();
                                wasm_bindgen_futures::spawn_local(async move {
                                    match api_client.add_money(request).await {
                                        Ok(_) => {
                                            gloo::console::log!("✅ API call successful!");
                                            
                                            // Trigger UI refresh using the callback
                                            on_refresh.emit(());
                                            gloo::console::log!("✅ UI refresh triggered!");
                                        }
                                        Err(e) => {
                                            gloo::console::log!("❌ API call failed:", format!("{:?}", e));
                                        }
                                    }
                                });
                            } else {
                                gloo::console::log!("❌ Amount must be positive");
                                on_debug.emit("Error: Amount must be positive".to_string());
                            }
                        } else {
                            gloo::console::log!("❌ Invalid amount format");
                            on_debug.emit("Error: Invalid amount format".to_string());
                        }
                    } else {
                        gloo::console::log!("❌ Missing description or amount");
                        on_debug.emit("Error: Please fill in all fields".to_string());
                    }
                })
            }>
                <div class="form-group">
                    <label for="description">{"What did you get money for?"}</label>
                    <input 
                        type="text"
                        id="description"
                        name="description"
                        placeholder="Birthday gift, chores, found money..."
                        value={props.description.clone()}
                        onchange={props.on_description_change.clone()}
                        disabled={props.creating_transaction}
                    />
                </div>
                
                <div class="form-group">
                    <label for="amount">{"How much money? (dollars)"}</label>
                    <input 
                        type="number" 
                        id="amount"
                        name="amount"
                        placeholder="5.00"
                        step="0.01"
                        min="0.01"
                        value={props.amount.clone()}
                        onchange={props.on_amount_change.clone()}
                        disabled={props.creating_transaction}
                    />
                </div>
                
                <button 
                    type="submit" 
                    class="btn btn-primary add-money-btn"
                    disabled={props.creating_transaction}
                >
                    {if props.creating_transaction {
                        "Adding Money..."
                    } else {
                        "💰 Add Money"
                    }}
                </button>
            </form>
        </section>
    }
} 