# Testing Guide: Automatic Cleanup & Test Utilities

## Overview

This project uses **RAII-based automatic cleanup** to ensure test data is always cleaned up, even if tests panic or fail. This eliminates the problem of orphaned test directories accumulating over time.

## Key Benefits

✅ **Guaranteed Cleanup**: Test data is cleaned up automatically when tests complete, panic, or fail  
✅ **No Manual Cleanup**: No need to call cleanup functions manually  
✅ **Consistent Environment**: Every test starts with a fresh, isolated environment  
✅ **Performance**: Tests run faster without accumulating orphaned directories  
✅ **Reliability**: Test failures don't leave behind test artifacts  

## Test Utilities

### `TestEnvironment` - Basic Test Environment

For simple tests that need just a CSV connection:

```rust
use crate::backend::storage::csv::test_utils::TestEnvironment;

#[tokio::test]
async fn test_my_feature() -> Result<()> {
    let env = TestEnvironment::new().await?;
    let repo = MyRepository::new(env.connection.clone());
    
    // Your test code here
    // Cleanup happens automatically when env goes out of scope
    
    Ok(())
}
```

### `RepositoryTestHelper` - Full Repository Access

For complex tests that need multiple repositories:

```rust
use crate::backend::storage::csv::test_utils::RepositoryTestHelper;

#[tokio::test]
async fn test_complex_scenario() -> Result<()> {
    let helper = RepositoryTestHelper::new().await?;
    
    // Create a test child
    let child = helper.create_test_child("Test Child", "123").await?;
    
    // Use any repository
    helper.transaction_repo.store_transaction(&transaction).await?;
    helper.goal_repo.store_goal(&goal).await?;
    helper.child_repo.set_active_child(&child.id).await?;
    
    // Automatic cleanup when helper goes out of scope
    Ok(())
}
```

## Migration from Old Patterns

### ❌ Old Pattern (Manual Cleanup)

```rust
// DON'T USE - This leaves test directories if tests panic
async fn setup_test_repo() -> (Repository, impl Fn() -> Result<()>) {
    let connection = CsvConnection::new_for_testing().await.unwrap();
    let cleanup_dir = connection.base_directory().to_path_buf();
    let repo = Repository::new(connection);
    
    let cleanup = move || {
        if cleanup_dir.exists() {
            std::fs::remove_dir_all(&cleanup_dir)?;
        }
        Ok(())
    };
    
    (repo, cleanup)
}

#[tokio::test]
async fn test_old_way() {
    let (repo, cleanup) = setup_test_repo().await;
    
    // Test code...
    
    cleanup().unwrap(); // ❌ Never called if test panics!
}
```

### ✅ New Pattern (Automatic Cleanup)

```rust
// USE THIS - Guaranteed cleanup via RAII
#[tokio::test] 
async fn test_new_way() -> Result<()> {
    let helper = RepositoryTestHelper::new().await?;
    
    // Test code...
    
    Ok(()) // ✅ Cleanup happens automatically
}
```

## Test Environment Features

### Automatic Cleanup

- **RAII Pattern**: Cleanup happens when `TestEnvironment` goes out of scope
- **Panic Safety**: Even if tests panic, cleanup still occurs
- **No Memory Leaks**: TempDir ensures complete cleanup

### Isolated Environments

- Each test gets a unique temporary directory
- No interference between concurrent tests
- Completely isolated from production data

### Built-in Utilities

```rust
// Create standard test child
let child = helper.create_test_child("Child Name", "unique_id").await?;

// Create child with custom birthdate  
let child = helper.create_test_child_with_birthdate(
    "Child Name", 
    "unique_id",
    chrono::NaiveDate::from_ymd_opt(2015, 5, 15).unwrap()
).await?;

// Access all repositories
helper.transaction_repo
helper.child_repo
helper.goal_repo  
helper.allowance_repo
helper.parental_control_repo
helper.global_config_repo
```

## Debugging Tests

### Enable Debug Output

Set environment variable to see cleanup logs:

```bash
export ALLOWANCE_TRACKER_DEBUG_TESTS=1
cargo test
```

This will show:
```
🧹 Cleaning up test environment: /tmp/.tmpXXXXXX
```

### Custom Test Prefixes

For easier debugging, use custom prefixes:

```rust
let helper = RepositoryTestHelper::new_with_prefix("my_test").await?;
// Creates directories like: /tmp/my_testXXXXXX
```

## Orphaned Directory Cleanup

The system includes a global cleanup function for any directories left by older test patterns:

```rust
use crate::backend::storage::csv::test_utils::cleanup_orphaned_test_directories;

// Call this to clean up any old test_data_* directories
cleanup_orphaned_test_directories()?;
```

## Best Practices

### 1. Always Use Result Types

```rust
#[tokio::test]
async fn my_test() -> Result<()> { // ✅ Use Result<()>
    // Test code
    Ok(())
}
```

### 2. Let RAII Handle Cleanup

```rust
// ✅ Good - automatic cleanup
{
    let helper = RepositoryTestHelper::new().await?;
    // Use helper...
} // Cleanup happens here automatically

// ❌ Bad - manual cleanup
let helper = RepositoryTestHelper::new().await?;
// Use helper...
helper.cleanup()?; // Manual cleanup can be forgotten
```

### 3. Use Helper Methods

```rust
// ✅ Good - use helper methods
let child = helper.create_test_child("Test Child", "123").await?;

// ❌ Harder - manual child creation
let child = DomainChild { /* ... */ };
helper.child_repo.store_child(&child).await?;
```

### 4. Test Isolation

```rust
// ✅ Good - each test gets fresh environment
#[tokio::test]
async fn test_a() -> Result<()> {
    let helper = RepositoryTestHelper::new().await?; // Fresh environment
    // Test A logic
    Ok(())
}

#[tokio::test] 
async fn test_b() -> Result<()> {
    let helper = RepositoryTestHelper::new().await?; // Different fresh environment
    // Test B logic  
    Ok(())
}
```

## Performance Benefits

- **No Accumulation**: Test directories don't accumulate over time
- **Faster CI**: No need to clean up orphaned directories between runs
- **Parallel Safe**: Each test has isolated environment
- **Resource Efficient**: Immediate cleanup releases disk space

## Troubleshooting

### Test Failing with "No such file or directory"

This usually means the test is trying to access data after cleanup. Make sure your test variables are properly scoped:

```rust
// ❌ Bad - trying to use data after cleanup
let path;
{
    let env = TestEnvironment::new().await?;
    path = env.base_directory().to_path_buf();
} // env cleaned up here
assert!(path.exists()); // ❌ Will fail

// ✅ Good - use data within scope
{
    let env = TestEnvironment::new().await?;
    let path = env.base_directory();
    assert!(path.exists()); // ✅ Works
} // Cleanup after assertion
```

### Tests Leaving Directories

If you see `test_data_*` directories, it means:

1. Old test patterns are still being used
2. Tests are using `CsvConnection::new_for_testing()` directly

**Solution**: Migrate to `TestEnvironment` or `RepositoryTestHelper`

## Summary

The new test utilities provide:

- ✅ **Guaranteed cleanup** via RAII pattern
- ✅ **Simple migration** from old patterns  
- ✅ **Better performance** without orphaned directories
- ✅ **Improved reliability** with panic-safe cleanup
- ✅ **Developer-friendly** helper methods

All new tests should use `TestEnvironment` or `RepositoryTestHelper` for automatic cleanup and better reliability. 