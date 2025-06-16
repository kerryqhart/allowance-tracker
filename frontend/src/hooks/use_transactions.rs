use yew::prelude::*;
use shared::{AddMoneyRequest, SpendMoneyRequest, FormattedTransaction};
use wasm_bindgen_futures::spawn_local;
use web_sys::HtmlInputElement;
use crate::services::api::ApiClient;

#[derive(Clone)]
pub struct TransactionState {
    pub formatted_transactions: Vec<FormattedTransaction>,
    pub loading: bool,
    pub current_balance: f64,
    
    // Add money form state
    pub description: String,
    pub amount: String,
    pub creating_transaction: bool,
    pub form_error: Option<String>,
    pub form_success: bool,
    pub validation_suggestions: Vec<String>,
    
    // Spend money form state
    pub spend_description: String,
    pub spend_amount: String,
    pub creating_spend_transaction: bool,
    pub spend_form_error: Option<String>,
    pub spend_form_success: bool,
    pub spend_validation_suggestions: Vec<String>,
}

pub struct UseTransactionsResult {
    pub state: TransactionState,
    pub actions: UseTransactionsActions,
}

#[derive(Clone)]
pub struct UseTransactionsActions {
    pub refresh_transactions: Callback<()>,
    pub add_money: Callback<()>,
    pub spend_money: Callback<()>,
    pub on_description_change: Callback<Event>,
    pub on_amount_change: Callback<Event>,
    pub on_spend_description_change: Callback<Event>,
    pub on_spend_amount_change: Callback<Event>,
}

#[hook]
pub fn use_transactions(api_client: &ApiClient) -> UseTransactionsResult {
    let formatted_transactions = use_state(|| Vec::<FormattedTransaction>::new());
    let loading = use_state(|| true);
    let current_balance = use_state(|| 0.0f64);
    
    // Add money form states
    let description = use_state(|| String::new());
    let amount = use_state(|| String::new());
    let creating_transaction = use_state(|| false);
    let form_error = use_state(|| Option::<String>::None);
    let form_success = use_state(|| false);
    let validation_suggestions = use_state(|| Vec::<String>::new());
    
    // Spend money form states
    let spend_description = use_state(String::new);
    let spend_amount = use_state(String::new);
    let creating_spend_transaction = use_state(|| false);
    let spend_form_error = use_state(|| None::<String>);
    let spend_form_success = use_state(|| false);
    let spend_validation_suggestions = use_state(Vec::<String>::new);

    // Refresh transactions callback
    let refresh_transactions = {
        let api_client = api_client.clone();
        let formatted_transactions = formatted_transactions.clone();
        let current_balance = current_balance.clone();
        let loading = loading.clone();
        
        use_callback((), move |_, _| {
            let api_client = api_client.clone();
            let formatted_transactions = formatted_transactions.clone();
            let current_balance = current_balance.clone();
            let loading = loading.clone();
            
            spawn_local(async move {
                loading.set(true);
                
                match api_client.get_transactions(Some(10)).await {
                    Ok(data) => {
                        if let Some(first_tx) = data.formatted_transactions.first() {
                            current_balance.set(first_tx.raw_balance);
                        }
                        formatted_transactions.set(data.formatted_transactions);
                    }
                    Err(e) => {
                        gloo::console::error!("Failed to fetch transactions:", e);
                    }
                }
                
                loading.set(false);
            });
        })
    };

    // Add money callback
    let add_money = {
        let api_client = api_client.clone();
        let description = description.clone();
        let amount = amount.clone();
        let creating_transaction = creating_transaction.clone();
        let form_error = form_error.clone();
        let form_success = form_success.clone();
        let refresh_transactions = refresh_transactions.clone();
        
        use_callback((), move |_, _| {
            // IMMEDIATE DEBUG - This should show if callback is called at all
            gloo::console::log!("🚨 ADD_MONEY CALLBACK TRIGGERED! 🚨");
            
            let api_client = api_client.clone();
            let description = description.clone();
            let amount = amount.clone();
            let creating_transaction = creating_transaction.clone();
            let form_error = form_error.clone();
            let form_success = form_success.clone();
            let refresh_transactions = refresh_transactions.clone();
            
            spawn_local(async move {
                // Debug logging - show current state values
                let desc_value = (*description).clone();
                let amount_str = (*amount).clone();
                
                gloo::console::log!("=== ADD MONEY CALLBACK START ===");
                gloo::console::log!("Raw description state:", &desc_value);
                gloo::console::log!("Raw amount state:", &amount_str);
                
                form_error.set(None);
                form_success.set(false);
                creating_transaction.set(true);
                
                let amount_value = match amount_str.trim().parse::<f64>() {
                    Ok(val) => {
                        gloo::console::log!("Parsed amount successfully:", val.to_string());
                        val
                    },
                    Err(e) => {
                        gloo::console::log!("Failed to parse amount:", format!("{:?}", e));
                        0.0
                    },
                };
                
                let request = AddMoneyRequest {
                    description: desc_value.clone(),
                    amount: amount_value,
                    date: None,
                };
                
                gloo::console::log!("=== FINAL REQUEST BEING SENT ===");
                gloo::console::log!("Request description:", &request.description);
                gloo::console::log!("Request amount:", request.amount.to_string());
                gloo::console::log!("Full request:", format!("{:?}", request));
                
                match api_client.add_money(request).await {
                    Ok(_response) => {
                        description.set(String::new());
                        amount.set(String::new());
                        form_success.set(true);
                        refresh_transactions.emit(());
                        
                        let form_success_clear = form_success.clone();
                        spawn_local(async move {
                            gloo::timers::future::TimeoutFuture::new(3000).await;
                            form_success_clear.set(false);
                        });
                    }
                    Err(error_message) => {
                        form_error.set(Some(error_message));
                    }
                }
                
                creating_transaction.set(false);
            });
        })
    };

    // Spend money callback
    let spend_money = {
        let api_client = api_client.clone();
        let spend_description = spend_description.clone();
        let spend_amount = spend_amount.clone();
        let creating_spend_transaction = creating_spend_transaction.clone();
        let spend_form_error = spend_form_error.clone();
        let spend_form_success = spend_form_success.clone();
        let refresh_transactions = refresh_transactions.clone();
        
        use_callback((), move |_, _| {
            // IMMEDIATE DEBUG - This should show if callback is called at all
            gloo::console::log!("🚨 SPEND_MONEY CALLBACK TRIGGERED! 🚨");
            
            let api_client = api_client.clone();
            let spend_description = spend_description.clone();
            let spend_amount = spend_amount.clone();
            let creating_spend_transaction = creating_spend_transaction.clone();
            let spend_form_error = spend_form_error.clone();
            let spend_form_success = spend_form_success.clone();
            let refresh_transactions = refresh_transactions.clone();
            
            spawn_local(async move {
                // Debug logging - show current state values
                let desc_value = (*spend_description).clone();
                let amount_str = (*spend_amount).clone();
                
                gloo::console::log!("=== SPEND MONEY CALLBACK START ===");
                gloo::console::log!("Raw spend_description state:", &desc_value);
                gloo::console::log!("Raw spend_amount state:", &amount_str);
                
                spend_form_error.set(None);
                spend_form_success.set(false);
                creating_spend_transaction.set(true);
                
                let amount_value = match amount_str.trim().parse::<f64>() {
                    Ok(val) => {
                        gloo::console::log!("Parsed amount successfully:", val.to_string());
                        val
                    },
                    Err(e) => {
                        gloo::console::log!("Failed to parse amount:", format!("{:?}", e));
                        0.0
                    },
                };
                
                let request = SpendMoneyRequest {
                    description: desc_value.clone(),
                    amount: amount_value,
                    date: None,
                };
                
                gloo::console::log!("=== FINAL REQUEST BEING SENT ===");
                gloo::console::log!("Request description:", &request.description);
                gloo::console::log!("Request amount:", request.amount.to_string());
                gloo::console::log!("Full request:", format!("{:?}", request));
                
                match api_client.spend_money(request).await {
                    Ok(_response) => {
                        spend_description.set(String::new());
                        spend_amount.set(String::new());
                        spend_form_success.set(true);
                        refresh_transactions.emit(());
                        
                        let spend_form_success_clear = spend_form_success.clone();
                        spawn_local(async move {
                            gloo::timers::future::TimeoutFuture::new(3000).await;
                            spend_form_success_clear.set(false);
                        });
                    }
                    Err(error_message) => {
                        spend_form_error.set(Some(error_message));
                    }
                }
                
                creating_spend_transaction.set(false);
            });
        })
    };

    // Form input handlers
    let on_description_change = {
        let description = description.clone();
        let form_error = form_error.clone();
        use_callback((), move |e: Event, _| {
            let input: HtmlInputElement = e.target_unchecked_into();
            description.set(input.value());
            form_error.set(None);
        })
    };

    let on_amount_change = {
        let amount = amount.clone();
        let form_error = form_error.clone();
        use_callback((), move |e: Event, _| {
            let input: HtmlInputElement = e.target_unchecked_into();
            amount.set(input.value());
            form_error.set(None);
        })
    };

    let on_spend_description_change = {
        let spend_description = spend_description.clone();
        let spend_form_error = spend_form_error.clone();
        use_callback((), move |e: Event, _| {
            let input: HtmlInputElement = e.target_unchecked_into();
            spend_description.set(input.value());
            spend_form_error.set(None);
        })
    };

    let on_spend_amount_change = {
        let spend_amount = spend_amount.clone();
        let spend_form_error = spend_form_error.clone();
        use_callback((), move |e: Event, _| {
            let input: HtmlInputElement = e.target_unchecked_into();
            spend_amount.set(input.value());
            spend_form_error.set(None);
        })
    };



    let state = TransactionState {
        formatted_transactions: (*formatted_transactions).clone(),
        loading: *loading,
        current_balance: *current_balance,
        description: (*description).clone(),
        amount: (*amount).clone(),
        creating_transaction: *creating_transaction,
        form_error: (*form_error).clone(),
        form_success: *form_success,
        validation_suggestions: (*validation_suggestions).clone(),
        spend_description: (*spend_description).clone(),
        spend_amount: (*spend_amount).clone(),
        creating_spend_transaction: *creating_spend_transaction,
        spend_form_error: (*spend_form_error).clone(),
        spend_form_success: *spend_form_success,
        spend_validation_suggestions: (*spend_validation_suggestions).clone(),
    };

    let actions = UseTransactionsActions {
        refresh_transactions,
        add_money,
        spend_money,
        on_description_change,
        on_amount_change,
        on_spend_description_change,
        on_spend_amount_change,
    };

    UseTransactionsResult { state, actions }
} 