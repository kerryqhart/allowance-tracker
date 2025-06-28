use yew::prelude::*;
use web_sys::MouseEvent;
use shared::Child;

#[derive(Properties, PartialEq)]
pub struct ProfileModalProps {
    pub is_open: bool,
    pub on_close: Callback<()>,
    pub active_child: Option<Child>,
}

#[function_component(ProfileModal)]
pub fn profile_modal(props: &ProfileModalProps) -> Html {
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

    let on_close_click = {
        let on_close = props.on_close.clone();
        Callback::from(move |_: MouseEvent| {
            on_close.emit(());
        })
    };

    // Format birthdate from YYYY-MM-DD to a more readable format
    let format_birthdate = |birthdate: &str| -> String {
        if let Ok(parts) = parse_date(birthdate) {
            let (year, month, day) = parts;
            let month_name = match month {
                1 => "January",
                2 => "February", 
                3 => "March",
                4 => "April",
                5 => "May",
                6 => "June",
                7 => "July",
                8 => "August",
                9 => "September",
                10 => "October",
                11 => "November",
                12 => "December",
                _ => return birthdate.to_string(), // fallback to original
            };
            format!("{} {}, {}", month_name, day, year)
        } else {
            birthdate.to_string() // fallback to original if parsing fails
        }
    };

    if !props.is_open {
        return html! {};
    }

    html! {
        <div class="profile-modal-backdrop" onclick={on_backdrop_click}>
            <div class="profile-modal" onclick={on_modal_click}>
                <div class="profile-modal-content">
                    <h3 class="profile-title">{"👤 Child Profile"}</h3>
                    
                    {if let Some(child) = &props.active_child {
                        html! {
                            <div class="profile-info">
                                <div class="profile-field">
                                    <label class="profile-label">{"Name"}</label>
                                    <div class="profile-value">{&child.name}</div>
                                </div>
                                
                                <div class="profile-field">
                                    <label class="profile-label">{"Birthday"}</label>
                                    <div class="profile-value">{format_birthdate(&child.birthdate)}</div>
                                </div>
                            </div>
                        }
                    } else {
                        html! {
                            <div class="profile-no-child">
                                <p>{"No active child selected"}</p>
                                <small>{"Please create a child profile first"}</small>
                            </div>
                        }
                    }}
                    
                    <div class="profile-buttons">
                        <button 
                            type="button" 
                            class="btn btn-secondary"
                            onclick={on_close_click}
                        >
                            {"Close"}
                        </button>
                    </div>
                </div>
            </div>
        </div>
    }
}

// Helper function to parse YYYY-MM-DD date format
fn parse_date(date_str: &str) -> Result<(u32, u32, u32), ()> {
    let parts: Vec<&str> = date_str.split('-').collect();
    if parts.len() != 3 {
        return Err(());
    }
    
    let year = parts[0].parse::<u32>().map_err(|_| ())?;
    let month = parts[1].parse::<u32>().map_err(|_| ())?;
    let day = parts[2].parse::<u32>().map_err(|_| ())?;
    
    Ok((year, month, day))
} 