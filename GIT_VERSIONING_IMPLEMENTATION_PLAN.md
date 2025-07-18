# Git Versioning Implementation Plan

## Status: ✅ COMPLETED

## Overview
Implement local git repository versioning for each child directory in the allowance tracker. Each child's data will be version controlled independently using local git repositories.

## Design Decisions

### Git Library Choice
- **Selected**: `git2` (libgit2 bindings for Rust)
- **Rationale**: Mature, stable, well-documented, perfect for local git operations
- **Alternative considered**: `gix` (gitoxide) - too complex for our needs

### Scope
- **Layer**: Storage layer only (`src-tauri/src/backend/storage/`)
- **Child directories**: Each becomes its own git repository
- **Global config**: NOT version controlled (remains outside child directories)

### Files to Version Control
Per child directory:
- ✅ `child.yaml` (child configuration)
- ✅ `transactions.csv` (child's transaction data)
- ✅ `allowance_config.yaml` (child's allowance settings)
- ✅ `parental_control_attempts.csv` (child's parental control attempts)
- ❌ Global files (outside child scope)

### Current Directory Structure
```
data/
├── global_config.yaml          # Global settings (NOT versioned)
└── {child_name}/              # Each child has own directory (GIT REPO)
    ├── .git/                  # Git repository data
    ├── child.yaml             # Child configuration (versioned)
    ├── transactions.csv       # Child's transaction data (versioned)
    ├── allowance_config.yaml  # Child's allowance settings (versioned)
    └── parental_control_attempts.csv  # Child's attempts (versioned)
```

## Implementation Plan

### Phase 1: Add Git Dependencies
- Add `git2 = "0.20"` to `src-tauri/Cargo.toml`

### Phase 2: Create Git Module
- Create `src-tauri/src/backend/storage/git/mod.rs`
- Implement `GitManager` struct with methods:
  - `init_repo(path)` - Initialize git repository
  - `add_file(repo_path, file_path)` - Stage file for commit
  - `commit(repo_path, message)` - Create commit
  - `get_status(repo_path)` - Check repository status
  - `ensure_repo_exists(path)` - Initialize if needed

### Phase 3: Update CSV Repositories
Modify these repositories to integrate git operations:
- `TransactionRepository` - on CSV writes
- `ChildRepository` - on child.yaml writes
- `AllowanceRepository` - on allowance_config.yaml writes
- `ParentalControlRepository` - on CSV writes

### Phase 4: Git Integration Pattern
For each repository write operation:
1. Perform existing file write (maintain current behavior)
2. Ensure git repository exists in child directory
3. Stage the modified file
4. Commit with descriptive message
5. Handle git errors gracefully (log but don't fail operation)

## Configuration

### Git Settings
Default configuration (can be made configurable later):
- **Author Name**: "Allowance Tracker"
- **Author Email**: "allowance@tracker.local"
- **Auto-initialize**: true (create git repos for existing child directories)

### Commit Messages
Format: `"Update {filename}: {action}"`
Examples:
- `"Update transactions.csv: Added $5.00 allowance transaction"`
- `"Update child.yaml: Updated child configuration"`
- `"Update allowance_config.yaml: Updated allowance settings"`

## Error Handling Strategy
- Git operations are **non-blocking**
- If git operation fails:
  - Log the error with details
  - Continue with the main operation
  - Don't prevent user functionality
- This ensures git versioning is additive, not disruptive

## Backward Compatibility
- Existing child directories continue to work
- Git repositories are created on-demand
- No changes to existing API contracts
- No changes above storage layer

## Testing Strategy
- Unit tests for `GitManager` operations
- Integration tests for repository git integration
- Test error handling scenarios
- Test with existing child directories

## Files to Modify
### New Files
- `src-tauri/src/backend/storage/git/mod.rs`

### Modified Files
- `src-tauri/Cargo.toml` (add git2 dependency)
- `src-tauri/src/backend/storage/mod.rs` (expose git module)
- `src-tauri/src/backend/storage/csv/transaction_repository.rs`
- `src-tauri/src/backend/storage/csv/child_repository.rs`
- `src-tauri/src/backend/storage/csv/allowance_repository.rs`
- `src-tauri/src/backend/storage/csv/parental_control_repository.rs`

## Success Criteria ✅
- ✅ Each child directory becomes a git repository
- ✅ All file modifications are automatically committed
- ✅ Existing functionality remains unchanged
- ✅ Git operations don't block or fail main operations
- ✅ Clear commit history for each child's data changes

## Implementation Summary
The git versioning feature has been successfully implemented with the following components:

### ✅ Completed Components
1. **GitManager Module** (`src-tauri/src/backend/storage/git/mod.rs`)
   - Full git repository management functionality
   - Non-blocking error handling
   - Comprehensive test coverage

2. **Repository Integration**
   - ✅ TransactionRepository: Auto-commits `transactions.csv` changes
   - ✅ ChildRepository: Auto-commits `child.yaml` changes  
   - ✅ AllowanceRepository: Auto-commits `allowance_config.yaml` changes
   - ✅ ParentalControlRepository: Auto-commits `parental_control_attempts.csv` changes

3. **Git Features**
   - ✅ Automatic repository initialization
   - ✅ File staging and committing
   - ✅ Descriptive commit messages
   - ✅ Repository status checking
   - ✅ Non-blocking operation (errors logged but don't fail main operations)

### ✅ Verified Functionality
- Git repositories are automatically created in child directories
- File changes are committed with appropriate messages
- Error handling works correctly (non-blocking)
- Git operations integrate seamlessly with existing CSV storage
- All tests pass successfully

## Future Enhancements (Out of Scope)
- Git repository browsing UI
- Rollback functionality
- Branch management
- Remote repository integration
- Diff viewing 