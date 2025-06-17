use yew::prelude::*;
use web_sys::{HtmlInputElement, MouseEvent};
use wasm_bindgen_futures::spawn_local;
use crate::services::api::ApiClient;

#[derive(Debug, Clone, PartialEq)]
pub enum ChallengeStep {
    Initial,
    SecondChallenge,
    Validating,
    Success,
}

#[derive(Properties, PartialEq)]
pub struct ChallengeModalProps {
    pub is_open: bool,
    pub on_success: Callback<()>,
    pub on_close: Callback<()>,
}

#[function_component(ChallengeModal)]
pub fn challenge_modal(props: &ChallengeModalProps) -> Html {
    let current_step = use_state(|| ChallengeStep::Initial);
    let answer_input = use_state(|| String::new());
    let error_message = use_state(|| Option::<String>::None);
    let api_client = ApiClient::new();

    // Reset state when modal opens
    use_effect_with(props.is_open, {
        let current_step = current_step.clone();
        let answer_input = answer_input.clone();
        let error_message = error_message.clone();
        move |is_open| {
            if *is_open {
                current_step.set(ChallengeStep::Initial);
                answer_input.set(String::new());
                error_message.set(None);
            }
            || ()
        }
    });

    let on_yes_click = {
        let current_step = current_step.clone();
        Callback::from(move |_: MouseEvent| {
            current_step.set(ChallengeStep::SecondChallenge);
        })
    };

    let on_no_click = {
        let on_close = props.on_close.clone();
        Callback::from(move |_: MouseEvent| {
            on_close.emit(());
        })
    };

    let on_input_change = {
        let answer_input = answer_input.clone();
        Callback::from(move |e: Event| {
            let input: HtmlInputElement = e.target_unchecked_into();
            answer_input.set(input.value());
        })
    };

    let on_submit_answer = {
        let answer_input = answer_input.clone();
        let current_step = current_step.clone();
        let error_message = error_message.clone();
        let on_success = props.on_success.clone();
        let api_client = api_client.clone();
        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            let answer = (*answer_input).clone().trim().to_string();
            
            if answer.is_empty() {
                error_message.set(Some("Please enter an answer".to_string()));
                return;
            }
            
            // Set loading state
            current_step.set(ChallengeStep::Validating);
            error_message.set(None);
            
            // Make API call
            let answer_input = answer_input.clone();
            let current_step = current_step.clone();
            let error_message = error_message.clone();
            let on_success = on_success.clone();
            let api_client = api_client.clone();
            
            spawn_local(async move {
                match api_client.validate_parental_control(&answer).await {
                    Ok(response) => {
                        if response.success {
                            current_step.set(ChallengeStep::Success);
                            on_success.emit(());
                        } else {
                            current_step.set(ChallengeStep::SecondChallenge);
                            error_message.set(Some("Incorrect answer. Try again!".to_string()));
                            answer_input.set(String::new());
                        }
                    }
                    Err(e) => {
                        current_step.set(ChallengeStep::SecondChallenge);
                        error_message.set(Some(format!("Network error: {}", e)));
                        answer_input.set(String::new());
                    }
                }
            });
        })
    };

    let on_backdrop_click = {
        let on_close = props.on_close.clone();
        Callback::from(move |e: MouseEvent| {
            e.stop_propagation();
            on_close.emit(());
        })
    };

    let on_modal_click = Callback::from(|e: MouseEvent| {
        e.stop_propagation();
    });

    if !props.is_open {
        return html! {};
    }

    html! {
        <div class="challenge-modal-backdrop" onclick={on_backdrop_click}>
            <div class="challenge-modal" onclick={on_modal_click}>
                <div class="challenge-modal-content">
                    {match (*current_step).clone() {
                        ChallengeStep::Initial => html! {
                            <>
                                <h3 class="challenge-title">{"🔒 Settings Access"}</h3>
                                <p class="challenge-question">{"Are you Mom or Dad?"}</p>
                                <div class="challenge-buttons">
                                    <button class="btn btn-primary" onclick={on_yes_click}>
                                        {"Yes"}
                                    </button>
                                    <button class="btn btn-secondary" onclick={on_no_click}>
                                        {"No"}
                                    </button>
                                </div>
                            </>
                        },
                        ChallengeStep::SecondChallenge => html! {
                            <>
                                <h3 class="challenge-title">{"🤔 Prove It!"}</h3>
                                <p class="challenge-question">{"Oh yeah? If so, what's cooler than cool???"}</p>
                                {if let Some(error) = (*error_message).clone() {
                                    html! {
                                        <div class="challenge-error">
                                            {error}
                                        </div>
                                    }
                                } else {
                                    html! {}
                                }}
                                <form class="challenge-form" onsubmit={on_submit_answer}>
                                    <input
                                        type="text"
                                        class="challenge-input"
                                        placeholder="Your answer..."
                                        value={(*answer_input).clone()}
                                        onchange={on_input_change}
                                        autofocus=true
                                    />
                                    <div class="challenge-buttons">
                                        <button type="submit" class="btn btn-primary">
                                            {"Submit"}
                                        </button>
                                        <button type="button" class="btn btn-secondary" onclick={on_no_click}>
                                            {"Cancel"}
                                        </button>
                                    </div>
                                </form>
                            </>
                        },
                        ChallengeStep::Validating => html! {
                            <>
                                <h3 class="challenge-title">{"⏳ Validating..."}</h3>
                                <p class="challenge-question">{"Checking your answer..."}</p>
                                <div class="challenge-spinner">
                                    <div class="spinner"></div>
                                </div>
                            </>
                        },
                        ChallengeStep::Success => html! {
                            <>
                                <h3 class="challenge-title">{"✅ Access Granted!"}</h3>
                                <p class="challenge-question">{"Welcome to the settings!"}</p>
                            </>
                        }
                    }}
                </div>
            </div>
        </div>
    }
} 