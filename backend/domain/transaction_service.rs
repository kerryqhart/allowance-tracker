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

    storage::{Connection, TransactionStorage},
};
use crate::backend::domain::commands::transactions::{CreateTransactionCommand, TransactionListQuery, TransactionListResult, DeleteTransactionsCommand, DeleteTransactionsResult, PaginationInfo as DomainPagination, CalendarTransactionsQuery, CalendarTransactionsResult};
use anyhow::{anyhow, Result};
use chrono::{Local, NaiveDate};
use log::{error, info};


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

    pub fn create_transaction_domain(
        &self,
        command: CreateTransactionCommand,
    ) -> Result<DomainTransaction> {
        // Validate description length here (moving logic from DTO layer)
        if command.description.is_empty() || command.description.len() > 256 {
            return Err(anyhow!("Description must be between 1 and 256 characters"));
        }

        let active_child = self.get_active_child()?;
        let now_millis = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64;
        let transaction_id = DomainTransaction::generate_id(command.amount, now_millis);

        let transaction_date = match command.date {
            Some(d) => d,
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
                command.amount,
            )?;

        let domain_transaction = DomainTransaction {
            id: transaction_id,
            child_id: active_child.id.clone(),
            date: transaction_date.clone(),
            description: command.description,
            amount: command.amount,
            balance: transaction_balance,
            transaction_type: if command.amount >= 0.0 {
                DomainTransactionType::Income
            } else {
                DomainTransactionType::Expense
            },
        };

        self.transaction_repository
            .store_transaction(&domain_transaction)?;

        if self
            .balance_service
            .requires_balance_recalculation(&active_child.id, &transaction_date)?
        {
            self.balance_service
                .recalculate_balances_from_date(&active_child.id, &transaction_date)?;
        }

        Ok(domain_transaction)
    }

    pub fn create_transaction(
        &self,
        cmd: CreateTransactionCommand,
    ) -> Result<DomainTransaction> {
        self.create_transaction_domain(cmd)
    }

    pub fn list_transactions_domain(
        &self,
        query: TransactionListQuery,
    ) -> Result<TransactionListResult> {
        info!("🎯 ALLOWANCE DEBUG: list_transactions_domain() called - this will trigger allowance check");
        self.check_and_issue_pending_allowances()?;
        let active_child = self.get_active_child()?;

        let limit = query.limit.unwrap_or(20);
        let query_limit = limit + 1;

        // Decide which repository method to use based on date filters
        let mut db_transactions = if query.start_date.is_some() || query.end_date.is_some() {
            // Fetch chronologically within range then reverse so newest first
            let mut txs = self
                .transaction_repository
                .list_transactions_chronological(&active_child.id, query.start_date.clone(), query.end_date.clone())?;
            txs.reverse();
            // Apply cursor & limit manually (after is applied after reversing because IDs unique)
            if let Some(after_id) = query.after.clone() {
                if let Some(idx) = txs.iter().position(|t| t.id == after_id) {
                    txs = txs.into_iter().skip(idx + 1).collect();
                }
            }
            if let Some(lim) = query.limit {
                txs.truncate(lim as usize + 1); // +1 to detect has_more later
            }
            txs
        } else {
            self
                .transaction_repository
                .list_transactions(&active_child.id, Some(query_limit), query.after)?
        };

        // TODO: reintegrate future allowances generation once domain models are finished
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

        Ok(TransactionListResult {
            transactions: db_transactions,
            pagination: DomainPagination { has_more, next_cursor },
        })
    }

    pub fn list_transactions(
        &self,
        query: TransactionListQuery,
    ) -> Result<TransactionListResult> {
        self.list_transactions_domain(query)
    }



    /// List transactions for calendar display, including future allowances
    /// This method orchestrates getting historical transactions and generating
    /// future allowances for the specified month
    pub fn list_transactions_for_calendar(
        &self,
        query: CalendarTransactionsQuery,
    ) -> Result<CalendarTransactionsResult> {
        info!("🗓️ Getting transactions for calendar: month={}, year={}", query.month, query.year);

        // Get the active child
        let active_child = self.get_active_child()?;

        // Calculate days in month for end date
        let days_in_month = match query.month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 => {
                // Check for leap year
                let year = query.year as i32;
                if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) {
                    29
                } else {
                    28
                }
            }
            _ => return Err(anyhow!("Invalid month: {}", query.month)),
        };

        // Get historical transactions up to the end of the requested month
        let end_date = format!("{:04}-{:02}-{:02}T23:59:59Z", query.year, query.month, days_in_month);
        info!("🗓️ Fetching historical transactions up to: {}", end_date);

        let transaction_query = TransactionListQuery {
            after: None,
            limit: Some(10000), // Get all transactions for calendar
            start_date: None,
            end_date: Some(end_date),
        };

        let transaction_result = self.list_transactions_domain(transaction_query)?;
        let mut all_transactions = transaction_result.transactions;
        
        info!("🗓️ Found {} historical transactions", all_transactions.len());

        // Generate future allowances for the requested month
        let start_date = match NaiveDate::from_ymd_opt(query.year as i32, query.month, 1) {
            Some(date) => date,
            None => return Err(anyhow!("Invalid date: {}/{}", query.month, query.year)),
        };
        
        let end_date = match NaiveDate::from_ymd_opt(query.year as i32, query.month, days_in_month) {
            Some(date) => date,
            None => return Err(anyhow!("Invalid end date: {}/{}/{}", query.month, days_in_month, query.year)),
        };

        info!("🗓️ Generating future allowances for child {} from {} to {}", 
              active_child.id, start_date, end_date);

        match self.allowance_service.generate_future_allowance_transactions(&active_child.id, start_date, end_date) {
            Ok(future_allowances) => {
                info!("🗓️ Generated {} future allowances", future_allowances.len());
                for (i, allowance) in future_allowances.iter().enumerate().take(3) {
                    info!("🗓️ Future allowance {}: id={}, date={}, amount={}", 
                         i + 1, allowance.id, allowance.date, allowance.amount);
                }
                if future_allowances.len() > 3 {
                    info!("🗓️ ... and {} more future allowances", future_allowances.len() - 3);
                }
                all_transactions.extend(future_allowances);
            },
            Err(e) => {
                error!("❌ Failed to generate future allowances: {}", e);
                // Continue without future allowances rather than failing
            }
        }

        info!("🗓️ Total transactions for calendar: {}", all_transactions.len());

        Ok(CalendarTransactionsResult {
            transactions: all_transactions,
        })
    }

    pub fn delete_transactions_domain(
        &self,
        cmd: DeleteTransactionsCommand,
    ) -> Result<DeleteTransactionsResult> {
        let active_child = self.get_active_child()?;
        let existing_ids = self
            .transaction_repository
            .check_transactions_exist(&active_child.id, &cmd.transaction_ids)?;
        let not_found_ids: Vec<String> = cmd
            .transaction_ids
            .iter()
            .filter(|id| !existing_ids.contains(id))
            .cloned()
            .collect();

        let deleted_count = if !existing_ids.is_empty() {
            self.transaction_repository
                .delete_transactions(&active_child.id, &existing_ids)?
        } else {
            0
        };

        if deleted_count > 0 {
            self.balance_service
                .recalculate_balances_from_date(&active_child.id, "1970-01-01T00:00:00Z")?;
        }

        let success_message = match deleted_count {
            0 => "No transactions were deleted".to_string(),
            1 => "1 transaction deleted successfully".to_string(),
            n => format!("{} transactions deleted successfully", n),
        };

        Ok(DeleteTransactionsResult {
            deleted_count: deleted_count as usize,
            success_message,
            not_found_ids,
        })
    }

    pub fn delete_transactions(
        &self,
        cmd: DeleteTransactionsCommand,
    ) -> Result<DeleteTransactionsResult> {
        self.delete_transactions_domain(cmd)
    }

    fn check_and_issue_pending_allowances(&self) -> Result<u32> {
        info!("🎯 ALLOWANCE DEBUG: check_and_issue_pending_allowances() called");
        if let Ok(active_child) = self.get_active_child() {
            info!("🎯 ALLOWANCE DEBUG: Found active child: {}", active_child.id);
            let current_date = Local::now().naive_local().date();
            let check_from_date = current_date - chrono::Duration::days(7);
            info!("🎯 ALLOWANCE DEBUG: Checking allowances from {} to {}", check_from_date, current_date);

            let pending_allowances = match self.allowance_service.get_pending_allowance_dates(&active_child.id, check_from_date, current_date) {
                Ok(dates) => dates,
                Err(e) => {
                    error!("🎯 ALLOWANCE DEBUG: Failed to get pending allowance dates: {}", e);
                    return Ok(0);
                }
            };
            info!("🎯 ALLOWANCE DEBUG: Found {} pending allowances", pending_allowances.len());
            
            let mut issued_count = 0;
            for (allowance_date, amount) in pending_allowances {
                info!("🎯 ALLOWANCE DEBUG: About to create allowance for {} (${:.2})", allowance_date, amount);
                match self
                    .create_allowance_transaction(&active_child.id, allowance_date, amount)
                {
                    Ok(transaction) => {
                        info!(
                            "🎯 ALLOWANCE DEBUG: Successfully issued allowance: {} for ${:.2} on {}",
                            transaction.id, amount, allowance_date
                        );
                        issued_count += 1;
                    }
                    Err(e) => {
                        error!(
                            "🎯 ALLOWANCE DEBUG: Failed to issue allowance for {} on {}: {}",
                            active_child.id, allowance_date, e
                        );
                    }
                }
            }
            info!("🎯 ALLOWANCE DEBUG: Total allowances issued: {}", issued_count);
            return Ok(issued_count);
        } else {
            info!("🎯 ALLOWANCE DEBUG: No active child found");
        }
        Ok(0)
    }

    fn create_allowance_transaction(
        &self,
        child_id: &str,
        date: NaiveDate,
        amount: f64,
    ) -> Result<DomainTransaction> {
        info!("🎯 ALLOWANCE DEBUG: create_allowance_transaction() called for child {}, date {}, amount ${:.2}", child_id, date, amount);
        let now_millis = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64;
        let transaction_id = DomainTransaction::generate_id(amount, now_millis);
        info!("🎯 ALLOWANCE DEBUG: Generated transaction ID: {}", transaction_id);
        
        let allowance_datetime = date.and_hms_opt(12, 0, 0).unwrap();
        let utc_datetime = chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(
            allowance_datetime,
            chrono::Utc,
        );
        let eastern_offset = chrono::FixedOffset::west_opt(5 * 3600).unwrap();
        let eastern_datetime = utc_datetime.with_timezone(&eastern_offset);
        let transaction_date = eastern_datetime.to_rfc3339();
        info!("🎯 ALLOWANCE DEBUG: Transaction date formatted as: {}", transaction_date);

        let transaction_balance = self
            .balance_service
            .calculate_balance_for_new_transaction(child_id, &transaction_date, amount)?;
        info!("🎯 ALLOWANCE DEBUG: Calculated balance: {}", transaction_balance);

        let domain_transaction = DomainTransaction {
            id: transaction_id.clone(),
            child_id: child_id.to_string(),
            date: transaction_date.clone(),
            description: "Weekly allowance".to_string(),
            amount,
            balance: transaction_balance,
            transaction_type: DomainTransactionType::Income,
        };

        info!("🎯 ALLOWANCE DEBUG: About to store transaction: {}", transaction_id);
        self.transaction_repository
            .store_transaction(&domain_transaction)?;
        info!("🎯 ALLOWANCE DEBUG: Transaction stored successfully: {}", transaction_id);

        if self
            .balance_service
            .requires_balance_recalculation(child_id, &transaction_date)?
        {
            info!("🎯 ALLOWANCE DEBUG: Recalculating balances from date: {}", transaction_date);
            self.balance_service
                .recalculate_balances_from_date(child_id, &transaction_date)?;
        }

        info!("🎯 ALLOWANCE DEBUG: create_allowance_transaction() completed for {}", transaction_id);
        Ok(domain_transaction)
    }

    fn get_active_child(&self) -> Result<DomainChild> {
        self.child_service
            .get_active_child()?
            .active_child.child
            .ok_or_else(|| anyhow!("No active child found."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{
        domain::{
            models::child::Child as DomainChild,
            commands::child::{CreateChildCommand, SetActiveChildCommand},
        },
        storage::{
            csv::{connection::CsvConnection},
        },
    };
    // Tests now use domain models instead of shared types

    async fn create_test_service() -> (TransactionService<CsvConnection>, Arc<CsvConnection>) {
        let connection = Arc::new(CsvConnection::new_for_testing().await.unwrap());
        let child_service = ChildService::new(connection.clone());
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
        child_service: &ChildService,
        child_name: &str,
    ) -> Result<DomainChild> {
        let create_child_command = CreateChildCommand {
            name: child_name.to_string(),
            birthdate: "2015-01-01".to_string(),
        };
        let result = child_service.create_child(create_child_command).await?;
        Ok(result.child)
    }

    #[tokio::test]
    async fn test_create_transaction_basic() {
        let (service, _conn) = create_test_service().await;
        let test_child = create_test_child(&service.child_service, "test_child").await.unwrap();
        let set_active_command = SetActiveChildCommand {
            child_id: test_child.id.clone(),
        };
        service
            .child_service
            .set_active_child(set_active_command)
            .await
            .unwrap();

        let cmd = CreateTransactionCommand {
            amount: 10.0,
            description: "Test transaction".to_string(),
            date: None,
        };
        let transaction = service.create_transaction(cmd).await.unwrap();
        assert_eq!(transaction.amount, 10.0);
        assert_eq!(transaction.description, "Test transaction");
        assert_eq!(transaction.balance, 10.0);
        assert_eq!(transaction.transaction_type, DomainTransactionType::Income);
    }
}