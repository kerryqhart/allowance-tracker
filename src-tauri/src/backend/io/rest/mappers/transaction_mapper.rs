use crate::backend::domain::models::transaction::{
    Transaction as DomainTransaction, TransactionType as DomainTransactionType,
};
use shared::{Transaction as SharedTransaction, TransactionType as SharedTransactionType};

pub struct TransactionMapper;

impl TransactionMapper {
    pub fn to_domain(dto: SharedTransaction) -> DomainTransaction {
        DomainTransaction {
            id: dto.id,
            child_id: dto.child_id,
            date: dto.date,
            description: dto.description,
            amount: dto.amount,
            balance: dto.balance,
            transaction_type: Self::to_domain_type(dto.transaction_type),
        }
    }

    pub fn to_dto(domain: DomainTransaction) -> SharedTransaction {
        SharedTransaction {
            id: domain.id,
            child_id: domain.child_id,
            date: domain.date,
            description: domain.description,
            amount: domain.amount,
            balance: domain.balance,
            transaction_type: Self::to_dto_type(domain.transaction_type),
        }
    }

    fn to_domain_type(dto_type: SharedTransactionType) -> DomainTransactionType {
        match dto_type {
            SharedTransactionType::Income => DomainTransactionType::Income,
            SharedTransactionType::Expense => DomainTransactionType::Expense,
            SharedTransactionType::FutureAllowance => DomainTransactionType::FutureAllowance,
        }
    }

    fn to_dto_type(domain_type: DomainTransactionType) -> SharedTransactionType {
        match domain_type {
            DomainTransactionType::Income => SharedTransactionType::Income,
            DomainTransactionType::Expense => SharedTransactionType::Expense,
            DomainTransactionType::FutureAllowance => SharedTransactionType::FutureAllowance,
        }
    }
} 