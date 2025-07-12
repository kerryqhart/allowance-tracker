#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_simple_validation() {
        // Test the exact logic from the validate_birthdate function
        let birthdate = "2020-13-01";
        let date_parts: Vec<&str> = birthdate.split('-').collect();
        println!("Parts: {:?}", date_parts);
        
        if date_parts.len() != 3 {
            panic!("Should have 3 parts");
        }

        let year: u32 = date_parts[0].parse().unwrap();
        let month: u32 = date_parts[1].parse().unwrap();
        let day: u32 = date_parts[2].parse().unwrap();
        
        println!("Year: {}, Month: {}, Day: {}", year, month, day);
        
        if year < 1900 || year > 2100 {
            panic!("Year should be valid");
        }
        if !(1..=12).contains(&month) {
            panic!("Month {} should be invalid!", month);
        }
        if !(1..=31).contains(&day) {
            panic!("Day should be valid");
        }
        
        println!("All validations passed - this should NOT happen!");
    }
}
