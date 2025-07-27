# Allowance Refresh Feature

## Overview

The allowance tracker now includes automatic periodic refresh capability that allows the app to issue pending allowances without requiring a restart. This is especially useful for long-running sessions where the app may be open for extended periods.

## How It Works

### Timing Strategy

The refresh feature uses a **throttled timing approach** to prevent excessive CPU usage:

- **`Instant::now()`** - Tracks when the last allowance check was performed
- **`Duration`** - Defines the refresh interval (default: 5 minutes)
- **Throttled checks** - Only checks allowances when enough time has passed

### Why This Approach?

Since egui's `update()` loop runs 60+ times per second, we need to prevent checking allowances every frame:

- ❌ **Frame counting** - Frame rates vary, so timing would be inconsistent
- ❌ **External timers** - Overkill for this simple use case
- ✅ **Instant/Duration** - Designed for measuring time intervals, perfect for this use case

### Implementation Details

#### UI State (`ui_state.rs`)
```rust
pub struct UIState {
    pub last_allowance_refresh: Option<Instant>,
    pub allowance_refresh_interval: Duration,
}

impl UIState {
    pub fn should_refresh_allowances(&self) -> bool {
        // Returns true if enough time has passed since last refresh
    }
    
    pub fn mark_allowance_refresh(&mut self) {
        // Updates the timestamp when refresh is performed
    }
}
```

#### App Coordinator (`app_coordinator.rs`)
```rust
impl eframe::App for AllowanceTrackerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // ... other update logic ...
        
        // Check for pending allowances periodically (throttled)
        self.refresh_allowances();
        
        // ... rest of update logic ...
    }
}

impl AllowanceTrackerApp {
    pub fn refresh_allowances(&mut self) {
        if self.ui.should_refresh_allowances() {
            // Use existing backend method to check and issue allowances
            match self.core.backend.transaction_service.check_and_issue_pending_allowances() {
                Ok(count) => {
                    if count > 0 {
                        log::info!("🎯 Periodic refresh: Issued {} pending allowances", count);
                    }
                }
                Err(e) => {
                    log::warn!("🎯 Periodic refresh failed: {}", e);
                }
            }
            
            self.ui.mark_allowance_refresh();
        }
    }
}
```

## Benefits

1. **Automatic Operation** - No user intervention required
2. **Efficient** - Only checks when needed, not every frame
3. **Configurable** - Easy to adjust refresh interval
4. **Non-blocking** - Doesn't freeze the UI during checks
5. **Reliable** - Uses system time, not frame-dependent
6. **Background Operation** - Errors are logged but don't interrupt user experience

## Configuration

The refresh interval can be adjusted by modifying:
```rust
pub allowance_refresh_interval: Duration = Duration::from_secs(300), // 5 minutes
```

Common intervals:
- `Duration::from_secs(60)` - Check every minute
- `Duration::from_secs(300)` - Check every 5 minutes (default)
- `Duration::from_secs(600)` - Check every 10 minutes

## Testing

The feature includes comprehensive tests in `ui_state.rs`:
- Tests initial refresh behavior
- Tests timing intervals
- Tests configuration changes

## Logging

The refresh feature logs its activity:
- `🔄 Performing periodic allowance refresh check` - When refresh starts
- `🎯 Periodic refresh: Issued X pending allowances` - When allowances are issued
- `🎯 Periodic refresh: No pending allowances found` - When no allowances are due
- `🎯 Periodic refresh failed: <error>` - When errors occur

## Future Enhancements

Potential improvements:
- User-configurable refresh intervals
- Visual indicators when allowances are issued
- More granular error handling
- Integration with system notifications 