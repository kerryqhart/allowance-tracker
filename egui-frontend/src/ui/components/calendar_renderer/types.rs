use eframe::egui;
use chrono::NaiveDate;
use shared::Transaction;
use crate::ui::app_state::OverlayType;

/// Represents the different types of day menu glyphs that can be displayed above a selected day
#[derive(Debug, Clone, PartialEq)]
pub enum DayMenuGlyph {
    AddMoney,
    SpendMoney,
}

impl DayMenuGlyph {
    /// Get the text to display for this glyph
    pub fn text(&self) -> &'static str {
        match self {
            DayMenuGlyph::AddMoney => "+$",
            DayMenuGlyph::SpendMoney => "-$",
        }
    }
    
    /// Get the overlay type this glyph should activate
    pub fn overlay_type(&self) -> OverlayType {
        match self {
            DayMenuGlyph::AddMoney => OverlayType::AddMoney,
            DayMenuGlyph::SpendMoney => OverlayType::SpendMoney,
        }
    }
    
    /// Get all available glyphs in order
    pub fn all() -> Vec<DayMenuGlyph> {
        vec![
            DayMenuGlyph::AddMoney,
            DayMenuGlyph::SpendMoney,
        ]
    }
    
    /// Get glyphs that should be shown for a specific date based on business rules
    pub fn for_date(date: NaiveDate) -> Vec<DayMenuGlyph> {
        let today = chrono::Local::now().date_naive();
        
        // Don't show glyphs for future dates (can't future-date transactions)
        if date > today {
            return Vec::new();
        }
        
        // Don't show glyphs for dates older than 45 days (prevent arbitrary backdating)
        let cutoff_date = today - chrono::Duration::days(45);
        if date < cutoff_date {
            return Vec::new();
        }
        
        // For current day and valid past days, show income and expense glyphs
        Self::all()
    }
}

/// Represents the type of calendar day for clear distinction between different day types
#[derive(Debug, Clone, PartialEq)]
pub enum CalendarDayType {
    /// A day in the current month being displayed
    CurrentMonth,
    /// A filler day (padding) from previous or next month to fill the calendar grid
    FillerDay,
}

impl CalendarDayType {
    /// Get the background color for this day type
    pub fn background_color(&self, is_today: bool) -> egui::Color32 {
        if is_today {
            // Light yellow tint for today (10% more opacity)
            egui::Color32::from_rgba_unmultiplied(255, 248, 220, 110)
        } else {
            match self {
                CalendarDayType::CurrentMonth => {
                    // Semi-transparent white background (10% more opacity)
                    egui::Color32::from_rgba_unmultiplied(255, 255, 255, 55)
                }
                CalendarDayType::FillerDay => {
                    // Darker gray for filler days (increased opacity for better visibility)
                    egui::Color32::from_rgba_unmultiplied(120, 120, 120, 120)
                }
            }
        }
    }

    /// Get the border color for this day type
    pub fn border_color(&self, is_today: bool) -> egui::Color32 {
        if is_today {
            // Pink outline for better visibility against gradient background
            egui::Color32::from_rgb(232, 150, 199)
        } else {
            match self {
                CalendarDayType::CurrentMonth => {
                    // Normal border
                    egui::Color32::from_rgba_unmultiplied(200, 200, 200, 100)
                }
                CalendarDayType::FillerDay => {
                    // Lighter border for filler days (increased opacity for better visibility)
                    egui::Color32::from_rgba_unmultiplied(150, 150, 150, 140)
                }
            }
        }
    }

    /// Get the day number text color for this day type
    pub fn day_text_color(&self) -> egui::Color32 {
        // Use normal colors for all days
        match self {
            CalendarDayType::CurrentMonth => {
                // Bold black for current month days (including today)
                egui::Color32::BLACK
            }
            CalendarDayType::FillerDay => {
                // Gray for filler days
                egui::Color32::from_rgb(150, 150, 150)
            }
        }
    }

    /// Get the balance text color for this day type
    pub fn balance_text_color(&self) -> egui::Color32 {
        match self {
            CalendarDayType::CurrentMonth => {
                // Normal gray
                egui::Color32::GRAY
            }
            CalendarDayType::FillerDay => {
                // More subdued gray for filler day balance
                egui::Color32::from_rgb(120, 120, 120)
            }
        }
    }
}

/// Represents the type of calendar transaction chip for visual distinction
#[derive(Debug, Clone, PartialEq)]
pub enum CalendarChipType {
    /// Negative amount transaction (completed)
    Expense,
    /// Positive amount transaction (completed)
    Income,
    /// Future allowance transaction (estimated)
    FutureAllowance,
    /// Ellipsis indicator for overflow transactions
    Ellipsis,
}

impl CalendarChipType {
    /// Get the primary color for this chip type
    pub fn primary_color(&self) -> egui::Color32 {
        match self {
            CalendarChipType::Expense => egui::Color32::from_rgb(128, 128, 128), // Gray for expenses
            CalendarChipType::Income => egui::Color32::from_rgb(46, 160, 67), // Green for income
            CalendarChipType::FutureAllowance => egui::Color32::from_rgb(46, 160, 67), // Green for future allowances
            CalendarChipType::Ellipsis => egui::Color32::from_rgb(120, 120, 120), // Medium gray for ellipsis
        }
    }
    
    /// Get the text color for this chip type
    pub fn text_color(&self) -> egui::Color32 {
        match self {
            CalendarChipType::Ellipsis => egui::Color32::from_rgb(120, 120, 120), // Medium gray - same as border for visibility
            _ => self.primary_color(), // Use same color as border for other chip types
        }
    }
    
    /// Whether this chip type should use a dotted border
    pub fn uses_dotted_border(&self) -> bool {
        matches!(self, CalendarChipType::FutureAllowance)
    }
}

/// Represents a transaction chip displayed on the calendar
#[derive(Debug, Clone)]
pub struct CalendarChip {
    /// The type of chip (expense, income, or future allowance)
    pub chip_type: CalendarChipType,
    /// The original transaction data
    pub transaction: Transaction,
    /// Pre-formatted display amount (e.g., "+$5.00", "-$2.50")
    pub display_amount: String,
}

impl CalendarChip {
    /// Create a new CalendarChip from a transaction
    pub fn from_transaction(transaction: Transaction, is_grid_layout: bool) -> Self {
        // Determine chip type based on transaction
        let chip_type = match transaction.transaction_type {
            shared::TransactionType::Income => CalendarChipType::Income,
            shared::TransactionType::Expense => CalendarChipType::Expense,
            shared::TransactionType::FutureAllowance => CalendarChipType::FutureAllowance,
        };
        
        // Format display amount based on type and layout
        let display_amount = if transaction.amount > 0.0 {
            if is_grid_layout {
                format!("+${:.2}", transaction.amount)
            } else {
                format!("+${:.0}", transaction.amount)
            }
        } else {
            if is_grid_layout {
                format!("-${:.2}", transaction.amount.abs())
            } else {
                format!("-${:.0}", transaction.amount.abs())
            }
        };
        
        Self {
            chip_type,
            transaction,
            display_amount,
        }
    }
    
    /// Convert a vector of transactions to calendar chips
    pub fn from_transactions(transactions: Vec<Transaction>, is_grid_layout: bool) -> Vec<Self> {
        transactions.into_iter()
            .map(|transaction| Self::from_transaction(transaction, is_grid_layout))
            .collect()
    }
    
    /// Create an ellipsis chip to indicate overflow transactions
    pub fn create_ellipsis() -> Self {
        // Create a dummy transaction for the ellipsis chip (only the display_amount matters)
        let dummy_transaction = Transaction {
            id: "ellipsis".to_string(),
            child_id: "ellipsis".to_string(),
            amount: 0.0,
            description: "Click to see more transactions".to_string(),
            date: chrono::Utc::now().with_timezone(&chrono::FixedOffset::east_opt(0).unwrap()),
            balance: 0.0,
            transaction_type: shared::TransactionType::Income, // Dummy type
        };
        
        Self {
            chip_type: CalendarChipType::Ellipsis,
            transaction: dummy_transaction,
            display_amount: "...".to_string(),
        }
    }
}

/// Represents a single day in the calendar with its associated state and rendering logic
pub struct CalendarDay {
    /// The day number (1-31)
    pub day_number: u32,
    /// The full date for this day
    pub date: NaiveDate,
    /// Whether this day is today
    pub is_today: bool,
    /// The type of day (current month or filler day)
    pub day_type: CalendarDayType,
    /// Transactions that occurred on this day
    pub transactions: Vec<Transaction>,
    /// The balance at the end of this day (for current month days only)
    pub balance: Option<f64>,
}

/// Configuration for calendar day rendering
pub struct RenderConfig {
    pub is_grid_layout: bool,
    pub enable_click_handler: bool,
    pub is_selected: bool,
    // Transaction selection state (for deletion mode)
    pub transaction_selection_mode: bool,
    pub selected_transaction_ids: std::collections::HashSet<String>,
    pub expanded_day: Option<NaiveDate>,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            is_grid_layout: false,
            enable_click_handler: false,
            is_selected: false,
            transaction_selection_mode: false,
            selected_transaction_ids: std::collections::HashSet::new(),
            expanded_day: None,
        }
    }
}

impl CalendarDay {
    /// Create a new CalendarDay instance
    pub fn new(day_number: u32, date: NaiveDate, is_today: bool, day_type: CalendarDayType) -> Self {
        Self {
            day_number,
            date,
            is_today,
            day_type,
            transactions: Vec::new(),
            balance: None,
        }
    }

    /// Add a transaction to this day
    pub fn add_transaction(&mut self, transaction: Transaction) {
        // Update balance from the transaction (this is the balance after the transaction)
        self.balance = Some(transaction.balance);
        self.transactions.push(transaction);
    }
} 