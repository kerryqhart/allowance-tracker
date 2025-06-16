use yew::prelude::*;
use shared::CalendarMonth;
use crate::services::date_utils::format_calendar_date;

#[derive(Properties, PartialEq)]
pub struct CalendarProps {
    pub calendar_data: CalendarMonth,
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
        calendar_days.push(html! {
            <div class="calendar-day">
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
                                {if transaction.amount >= 0.0 {
                                    format!("+${:.0}", transaction.amount)
                                } else {
                                    format!("-${:.0}", transaction.amount.abs())
                                }}
                                
                                // Custom tooltip div that will be shown on hover
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
                            </div>
                        }
                    })}
                </div>
            </div>
        });
    }
    
    html! {
        <div class="calendar">
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