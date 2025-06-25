# Goals Feature Implementation Plan

## Overview
This document outlines the implementation plan for adding a 'goals' feature to the allowance tracker. A goal is a target amount with a description (e.g., "buy a new lego set" with a $40 target) that tracks progress and calculates projected completion dates.

## Requirements Summary
- **One goal at a time per child** - only one active goal allowed
- **Goal lifecycle states**: `active`, `cancelled`, `completed`
- **History tracking** - append-only storage with full audit trail
- **Forward-looking calculations** - project completion dates using future allowances
- **Automatic completion** - mark goals as completed when balance meets target
- **Edge case handling** - proper error handling for impossible scenarios
- **Current balance + future allowances** - calculation starts from current balance

## Architecture Overview
Following the existing layered architecture:
```
Storage Layer (CSV) → Domain Layer (Services) → REST API Layer
```

## Implementation Steps

### Phase 1: Storage Layer (CSV Repository)

#### 1.1 Goal Storage Trait
**File**: `src-tauri/src/backend/storage/traits.rs`
- Add `GoalStorage` trait following existing patterns
- Methods: `store_goal`, `get_goal`, `list_goals`, `update_goal`, `delete_goal`
- Support for child-specific goal operations

#### 1.2 Goal CSV Repository  
**File**: `src-tauri/src/backend/storage/csv/goal_repository.rs`
- CSV structure: `id,child_id,description,target_amount,state,created_at,updated_at`
- Append-only approach with state tracking
- Per-child goal files: `{child_directory}/goals.csv`
- Git integration for version control
- Atomic file operations with temp files

#### 1.3 Storage Module Updates
**File**: `src-tauri/src/backend/storage/csv/mod.rs`
- Export `GoalRepository`
- Update module structure

**File**: `src-tauri/src/backend/storage/mod.rs`  
- Re-export `GoalStorage` trait

### Phase 2: Domain Layer (Business Logic)

#### 2.1 Shared Types
**File**: `shared/src/lib.rs`
- `Goal` struct with lifecycle states
- `GoalState` enum: `Active`, `Cancelled`, `Completed`
- Request/Response types for CRUD operations
- Goal calculation result types

#### 2.2 Goal Domain Service
**File**: `src-tauri/src/backend/domain/goal_service.rs`
- Core business logic for goal management
- Goal completion date calculations using `AllowanceService`
- State transition logic (active → completed/cancelled)
- Validation rules and business constraints
- Integration with balance and allowance services

#### 2.3 Domain Module Updates
**File**: `src-tauri/src/backend/domain/mod.rs`
- Export `GoalService`
- Add to module structure

### Phase 3: REST API Layer

#### 3.1 Goal REST APIs
**File**: `src-tauri/src/backend/io/rest/goal_apis.rs`
- `GET /api/goals/current` - Get current active goal with projected completion
- `POST /api/goals` - Create new goal (replaces any existing active goal)
- `PUT /api/goals` - Update current active goal
- `DELETE /api/goals` - Cancel current active goal
- `GET /api/goals/history` - Get goal history for analysis

#### 3.2 Backend Module Updates
**File**: `src-tauri/src/backend/mod.rs`
- Add `GoalService` to `AppState`
- Wire up goal routes in router
- Update service initialization

### Phase 4: Testing

#### 4.1 Unit Tests
- Goal repository operations
- Goal service business logic
- State transition scenarios
- Edge case handling
- Calculation accuracy

#### 4.2 Integration Tests
- End-to-end API testing
- Cross-service integration
- Data persistence verification

## Detailed Implementation Specifications

### Goal Data Model
```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Goal {
    pub id: String,
    pub child_id: String,
    pub description: String,
    pub target_amount: f64,
    pub state: GoalState,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum GoalState {
    Active,
    Cancelled,
    Completed,
}
```

### CSV File Structure
```csv
id,child_id,description,target_amount,state,created_at,updated_at
goal::1234567890,child::abc,Buy new lego set,40.0,active,2025-01-20T10:00:00Z,2025-01-20T10:00:00Z
goal::1234567891,child::abc,Buy new lego set,40.0,completed,2025-01-20T10:00:00Z,2025-02-15T14:30:00Z
```

### Calculation Logic
1. **Get current balance** from latest transaction
2. **Project future allowances** using existing `AllowanceService`
3. **Calculate weeks needed** to reach target amount
4. **Handle edge cases**:
   - Already at/above target → Error
   - No allowance configured → Error  
   - Takes >1 year → Special indicator
   - Zero/negative allowance → Error

### API Endpoints

#### GET /api/goals/current
```json
{
  "goal": {
    "id": "goal::1234567890",
    "child_id": "child::abc", 
    "description": "Buy new lego set",
    "target_amount": 40.0,
    "state": "active",
    "created_at": "2025-01-20T10:00:00Z",
    "updated_at": "2025-01-20T10:00:00Z"
  },
  "calculation": {
    "current_balance": 15.50,
    "amount_needed": 24.50,
    "projected_completion_date": "2025-03-15T12:00:00Z",
    "allowances_needed": 5,
    "is_achievable": true
  }
}
```

#### POST /api/goals
```json
{
  "description": "Buy new lego set",
  "target_amount": 40.0
}
```

### Error Handling
- **Already at target**: `GOAL_ALREADY_ACHIEVABLE`
- **No allowance**: `NO_ALLOWANCE_CONFIGURED`
- **Takes too long**: `GOAL_EXCEEDS_TIME_LIMIT`
- **Invalid amount**: `INVALID_TARGET_AMOUNT`
- **Active goal exists**: `ACTIVE_GOAL_EXISTS`

### Business Rules
1. **One active goal per child maximum**
2. **Automatic completion** when balance ≥ target
3. **Historical preservation** - never delete goal records
4. **State transitions**: active → (completed|cancelled)
5. **Target validation**: must be positive and > current balance
6. **Description limits**: 1-256 characters

## File Structure Overview
```
src-tauri/src/backend/
├── storage/
│   ├── csv/
│   │   ├── goal_repository.rs    # NEW
│   │   └── mod.rs               # UPDATED
│   ├── traits.rs                # UPDATED (add GoalStorage)
│   └── mod.rs                   # UPDATED
├── domain/
│   ├── goal_service.rs          # NEW
│   └── mod.rs                   # UPDATED
├── io/rest/
│   ├── goal_apis.rs             # NEW
│   └── mod.rs                   # UPDATED
└── mod.rs                       # UPDATED (AppState, router)

shared/src/
└── lib.rs                       # UPDATED (Goal types)
```

## Testing Strategy
- **Repository tests**: CSV operations, file handling
- **Service tests**: Business logic, calculations, edge cases
- **API tests**: HTTP endpoints, serialization
- **Integration tests**: End-to-end scenarios

## Success Criteria
- [ ] CSV repository with append-only goal history
- [ ] Goal service with accurate completion date calculations
- [ ] REST APIs for full CRUD operations
- [ ] Comprehensive test coverage
- [ ] Proper error handling for all edge cases
- [ ] Integration with existing allowance system
- [ ] Documentation and examples

This plan follows the existing codebase patterns and provides a solid foundation for implementing the goals feature while maintaining code quality and consistency. 