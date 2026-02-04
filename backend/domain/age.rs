//! Age calculation utilities for the allowance tracker.

use chrono::{Datelike, NaiveDate};

/// Calculate a person's age in years on a specific date.
///
/// Returns the age as of the target date. On the birthday itself,
/// returns the new age (e.g., turning 6 on Feb 8 means age is 6 on Feb 8).
pub fn age_on_date(birthdate: NaiveDate, target_date: NaiveDate) -> i32 {
    let years = target_date.year() - birthdate.year();
    let had_birthday = (target_date.month(), target_date.day())
                       >= (birthdate.month(), birthdate.day());
    if had_birthday { years } else { years - 1 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_age_birthday_today_gets_new_age() {
        let birthdate = NaiveDate::from_ymd_opt(2019, 2, 8).unwrap();
        let target = NaiveDate::from_ymd_opt(2025, 2, 8).unwrap();
        assert_eq!(age_on_date(birthdate, target), 6);
    }

    #[test]
    fn test_age_birthday_tomorrow_gets_old_age() {
        let birthdate = NaiveDate::from_ymd_opt(2019, 2, 8).unwrap();
        let target = NaiveDate::from_ymd_opt(2025, 2, 7).unwrap();
        assert_eq!(age_on_date(birthdate, target), 5);
    }

    #[test]
    fn test_age_birthday_yesterday_gets_new_age() {
        let birthdate = NaiveDate::from_ymd_opt(2019, 2, 8).unwrap();
        let target = NaiveDate::from_ymd_opt(2025, 2, 9).unwrap();
        assert_eq!(age_on_date(birthdate, target), 6);
    }

    #[test]
    fn test_age_leap_year_birthday_on_march_1() {
        // Born Feb 29, 2020 - on non-leap years, test March 1
        let birthdate = NaiveDate::from_ymd_opt(2020, 2, 29).unwrap();
        // March 1, 2025 - they should be 5 (birthday passed)
        let target = NaiveDate::from_ymd_opt(2025, 3, 1).unwrap();
        assert_eq!(age_on_date(birthdate, target), 5);
    }

    #[test]
    fn test_age_leap_year_birthday_on_feb_28() {
        // Born Feb 29, 2020 - on Feb 28, 2025, still 4
        let birthdate = NaiveDate::from_ymd_opt(2020, 2, 29).unwrap();
        let target = NaiveDate::from_ymd_opt(2025, 2, 28).unwrap();
        assert_eq!(age_on_date(birthdate, target), 4);
    }

    #[test]
    fn test_age_infant() {
        let birthdate = NaiveDate::from_ymd_opt(2025, 1, 15).unwrap();
        let target = NaiveDate::from_ymd_opt(2025, 2, 1).unwrap();
        assert_eq!(age_on_date(birthdate, target), 0);
    }
}
