use shared::*;

/// Helper function to convert domain child to shared child
pub fn to_dto(child: crate::backend::domain::models::child::Child) -> Child {
    Child {
        id: child.id,
        name: child.name,
        birthdate: child.birthdate,
        created_at: child.created_at,
        updated_at: child.updated_at,
    }
}

/// Re-export transaction mapper from backend for convenience
pub use crate::backend::domain::mappers::transaction_to_dto;

/// Wrapper struct for backward compatibility with existing code using TransactionMapper::to_dto
pub struct TransactionMapper;

impl TransactionMapper {
    pub fn to_dto(domain_tx: crate::backend::domain::models::transaction::Transaction) -> Transaction {
        transaction_to_dto(domain_tx)
    }
} 