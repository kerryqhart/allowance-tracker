use yew::prelude::*;
use web_sys::MouseEvent;
use wasm_bindgen_futures::spawn_local;
use crate::services::api::ApiClient;

#[derive(Debug, Clone, PartialEq)]
pub enum SetupWizardStep {
    Welcome,
    AddExistingChildren,
    BrowseForChild,
    Confirmation,
    Processing,
    Complete,
    Error { message: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExistingChild {
    pub name: String,
    pub path: String,
    pub valid: bool,
    pub validation_message: String,
}

#[derive(Properties, PartialEq)]
pub struct SetupWizardModalProps {
    pub is_open: bool,
    pub on_complete: Callback<()>,
    pub api_client: ApiClient,
}

#[function_component(SetupWizardModal)]
pub fn setup_wizard_modal(props: &SetupWizardModalProps) -> Html {
    let current_step = use_state(|| SetupWizardStep::Welcome);
    let existing_children = use_state(|| Vec::<ExistingChild>::new());
    let current_browse_path = use_state(|| String::new());
    let want_existing_children = use_state(|| false);

    // Reset wizard when opened
    use_effect_with(props.is_open, {
        let current_step = current_step.clone();
        let existing_children = existing_children.clone();
        let current_browse_path = current_browse_path.clone();
        let want_existing_children = want_existing_children.clone();
        
        move |is_open| {
            if *is_open {
                current_step.set(SetupWizardStep::Welcome);
                existing_children.set(Vec::new());
                current_browse_path.set(String::new());
                want_existing_children.set(false);  
            }
        }
    });

    let on_welcome_next = {
        let current_step = current_step.clone();
        Callback::from(move |_: MouseEvent| {
            current_step.set(SetupWizardStep::AddExistingChildren);
        })
    };

    // Add existing children screen handlers
    let on_yes_existing = {
        let want_existing_children = want_existing_children.clone();
        let current_step = current_step.clone();
        Callback::from(move |_: MouseEvent| {
            want_existing_children.set(true);
            current_step.set(SetupWizardStep::BrowseForChild);
        })
    };

    let on_no_existing = {
        let want_existing_children = want_existing_children.clone();
        let current_step = current_step.clone();
        Callback::from(move |_: MouseEvent| {
            want_existing_children.set(false);
            current_step.set(SetupWizardStep::Confirmation);
        })
    };

    let on_browse_next = {
        let current_step = current_step.clone();
        Callback::from(move |_: MouseEvent| {
            current_step.set(SetupWizardStep::Confirmation);
        })
    };

    let on_browse_back = {
        let current_step = current_step.clone();
        Callback::from(move |_: MouseEvent| {
            current_step.set(SetupWizardStep::AddExistingChildren);
        })
    };

    let on_confirm_setup = {
        let current_step = current_step.clone();
        let on_complete = props.on_complete.clone();
        
        Callback::from(move |_: MouseEvent| {
            current_step.set(SetupWizardStep::Processing);
            
            let current_step = current_step.clone();
            let on_complete = on_complete.clone();
            
            spawn_local(async move {
                // Simulate setup processing
                gloo::timers::callback::Timeout::new(2000, move || {
                    current_step.set(SetupWizardStep::Complete);
                    
                    gloo::timers::callback::Timeout::new(2000, move || {
                        on_complete.emit(());
                    }).forget();
                }).forget();
            });
        })
    };

    let on_confirmation_back = {
        let current_step = current_step.clone();
        let want_existing_children = want_existing_children.clone();
        Callback::from(move |_: MouseEvent| {
            if *want_existing_children {
                current_step.set(SetupWizardStep::BrowseForChild);
            } else {
                current_step.set(SetupWizardStep::AddExistingChildren);
            }
        })
    };

    if !props.is_open {
        return html! {};
    }

    html! {
        <div class="setup-wizard-backdrop">
            <div class="setup-wizard-modal">
                {match (*current_step).clone() {
                    SetupWizardStep::Welcome => html! {
                        <div class="setup-wizard-content">
                            <div class="setup-wizard-header">
                                <h1 class="setup-title">{"Welcome to Allowance Tracker!"}</h1>
                                <p class="setup-subtitle">{"Let's get you set up in just a few steps."}</p>
                            </div>
                            
                            <div class="setup-actions">
                                <button class="setup-button setup-button-primary" onclick={on_welcome_next}>
                                    {"Get Started"}
                                </button>
                            </div>
                        </div>
                    },
                    
                    SetupWizardStep::AddExistingChildren => html! {
                        <div class="setup-wizard-content">
                            <div class="setup-wizard-header">
                                <h2 class="setup-title">{"Do you have existing allowance data?"}</h2>
                                <p class="setup-subtitle">{"If you've been tracking allowances elsewhere, you can import that data."}</p>
                            </div>
                            
                            <div class="setup-body">
                                <div class="choice-options">
                                    <div class="choice-option" onclick={on_yes_existing}>
                                        <h3>{"Yes, I have existing data"}</h3>
                                        <p>{"I'll help you import your existing child data directories."}</p>
                                    </div>
                                    
                                    <div class="choice-option" onclick={on_no_existing}>
                                        <h3>{"No, start fresh"}</h3>
                                        <p>{"I'll start with a clean setup and create children later."}</p>
                                    </div>
                                </div>
                            </div>
                        </div>
                    },
                    
                    SetupWizardStep::BrowseForChild => html! {
                        <div class="setup-wizard-content">
                            <div class="setup-wizard-header">
                                <h2 class="setup-title">{"Add Existing Children"}</h2>
                                <p class="setup-subtitle">{"Enter the full path to each child's data directory."}</p>
                            </div>
                            
                            <div class="setup-body">
                                <div class="add-child-form">
                                    <div class="form-group">
                                        <label for="child-path" class="form-label">{"Child Directory Path"}</label>
                                        <div class="path-input-group">
                                            <input 
                                                id="child-path"
                                                type="text" 
                                                class="form-input"
                                                placeholder="/Users/username/Documents/OldAllowanceData/ChildName"
                                                value={(*current_browse_path).clone()}
                                            />
                                            <button 
                                                type="button" 
                                                class="add-path-button"
                                                disabled={(*current_browse_path).trim().is_empty()}
                                            >
                                                {"Add"}
                                            </button>
                                        </div>
                                        <div class="form-help">
                                            {"Enter the full path to a directory containing child.yaml and transactions.csv files."}
                                        </div>
                                    </div>
                                </div>
                            </div>
                            
                            <div class="setup-actions">
                                <button class="setup-button setup-button-secondary" onclick={on_browse_back}>
                                    {"Back"}
                                </button>  
                                <button class="setup-button setup-button-primary" onclick={on_browse_next}>
                                    {"Continue"}
                                </button>
                            </div>
                        </div>
                    },
                    
                    SetupWizardStep::Confirmation => html! {
                        <div class="setup-wizard-content">
                            <div class="setup-wizard-header">
                                <h2 class="setup-title">{"Confirm Setup"}</h2>
                                <p class="setup-subtitle">{"Review your configuration before we set everything up."}</p>
                            </div>
                            
                            <div class="setup-body">
                                <div class="confirmation-summary">
                                    <div class="summary-item">
                                        <div class="summary-content">
                                            <h3>{"Default Directory"}</h3>
                                            <p>{"~/Documents/Allowance Tracker will be created"}</p>
                                        </div>
                                    </div>
                                    
                                    {if *want_existing_children {
                                        html! {
                                            <div class="summary-item">
                                                <div class="summary-content">
                                                    <h3>{"Existing Children"}</h3>
                                                    <p>{"Ready to import existing child directories"}</p>
                                                </div>
                                            </div>
                                        }
                                    } else {
                                        html! {
                                            <div class="summary-item">
                                                <div class="summary-content">
                                                    <h3>{"Fresh Start"}</h3>
                                                    <p>{"You can create children later using the settings menu"}</p>
                                                </div>
                                            </div>
                                        }
                                    }}
                                </div>
                            </div>
                            
                            <div class="setup-actions">
                                <button class="setup-button setup-button-secondary" onclick={on_confirmation_back}>
                                    {"Back"}
                                </button>
                                <button class="setup-button setup-button-primary" onclick={on_confirm_setup}>
                                    {"Complete Setup"}
                                </button>
                            </div>
                        </div>
                    },
                    
                    SetupWizardStep::Processing => html! {
                        <div class="setup-wizard-content">
                            <div class="setup-processing">
                                <div class="processing-spinner">
                                    <div class="spinner"></div>
                                </div>
                                <h2 class="setup-title">{"Setting up Allowance Tracker..."}</h2>
                                <p class="setup-subtitle">{"Please wait while we configure your directories and import data."}</p>
                            </div>
                        </div>
                    },
                    
                    SetupWizardStep::Complete => html! {
                        <div class="setup-wizard-content">
                            <div class="setup-complete">
                                <h2 class="setup-title">{"Setup Complete!"}</h2>
                                <p class="setup-subtitle">{"Allowance Tracker is ready to use. Welcome aboard!"}</p>
                            </div>
                        </div>
                    },
                    
                    SetupWizardStep::Error { message } => html! {
                        <div class="setup-wizard-content">
                            <div class="setup-error">
                                <h2 class="setup-title">{"Setup Error"}</h2>
                                <p class="setup-subtitle">{message}</p>
                                <button class="setup-button setup-button-secondary" onclick={
                                    let current_step = current_step.clone();
                                    Callback::from(move |_: MouseEvent| {
                                        current_step.set(SetupWizardStep::Welcome);
                                    })
                                }>
                                    {"Start Over"}
                                </button>
                            </div>
                        </div>
                    }
                }}
            </div>
        </div>
    }
}
