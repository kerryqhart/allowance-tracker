# CSV Storage Layer - Architectural Invariants

## **CRITICAL INVARIANT: Date String Isolation**

**The CSV layer must handle ALL datetime string parsing internally. No date strings should ever leave the CSV layer.**

### Rule:
- **CSV Layer**: Reads date strings from files, parses them into proper datetime objects
- **Domain Layer**: Only works with structured datetime objects (never strings)
- **Frontend Layer**: Only receives structured datetime objects (never strings)

### Current Violation:
The domain `Transaction` model currently has:
```rust
pub date: String,  // ❌ VIOLATION - should be DateTime or similar
```

This should be:
```rust
pub date: chrono::DateTime<chrono::FixedOffset>,  // ✅ CORRECT
```

### Why This Matters:
1. **Single Source of Truth**: Date parsing logic concentrated in one place
2. **Type Safety**: Compile-time guarantees about date validity  
3. **No Format Surprises**: Upper layers never encounter unexpected date formats
4. **Easier Testing**: Mock datetime objects instead of string formats
5. **Better Debugging**: Parsing errors caught at the storage boundary

### Implementation Pattern:
```rust
// CSV Layer (ONLY place that deals with date strings)
fn read_transaction_from_csv(row: &StringRecord) -> Result<DomainTransaction> {
    let date_str = row.get(2).unwrap();
    let parsed_date = chrono::DateTime::parse_from_rfc3339(date_str)
        .or_else(|_| parse_alternative_format(date_str))?;
    
    Ok(DomainTransaction {
        date: parsed_date,  // Structured datetime object
        // ... other fields
    })
}

// Domain Layer (works with structured objects)
fn process_transaction(tx: &DomainTransaction) {
    let month = tx.date.month();  // Direct access, no parsing
    // ... business logic
}
```

### Test Requirements:
All tests MUST enforce this invariant:

1. **CSV Layer Tests**: Test that date strings are parsed into DateTime objects
2. **Domain Model Tests**: Verify domain models only accept DateTime objects
3. **Integration Tests**: Ensure no date strings cross layer boundaries
4. **Type Safety Tests**: Compile-time verification of date field types

### Current Status: 🚨 VIOLATED
The system currently leaks date strings from CSV → Domain → Frontend, causing parsing errors in the frontend.

### Test Coverage Required:
- Date parsing from multiple timezone formats
- Error handling for invalid date strings
- Type safety verification
- Integration across all layers 