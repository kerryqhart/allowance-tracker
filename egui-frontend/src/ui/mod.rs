pub mod fonts;
pub mod mappers;
pub mod components;
pub mod app_state;
pub mod app_coordinator;
pub mod state;  // NEW: Organized state management

pub use fonts::*;
pub use mappers::*;
pub use components::*;
// pub use app_state::*;  // Temporarily disabled to avoid ambiguous exports
pub use app_state::AllowanceTrackerApp;  // Keep the main app struct available
pub use state::*;  // NEW: Re-export organized state 