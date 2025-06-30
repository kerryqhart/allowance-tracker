use chrono::{DateTime, FixedOffset, Datelike, Timelike};

/// Utilities for parsing and formatting dates with robust timezone format handling
pub struct DateUtils;

impl DateUtils {
    /// Parse a date string with flexible timezone format handling
    /// This specifically handles the `-0500` vs `-05:00` timezone format issue
    pub fn parse_flexible_rfc3339(date_str: &str) -> Result<DateTime<FixedOffset>, String> {
        // First try parsing as-is in case it's already properly formatted
        if let Ok(dt) = DateTime::parse_from_rfc3339(date_str) {
            return Ok(dt);
        }
        
        // If that fails, fix the timezone format and try again
        let normalized = Self::normalize_timezone_format(date_str);
        match DateTime::parse_from_rfc3339(&normalized) {
            Ok(dt) => Ok(dt),
            Err(e) => Err(format!("Failed to parse date '{}' (normalized: '{}'): {}", date_str, normalized, e))
        }
    }
    
    /// Normalize timezone format from `-0500` to `-05:00` or `+0500` to `+05:00`
    /// This is a simple string manipulation that's guaranteed to work in WASM
    pub fn normalize_timezone_format(date_str: &str) -> String {
        // Check if the string ends with a 5-character timezone pattern like -0500 or +0500
        if date_str.len() >= 5 {
            let last_5 = &date_str[date_str.len()-5..];
            
            // Check if it matches the pattern: [+-]DDDD
            if let Some(first_char) = last_5.chars().nth(0) {
                if (first_char == '-' || first_char == '+') && last_5.chars().skip(1).all(|c| c.is_ascii_digit()) {
                    // Split the timezone: -0500 becomes - and 0500
                    let sign = &last_5[0..1];
                    let digits = &last_5[1..5];
                    
                    if digits.len() == 4 {
                        let hours = &digits[0..2];
                        let minutes = &digits[2..4];
                        let base_part = &date_str[..date_str.len()-5];
                        return format!("{}{}{}:{}", base_part, sign, hours, minutes);
                    }
                }
            }
        }
        
        // Return as-is if no timezone pattern found or already formatted
        date_str.to_string()
    }
    
    /// Parse a date for chart display, returning timestamp in milliseconds
    /// This is useful for JavaScript chart libraries that expect numeric timestamps
    pub fn parse_for_chart(date_str: &str) -> Result<i64, String> {
        match Self::parse_flexible_rfc3339(date_str) {
            Ok(dt) => Ok(dt.timestamp_millis()),
            Err(e) => Err(e)
        }
    }
    
    /// Format a DateTime for display in charts or UI
    pub fn format_for_display(dt: &DateTime<FixedOffset>) -> String {
        dt.format("%Y-%m-%d").to_string()
    }
    
    /// Get current date as a formatted string
    pub fn current_date_string() -> String {
        let now = js_sys::Date::new_0();
        let year = now.get_full_year() as i32;
        let month = (now.get_month() as u32) + 1; // JS months are 0-based
        let day = now.get_date() as u32;
        
        format!("{:04}-{:02}-{:02}", year, month, day)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_normalize_timezone_format_negative() {
        let input = "2025-06-28T17:21:13-0500";
        let expected = "2025-06-28T17:21:13-05:00";
        assert_eq!(DateUtils::normalize_timezone_format(input), expected);
    }
    
    #[test]
    fn test_normalize_timezone_format_positive() {
        let input = "2025-06-28T17:21:13+0500";
        let expected = "2025-06-28T17:21:13+05:00";
        assert_eq!(DateUtils::normalize_timezone_format(input), expected);
    }
    
    #[test]
    fn test_normalize_timezone_format_already_formatted() {
        let input = "2025-06-28T17:21:13-05:00";
        let expected = "2025-06-28T17:21:13-05:00";
        assert_eq!(DateUtils::normalize_timezone_format(input), expected);
    }
    
    #[test]
    fn test_normalize_timezone_format_no_timezone() {
        let input = "2025-06-28T17:21:13";
        let expected = "2025-06-28T17:21:13";
        assert_eq!(DateUtils::normalize_timezone_format(input), expected);
    }
    
    #[test]
    fn test_normalize_timezone_format_utc() {
        let input = "2025-06-28T17:21:13Z";
        let expected = "2025-06-28T17:21:13Z";
        assert_eq!(DateUtils::normalize_timezone_format(input), expected);
    }
    
    #[test]
    fn test_parse_flexible_rfc3339_timezone_without_colon() {
        let input = "2025-06-28T17:21:13-0500";
        let result = DateUtils::parse_flexible_rfc3339(input);
        assert!(result.is_ok(), "Failed to parse date: {} - error: {:?}", input, result.err());
        
        let dt = result.unwrap();
        assert_eq!(dt.year(), 2025);
        assert_eq!(dt.month(), 6);
        assert_eq!(dt.day(), 28);
        assert_eq!(dt.hour(), 17);
        assert_eq!(dt.minute(), 21);
        assert_eq!(dt.second(), 13);
    }
    
    #[test]
    fn test_parse_flexible_rfc3339_timezone_with_colon() {
        let input = "2025-06-28T17:21:13-05:00";
        let result = DateUtils::parse_flexible_rfc3339(input);
        assert!(result.is_ok(), "Failed to parse date: {} - error: {:?}", input, result.err());
        
        let dt = result.unwrap();
        assert_eq!(dt.year(), 2025);
        assert_eq!(dt.month(), 6);
        assert_eq!(dt.day(), 28);
    }
    
    #[test]
    fn test_parse_flexible_rfc3339_utc() {
        let input = "2025-06-28T17:21:13Z";
        let result = DateUtils::parse_flexible_rfc3339(input);
        assert!(result.is_ok(), "Failed to parse UTC date: {} - error: {:?}", input, result.err());
    }
    
    #[test]
    fn test_parse_flexible_rfc3339_positive_timezone() {
        let input = "2025-06-28T17:21:13+0500";
        let result = DateUtils::parse_flexible_rfc3339(input);
        assert!(result.is_ok(), "Failed to parse positive timezone: {} - error: {:?}", input, result.err());
    }
    
    #[test]
    fn test_parse_flexible_rfc3339_invalid_date() {
        let input = "not-a-date";
        let result = DateUtils::parse_flexible_rfc3339(input);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Failed to parse date"));
    }
    
    #[test]
    fn test_parse_for_chart_success() {
        let input = "2025-06-28T17:21:13-0500";
        let result = DateUtils::parse_for_chart(input);
        assert!(result.is_ok(), "Failed to parse date for chart: {} - error: {:?}", input, result.err());
        
        let timestamp = result.unwrap();
        assert!(timestamp > 0, "Timestamp should be positive: {}", timestamp);
    }
    
    #[test]
    fn test_parse_for_chart_failure() {
        let input = "invalid-date";
        let result = DateUtils::parse_for_chart(input);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Failed to parse date"));
    }
    
    #[test]
    fn test_format_for_display() {
        let input = "2025-06-28T17:21:13-0500";
        let dt = DateUtils::parse_flexible_rfc3339(input).unwrap();
        let formatted = DateUtils::format_for_display(&dt);
        assert_eq!(formatted, "2025-06-28");
    }
    
    #[test]
    fn test_all_sample_dates_from_logs() {
        // These are the actual dates from the logs that were failing
        let test_dates = vec![
            "2025-06-28T17:21:13-0500",
            "2025-06-27T07:00:00-0500", 
            "2025-06-23T00:00:00-0500",
            "2025-06-20T00:00:01-0500",
            "2025-06-20T00:00:00-0500",
            "2025-06-15T00:00:00-0500",
        ];
        
        for date_str in test_dates {
            let result = DateUtils::parse_flexible_rfc3339(date_str);
            assert!(result.is_ok(), "Failed to parse date: {} - error: {:?}", date_str, result.err());
            
            let chart_result = DateUtils::parse_for_chart(date_str);
            assert!(chart_result.is_ok(), "Failed to parse date for chart: {} - error: {:?}", date_str, chart_result.err());
        }
    }
    
    #[test]
    fn test_timezone_edge_cases() {
        let test_cases = vec![
            ("2025-06-28T17:21:13+0000", "2025-06-28T17:21:13+00:00"),
            ("2025-06-28T17:21:13-1200", "2025-06-28T17:21:13-12:00"),
            ("2025-06-28T17:21:13+1400", "2025-06-28T17:21:13+14:00"),
            ("2025-06-28T17:21:13+05:00", "2025-06-28T17:21:13+05:00"), // Already formatted
            ("2025-06-28T17:21:13-05:00", "2025-06-28T17:21:13-05:00"), // Already formatted
        ];
        
        for (input, expected) in test_cases {
            let normalized = DateUtils::normalize_timezone_format(input);
            assert_eq!(normalized, expected, "Failed to normalize: {} -> {}", input, expected);
            
            let result = DateUtils::parse_flexible_rfc3339(input);
            assert!(result.is_ok(), "Failed to parse date: {} - error: {:?}", input, result.err());
        }
    }
}

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