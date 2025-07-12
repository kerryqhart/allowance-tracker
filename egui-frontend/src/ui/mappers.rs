use shared::*;

/// Helper function to convert domain child to shared child
pub fn to_dto(child: crate::backend::domain::models::child::Child) -> Child {
    Child {
        id: child.id,
        name: child.name,
        birthdate: child.birthdate.to_string(),
        created_at: child.created_at.to_rfc3339(),
        updated_at: child.updated_at.to_rfc3339(),
    }
}

/// Simple transaction mapper for converting domain transactions to DTOs
pub struct TransactionMapper;

impl TransactionMapper {
    pub fn to_dto(domain_tx: crate::backend::domain::models::transaction::Transaction) -> Transaction {
        Transaction {
            id: domain_tx.id,
            child_id: domain_tx.child_id,
            date: domain_tx.date,
            description: domain_tx.description,
            amount: domain_tx.amount,
            balance: domain_tx.balance,
            transaction_type: match domain_tx.transaction_type {
                crate::backend::domain::models::transaction::TransactionType::Income => TransactionType::Income,
                crate::backend::domain::models::transaction::TransactionType::Expense => TransactionType::Expense,
                crate::backend::domain::models::transaction::TransactionType::FutureAllowance => TransactionType::FutureAllowance,
            },
        }
    }
} 