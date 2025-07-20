# Allowance Tracker

A fun and engaging desktop allowance tracking application for kids, built with Rust and egui. Help children learn financial responsibility by tracking their allowance, spending, and savings goals with an intuitive visual interface.

## Features

- **Visual Calendar View** - See transactions and allowances on an interactive calendar
- **Transaction Management** - Add money, record spending, and track transaction history
- **Balance Tracking** - Real-time balance calculations with visual charts
- **Savings Goals** - Set and track progress toward savings targets
- **Child Profiles** - Support for multiple children with individual accounts
- **CSV Data Storage** - Human-readable data files for easy backup and portability
- **Native Desktop UI** - Fast, responsive interface built with egui

## Architecture

This is a native Rust desktop application with a clean, modular architecture:

```
allowance-tracker/
├── egui-frontend/      # egui native GUI frontend
├── shared/            # Shared types and utilities  
├── backend/           # Domain services and storage
│   ├── domain/        # Business logic services
│   └── storage/       # CSV and future SQLite repositories
└── Cargo.toml         # Workspace configuration
```

### Technology Stack

- **[egui](https://github.com/emilk/egui)** - Immediate mode GUI framework for Rust
- **[eframe](https://github.com/emilk/egui/tree/master/crates/eframe)** - Application framework for egui
- **CSV Storage** - File-based data persistence for human-readable data
- **[Chrono](https://github.com/chronotope/chrono)** - Date and time handling
- **[Serde](https://github.com/serde-rs/serde)** - Serialization and deserialization
- **[Anyhow](https://github.com/dtolnay/anyhow)** - Error handling

## Quick Start

### Prerequisites

- [Rust](https://rustup.rs/) (latest stable version)
- Git

### Installation

1. **Clone the repository:**
   ```bash
   git clone <repository-url>
   cd allowance-tracker
   ```

2. **Build and run:**
   ```bash
   cargo run --bin allowance-tracker-egui
   ```

The application will create data files in your user directory automatically on first run.

### First Time Setup

1. Launch the application
2. Create a child profile
3. Set up allowance configuration (optional)
4. Start adding transactions!

## Development

### Development Commands

- **Run in development mode:**
  ```bash
  cargo run --bin allowance-tracker-egui
  ```

- **Check all workspace members:**
  ```bash
  cargo check --workspace
  ```

- **Run all tests:**
  ```bash
  cargo test --workspace
  ```

- **Build optimized release:**
  ```bash
  cargo build --release
  ```

### Project Structure

#### Frontend (`egui-frontend/`)
- **Immediate Mode GUI** - UI state computed in render functions, not stored
- **Native Desktop Feel** - Responsive layout that adapts to window resizing
- **Built-in Widgets** - Leverages egui's efficient rendering and input handling

#### Backend (`backend/`)
- **Domain Services** - UI-agnostic business logic
- **CSV Storage** - Human-readable data persistence with proper error handling
- **Clean Architecture** - Separation of concerns between UI and business logic

#### Shared (`shared/`)
- **Common Types** - Request/response DTOs and domain models
- **Type Safety** - Compile-time guarantees for data consistency
- **Cross-Component Contract** - Shared interface between frontend and backend

### Code Guidelines

- **Rust Best Practices** - Idiomatic Rust with proper error handling
- **Immediate Mode Patterns** - UI state in functions, not stored
- **Type Safety** - Use `Result<T, E>` for error handling
- **Clean Separation** - Domain services remain UI-agnostic

## Data Storage

### CSV-Based Persistence

Data is stored in human-readable CSV files:

- **Transactions** - All money movements with timestamps and descriptions
- **Children** - Child profiles with names and birthdates  
- **Allowances** - Recurring allowance configurations
- **Goals** - Savings goals and progress tracking

### Data Location

Data files are stored in platform-appropriate directories:
- **macOS**: `~/Library/Application Support/allowance-tracker/`
- **Windows**: `%APPDATA%\allowance-tracker\`
- **Linux**: `~/.local/share/allowance-tracker/`

### Backup and Portability

Since data is stored in CSV format, it's easy to:
- Back up by copying the data directory
- View/edit data in any spreadsheet application
- Transfer between computers
- Import/export for analysis

## Contributing

### Development Workflow

1. **Make Changes** - Edit code in your preferred editor
2. **Test Locally** - Run `cargo run --bin allowance-tracker-egui` to test
3. **Check Build** - Ensure `cargo check --workspace` passes
4. **Run Tests** - Verify `cargo test --workspace` succeeds
5. **Format Code** - Use `cargo fmt` for consistent styling

### Adding Features

1. **Domain Logic** - Add business logic to `backend/domain/`
2. **Storage** - Update CSV repositories in `backend/storage/csv/` 
3. **Types** - Add shared types to `shared/src/lib.rs`
4. **UI** - Implement egui components in `egui-frontend/src/ui/`

## License

[Add your license information here]
