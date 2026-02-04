# Age-Based Allowance Feature Design

## Overview

Add an allowance policy option that automatically sets the allowance amount to the child's age in years. When enabled, the system calculates the appropriate amount based on the child's birthdate, including correct handling of birthday transitions in forward projections.

## Requirements

- Checkbox in allowance config to enable age-based amounts
- When enabled, manual amount field is grayed out and displays the age-derived amount
- Forward projections account for upcoming birthdays (amount changes at the correct week)
- Birthday falling ON allowance day gets the new age amount

## Design Decisions

| Question | Decision |
|----------|----------|
| No birthdate stored? | Checkbox is disabled until birthdate exists |
| Birthday on allowance day? | Gets the new age that day |
| Toggle off age-based mode? | Pre-fill manual field with current age |
| Age limits (min/max)? | None - age is age, pure math |

## Data Model Changes

### AllowanceConfig

Add one field to the allowance configuration:

```rust
pub use_age_based_amount: bool,  // defaults to false
```

When `use_age_based_amount` is `true`:
- The stored `amount` field is ignored for calculations
- The child's age (in years) at the transaction date determines the dollar amount

The `amount` field remains stored even in age-based mode to preserve the last manual value.

No changes to Child model - it already has `birthdate: NaiveDate`.

## UI Changes

### Allowance Config Modal

Add checkbox above the amount field:

```
┌─────────────────────────────────────────┐
│  Allowance Configuration                │
├─────────────────────────────────────────┤
│  ☑ Use age-based amount                 │
│     (Child's age in years = $ amount)   │
│                                         │
│  Weekly Amount: [$8.00    ] ← grayed    │
│                                         │
│  Day of Week:  [Friday ▼]               │
│                                         │
│  ☑ Active                               │
│                                         │
│        [Cancel]  [Save]                 │
└─────────────────────────────────────────┘
```

**Behavior:**
- Checkbox disabled if child has no birthdate (tooltip: "Requires birthdate")
- When checked: Amount field becomes read-only, displays current age as dollars
- When unchecked: Amount field is editable; if transitioning from checked, pre-fill with age value

## Projection Logic

### Current Logic

```
For each future date matching allowance day:
  → Create FutureAllowance with config.amount
```

### New Logic

```
For each future date matching allowance day:
  → If config.use_age_based_amount:
      Calculate child's age ON THAT DATE
      amount = age_in_years
  → Else:
      amount = config.amount
  → Create FutureAllowance with amount
```

### Age Calculation

```rust
fn age_on_date(birthdate: NaiveDate, target_date: NaiveDate) -> i32 {
    let years = target_date.year() - birthdate.year();
    let had_birthday = (target_date.month(), target_date.day())
                       >= (birthdate.month(), birthdate.day());
    if had_birthday { years } else { years - 1 }
}
```

**Example:** Child born Feb 8, 2019. Allowance day is Friday.
- Friday Feb 3, 2025 → age is 5 → $5
- Friday Feb 10, 2025 → age is 6 → $6 (birthday was Feb 8)

## Backend Changes

### AllowanceService

1. `update_allowance_config()` - Accept and store `use_age_based_amount` field
2. `generate_future_allowance_transactions()` - Fetch child birthdate when age-based mode is enabled, calculate age per projection date

### AllowanceRepository

- Update YAML serialization to include `use_age_based_amount`
- Migration: existing configs without this field default to `false`

### Shared DTOs

- `AllowanceConfig` in `shared/src/lib.rs` gets `use_age_based_amount: bool`
- `UpdateAllowanceConfigRequest` gets the same field

### No Changes Needed

- BalanceService (calculates projected balance the same way)
- Calendar logic (displays what it's given)
- Transaction storage

## Files to Modify

| File | Change |
|------|--------|
| `shared/src/lib.rs` | Add field to DTOs |
| `backend/domain/models/allowance.rs` | Add field to domain model |
| `backend/storage/csv/allowance_repository.rs` | Update YAML serialization |
| `backend/domain/allowance_service.rs` | Update projection logic |
| `egui-frontend/src/ui/components/settings/allowance_config_modal.rs` | Add checkbox UI |
| `egui-frontend/src/ui/components/settings/state.rs` | Add form state field |

## Testing Strategy

### Unit Tests

1. **Age calculation function**
   - Birthday today → new age
   - Birthday tomorrow → old age
   - Birthday yesterday → new age
   - Leap year birthdays (Feb 29)

2. **Projection generation with age-based amounts**
   - Multiple weeks, no birthday crossing → same amount
   - Birthday in the middle of range → amount changes at correct week
   - Birthday ON allowance day → gets new age that day

3. **Config persistence**
   - Save with `use_age_based_amount: true`, reload, verify
   - Migration: load old config without field → defaults to `false`

4. **UI form state**
   - Toggle on: amount field shows age, becomes read-only
   - Toggle off: amount field pre-fills with age, becomes editable
   - No birthdate: checkbox disabled

### Manual Testing

- Set up child with upcoming birthday
- Enable age-based allowance
- Verify calendar shows correct amounts before/after birthday
