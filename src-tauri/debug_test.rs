use std::fs;

fn main() {
    println!("Testing month validation:");
    let month = 13u32;
    println!("Month: {}", month);
    println!("Range check: {}", (1..=12).contains(&month));
    println!("Negated: {}", !(1..=12).contains(&month));
    
    if !(1..=12).contains(&month) {
        println!("Should return error for month {}", month);
    } else {
        println!("Month {} is valid", month);
    }
}
