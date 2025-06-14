use gloo::net::http::Request;
use shared::KeyValue;
use wasm_bindgen_futures::spawn_local;
use web_sys::HtmlInputElement;
use yew::prelude::*;

const API_BASE_URL: &str = "/api";

#[function_component(App)]
fn app() -> Html {
    let keys = use_state(|| Vec::<String>::new());
    let current_value = use_state(|| String::new());
    let error_message = use_state(String::new);
    let success_message = use_state(String::new);
    
    // Clear messages after a delay
    let clear_messages = {
        let error_message = error_message.clone();
        let success_message = success_message.clone();
        Callback::from(move |_| {
            let error_msg = error_message.clone();
            let success_msg = success_message.clone();
            spawn_local(async move {
                gloo::timers::future::TimeoutFuture::new(3000).await;
                error_msg.set(String::new());
                success_msg.set(String::new());
            });
        })
    };

    // For simplicity, we'll maintain our own list of keys since the backend
    // doesn't have a list keys endpoint exposed via REST yet
    let refresh_keys = {
        let keys = keys.clone();
        let error_message = error_message.clone();
        Callback::from(move |_| {
            // For now, we'll just clear the keys list
            // In a real implementation, we'd call a backend endpoint
            keys.set(Vec::new());
            error_message.set("Key list refresh not yet implemented".to_string());
        })
    };

    html! {
        <div class="container">
            <header>
                <h1>{"Key-Value Store"}</h1>
                <p>{"Simple key-value database interface"}</p>
            </header>

            <main>
                // Messages
                <div class="messages">
                    if !error_message.is_empty() {
                        <div class="error-message">
                            { &*error_message }
                        </div>
                    }
                    if !success_message.is_empty() {
                        <div class="success-message">
                            { &*success_message }
                        </div>
                    }
                </div>

                // Add new key-value pair
                <section class="add-section">
                    <h2>{"Add New Key-Value Pair"}</h2>
                    <AddValueForm 
                        on_success={
                            let success_message = success_message.clone();
                            let clear_messages = clear_messages.clone();
                            Callback::from(move |msg: String| {
                                success_message.set(msg);
                                clear_messages.emit(());
                            })
                        }
                        on_error={
                            let error_message = error_message.clone();
                            let clear_messages = clear_messages.clone();
                            Callback::from(move |msg: String| {
                                error_message.set(msg);
                                clear_messages.emit(());
                            })
                        }
                    />
                </section>

                // Retrieve value by key
                <section class="retrieve-section">
                    <h2>{"Retrieve Value by Key"}</h2>
                    <RetrieveValueForm 
                        current_value={(*current_value).clone()}
                        on_value_retrieved={
                            let current_value = current_value.clone();
                            Callback::from(move |value: String| {
                                current_value.set(value);
                            })
                        }
                        on_error={
                            let error_message = error_message.clone();
                            let clear_messages = clear_messages.clone();
                            Callback::from(move |msg: String| {
                                error_message.set(msg);
                                clear_messages.emit(());
                            })
                        }
                    />
                </section>

                // Display current value
                if !current_value.is_empty() {
                    <section class="current-value">
                        <h3>{"Retrieved Value:"}</h3>
                        <div class="value-display">
                            { &*current_value }
                        </div>
                    </section>
                }

                // Keys list (placeholder for future implementation)
                <section class="keys-section">
                    <h2>{"Stored Keys"}</h2>
                    <button onclick={refresh_keys} class="refresh-btn">
                        {"Refresh Keys (Not Yet Implemented)"}
                    </button>
                    <div class="keys-list">
                        if keys.is_empty() {
                            <p class="no-keys">{"No keys available or not yet loaded"}</p>
                        } else {
                            <ul>
                                { for keys.iter().map(|key| html! {
                                    <li>{ key }</li>
                                }) }
                            </ul>
                        }
                    </div>
                </section>
            </main>
        </div>
    }
}

#[derive(Properties, PartialEq)]
struct AddValueFormProps {
    pub on_success: Callback<String>,
    pub on_error: Callback<String>,
}

#[function_component(AddValueForm)]
fn add_value_form(props: &AddValueFormProps) -> Html {
    let key_input = use_node_ref();
    let value_input = use_node_ref();

    let on_submit = {
        let key_input = key_input.clone();
        let value_input = value_input.clone();
        let on_success = props.on_success.clone();
        let on_error = props.on_error.clone();
        
        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            
            let key_element = key_input.cast::<HtmlInputElement>().unwrap();
            let value_element = value_input.cast::<HtmlInputElement>().unwrap();
            
            let key = key_element.value().trim().to_string();
            let value = value_element.value().trim().to_string();
            
            if key.is_empty() || value.is_empty() {
                on_error.emit("Both key and value are required".to_string());
                return;
            }

            let kv = KeyValue { key, value };
            let on_success = on_success.clone();
            let on_error = on_error.clone();
            let key_elem = key_element.clone();
            let value_elem = value_element.clone();

            spawn_local(async move {
                match Request::post(&format!("{}/values", API_BASE_URL))
                    .json(&kv)
                    .unwrap()
                    .send()
                    .await
                {
                    Ok(resp) => {
                        if resp.status() == 201 {
                            on_success.emit(format!("Successfully added key '{}'", kv.key));
                            key_elem.set_value("");
                            value_elem.set_value("");
                        } else {
                            on_error.emit(format!("Error adding value: {}", resp.status()));
                        }
                    }
                    Err(_) => {
                        on_error.emit("Failed to connect to server".to_string());
                    }
                }
            });
        })
    };

    html! {
        <form onsubmit={on_submit} class="add-form">
            <div class="form-group">
                <label for="key-input">{"Key:"}</label>
                <input 
                    ref={key_input}
                    id="key-input"
                    type="text" 
                    placeholder="Enter key"
                    class="form-input"
                />
            </div>
            <div class="form-group">
                <label for="value-input">{"Value:"}</label>
                <input 
                    ref={value_input}
                    id="value-input"
                    type="text" 
                    placeholder="Enter value"
                    class="form-input"
                />
            </div>
            <button type="submit" class="submit-btn">
                {"Add Key-Value Pair"}
            </button>
        </form>
    }
}

#[derive(Properties, PartialEq)]
struct RetrieveValueFormProps {
    pub current_value: String,
    pub on_value_retrieved: Callback<String>,
    pub on_error: Callback<String>,
}

#[function_component(RetrieveValueForm)]
fn retrieve_value_form(props: &RetrieveValueFormProps) -> Html {
    let key_input = use_node_ref();

    let on_submit = {
        let key_input = key_input.clone();
        let on_value_retrieved = props.on_value_retrieved.clone();
        let on_error = props.on_error.clone();
        
        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            
            let key_element = key_input.cast::<HtmlInputElement>().unwrap();
            let key = key_element.value().trim().to_string();
            
            if key.is_empty() {
                on_error.emit("Key is required".to_string());
                return;
            }

            let on_value_retrieved = on_value_retrieved.clone();
            let on_error = on_error.clone();

            spawn_local(async move {
                match Request::get(&format!("{}/values/{}", API_BASE_URL, key))
                    .send()
                    .await
                {
                    Ok(resp) => {
                        if resp.status() == 200 {
                            match resp.json::<KeyValue>().await {
                                Ok(kv) => {
                                    on_value_retrieved.emit(format!("Key: '{}' = Value: '{}'", kv.key, kv.value));
                                }
                                Err(_) => {
                                    on_error.emit("Failed to parse response".to_string());
                                }
                            }
                        } else if resp.status() == 404 {
                            on_error.emit(format!("Key '{}' not found", key));
                        } else {
                            on_error.emit(format!("Error retrieving value: {}", resp.status()));
                        }
                    }
                    Err(_) => {
                        on_error.emit("Failed to connect to server".to_string());
                    }
                }
            });
        })
    };

    html! {
        <form onsubmit={on_submit} class="retrieve-form">
            <div class="form-group">
                <label for="retrieve-key-input">{"Key to retrieve:"}</label>
                <input 
                    ref={key_input}
                    id="retrieve-key-input"
                    type="text" 
                    placeholder="Enter key to retrieve"
                    class="form-input"
                />
            </div>
            <button type="submit" class="submit-btn">
                {"Get Value"}
            </button>
        </form>
    }
}

fn main() {
    yew::Renderer::<App>::new().render();
}
