use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct SpendMoneyFormProps {
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
}

#[function_component(SpendMoneyForm)]
pub fn spend_money_form(props: &SpendMoneyFormProps) -> Html {
    html! {
        <section class="spend-money-section">
            <h2>{"💸 Spend Money"}</h2>
            
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
                        {"💸 Money spent successfully!"}
                    </div>
                }
            } else { html! {} }}
            
            <form class="spend-money-form" onsubmit={
                let on_submit = props.on_submit.clone();
                Callback::from(move |e: SubmitEvent| {
                    e.prevent_default();
                    on_submit.emit(());
                })
            }>
                <div class="form-group">
                    <label for="spend-description">{"What did you spend money on?"}</label>
                    <input 
                        type="text"
                        id="spend-description"
                        placeholder="Toy, candy, book, game..."
                        value={props.description.clone()}
                        onchange={props.on_description_change.clone()}
                        disabled={props.creating_transaction}
                    />
                </div>
                
                <div class="form-group">
                    <label for="spend-amount">{"How much did you spend? (dollars)"}</label>
                    <input 
                        type="number" 
                        id="spend-amount"
                        placeholder="2.50"
                        step="0.01"
                        min="0.01"
                        value={props.amount.clone()}
                        onchange={props.on_amount_change.clone()}
                        disabled={props.creating_transaction}
                    />
                </div>
                
                <button 
                    type="submit" 
                    class="btn btn-secondary spend-money-btn"
                    disabled={props.creating_transaction}
                >
                    {if props.creating_transaction {
                        "Recording Spending..."
                    } else {
                        "💸 Record Spending"
                    }}
                </button>
            </form>
        </section>
    }
} 