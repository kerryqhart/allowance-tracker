/// Simple helper function for calendar tooltips (calendar uses raw transactions)
pub fn format_calendar_date(rfc3339_date: &str) -> String {
    if let Some(date_part) = rfc3339_date.split('T').next() {
        if let Ok(parts) = date_part.split('-').collect::<Vec<_>>().try_into() {
            let [year, month, day]: [&str; 3] = parts;
            if let (Ok(y), Ok(m), Ok(d)) = (year.parse::<u32>(), month.parse::<u32>(), day.parse::<u32>()) {
                let month_name = match m {
                    1 => "January", 2 => "February", 3 => "March", 4 => "April",
                    5 => "May", 6 => "June", 7 => "July", 8 => "August",
                    9 => "September", 10 => "October", 11 => "November", 12 => "December",
                    _ => "January",
                };
                return format!("{} {}, {}", month_name, d, y);
            }
        }
    }
    rfc3339_date.to_string()
}

/// Get current date in YYYY-MM-DD format
pub fn get_current_date() -> String {
    use js_sys::Date;
    let now = Date::new_0();
    let year = now.get_full_year();
    let month = now.get_month() + 1; // JavaScript months are 0-indexed
    let day = now.get_date();
    
    format!("{:04}-{:02}-{:02}", year as u32, month as u32, day as u32)
}

/// Get current date formatted for display (e.g., "January 15, 2025")
pub fn get_current_date_display() -> String {
    use js_sys::Date;
    let now = Date::new_0();
    let year = now.get_full_year();
    let month = now.get_month() + 1;
    let day = now.get_date();
    
    let month_name = match month as u32 {
        1 => "January", 2 => "February", 3 => "March", 4 => "April",
        5 => "May", 6 => "June", 7 => "July", 8 => "August",
        9 => "September", 10 => "October", 11 => "November", 12 => "December",
        _ => "January",
    };
    format!("{} {}, {}", month_name, day as u32, year as u32)
}

/// Calculate the earliest valid date (45 days ago)
pub fn get_earliest_valid_date() -> String {
    use js_sys::Date;
    use wasm_bindgen::JsValue;
    let now = Date::new_0();
    let forty_five_days_ago = Date::new(&JsValue::from_f64(now.get_time() - 45.0 * 24.0 * 60.0 * 60.0 * 1000.0));
    
    let year = forty_five_days_ago.get_full_year();
    let month = forty_five_days_ago.get_month() + 1;
    let day = forty_five_days_ago.get_date();
    
    format!("{:04}-{:02}-{:02}", year as u32, month as u32, day as u32)
}

/// Check if a date string (YYYY-MM-DD) is valid for transaction entry
pub fn is_valid_transaction_date(date_str: &str) -> bool {
    let current_date = get_current_date();
    let earliest_date = get_earliest_valid_date();
    
    // Date must be between earliest_date and current_date (inclusive)
    date_str >= earliest_date.as_str() && date_str <= current_date.as_str()
}

/// Parse YYYY-MM-DD date string into components
pub fn parse_date_string(date_str: &str) -> Option<(u32, u32, u32)> {
    let parts: Vec<&str> = date_str.split('-').collect();
    if parts.len() != 3 {
        return None;
    }
    
    let year = parts[0].parse::<u32>().ok()?;
    let month = parts[1].parse::<u32>().ok()?;
    let day = parts[2].parse::<u32>().ok()?;
    
    if month >= 1 && month <= 12 && day >= 1 && day <= 31 {
        Some((year, month, day))
    } else {
        None
    }
}

/// Format YYYY-MM-DD date string for display
pub fn format_date_for_display(date_str: &str) -> String {
    if let Some((year, month, day)) = parse_date_string(date_str) {
        let month_name = match month {
            1 => "January", 2 => "February", 3 => "March", 4 => "April",
            5 => "May", 6 => "June", 7 => "July", 8 => "August",
            9 => "September", 10 => "October", 11 => "November", 12 => "December",
            _ => "January",
        };
        format!("{} {}, {}", month_name, day, year)
    } else {
        date_str.to_string()
    }
}

/// Check if a date string represents today
pub fn is_today(date_str: &str) -> bool {
    date_str == get_current_date()
}

/// Get days in a month (accounting for leap years)
pub fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap_year(year) { 29 } else { 28 }
        }
        _ => 30,
    }
}

/// Check if a year is a leap year
pub fn is_leap_year(year: u32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

/// Get the first day of the week for a given month/year (0 = Sunday, 1 = Monday, etc.)
pub fn get_first_day_of_month(year: u32, month: u32) -> u32 {
    use js_sys::Date;
    let date = Date::new_with_year_month_day(year, (month - 1) as i32, 1);
    date.get_day() as u32
}