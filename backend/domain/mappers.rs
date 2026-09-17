//! Domain model to DTO mappers
//!
//! This module provides conversion functions from domain models to shared DTOs.

use crate::backend::domain::models::transaction::{Transaction as DomainTransaction, TransactionType as DomainTransactionType};
use shared::{Transaction, TransactionType};

/// Maps domain Transaction to shared Transaction DTO
///
/// Boundary conversion: `shared::Transaction` is a separate, unrelated type
/// (out of scope for the Money migration) whose `amount`/`balance` fields
/// stay `f64` for the UI layer. `DomainTransaction::BALANCE_PENDING` maps
/// back to `f64::NAN` so downstream `.is_nan()` projection logic (e.g.
/// `CalendarService`) keeps working unchanged.
pub fn transaction_to_dto(domain_tx: DomainTransaction) -> Transaction {
    let balance = if domain_tx.balance == DomainTransaction::BALANCE_PENDING {
        f64::NAN
    } else {
        domain_tx.balance.cents() as f64 / 100.0
    };

    Transaction {
        id: domain_tx.id,
        child_id: domain_tx.child_id,
        date: domain_tx.date,
        description: domain_tx.description,
        amount: domain_tx.amount.cents() as f64 / 100.0,
        balance,
        transaction_type: match domain_tx.transaction_type {
            DomainTransactionType::Allowance => TransactionType::Allowance,
            DomainTransactionType::OneOffIncome => TransactionType::OneOffIncome,
            DomainTransactionType::Expense => TransactionType::Expense,
            DomainTransactionType::FutureAllowance => TransactionType::FutureAllowance,
        },
    }
}
