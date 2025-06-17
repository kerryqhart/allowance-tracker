use yew::prelude::*;
use shared::CalendarMonth;
use crate::services::date_utils::format_calendar_date;

#[derive(Properties, PartialEq)]
pub struct CalendarProps {
    pub calendar_data: CalendarMonth,
    pub delete_mode: bool,
    pub selected_transactions: Vec<String>,
    pub on_toggle_transaction_selection: Callback<String>,
    pub on_delete_selected: Callback<()>,
}

#[function_component(Calendar)]
pub fn calendar(props: &CalendarProps) -> Html {
    let calendar_data = &props.calendar_data;
    
    // Create calendar grid using backend-provided data
    let mut calendar_days = Vec::new();
    
    // Add empty cells for days before the first day of month
    for _ in 0..calendar_data.first_day_of_week {
        calendar_days.push(html! {
            <div class="calendar-day empty"></div>
        });
    }
    
    // Add days of the month using backend-provided day data
    for day_data in &calendar_data.days {
        let day_class = if props.delete_mode {
            "calendar-day delete-mode"
        } else {
            "calendar-day"
        };
        
        calendar_days.push(html! {
            <div class={day_class}>
                <div class="day-header">
                    <div class="day-number">{day_data.day}</div>
                    <div class="day-balance-subtle">
                        {format!("${:.0}", day_data.balance)}
                    </div>
                </div>
                
                <div class="day-transactions">
                    {for day_data.transactions.iter().map(|transaction| {
                        let chip_class = if transaction.amount >= 0.0 {
                            "transaction-chip positive"
                        } else {
                            "transaction-chip negative"
                        };
                        
                        // Create detailed tooltip content
                        let tooltip_content = format!(
                            "💰 {}\n💵 Amount: ${:.2}\n📅 Date: {}\n💳 Balance: ${:.2}",
                            transaction.description,
                            transaction.amount,
                            format_calendar_date(&transaction.date),
                            transaction.balance
                        );
                        
                        html! {
                            <div class={format!("{} transaction-tooltip", chip_class)} 
                                 title={tooltip_content}
                                 data-description={transaction.description.clone()}
                                 data-amount={format!("{:.2}", transaction.amount)}
                                 data-date={format_calendar_date(&transaction.date)}
                                 data-balance={format!("{:.2}", transaction.balance)}>
                                
                                // Show checkbox in delete mode
                                {if props.delete_mode {
                                    let transaction_id = transaction.id.clone();
                                    let is_selected = props.selected_transactions.contains(&transaction_id);
                                    let on_toggle = props.on_toggle_transaction_selection.clone();
                                    
                                    html! {
                                        <input 
                                            type="checkbox" 
                                            class="transaction-checkbox"
                                            checked={is_selected}
                                            onchange={{
                                                let transaction_id = transaction_id.clone();
                                                Callback::from(move |_| {
                                                    on_toggle.emit(transaction_id.clone());
                                                })
                                            }}
                                        />
                                    }
                                } else {
                                    html! {}
                                }}
                                
                                <span class="transaction-amount">
                                    {if transaction.amount >= 0.0 {
                                        format!("+${:.0}", transaction.amount)
                                    } else {
                                        format!("-${:.0}", transaction.amount.abs())
                                    }}
                                </span>
                                
                                // Custom tooltip div that will be shown on hover (only if not in delete mode)
                                {if !props.delete_mode {
                                    html! {
                                        <div class="custom-tooltip">
                                            <div class="tooltip-header">
                                                <strong>{&transaction.description}</strong>
                                            </div>
                                            <div class="tooltip-body">
                                                <div class="tooltip-row">
                                                    <span class="tooltip-label">{"💵 Amount:"}</span>
                                                    <span class={if transaction.amount >= 0.0 { "tooltip-value positive" } else { "tooltip-value negative" }}>
                                                        {format!("${:.2}", transaction.amount)}
                                                    </span>
                                                </div>
                                                <div class="tooltip-row">
                                                    <span class="tooltip-label">{"📅 Date:"}</span>
                                                    <span class="tooltip-value">{format_calendar_date(&transaction.date)}</span>
                                                </div>
                                                <div class="tooltip-row">
                                                    <span class="tooltip-label">{"💳 Balance:"}</span>
                                                    <span class="tooltip-value">{format!("${:.2}", transaction.balance)}</span>
                                                </div>
                                            </div>
                                        </div>
                                    }
                                } else {
                                    html! {}
                                }}
                            </div>
                        }
                    })}
                </div>
            </div>
        });
    }
    
    html! {
        <div class="calendar">
            // Delete button that appears when transactions are selected
            {if props.delete_mode && !props.selected_transactions.is_empty() {
                html! {
                    <div class="delete-actions-bar">
                        <div class="delete-info">
                            {format!("{} transaction{} selected", 
                                props.selected_transactions.len(),
                                if props.selected_transactions.len() == 1 { "" } else { "s" }
                            )}
                        </div>
                        <button 
                            class="delete-button"
                            onclick={{
                                let on_delete = props.on_delete_selected.clone();
                                Callback::from(move |_: web_sys::MouseEvent| {
                                    on_delete.emit(());
                                })
                            }}
                            title="Delete selected transactions"
                        >
                            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                                <path d="M6 19c0 1.1.9 2 2 2h8c1.1 0 2-.9 2-2V7H6v12zM19 4h-3.5l-1-1h-5l-1 1H5v2h14V4z" fill="currentColor"/>
                            </svg>
                            {"Delete"}
                        </button>
                    </div>
                }
            } else {
                html! {}
            }}
            
            <div class="calendar-weekdays">
                <div class="weekday">{"Sun"}</div>
                <div class="weekday">{"Mon"}</div>
                <div class="weekday">{"Tue"}</div>
                <div class="weekday">{"Wed"}</div>
                <div class="weekday">{"Thu"}</div>
                <div class="weekday">{"Fri"}</div>
                <div class="weekday">{"Sat"}</div>
            </div>
            <div class="calendar-grid">
                {for calendar_days}
            </div>
        </div>
    }
} 