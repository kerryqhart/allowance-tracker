# Shared DTO to Domain Model Migration Plan

This document outlines the plan to refactor the backend to enforce a strict separation between shared Data Transfer Objects (DTOs) and internal domain models. The goal is to improve code maintainability, enforce encapsulation, and allow the backend domain to evolve independently from the public API contract.

## Guiding Principle
- **Shared DTOs (`shared` crate):** These are for transport-level concerns only. They define the shape of data for the frontend (Yew/WASM) and the REST API boundary. They should NEVER be used inside the domain or storage layers.
- **Domain Models (`src-tauri/src/backend/domain`):** These represent the core business logic, rules, and entities. They are the heart of the application and should not be influenced by how data is presented to the outside world.
- **Mapping Layer (`src-tauri/src/backend/io/rest/mappers`):** This layer is responsible for the bidirectional translation between shared DTOs and internal domain models.

## Phase 1: Establish Domain Model Foundation

1.  **Create Domain Model Directory:** For each major entity in `shared/src/lib.rs`, create a corresponding, business-focused domain model inside `src-tauri/src/backend/domain/`.
    -   `src-tauri/src/backend/domain/models/transaction.rs`
    -   `src-tauri/src/backend/domain/models/child.rs`
    -   `src-tauri/src/backend/domain/models/goal.rs`
    -   `src-tauri/src/backend/domain/models/allowance.rs`

2.  **Define Rich Domain Models:** Unlike the simple data structures in `shared`, these models can contain business logic, validation, and domain-specific types (e.g., `TransactionId`, `Money`).

## Phase 2: Implement the Mapping (Anti-Corruption) Layer

1.  **Create Mapper Directory:** Create a dedicated module for the translation logic at `src-tauri/src/backend/io/rest/mappers/`.

2.  **Implement Mappers:** For each domain model, create a corresponding mapper responsible for converting between the DTO and the domain model.
    -   `transaction_mapper.rs`: `TransactionMapper::to_domain(dto: shared::Transaction) -> domain::Transaction` and `TransactionMapper::to_dto(domain: domain::Transaction) -> shared::Transaction`.
    -   `child_mapper.rs`
    -   And so on for other models.

## Phase 3: Refactor from the Outside In

This phase will be executed iteratively for each bounded context (e.g., Child, Transaction, etc.).

1.  **Update REST API Endpoints:**
    -   Modify the Axum handlers in `src-tauri/src/backend/io/rest/` to accept shared DTOs.
    -   Immediately use the mapping layer to convert the DTO to a domain model.
    -   Pass the domain model to the corresponding domain service.
    -   When the service returns a result (as a domain model), use the mapper to convert it back to a shared DTO before sending the response.

2.  **Refactor Domain Services:**
    -   Update all public methods in the domain services (`src-tauri/src/backend/domain/`) to exclusively accept and return domain models.
    -   Remove all `use shared::...` statements from the domain services. The compiler will be our guide here.

3.  **Adapt Storage Layer:**
    -   Update the repository traits in `src-tauri/src/backend/storage/traits.rs` to use domain models.
    -   Update the CSV/SQLite implementations in `src-tauri/src/backend/storage/` to handle the new domain models. This may involve internal mapping if the storage representation differs from the domain representation.

## Implementation Order (Iterative)

We will migrate one bounded context at a time to keep the application in a working state.

1.  **Child Bounded Context (START HERE)**
    -   `shared::Child` -> `domain::Child`
    -   `shared::ActiveChildResponse` -> `domain::ActiveChild` (or similar)
    -   Refactor `child_service.rs`, `child_apis.rs`, and `child_repository.rs`.
    -   Update or create unit tests.
    -   Verify with `cargo check` and `cargo tauri dev`.

2.  **Transaction Bounded Context**
    -   `shared::Transaction` -> `domain::Transaction`
    -   Refactor `transaction_service.rs`, `transaction_apis.rs`, etc.

3.  **Allowance Bounded Context**
    -   `shared::Allowance` -> `domain::Allowance`
    -   Refactor `allowance_service.rs`, `allowance_apis.rs`, etc.

4.  **Goal Bounded Context**
    -   `shared::Goal` -> `domain::Goal`
    -   Refactor `goal_service.rs`, `goal_apis.rs`, etc.

5.  **Continue for all other shared types.**

## Success Criteria for Each Step

-   **Clean Boundaries:** No `shared` imports are present in the `domain` or `storage` layers for the migrated context.
-   **Passing Tests:** All relevant unit and integration tests pass.
-   **Working Application:** The application builds and runs successfully via `cargo tauri dev` after each context is migrated.
-   **Compiler Driven:** We will rely heavily on `cargo check` to identify all locations that require changes. 