/// Helper function to get month name from number
pub fn month_name(month: u32) -> &'static str {
    match month {
        1 => "January", 2 => "February", 3 => "March", 4 => "April",
        5 => "May", 6 => "June", 7 => "July", 8 => "August",
        9 => "September", 10 => "October", 11 => "November", 12 => "December",
        _ => "January",
    }
}

/// Simple helper function for calendar tooltips (calendar uses raw transactions)
pub fn format_calendar_date(rfc3339_date: &str) -> String {
    if let Some(date_part) = rfc3339_date.split('T').next() {
        if let Ok(parts) = date_part.split('-').collect::<Vec<_>>().try_into() {
            let [year, month, day]: [&str; 3] = parts;
            if let (Ok(y), Ok(m), Ok(d)) = (year.parse::<u32>(), month.parse::<u32>(), day.parse::<u32>()) {
                return format!("{} {}, {}", month_name(m), d, y);
            }
        }
    }
    rfc3339_date.to_string()
} 