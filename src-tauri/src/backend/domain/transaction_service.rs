//! Transaction service domain logic for the allowance tracker.
use crate::backend::{
    domain::{
        allowance_service::AllowanceService,
        balance_service::BalanceService,
        child_service::ChildService,
        models::{
            child::Child as DomainChild,
            transaction::{Transaction as DomainTransaction, TransactionType as DomainTransactionType},
        },
    },
    io::rest::mappers::transaction_mapper::TransactionMapper,
    storage::{Connection, TransactionStorage},
};
use anyhow::{anyhow, Result};
use chrono::{Local, NaiveDate};
use log::{error, info};
use shared::{
    CreateTransactionRequest, DeleteTransactionsRequest, DeleteTransactionsResponse, PaginationInfo,
    Transaction as SharedTransaction, TransactionListRequest, TransactionListResponse,
};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use time::format_description::well_known::Rfc3339;

#[derive(Clone)]
pub struct TransactionService<C: Connection> {
    transaction_repository: C::TransactionRepository,
    child_service: ChildService,
    allowance_service: AllowanceService,
    balance_service: BalanceService<C>,
}

impl<C: Connection> TransactionService<C> {
    pub fn new(
        connection: Arc<C>,
        child_service: ChildService,
        allowance_service: AllowanceService,
        balance_service: BalanceService<C>,
    ) -> Self {
        let transaction_repository = connection.create_transaction_repository();
        Self {
            transaction_repository,
            child_service,
            allowance_service,
            balance_service,
        }
    }

    pub async fn create_transaction(
        &self,
        request: CreateTransactionRequest,
    ) -> Result<SharedTransaction> {
        if request.description.is_empty() || request.description.len() > 256 {
            return Err(anyhow!("Description must be between 1 and 256 characters"));
        }

        let active_child = self.get_active_child().await?;
        let now_millis = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64;
        let transaction_id = DomainTransaction::generate_id(request.amount, now_millis);

        let transaction_date = match request.date {
            Some(date) => date,
            None => {
                let now = time::OffsetDateTime::from(SystemTime::now());
                let eastern_offset = time::UtcOffset::from_hms(-5, 0, 0)?;
                now.to_offset(eastern_offset).format(&Rfc3339)?
            }
        };

        let transaction_balance = self
            .balance_service
            .calculate_balance_for_new_transaction(
                &active_child.id,
                &transaction_date,
                request.amount,
            )
            .await?;

        let domain_transaction = DomainTransaction {
            id: transaction_id,
            child_id: active_child.id.clone(),
            date: transaction_date.clone(),
            description: request.description,
            amount: request.amount,
            balance: transaction_balance,
            transaction_type: if request.amount >= 0.0 {
                DomainTransactionType::Income
            } else {
                DomainTransactionType::Expense
            },
        };

        self.transaction_repository
            .store_transaction(&domain_transaction)
            .await?;

        if self
            .balance_service
            .requires_balance_recalculation(&active_child.id, &transaction_date)
            .await?
        {
            self.balance_service
                .recalculate_balances_from_date(&active_child.id, &transaction_date)
                .await?;
        }

        Ok(TransactionMapper::to_dto(domain_transaction))
    }

    pub async fn list_transactions(
        &self,
        request: TransactionListRequest,
    ) -> Result<TransactionListResponse> {
        self.check_and_issue_pending_allowances().await?;
        let active_child = self.get_active_child().await?;

        let limit = request.limit.unwrap_or(20);
        let query_limit = limit + 1;

        let mut db_transactions = self
            .transaction_repository
            .list_transactions(&active_child.id, Some(query_limit), request.after)
            .await?;

        // FIXME: This is a temporary fix to get the code to compile.
        // We need to figure out how to handle future allowances correctly.
        // let future_allowances = self
        //     .allowance_service
        //     .generate_future_allowance_transactions(
        //         &active_child.id,
        //         request
        //             .start_date
        //             .and_then(|d| NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok())
        //             .unwrap_or_else(|| Local::now().naive_local().date()),
        //         request
        //             .end_date
        //             .and_then(|d| NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok())
        //             .unwrap_or_else(|| (Local::now() + chrono::Duration::days(30)).naive_local().date()),
        //     )
        //     .await?;

        // db_transactions.extend(future_allowances);
        db_transactions.sort_by(|a, b| b.date.cmp(&a.date));

        let has_more = db_transactions.len() > limit as usize;
        if has_more {
            db_transactions.truncate(limit as usize);
        }

        let next_cursor = if has_more {
            db_transactions.last().map(|t| t.id.clone())
        } else {
            None
        };

        let dto_transactions = db_transactions
            .into_iter()
            .map(TransactionMapper::to_dto)
            .collect();

        Ok(TransactionListResponse {
            transactions: dto_transactions,
            pagination: PaginationInfo {
                has_more,
                next_cursor,
            },
        })
    }

    pub async fn delete_transactions(
        &self,
        request: DeleteTransactionsRequest,
    ) -> Result<DeleteTransactionsResponse> {
        let active_child = self.get_active_child().await?;
        let existing_ids = self
            .transaction_repository
            .check_transactions_exist(&active_child.id, &request.transaction_ids)
            .await?;
        let not_found_ids: Vec<String> = request
            .transaction_ids
            .iter()
            .filter(|id| !existing_ids.contains(id))
            .cloned()
            .collect();

        let deleted_count = if !existing_ids.is_empty() {
            self.transaction_repository
                .delete_transactions(&active_child.id, &existing_ids)
                .await?
        } else {
            0
        };

        if deleted_count > 0 {
            self.balance_service
                .recalculate_balances_from_date(&active_child.id, "1970-01-01T00:00:00Z")
                .await?;
        }

        let success_message = match deleted_count {
            0 => "No transactions were deleted".to_string(),
            1 => "1 transaction deleted successfully".to_string(),
            n => format!("{} transactions deleted successfully", n),
        };

        Ok(DeleteTransactionsResponse {
            deleted_count: deleted_count as usize,
            success_message,
            not_found_ids,
        })
    }

    async fn check_and_issue_pending_allowances(&self) -> Result<u32> {
        if let Ok(active_child) = self.get_active_child().await {
            let current_date = Local::now().naive_local().date();
            let check_from_date = current_date - chrono::Duration::days(7);

            let pending_allowances = self
                .allowance_service
                .get_pending_allowance_dates(&active_child.id, check_from_date, current_date)
                .await?;
            let mut issued_count = 0;
            for (allowance_date, amount) in pending_allowances {
                match self
                    .create_allowance_transaction(&active_child.id, allowance_date, amount)
                    .await
                {
                    Ok(transaction) => {
                        info!(
                            "Issued allowance: {} for ${:.2} on {}",
                            transaction.id, amount, allowance_date
                        );
                        issued_count += 1;
                    }
                    Err(e) => {
                        error!(
                            "Failed to issue allowance for {} on {}: {}",
                            active_child.id, allowance_date, e
                        );
                    }
                }
            }
            return Ok(issued_count);
        }
        Ok(0)
    }

    async fn create_allowance_transaction(
        &self,
        child_id: &str,
        date: NaiveDate,
        amount: f64,
    ) -> Result<SharedTransaction> {
        let now_millis = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64;
        let transaction_id = DomainTransaction::generate_id(amount, now_millis);
        let allowance_datetime = date.and_hms_opt(12, 0, 0).unwrap();
        let utc_datetime = chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(
            allowance_datetime,
            chrono::Utc,
        );
        let eastern_offset = chrono::FixedOffset::west_opt(5 * 3600).unwrap();
        let eastern_datetime = utc_datetime.with_timezone(&eastern_offset);
        let transaction_date = eastern_datetime.to_rfc3339();

        let transaction_balance = self
            .balance_service
            .calculate_balance_for_new_transaction(child_id, &transaction_date, amount)
            .await?;

        let domain_transaction = DomainTransaction {
            id: transaction_id,
            child_id: child_id.to_string(),
            date: transaction_date.clone(),
            description: "Weekly allowance".to_string(),
            amount,
            balance: transaction_balance,
            transaction_type: DomainTransactionType::Income,
        };

        self.transaction_repository
            .store_transaction(&domain_transaction)
            .await?;

        if self
            .balance_service
            .requires_balance_recalculation(child_id, &transaction_date)
            .await?
        {
            self.balance_service
                .recalculate_balances_from_date(child_id, &transaction_date)
                .await?;
        }

        Ok(TransactionMapper::to_dto(domain_transaction))
    }

    async fn get_active_child(&self) -> Result<DomainChild> {
        self.child_service
            .get_active_child()
            .await?
            .child
            .ok_or_else(|| anyhow!("No active child found."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{
        domain::models::child::Child as DomainChild,
        storage::{
            csv::{child_repository::ChildRepository, connection::CsvConnection},
            ChildStorage,
        },
    };
    use shared::{Child as SharedChild, TransactionType as SharedTransactionType};

    async fn create_test_service() -> (TransactionService<CsvConnection>, Arc<CsvConnection>) {
        let connection = Arc::new(CsvConnection::new_for_test().await.unwrap());
        let child_repo = ChildRepository::new(connection.clone());
        let child_service = ChildService::new(child_repo);
        let allowance_service = AllowanceService::new(connection.clone());
        let balance_service = BalanceService::new(connection.clone());
        let transaction_service = TransactionService::new(
            connection.clone(),
            child_service,
            allowance_service,
            balance_service,
        );
        (transaction_service, connection)
    }

    async fn create_test_child(
        child_repo: &ChildRepository,
        child_name: &str,
    ) -> Result<SharedChild> {
        let child = SharedChild {
            id: child_name.to_string(),
            name: child_name.to_string(),
        };
        child_repo.add_child(&child).await?;
        Ok(child)
    }

    #[tokio::test]
    async fn test_create_transaction_basic() {
        let (service, conn) = create_test_service().await;
        let child_repo = conn.create_child_repository();
        let _test_child = create_test_child(&child_repo, "test_child").await.unwrap();
        service
            .child_service
            .set_active_child("test_child".to_string())
            .await
            .unwrap();

        let request = CreateTransactionRequest {
            amount: 10.0,
            description: "Test transaction".to_string(),
            date: None,
        };
        let transaction = service.create_transaction(request).await.unwrap();
        assert_eq!(transaction.amount, 10.0);
        assert_eq!(transaction.description, "Test transaction");
        assert_eq!(transaction.balance, 10.0);
        assert_eq!(transaction.transaction_type, SharedTransactionType::Income);
    }
}