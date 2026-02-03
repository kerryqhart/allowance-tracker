//! Domain model to DTO mappers
//!
//! This module provides conversion functions from domain models to shared DTOs.

use crate::backend::domain::models::transaction::{Transaction as DomainTransaction, TransactionType as DomainTransactionType};
use shared::{Transaction, TransactionType};

/// Maps domain Transaction to shared Transaction DTO
pub fn transaction_to_dto(domain_tx: DomainTransaction) -> Transaction {
    Transaction {
        id: domain_tx.id,
        child_id: domain_tx.child_id,
        date: domain_tx.date,
        description: domain_tx.description,
        amount: domain_tx.amount,
        balance: domain_tx.balance,
        transaction_type: match domain_tx.transaction_type {
            DomainTransactionType::Income => TransactionType::Income,
            DomainTransactionType::Expense => TransactionType::Expense,
            DomainTransactionType::FutureAllowance => TransactionType::FutureAllowance,
        },
    }
}
