pub mod constants;
pub mod fonts;
pub mod mappers;
pub mod components;
pub mod app_state;
pub mod app_coordinator;
pub mod state;  // NEW: Organized state management
pub mod form_state;
// test_support lives at src/ui/test_support.rs but is declared inside
// app_coordinator.rs (as `ui::app_coordinator::test_support`) rather than
// registered here as a sibling of app_coordinator — see the `#[path =
// "test_support.rs"] pub mod test_support;` declaration in app_coordinator.rs
// for why: it needs private-method access that a sibling module cannot get.

pub use fonts::*;
pub use mappers::*;
pub use components::*;
// pub use app_state::*;  // Temporarily disabled to avoid ambiguous exports
pub use app_state::AllowanceTrackerApp;  // Keep the main app struct available
pub use state::*;  // NEW: Re-export organized state
pub use form_state::*; 