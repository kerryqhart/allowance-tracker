use yew::prelude::*;
use web_sys::{HtmlInputElement, MouseEvent};

#[derive(Debug, Clone, PartialEq)]
pub enum ChallengeStep {
    Initial,
    SecondChallenge,
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

    // Reset state when modal opens
    use_effect_with(props.is_open, {
        let current_step = current_step.clone();
        let answer_input = answer_input.clone();
        move |is_open| {
            if *is_open {
                current_step.set(ChallengeStep::Initial);
                answer_input.set(String::new());
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
        let on_success = props.on_success.clone();
        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            let answer = (*answer_input).clone();
            if answer.to_lowercase().trim() == "ice cold" {
                current_step.set(ChallengeStep::Success);
                on_success.emit(());
            } else {
                // Wrong answer - could add error feedback here
                answer_input.set(String::new());
            }
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