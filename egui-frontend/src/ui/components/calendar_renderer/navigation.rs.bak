use chrono::{NaiveDate, Datelike};
use shared::Transaction;
use crate::ui::app_state::AllowanceTrackerApp;
use super::types::{CalendarDay, CalendarDayType};

impl AllowanceTrackerApp {
    /// Navigate to a different month
    pub fn navigate_month(&mut self, delta: i32) {
        let old_month = self.selected_month;
        let old_year = self.selected_year;
        
        println!("🗓️  Navigating from {}/{} with delta {}", old_month, old_year, delta);
        
        if delta > 0 {
            if self.selected_month == 12 {
                self.selected_month = 1;
                self.selected_year += 1;
            } else {
                self.selected_month += 1;
            }
        } else if delta < 0 {
            if self.selected_month == 1 {
                self.selected_month = 12;
                self.selected_year -= 1;
            } else {
                self.selected_month -= 1;
            }
        }
        
        println!("🗓️  Navigation complete: {}/{} → {}/{}", 
                  old_month, old_year, self.selected_month, self.selected_year);
        
        if self.selected_month == 6 {
            println!("🗓️  🎯 Navigated to June {} - about to load calendar data", self.selected_year);
        }
        
        self.load_calendar_data();
        
        println!("🔄 Calendar data reloaded for {}/{}", self.selected_month, self.selected_year);
    }

    /// Convert backend CalendarDay to frontend CalendarDay structure
    pub fn convert_backend_calendar_day(&self, backend_day: &shared::CalendarDay, day_index: usize) -> CalendarDay {
        // Convert day type from backend to frontend enum
        let day_type = match backend_day.day_type {
            shared::CalendarDayType::MonthDay => CalendarDayType::CurrentMonth,
            shared::CalendarDayType::PaddingBefore | shared::CalendarDayType::PaddingAfter => CalendarDayType::FillerDay,
        };
        
        // Create date for this day
        let date = if backend_day.day == 0 {
            // For filler days, calculate the actual previous/next month days they represent
            match backend_day.day_type {
                shared::CalendarDayType::PaddingBefore => {
                    // Calculate previous month date
                    let (prev_year, prev_month) = if self.selected_month == 1 {
                        (self.selected_year - 1, 12)
                    } else {
                        (self.selected_year, self.selected_month - 1)
                    };
                    
                    // Get the last day of previous month
                    let prev_month_first = NaiveDate::from_ymd_opt(prev_year, prev_month, 1).unwrap();
                    let next_month_first = if prev_month == 12 {
                        NaiveDate::from_ymd_opt(prev_year + 1, 1, 1).unwrap()
                    } else {
                        NaiveDate::from_ymd_opt(prev_year, prev_month + 1, 1).unwrap()
                    };
                    let days_in_prev_month = (next_month_first - prev_month_first).num_days() as u32;
                    
                    // Calculate which day of previous month this represents
                    // First day of current month
                    let current_month_first = NaiveDate::from_ymd_opt(self.selected_year, self.selected_month, 1).unwrap();
                    let weekday_of_first = current_month_first.weekday().num_days_from_sunday() as usize;
                    
                    // This filler day represents (days_in_prev_month - weekday_of_first + day_index + 1)
                    let prev_day = days_in_prev_month - weekday_of_first as u32 + day_index as u32 + 1;
                    NaiveDate::from_ymd_opt(prev_year, prev_month, prev_day).unwrap()
                }
                shared::CalendarDayType::PaddingAfter => {
                    // Calculate next month date
                    let (next_year, next_month) = if self.selected_month == 12 {
                        (self.selected_year + 1, 1)
                    } else {
                        (self.selected_year, self.selected_month + 1)
                    };
                    
                    // Find how many days we are past the end of current month
                    let current_month_first = NaiveDate::from_ymd_opt(self.selected_year, self.selected_month, 1).unwrap();
                    let next_month_first = if self.selected_month == 12 {
                        NaiveDate::from_ymd_opt(self.selected_year + 1, 1, 1).unwrap()
                    } else {
                        NaiveDate::from_ymd_opt(self.selected_year, self.selected_month + 1, 1).unwrap()
                    };
                    let days_in_current_month = (next_month_first - current_month_first).num_days() as u32;
                    let weekday_of_first = current_month_first.weekday().num_days_from_sunday() as usize;
                    
                    // Calculate which day of next month this represents
                    let next_day = day_index as u32 - (weekday_of_first + days_in_current_month as usize) as u32 + 1;
                    NaiveDate::from_ymd_opt(next_year, next_month, next_day).unwrap()
                }
                shared::CalendarDayType::MonthDay => {
                    // This shouldn't happen for day == 0, but fallback
                    NaiveDate::from_ymd_opt(self.selected_year, self.selected_month, 1).unwrap()
                }
            }
        } else {
            NaiveDate::from_ymd_opt(self.selected_year, self.selected_month, backend_day.day).unwrap()
        };
        
        // Check if this is today
        let today = chrono::Local::now();
        let is_today = today.year() == self.selected_year 
            && today.month() == self.selected_month 
            && today.day() == backend_day.day;
        
        CalendarDay {
            day_number: backend_day.day,
            date,
            is_today,
            day_type,
            transactions: backend_day.transactions.clone(),
            balance: Some(backend_day.balance),
        }
    }

    /// Get color for day header based on index (for gradient effect)
    pub fn get_day_header_color(&self, day_index: usize) -> eframe::egui::Color32 {
        // Use smooth pink-to-purple gradient matching the draw_day_header_gradient function
        let t = day_index as f32 / 6.0; // 0.0 to 1.0
        
        // Interpolate between pink and purple (no blue)
        let pink = eframe::egui::Color32::from_rgb(255, 182, 193); // Light pink
        let purple = eframe::egui::Color32::from_rgb(186, 85, 211); // Purple
        
        eframe::egui::Color32::from_rgb(
            (pink.r() as f32 * (1.0 - t) + purple.r() as f32 * t) as u8,
            (pink.g() as f32 * (1.0 - t) + purple.g() as f32 * t) as u8,
            (pink.b() as f32 * (1.0 - t) + purple.b() as f32 * t) as u8,
        )
    }
}

/// Calculate the number of days in a given month
pub fn days_in_month(year: i32, month: u32) -> u32 {
    let first_day = NaiveDate::from_ymd_opt(year, month, 1).unwrap();
    let next_month_first = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1).unwrap()
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1).unwrap()
    };
    (next_month_first - first_day).num_days() as u32
}

/// Calculate the weekday offset for the first day of a month (Sunday = 0)
pub fn first_day_offset(year: i32, month: u32) -> usize {
    let first_day = NaiveDate::from_ymd_opt(year, month, 1).unwrap();
    first_day.weekday().num_days_from_sunday() as usize
}

/// Calculate previous month and year
pub fn previous_month(year: i32, month: u32) -> (i32, u32) {
    if month == 1 {
        (year - 1, 12)
    } else {
        (year, month - 1)
    }
}

/// Calculate next month and year
pub fn next_month(year: i32, month: u32) -> (i32, u32) {
    if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    }
}

/// Check if a date is today
pub fn is_today(date: NaiveDate) -> bool {
    let today = chrono::Local::now().date_naive();
    date == today
}

/// Check if a date is in the current month being displayed
pub fn is_current_month(date: NaiveDate, displayed_year: i32, displayed_month: u32) -> bool {
    date.year() == displayed_year && date.month() == displayed_month
} 