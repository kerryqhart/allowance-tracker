use yew::prelude::*;
use shared::FormattedTransaction;
use super::transaction_table::TransactionTable;
use super::rust_chart::RustChart;

#[derive(Properties, PartialEq)]
pub struct TransactionViewContainerProps {
    pub transactions: Vec<FormattedTransaction>,
    pub loading: bool,
}

#[derive(Clone, PartialEq)]
pub enum ViewType {
    Table,
    Chart,
}

pub enum Msg {
    SwitchToTable,
    SwitchToChart,
}

pub struct TransactionViewContainer {
    current_view: ViewType,
}

impl Component for TransactionViewContainer {
    type Message = Msg;
    type Properties = TransactionViewContainerProps;

    fn create(_ctx: &Context<Self>) -> Self {
        Self {
            current_view: ViewType::Table,
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::SwitchToTable => {
                self.current_view = ViewType::Table;
                true
            }
            Msg::SwitchToChart => {
                self.current_view = ViewType::Chart;
                true
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let loading = ctx.props().loading;

        html! {
            <div class="transaction-view-container">
                // Header with view toggle buttons
                <div class="transaction-view-header">
                    <h2>{"Recent Transactions"}</h2>
                    
                    // Icon-based view toggle buttons
                    <div class="view-toggle-buttons">
                        <button 
                            class={classes!("view-toggle-btn", 
                                if matches!(self.current_view, ViewType::Table) { Some("active") } else { None })}
                            onclick={ctx.link().callback(|_| Msg::SwitchToTable)}
                            title="Table View"
                        >
                            <i class="fas fa-table"></i>
                            <span class="btn-label">{"Table"}</span>
                        </button>
                        
                        <button 
                            class={classes!("view-toggle-btn", 
                                if matches!(self.current_view, ViewType::Chart) { Some("active") } else { None })}
                            onclick={ctx.link().callback(|_| Msg::SwitchToChart)}
                            title="Chart View"
                        >
                            <i class="fas fa-chart-line"></i>
                            <span class="btn-label">{"Chart"}</span>
                        </button>
                    </div>
                </div>

                // Content area with both views
                <div class="transaction-views">
                    // Table view
                    <div class={classes!("view-content", "table-view", 
                        if matches!(self.current_view, ViewType::Table) { Some("active") } else { None })}>
                        <TransactionTable 
                            transactions={ctx.props().transactions.clone()}
                            loading={loading}
                        />
                    </div>
                    
                    // Chart view
                    <div class={classes!("view-content", "chart-view", 
                        if matches!(self.current_view, ViewType::Chart) { Some("active") } else { None })}>
                        <RustChart 
                            transactions={ctx.props().transactions.clone()}
                            loading={loading}
                        />
                    </div>
                </div>
            </div>
        }
    }
} 