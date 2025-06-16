use yew::prelude::*;
use shared::{FormattedTransaction, AmountType};

#[derive(Properties, PartialEq)]
pub struct TransactionTableProps {
    pub transactions: Vec<FormattedTransaction>,
    pub loading: bool,
}

#[function_component(TransactionTable)]
pub fn transaction_table(props: &TransactionTableProps) -> Html {
    html! {
        <section class="transactions-section">
            <h2>{"Recent Transactions"}</h2>
            
            {if props.loading {
                html! { <div class="loading">{"Loading transactions..."}</div> }
            } else {
                html! {
                    <div class="table-container">
                        <table class="transactions-table">
                            <thead>
                                <tr>
                                    <th>{"Date"}</th>
                                    <th>{"Description"}</th>
                                    <th>{"Amount"}</th>
                                    <th>{"Balance"}</th>
                                </tr>
                            </thead>
                            <tbody>
                                {for props.transactions.iter().map(|transaction| {
                                    // Use backend-provided CSS class based on amount type
                                    let amount_class = match transaction.amount_type {
                                        AmountType::Positive => "amount positive",
                                        AmountType::Negative => "amount negative",
                                        AmountType::Zero => "amount zero",
                                    };
                                    
                                    html! {
                                        <tr>
                                            <td class="date">{&transaction.formatted_date}</td>
                                            <td class="description">{&transaction.description}</td>
                                            <td class={amount_class}>
                                                {&transaction.formatted_amount}
                                            </td>
                                            <td class="balance">{&transaction.formatted_balance}</td>
                                        </tr>
                                    }
                                })}
                            </tbody>
                        </table>
                    </div>
                }
            }}
        </section>
    }
} 