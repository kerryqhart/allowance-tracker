use gloo::net::http::Request;
use shared::{Child, Transaction, TransactionType};
use wasm_bindgen_futures::spawn_local;
use web_sys::HtmlInputElement;
use yew::prelude::*;

const API_BASE_URL: &str = "http://localhost:3000/api";

#[function_component(App)]
fn app() -> Html {
    let children = use_state(|| Vec::<Child>::new());
    let selected_child = use_state(|| None::<Child>);
    let transactions = use_state(|| Vec::<Transaction>::new());
    let error_message = use_state(String::new);
    
    // Fetch all children when the app loads
    {
        let children = children.clone();
        let error_message = error_message.clone();

        use_effect(move || {
            spawn_local(async move {
                match Request::get(&format!("{}/children", API_BASE_URL))
                    .send()
                    .await
                {
                    Ok(resp) => {
                        if resp.status() == 200 {
                            match resp.json::<Vec<Child>>().await {
                                Ok(data) => children.set(data),
                                Err(_) => error_message.set("Failed to parse children data".to_string()),
                            }
                        } else {
                            error_message.set(format!("Error: {}", resp.status()))
                        }
                    }
                    Err(_) => error_message.set("Failed to fetch children".to_string()),
                }
            });
            || ()
        });
    }

    // Fetch transactions when a child is selected
    let selected_child_for_effect = selected_child.clone();
    {
        let transactions = transactions.clone();
        let error_message = error_message.clone();
        
        use_effect_with(
            move |selected: &UseStateHandle<Option<Child>>| {
                if let Some(child) = selected.as_ref() {
                    let child_id = child.id.to_string();
                    spawn_local(async move {
                        match Request::get(&format!("{}/children/{}/transactions", API_BASE_URL, child_id))
                            .send()
                            .await
                        {
                            Ok(resp) => {
                                if resp.status() == 200 {
                                    match resp.json::<Vec<Transaction>>().await {
                                        Ok(data) => transactions.set(data),
                                        Err(_) => error_message.set("Failed to parse transactions".to_string()),
                                    }
                                } else {
                                    error_message.set(format!("Error: {}", resp.status()))
                                }
                            }
                            Err(_) => error_message.set("Failed to fetch transactions".to_string()),
                        }
                    });
                }
                || ()
            },
            selected_child_for_effect,
        );
    }

    let on_child_select = {
        let selected_child = selected_child.clone();
        let children = children.clone();
        
        Callback::from(move |child_id: String| {
            if let Ok(uuid) = uuid::Uuid::parse_str(&child_id) {
                for child in children.iter() {
                    if child.id == uuid {
                        selected_child.set(Some(child.clone()));
                        return;
                    }
                }
            }
            selected_child.set(None);
        })
    };

    let on_new_child = {
        let children = children.clone();
        let error_message = error_message.clone();
        
        Callback::from(move |new_child: Child| {
            let children = children.clone();
            let error_message = error_message.clone();
            
            spawn_local(async move {
                match Request::post(&format!("{}/children", API_BASE_URL))
                    .json(&new_child)
                    .expect("Failed to serialize")
                    .send()
                    .await
                {
                    Ok(resp) => {
                        if resp.status() == 201 {
                            match resp.json::<Child>().await {
                                Ok(created_child) => {
                                    let mut updated_children = (*children).clone();
                                    updated_children.push(created_child);
                                    children.set(updated_children);
                                },
                                Err(_) => error_message.set("Failed to parse created child".to_string()),
                            }
                        } else {
                            error_message.set(format!("Error creating child: {}", resp.status()))
                        }
                    }
                    Err(_) => error_message.set("Failed to create child".to_string()),
                }
            });
        })
    };

    html! {
        <div class="container">
            <nav class="navbar is-primary" role="navigation">
                <div class="navbar-brand">
                    <a class="navbar-item" href="#">
                        <i class="fas fa-piggy-bank mr-2"></i>
                        <span class="has-text-weight-bold">{"Allowance Tracker"}</span>
                    </a>
                </div>
            </nav>
            
            if !error_message.is_empty() {
                <div class="notification is-danger">
                    <button class="delete" onclick={let error = error_message.clone(); Callback::from(move |_| error.set(String::new()))}></button>
                    {error_message.as_str()}
                </div>
            }

            <div class="columns">
                <div class="column is-3">
                    <ChildrenList 
                        children={(*children).clone()} 
                        on_select={on_child_select} 
                        selected_child_id={selected_child.as_ref().map(|c| c.id)}
                    />
                    <NewChildForm on_add={on_new_child} />
                </div>
                <div class="column">
                    {
                        if let Some(child) = (*selected_child).clone() {
                            html! {
                                <ChildDetail 
                                    child={child.clone()} 
                                    transactions={(*transactions).clone()} 
                                />
                            }
                        } else {
                            html! {
                                <div class="notification is-info">
                                    {"Select a child from the list to view or add transactions"}
                                </div>
                            }
                        }
                    }
                </div>
            </div>
        </div>
    }
}

#[derive(Properties, PartialEq)]
struct ChildrenListProps {
    children: Vec<Child>,
    on_select: Callback<String>,
    selected_child_id: Option<uuid::Uuid>,
}

#[function_component(ChildrenList)]
fn children_list(props: &ChildrenListProps) -> Html {
    html! {
        <div class="panel">
            <p class="panel-heading">
                {"Children"}
            </p>
            {
                if props.children.is_empty() {
                    html! {
                        <div class="panel-block">
                            {"No children added yet. Add a new child below."}
                        </div>
                    }
                } else {
                    html! {
                        for props.children.iter().map(|child| {
                            let is_active = props.selected_child_id
                                .map(|id| id == child.id)
                                .unwrap_or(false);
                            let child_id = child.id.to_string();
                            let on_click = {
                                let on_select = props.on_select.clone();
                                let id = child_id.clone();
                                Callback::from(move |_| on_select.emit(id.clone()))
                            };
                            
                            html! {
                                <a class={classes!("panel-block", is_active.then(|| "is-active"))}
                                   onclick={on_click}>
                                    <span class="panel-icon">
                                        <i class="fas fa-user"></i>
                                    </span>
                                    <div class="is-flex is-justify-content-space-between is-flex-grow-1">
                                        <span>{&child.name}</span>
                                        <span class="has-text-weight-bold">
                                            {format!("${:.2}", child.balance)}
                                        </span>
                                    </div>
                                </a>
                            }
                        })
                    }
                }
            }
        </div>
    }
}

#[derive(Properties, PartialEq)]
struct NewChildFormProps {
    on_add: Callback<Child>,
}

#[function_component(NewChildForm)]
fn new_child_form(props: &NewChildFormProps) -> Html {
    let name = use_state(String::new);
    let weekly_allowance = use_state(|| "0.00".to_string());
    let is_expanded = use_state(|| false);

    let toggle_form = {
        let is_expanded = is_expanded.clone();
        Callback::from(move |_| {
            is_expanded.set(!*is_expanded);
        })
    };

    let on_submit = {
        let on_add = props.on_add.clone();
        let name = name.clone();
        let weekly_allowance = weekly_allowance.clone();
        let is_expanded = is_expanded.clone();
        
        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            
            if !name.is_empty() {
                if let Ok(amount) = weekly_allowance.parse::<f64>() {
                    on_add.emit(Child {
                        id: uuid::Uuid::new_v4(),
                        name: (*name).clone(),
                        balance: 0.0,
                        weekly_allowance: amount,
                    });
                    name.set(String::new());
                    weekly_allowance.set("0.00".to_string());
                    is_expanded.set(false);
                }
            }
        })
    };

    html! {
        <div class="card">
            <header class="card-header">
                <p class="card-header-title">
                    {"Add New Child"}
                </p>
                <button class="card-header-icon" aria-label="toggle form" onclick={toggle_form}>
                    <span class="icon">
                        <i class={classes!("fas", if *is_expanded {"fa-angle-down"} else {"fa-angle-up"})}></i>
                    </span>
                </button>
            </header>
            
            if *is_expanded {
                <div class="card-content">
                    <form onsubmit={on_submit}>
                        <div class="field">
                            <label class="label">{"Name"}</label>
                            <div class="control">
                                <input 
                                    class="input" 
                                    type="text" 
                                    placeholder="Child's name"
                                    value={(*name).clone()}
                                    onchange={{
                                        let name = name.clone();
                                        Callback::from(move |e: Event| {
                                            let input: HtmlInputElement = e.target_unchecked_into();
                                            name.set(input.value());
                                        })
                                    }}
                                />
                            </div>
                        </div>

                        <div class="field">
                            <label class="label">{"Weekly Allowance"}</label>
                            <div class="control has-icons-left">
                                <input 
                                    class="input" 
                                    type="number" 
                                    step="0.01"
                                    min="0"
                                    placeholder="0.00"
                                    value={(*weekly_allowance).clone()}
                                    onchange={{
                                        let weekly_allowance = weekly_allowance.clone();
                                        Callback::from(move |e: Event| {
                                            let input: HtmlInputElement = e.target_unchecked_into();
                                            weekly_allowance.set(input.value());
                                        })
                                    }}
                                />
                                <span class="icon is-small is-left">
                                    <i class="fas fa-dollar-sign"></i>
                                </span>
                            </div>
                        </div>

                        <div class="field">
                            <div class="control">
                                <button class="button is-primary" type="submit">
                                    {"Add Child"}
                                </button>
                            </div>
                        </div>
                    </form>
                </div>
            }
        </div>
    }
}

#[derive(Properties, PartialEq)]
struct ChildDetailProps {
    child: Child,
    transactions: Vec<Transaction>,
}

#[function_component(ChildDetail)]
fn child_detail(props: &ChildDetailProps) -> Html {
    html! {
        <>
            <div class="card">
                <div class="card-content">
                    <div class="media">
                        <div class="media-left">
                            <figure class="image is-48x48">
                                <div class="is-flex is-justify-content-center is-align-items-center has-background-money" style="width: 48px; height: 48px; border-radius: 50%;">
                                    <i class="fas fa-child fa-2x"></i>
                                </div>
                            </figure>
                        </div>
                        <div class="media-content">
                            <p class="title is-4">{&props.child.name}</p>
                            <p class="subtitle is-6">
                                {format!("Weekly Allowance: ${:.2}", props.child.weekly_allowance)}
                            </p>
                        </div>
                        <div class="media-right">
                            <div class="box has-text-centered p-3">
                                <p class="heading">{"Current Balance"}</p>
                                <p class="title">{format!("${:.2}", props.child.balance)}</p>
                            </div>
                        </div>
                    </div>
                </div>
            </div>

            <div class="card">
                <div class="card-header">
                    <p class="card-header-title">
                        {"Transactions"}
                    </p>
                </div>
                <div class="card-content">
                    {
                        if props.transactions.is_empty() {
                            html! {
                                <div class="notification">
                                    {"No transactions yet for this child."}
                                </div>
                            }
                        } else {
                            html! {
                                <div class="table-container">
                                    <table class="table is-fullwidth">
                                        <thead>
                                            <tr>
                                                <th>{"Date"}</th>
                                                <th>{"Description"}</th>
                                                <th>{"Type"}</th>
                                                <th class="has-text-right">{"Amount"}</th>
                                            </tr>
                                        </thead>
                                        <tbody>
                                            {
                                                for props.transactions.iter().map(|transaction| {
                                                    let amount_class = match transaction.transaction_type {
                                                        TransactionType::Allowance | TransactionType::Bonus => "has-text-success",
                                                        TransactionType::Purchase => "has-text-danger",
                                                        TransactionType::Savings => "has-text-info",
                                                    };
                                                    
                                                    html! {
                                                        <tr>
                                                            <td>{transaction.date.to_string()}</td>
                                                            <td>{&transaction.description}</td>
                                                            <td>{format!("{:?}", transaction.transaction_type)}</td>
                                                            <td class={classes!("has-text-right", amount_class)}>
                                                                {format!("${:.2}", transaction.amount)}
                                                            </td>
                                                        </tr>
                                                    }
                                                })
                                            }
                                        </tbody>
                                    </table>
                                </div>
                            }
                        }
                    }
                </div>
            </div>
            
            // Transaction form would go here
        </>
    }
}

fn main() {
    yew::Renderer::<App>::new().render();
}
