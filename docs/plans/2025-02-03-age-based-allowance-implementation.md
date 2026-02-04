# Age-Based Allowance Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add option to automatically set allowance amount to child's age in years, with correct birthday handling in projections.

**Architecture:** Add `use_age_based_amount: bool` field to AllowanceConfig at all layers (shared DTOs, domain model, storage). When enabled, projection logic calculates age on each future date rather than using fixed amount.

**Tech Stack:** Rust, egui, chrono, serde_yaml

---

## Task 1: Add age calculation helper function

**Files:**
- Create: `backend/domain/age.rs`
- Modify: `backend/domain/mod.rs`

**Step 1: Write the failing test**

Create `backend/domain/age.rs`:

```rust
//! Age calculation utilities for the allowance tracker.

use chrono::NaiveDate;

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
```

**Step 2: Add module to mod.rs**

Edit `backend/domain/mod.rs`, add after other pub mod declarations:

```rust
pub mod age;
```

**Step 3: Run test to verify it passes**

Run: `cargo test -p allowance-tracker-egui age_on_date`
Expected: All 6 tests PASS

**Step 4: Commit**

```bash
git add backend/domain/age.rs backend/domain/mod.rs
git commit -m "feat: add age calculation helper function

Calculates age in years on a specific date, correctly handling
birthdays (same-day birthday = new age) and leap years.

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Task 2: Add use_age_based_amount field to shared DTOs

**Files:**
- Modify: `shared/src/lib.rs:468-477` (AllowanceConfig struct)
- Modify: `shared/src/lib.rs:491-498` (UpdateAllowanceConfigRequest struct)

**Step 1: Add field to AllowanceConfig DTO**

In `shared/src/lib.rs`, find the `AllowanceConfig` struct (around line 468) and add field:

```rust
/// Represents an allowance configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AllowanceConfig {
    pub child_id: String,
    pub amount: f64,
    pub day_of_week: u8, // 0 = Sunday, 1 = Monday, ..., 6 = Saturday
    pub is_active: bool,
    #[serde(default)]
    pub use_age_based_amount: bool, // If true, amount = child's age in years
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

**Step 2: Add field to UpdateAllowanceConfigRequest**

In `shared/src/lib.rs`, find `UpdateAllowanceConfigRequest` (around line 491) and add field:

```rust
/// Request for updating allowance configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UpdateAllowanceConfigRequest {
    pub child_id: Option<String>, // If None, uses active child
    pub amount: f64,
    pub day_of_week: u8, // 0 = Sunday, 1 = Monday, ..., 6 = Saturday
    pub is_active: bool,
    #[serde(default)]
    pub use_age_based_amount: bool,
}
```

**Step 3: Run tests to verify compilation**

Run: `cargo test -p shared`
Expected: PASS (existing tests still work, new field defaults to false)

**Step 4: Commit**

```bash
git add shared/src/lib.rs
git commit -m "feat: add use_age_based_amount field to shared DTOs

Adds new boolean field to AllowanceConfig and UpdateAllowanceConfigRequest.
Uses serde default for backward compatibility with existing configs.

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Task 3: Add use_age_based_amount field to domain model

**Files:**
- Modify: `backend/domain/models/allowance.rs`

**Step 1: Add field to domain AllowanceConfig**

Edit `backend/domain/models/allowance.rs`:

```rust
//! Domain model for an allowance configuration.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AllowanceConfig {
    pub child_id: String,
    pub amount: f64,
    pub day_of_week: u8, // 0 = Sunday, 1 = Monday, ..., 6 = Saturday
    pub is_active: bool,
    #[serde(default)]
    pub use_age_based_amount: bool, // If true, amount = child's age in years
    pub created_at: String, // RFC 3339 timestamp
    pub updated_at: String, // RFC 3339 timestamp
}

impl AllowanceConfig {
    /// Get the day name for the configured day of week
    pub fn day_name(&self) -> &'static str {
        match self.day_of_week {
            0 => "Sunday",
            1 => "Monday",
            2 => "Tuesday",
            3 => "Wednesday",
            4 => "Thursday",
            5 => "Friday",
            6 => "Saturday",
            _ => "Invalid",
        }
    }

    /// Validate day of week value
    pub fn is_valid_day_of_week(day: u8) -> bool {
        day <= 6
    }
}
```

**Step 2: Run tests to verify compilation**

Run: `cargo test -p allowance-tracker-egui allowance`
Expected: PASS

**Step 3: Commit**

```bash
git add backend/domain/models/allowance.rs
git commit -m "feat: add use_age_based_amount to domain AllowanceConfig

Uses serde default for backward compatibility with existing YAML files.

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Task 4: Update AllowanceService to handle use_age_based_amount

**Files:**
- Modify: `backend/domain/allowance_service.rs`
- Modify: `backend/domain/commands/allowance.rs`

**Step 1: Add field to UpdateAllowanceConfigCommand**

In `backend/domain/commands/allowance.rs`, find `UpdateAllowanceConfigCommand` and add field:

```rust
#[derive(Debug, Clone)]
pub struct UpdateAllowanceConfigCommand {
    pub child_id: Option<String>,
    pub amount: f64,
    pub day_of_week: u8,
    pub is_active: bool,
    pub use_age_based_amount: bool,
}
```

**Step 2: Update update_allowance_config to store new field**

In `backend/domain/allowance_service.rs`, find `update_allowance_config` method. Update the config creation/update logic (around line 133-152):

```rust
let domain_allowance_config = match existing_domain_config {
    Some(mut config) => {
        // Update existing config
        config.amount = command.amount;
        config.day_of_week = command.day_of_week;
        config.is_active = command.is_active;
        config.use_age_based_amount = command.use_age_based_amount;
        config.updated_at = timestamp_rfc3339;
        config
    }
    None => {
        // Create new config
        AllowanceConfig {
            child_id: child_id.clone(),
            amount: command.amount,
            day_of_week: command.day_of_week,
            is_active: command.is_active,
            use_age_based_amount: command.use_age_based_amount,
            created_at: timestamp_rfc3339.clone(),
            updated_at: timestamp_rfc3339,
        }
    }
};
```

**Step 3: Run tests to verify**

Run: `cargo test -p allowance-tracker-egui update_allowance`
Expected: PASS (need to fix test commands first - see step 4)

**Step 4: Update test commands**

Find all `UpdateAllowanceConfigCommand` usages in tests and add `use_age_based_amount: false`:

```rust
let command = UpdateAllowanceConfigCommand {
    child_id: Some(child.id.clone()),
    amount: 10.0,
    day_of_week: 1,
    is_active: true,
    use_age_based_amount: false, // Add this line
};
```

**Step 5: Run all allowance tests**

Run: `cargo test -p allowance-tracker-egui allowance`
Expected: PASS

**Step 6: Commit**

```bash
git add backend/domain/allowance_service.rs backend/domain/commands/allowance.rs
git commit -m "feat: update AllowanceService to store use_age_based_amount

Updates UpdateAllowanceConfigCommand and storage logic to handle the new field.

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Task 5: Update projection logic for age-based amounts

**Files:**
- Modify: `backend/domain/allowance_service.rs`

**Step 1: Write failing test for age-based projection**

Add to `backend/domain/allowance_service.rs` tests module:

```rust
#[test]
fn test_generate_future_allowance_with_age_based_amount() {
    let service = setup_test();

    // Create child with known birthdate - born Feb 8, 2019
    let create_command = crate::backend::domain::commands::child::CreateChildCommand {
        name: "Test Child".to_string(),
        birthdate: "2019-02-08".to_string(),
    };
    let child_result = service.child_service.create_child(create_command)
        .expect("Failed to create test child");
    let child = child_result.child;

    // Create age-based allowance config for Friday (day_of_week: 5)
    let command = UpdateAllowanceConfigCommand {
        child_id: Some(child.id.clone()),
        amount: 0.0, // Amount ignored when use_age_based_amount is true
        day_of_week: 5, // Friday
        is_active: true,
        use_age_based_amount: true,
    };

    service.update_allowance_config(command).expect("Failed to create allowance config");

    // Generate allowances for a date range around the child's birthday
    // Child turns 6 on Feb 8, 2025
    let start_date = NaiveDate::from_ymd_opt(2025, 2, 3).unwrap(); // Monday before birthday
    let end_date = NaiveDate::from_ymd_opt(2025, 2, 14).unwrap(); // Friday after birthday

    let future_allowances = service
        .generate_future_allowance_transactions(&child.id, start_date, end_date)
        .expect("Failed to generate future allowances");

    // Should have 2 Fridays: Feb 7 (before birthday, age 5) and Feb 14 (after birthday, age 6)
    assert_eq!(future_allowances.len(), 2, "Should generate 2 future allowances");

    // First Friday (Feb 7) - still age 5
    let first = &future_allowances[0];
    assert_eq!(first.amount, 5.0, "Before birthday should be age 5 = $5");

    // Second Friday (Feb 14) - now age 6
    let second = &future_allowances[1];
    assert_eq!(second.amount, 6.0, "After birthday should be age 6 = $6");
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p allowance-tracker-egui test_generate_future_allowance_with_age_based_amount`
Expected: FAIL (amount will be 0.0 because we haven't implemented the logic yet)

**Step 3: Update generate_future_allowance_transactions**

In `backend/domain/allowance_service.rs`, find `generate_future_allowance_transactions` method. Update the amount calculation logic (around line 260-270):

First, add import at top of file:
```rust
use crate::backend::domain::age::age_on_date;
use crate::backend::domain::commands::child::GetChildCommand;
```

Then update the method to fetch child birthdate and calculate age:

```rust
/// Generate forward-looking allowance transactions for a given date range
pub fn generate_future_allowance_transactions(
    &self,
    child_id: &str,
    start_date: NaiveDate,
    end_date: NaiveDate,
) -> Result<Vec<DomainTransaction>> {
    info!("🔮 ALLOWANCE DEBUG: Generating future allowances for child {} from {} to {}",
         child_id, start_date, end_date);

    // Get allowance config for the child
    let allowance_config = self.allowance_repository.get_allowance_config(child_id)?;

    info!("🔮 ALLOWANCE DEBUG: Retrieved allowance config: {:?}", allowance_config);

    let config = match allowance_config {
        Some(config) if config.is_active => {
            info!("🔮 ALLOWANCE DEBUG: Found active config - amount: ${:.2}, day_of_week: {} ({}), use_age_based: {}",
                 config.amount, config.day_of_week, config.day_name(), config.use_age_based_amount);
            config
        },
        Some(_config) => {
            info!("🔮 ALLOWANCE DEBUG: Found inactive allowance config for child: {}", child_id);
            return Ok(Vec::new());
        },
        None => {
            info!("🔮 ALLOWANCE DEBUG: No allowance config found for child: {}", child_id);
            return Ok(Vec::new());
        }
    };

    // If using age-based amount, fetch child's birthdate
    let child_birthdate = if config.use_age_based_amount {
        let get_child_command = GetChildCommand { child_id: child_id.to_string() };
        match self.child_service.get_child(get_child_command)? {
            result if result.child.is_some() => {
                let child = result.child.unwrap();
                Some(child.birthdate)
            }
            _ => {
                warn!("🔮 ALLOWANCE DEBUG: Age-based amount enabled but child not found: {}", child_id);
                return Ok(Vec::new());
            }
        }
    } else {
        None
    };

    let mut future_allowances = Vec::new();
    let current_date = Local::now().date_naive();

    info!("🔮 ALLOWANCE DEBUG: Current date: {}", current_date);

    // Iterate through each date in the range
    let mut current = start_date;
    let mut checked_days = 0;
    while current <= end_date {
        checked_days += 1;
        let day_of_week = current.weekday().num_days_from_sunday() as u8;

        // Check if this date is in the future and matches the allowance day of week
        if current > current_date && day_of_week == config.day_of_week {
            // Calculate the amount based on mode
            let amount = if config.use_age_based_amount {
                if let Some(birthdate) = child_birthdate {
                    let age = age_on_date(birthdate, current);
                    info!("🔮 ALLOWANCE DEBUG: Age-based amount for {} on {}: age {} = ${}",
                         child_id, current, age, age);
                    age as f64
                } else {
                    config.amount
                }
            } else {
                config.amount
            };

            info!("🔮 ALLOWANCE DEBUG: ✅ CREATING future allowance for {} on {} - ${:.2}", child_id, current, amount);

            // Create DateTime at 12:00 UTC for the date
            let naive_datetime = current.and_hms_opt(12, 0, 0).unwrap();
            let utc_offset = FixedOffset::east_opt(0).unwrap();
            let transaction_datetime = naive_datetime.and_local_timezone(utc_offset)
                .single()
                .unwrap();

            let allowance_transaction = DomainTransaction {
                id: format!("future-allowance::{}::{}", child_id, current.format("%Y-%m-%d")),
                child_id: child_id.to_string(),
                date: transaction_datetime,
                description: "Upcoming allowance".to_string(),
                amount,
                balance: f64::NAN,
                transaction_type: DomainTransactionType::FutureAllowance,
            };

            future_allowances.push(allowance_transaction);
        }

        // Move to next day
        current = current.succ_opt().unwrap_or(current);
        if current == current.succ_opt().unwrap_or(current) {
            break;
        }
    }

    info!("🔮 ALLOWANCE DEBUG: Checked {} days total, generated {} future allowance transactions",
          checked_days, future_allowances.len());

    Ok(future_allowances)
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p allowance-tracker-egui test_generate_future_allowance_with_age_based_amount`
Expected: PASS

**Step 5: Run all allowance tests**

Run: `cargo test -p allowance-tracker-egui allowance`
Expected: PASS

**Step 6: Commit**

```bash
git add backend/domain/allowance_service.rs
git commit -m "feat: implement age-based amount calculation in projections

When use_age_based_amount is true, calculates child's age on each
projection date and uses that as the dollar amount. Correctly handles
birthday transitions mid-projection.

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Task 6: Add use_age_based_amount to form state

**Files:**
- Modify: `egui-frontend/src/ui/components/settings/state.rs`

**Step 1: Add field to AllowanceConfigFormState**

In `egui-frontend/src/ui/components/settings/state.rs`, find `AllowanceConfigFormState` struct (around line 304) and add new fields:

```rust
/// Form state for configuring allowance settings
#[derive(Debug, Clone)]
pub struct AllowanceConfigFormState {
    pub amount: String,
    pub day_of_week: u8,
    pub use_age_based_amount: bool,  // NEW: Age-based toggle
    pub amount_error: Option<String>,
    pub is_valid: bool,
    pub is_saving: bool,
    pub success_message: Option<String>,
    pub error_message: Option<String>,

    // Original values for change detection
    pub original_amount: Option<f64>,
    pub original_day_of_week: Option<u8>,
    pub original_use_age_based_amount: Option<bool>,  // NEW
    pub has_existing_config: bool,

    // Child info for age calculation
    pub child_birthdate: Option<chrono::NaiveDate>,  // NEW: For displaying current age
}
```

**Step 2: Update new() method**

Update the `new()` method:

```rust
impl AllowanceConfigFormState {
    pub fn new() -> Self {
        Self {
            amount: "5.00".to_string(),
            day_of_week: 5,
            use_age_based_amount: false,
            amount_error: None,
            is_valid: true,
            is_saving: false,
            success_message: None,
            error_message: None,
            original_amount: None,
            original_day_of_week: None,
            original_use_age_based_amount: None,
            has_existing_config: false,
            child_birthdate: None,
        }
    }
```

**Step 3: Update clear() method**

```rust
pub fn clear(&mut self) {
    self.amount = "5.00".to_string();
    self.day_of_week = 5;
    self.use_age_based_amount = false;
    self.amount_error = None;
    self.is_valid = true;
    self.is_saving = false;
    self.success_message = None;
    self.error_message = None;
    self.original_amount = None;
    self.original_day_of_week = None;
    self.original_use_age_based_amount = None;
    self.has_existing_config = false;
    self.child_birthdate = None;
}
```

**Step 4: Update load_from_config() method**

```rust
pub fn load_from_config(&mut self, config: &crate::backend::domain::models::allowance::AllowanceConfig) {
    self.amount = format!("{:.2}", config.amount);
    self.day_of_week = config.day_of_week;
    self.use_age_based_amount = config.use_age_based_amount;
    self.original_amount = Some(config.amount);
    self.original_day_of_week = Some(config.day_of_week);
    self.original_use_age_based_amount = Some(config.use_age_based_amount);
    self.has_existing_config = true;
    self.amount_error = None;
    self.is_valid = true;
    self.success_message = None;
    self.error_message = None;

    log::info!("⚙️ LOADED_CONFIG: amount='{}', day={}, use_age_based={}",
        self.amount, self.day_of_week, self.use_age_based_amount);
}
```

**Step 5: Update has_changes() method**

```rust
pub fn has_changes(&self) -> bool {
    if !self.has_existing_config {
        return true;
    }

    let current_amount = self.amount.trim().parse::<f64>().unwrap_or(0.0);

    let amount_changed = self.original_amount.map(|orig| current_amount != orig).unwrap_or(true);
    let day_changed = self.original_day_of_week.map(|orig| orig != self.day_of_week).unwrap_or(true);
    let age_based_changed = self.original_use_age_based_amount
        .map(|orig| orig != self.use_age_based_amount)
        .unwrap_or(true);

    amount_changed || day_changed || age_based_changed
}
```

**Step 6: Add helper method to get current age**

```rust
/// Get current age based on stored birthdate
pub fn current_age(&self) -> Option<i32> {
    self.child_birthdate.map(|birthdate| {
        use chrono::Local;
        let today = Local::now().date_naive();
        crate::backend::domain::age::age_on_date(birthdate, today)
    })
}

/// Check if age-based mode can be enabled (requires birthdate)
pub fn can_use_age_based(&self) -> bool {
    self.child_birthdate.is_some()
}
```

**Step 7: Run tests to verify compilation**

Run: `cargo build -p allowance-tracker-egui`
Expected: Compiles successfully

**Step 8: Commit**

```bash
git add egui-frontend/src/ui/components/settings/state.rs
git commit -m "feat: add use_age_based_amount to AllowanceConfigFormState

Adds toggle state, original value tracking for change detection,
child birthdate storage, and helper methods for age calculation.

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Task 7: Update modal to load child birthdate

**Files:**
- Modify: `egui-frontend/src/ui/components/settings/allowance_config_modal.rs`

**Step 1: Update load_allowance_config_for_modal**

In `allowance_config_modal.rs`, find `load_allowance_config_for_modal` method and update to also load child birthdate:

```rust
/// Load allowance configuration when modal opens
pub fn load_allowance_config_for_modal(&mut self) {
    let child_from_backend = self.get_current_child_from_backend();

    // Store child birthdate for age calculation
    if let Some(ref child) = child_from_backend {
        self.settings.allowance_config_form.child_birthdate = Some(child.birthdate);
        log::info!("🔍 MODAL_LOAD_DEBUG: Child birthdate: {}", child.birthdate);
    } else {
        self.settings.allowance_config_form.child_birthdate = None;
    }

    let child_id = child_from_backend.as_ref().map(|c| c.id.clone());
    log::info!("🔍 MODAL_LOAD_DEBUG: Using child_id for GetAllowanceConfigCommand: {:?}", child_id);

    let command = GetAllowanceConfigCommand { child_id };

    match self.backend().allowance_service.get_allowance_config(command) {
        Ok(result) => {
            if let Some(config) = result.allowance_config {
                log::info!("✅ Loaded existing allowance config: ${:.2} on {}, age_based={}",
                    config.amount, config.day_name(), config.use_age_based_amount);
                self.settings.allowance_config_form.load_from_config(&config);
            } else {
                log::info!("ℹ️ No existing allowance config found, using defaults");
                self.settings.allowance_config_form.clear();
                // Re-apply birthdate after clear
                if let Some(ref child) = child_from_backend {
                    self.settings.allowance_config_form.child_birthdate = Some(child.birthdate);
                }
            }
        }
        Err(e) => {
            log::error!("❌ Failed to load allowance config: {}", e);
            self.settings.allowance_config_form.error_message = Some(format!("Failed to load config: {}", e));
        }
    }
}
```

**Step 2: Run to verify compilation**

Run: `cargo build -p allowance-tracker-egui`
Expected: Compiles successfully

**Step 3: Commit**

```bash
git add egui-frontend/src/ui/components/settings/allowance_config_modal.rs
git commit -m "feat: load child birthdate when opening allowance modal

Stores birthdate in form state for age calculation display.

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Task 8: Add age-based checkbox to modal UI

**Files:**
- Modify: `egui-frontend/src/ui/components/settings/allowance_config_modal.rs`

**Step 1: Update render_allowance_config_form_content**

Replace the method with:

```rust
/// Render the form content for allowance configuration modal
fn render_allowance_config_form_content(&mut self, ui: &mut egui::Ui) {
    ui.vertical(|ui| {
        // Age-based toggle (above amount field)
        let can_use_age_based = self.settings.allowance_config_form.can_use_age_based();

        ui.horizontal(|ui| {
            let checkbox = egui::Checkbox::new(
                &mut self.settings.allowance_config_form.use_age_based_amount,
                egui::RichText::new("Use age-based amount")
                    .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
            );

            let response = ui.add_enabled(can_use_age_based, checkbox);

            if !can_use_age_based {
                response.on_disabled_hover_text("Requires child birthdate to be set");
            }
        });

        // Show age info when age-based is enabled
        if self.settings.allowance_config_form.use_age_based_amount {
            if let Some(age) = self.settings.allowance_config_form.current_age() {
                ui.label(egui::RichText::new(format!("(Child's age: {} = ${} allowance)", age, age))
                    .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
                    .color(egui::Color32::from_rgb(70, 130, 180)));
            }
        }

        ui.add_space(15.0);

        // Amount field - disabled when age-based
        let amount_enabled = !self.settings.allowance_config_form.use_age_based_amount;

        ui.label(egui::RichText::new("Weekly Allowance Amount")
            .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
            .strong()
            .color(if amount_enabled {
                egui::Color32::from_rgb(60, 60, 60)
            } else {
                egui::Color32::from_rgb(150, 150, 150)
            }));

        ui.add_space(5.0);

        // Show age-derived amount when in age-based mode
        if self.settings.allowance_config_form.use_age_based_amount {
            if let Some(age) = self.settings.allowance_config_form.current_age() {
                let display_amount = format!("${}.00", age);
                ui.add_enabled(false,
                    egui::TextEdit::singleline(&mut display_amount.clone())
                        .desired_width(200.0)
                        .hint_text("Age-based amount")
                );
            }
        } else {
            let amount_response = ui.add_enabled(amount_enabled,
                egui::TextEdit::singleline(&mut self.settings.allowance_config_form.amount)
                    .desired_width(200.0)
                    .hint_text("Enter dollar amount (e.g., 5.00)")
            );

            if amount_response.changed() {
                log::info!("⚙️ Amount changed to '{}'", self.settings.allowance_config_form.amount);
            }

            self.validate_allowance_config_form_field("amount");

            // Show amount error if present
            if let Some(ref error) = self.settings.allowance_config_form.amount_error {
                ui.label(egui::RichText::new(error)
                    .font(egui::FontId::new(13.0, egui::FontFamily::Proportional))
                    .color(egui::Color32::from_rgb(200, 50, 50)));
            }
        }

        ui.add_space(15.0);

        // Day of week field
        ui.label(egui::RichText::new("Day of Week")
            .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
            .strong()
            .color(egui::Color32::from_rgb(60, 60, 60)));

        ui.add_space(5.0);

        egui::ComboBox::from_label("")
            .width(200.0)
            .selected_text(self.settings.allowance_config_form.day_name())
            .show_ui(ui, |ui| {
                ui.style_mut().visuals.extreme_bg_color = egui::Color32::WHITE;
                ui.selectable_value(&mut self.settings.allowance_config_form.day_of_week, 0, "Sunday");
                ui.selectable_value(&mut self.settings.allowance_config_form.day_of_week, 1, "Monday");
                ui.selectable_value(&mut self.settings.allowance_config_form.day_of_week, 2, "Tuesday");
                ui.selectable_value(&mut self.settings.allowance_config_form.day_of_week, 3, "Wednesday");
                ui.selectable_value(&mut self.settings.allowance_config_form.day_of_week, 4, "Thursday");
                ui.selectable_value(&mut self.settings.allowance_config_form.day_of_week, 5, "Friday");
                ui.selectable_value(&mut self.settings.allowance_config_form.day_of_week, 6, "Saturday");
            });

        ui.add_space(10.0);

        // Help text
        let help_text = if self.settings.allowance_config_form.use_age_based_amount {
            "💡 Amount will automatically adjust when child has a birthday"
        } else {
            "💡 Allowance will be automatically added every week on this day"
        };
        ui.label(egui::RichText::new(help_text)
            .font(egui::FontId::new(13.0, egui::FontFamily::Proportional))
            .color(egui::Color32::from_rgb(120, 120, 120)));
    });
}
```

**Step 2: Run to verify compilation**

Run: `cargo build -p allowance-tracker-egui`
Expected: Compiles successfully

**Step 3: Commit**

```bash
git add egui-frontend/src/ui/components/settings/allowance_config_modal.rs
git commit -m "feat: add age-based checkbox to allowance config modal

Checkbox is disabled if child has no birthdate. When enabled,
amount field shows the calculated age and becomes read-only.
Help text updates to explain birthday-based adjustments.

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Task 9: Update form submission to include use_age_based_amount

**Files:**
- Modify: `egui-frontend/src/ui/components/settings/allowance_config_modal.rs`

**Step 1: Update submit_allowance_config_form**

Find `submit_allowance_config_form` and update the command creation:

```rust
/// Submit allowance configuration form
pub fn submit_allowance_config_form(&mut self) {
    log::info!("⚙️ Submitting allowance config form");

    // Skip amount validation when using age-based amount
    if !self.settings.allowance_config_form.use_age_based_amount {
        self.validate_allowance_config_form_field("amount");

        if !self.settings.allowance_config_form.is_valid {
            log::warn!("⚠️ Allowance config form validation failed");
            return;
        }
    }

    // Parse amount - use 0 for age-based (it's ignored)
    let amount = if self.settings.allowance_config_form.use_age_based_amount {
        0.0 // Amount is calculated from age, stored value doesn't matter
    } else {
        match self.settings.allowance_config_form.amount.trim().parse::<f64>() {
            Ok(amt) => amt,
            Err(e) => {
                log::error!("❌ Failed to parse amount: {}", e);
                self.settings.allowance_config_form.error_message = Some("Invalid amount format".to_string());
                return;
            }
        }
    };

    self.settings.allowance_config_form.is_saving = true;
    self.settings.allowance_config_form.error_message = None;

    let child_from_backend = self.get_current_child_from_backend();
    let child_id = child_from_backend.as_ref().map(|c| c.id.clone());

    let command = UpdateAllowanceConfigCommand {
        child_id,
        amount,
        day_of_week: self.settings.allowance_config_form.day_of_week,
        is_active: true,
        use_age_based_amount: self.settings.allowance_config_form.use_age_based_amount,
    };

    match self.backend().allowance_service.update_allowance_config(command) {
        Ok(result) => {
            log::info!("✅ Allowance config updated successfully: {}", result.success_message);
            self.settings.allowance_config_form.is_saving = false;

            // Generate appropriate success message
            let success_msg = if self.settings.allowance_config_form.use_age_based_amount {
                if let Some(age) = self.settings.allowance_config_form.current_age() {
                    format!("Age-based allowance: ${} every {}", age, self.settings.allowance_config_form.day_name())
                } else {
                    format!("Age-based allowance every {}", self.settings.allowance_config_form.day_name())
                }
            } else {
                self.settings.allowance_config_form.get_success_message()
            };
            self.settings.allowance_config_form.success_message = Some(success_msg);
            self.settings.allowance_config_form.error_message = None;

            // Update original values
            self.settings.allowance_config_form.original_amount = Some(amount);
            self.settings.allowance_config_form.original_day_of_week = Some(self.settings.allowance_config_form.day_of_week);
            self.settings.allowance_config_form.original_use_age_based_amount = Some(self.settings.allowance_config_form.use_age_based_amount);
            self.settings.allowance_config_form.has_existing_config = true;
        }
        Err(e) => {
            log::error!("❌ Failed to update allowance config: {}", e);
            self.settings.allowance_config_form.is_saving = false;
            self.settings.allowance_config_form.error_message = Some(format!("Failed to update: {}", e));
        }
    }
}
```

**Step 2: Run to verify compilation**

Run: `cargo build -p allowance-tracker-egui`
Expected: Compiles successfully

**Step 3: Commit**

```bash
git add egui-frontend/src/ui/components/settings/allowance_config_modal.rs
git commit -m "feat: update form submission to handle age-based mode

Skips amount validation when age-based is enabled. Generates
appropriate success message showing age-based or fixed amount.

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Task 10: Handle toggle-off pre-fill behavior

**Files:**
- Modify: `egui-frontend/src/ui/components/settings/allowance_config_modal.rs`

**Step 1: Update checkbox handler to pre-fill amount**

In `render_allowance_config_form_content`, update the checkbox handling:

```rust
// Age-based toggle (above amount field)
let can_use_age_based = self.settings.allowance_config_form.can_use_age_based();
let was_age_based = self.settings.allowance_config_form.use_age_based_amount;

ui.horizontal(|ui| {
    let checkbox = egui::Checkbox::new(
        &mut self.settings.allowance_config_form.use_age_based_amount,
        egui::RichText::new("Use age-based amount")
            .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
    );

    let response = ui.add_enabled(can_use_age_based, checkbox);

    if !can_use_age_based {
        response.on_disabled_hover_text("Requires child birthdate to be set");
    }
});

// Pre-fill amount when toggling OFF age-based mode
if was_age_based && !self.settings.allowance_config_form.use_age_based_amount {
    if let Some(age) = self.settings.allowance_config_form.current_age() {
        self.settings.allowance_config_form.amount = format!("{:.2}", age as f64);
        log::info!("⚙️ Pre-filled amount with current age: ${}", age);
    }
}
```

**Step 2: Run to verify compilation**

Run: `cargo build -p allowance-tracker-egui`
Expected: Compiles successfully

**Step 3: Run all tests**

Run: `cargo test -p allowance-tracker-egui`
Expected: All tests pass (172 passed, 1 pre-existing failure)

**Step 4: Commit**

```bash
git add egui-frontend/src/ui/components/settings/allowance_config_modal.rs
git commit -m "feat: pre-fill amount field when disabling age-based mode

When user toggles off age-based mode, the amount field is pre-filled
with the current age as a sensible default they can then adjust.

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Task 11: Manual testing and final verification

**Step 1: Build release version**

Run: `cargo build --release -p allowance-tracker-egui`
Expected: Builds successfully

**Step 2: Run the application**

Run: `cargo run --release -p allowance-tracker-egui`

**Step 3: Manual test checklist**

Test each scenario:

- [ ] Open allowance config modal
- [ ] Verify checkbox is disabled if child has no birthdate
- [ ] Enable age-based mode - amount field should gray out and show age
- [ ] Verify help text changes to mention birthday adjustments
- [ ] Save with age-based mode enabled
- [ ] Re-open modal - verify checkbox is still checked
- [ ] Disable age-based mode - verify amount pre-fills with age
- [ ] Navigate calendar to a month containing child's birthday
- [ ] Verify projected allowances show different amounts before/after birthday

**Step 4: Run full test suite**

Run: `cargo test -p allowance-tracker-egui`
Expected: All tests pass

**Step 5: Final commit**

```bash
git add -A
git commit -m "feat: complete age-based allowance implementation

Full implementation of age-based allowance feature:
- Checkbox in config modal to enable age-based amounts
- Amount field grays out when enabled, shows calculated age
- Pre-fills amount with age when disabling the feature
- Projection logic calculates age on each future date
- Birthday transitions correctly show different amounts

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Summary

| Task | Description | Files |
|------|-------------|-------|
| 1 | Age calculation helper | `backend/domain/age.rs`, `mod.rs` |
| 2 | Shared DTOs field | `shared/src/lib.rs` |
| 3 | Domain model field | `backend/domain/models/allowance.rs` |
| 4 | AllowanceService command | `allowance_service.rs`, `commands/allowance.rs` |
| 5 | Projection logic | `allowance_service.rs` |
| 6 | Form state | `settings/state.rs` |
| 7 | Load birthdate | `allowance_config_modal.rs` |
| 8 | Checkbox UI | `allowance_config_modal.rs` |
| 9 | Form submission | `allowance_config_modal.rs` |
| 10 | Toggle-off pre-fill | `allowance_config_modal.rs` |
| 11 | Testing | All |
