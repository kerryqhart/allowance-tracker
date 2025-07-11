use anyhow::Result;
use log::{info, warn};
use std::sync::Arc;

use crate::backend::storage::csv::{CsvConnection, ParentalControlRepository};
use crate::backend::storage::traits::ParentalControlStorage;
use crate::backend::domain::commands::parental_control::{ValidateParentalControlCommand, ValidateParentalControlResult};
use shared::{ParentalControlRequest, ParentalControlResponse};

/// Service for handling parental control validation
#[derive(Clone)]
pub struct ParentalControlService {
    parental_control_repository: ParentalControlRepository,
    correct_answer: String,
}

impl ParentalControlService {
    /// Create a new ParentalControlService with the default correct answer
    pub fn new(csv_conn: Arc<CsvConnection>) -> Self {
        let parental_control_repository = ParentalControlRepository::new((*csv_conn).clone());
        Self {
            parental_control_repository,
            correct_answer: "ice cold".to_string(),
        }
    }

    /// Create a new ParentalControlService with a custom correct answer (for testing)
    pub fn with_answer(csv_conn: Arc<CsvConnection>, answer: String) -> Self {
        let parental_control_repository = ParentalControlRepository::new((*csv_conn).clone());
        Self {
            parental_control_repository,
            correct_answer: answer.to_lowercase().trim().to_string(),
        }
    }

    /// Validate a parental control answer
    pub async fn validate_answer(&self, command: ValidateParentalControlCommand) -> Result<ValidateParentalControlResult> {
        let attempted_answer = command.answer.trim();
        info!("Validating parental control answer (length: {})", attempted_answer.len());

        // Perform case-insensitive comparison
        let is_correct = attempted_answer.to_lowercase() == self.correct_answer;

        // Record the attempt in the database (using "global" as child_id for parental control)
        match self.parental_control_repository.record_parental_control_attempt("global", attempted_answer, is_correct).await {
            Ok(attempt_id) => {
                info!("Recorded parental control attempt with ID: {}", attempt_id);
            }
            Err(e) => {
                warn!("Failed to record parental control attempt: {}", e);
                // Continue with validation even if recording fails
            }
        }

        // Generate response
        let result = if is_correct {
            info!("Parental control validation successful");
            ValidateParentalControlResult {
                success: true,
                message: "Access granted! Welcome to parental settings.".to_string(),
            }
        } else {
            info!("Parental control validation failed for answer: '{}'", attempted_answer);
            ValidateParentalControlResult {
                success: false,
                message: "Incorrect answer. Access denied.".to_string(),
            }
        };

        Ok(result)
    }

    /// Get the correct answer (for testing purposes)
    #[cfg(test)]
    pub fn get_correct_answer(&self) -> &str {
        &self.correct_answer
    }

    /// Get recent validation attempts for monitoring
    pub async fn get_recent_attempts(&self, limit: Option<u32>) -> Result<Vec<shared::ParentalControlAttempt>> {
        info!("Retrieving recent parental control attempts (limit: {:?})", limit);
        
        let attempts = self.parental_control_repository.get_parental_control_attempts("global", limit).await?;
        
        info!("Retrieved {} parental control attempts", attempts.len());
        Ok(attempts)
    }

    /// Get validation statistics
    pub async fn get_validation_stats(&self) -> Result<ParentalControlStats> {
        info!("Calculating parental control validation statistics");
        
        let all_attempts = self.parental_control_repository.get_parental_control_attempts("global", None).await?;
        
        let total_attempts = all_attempts.len();
        let successful_attempts = all_attempts.iter().filter(|a| a.success).count();
        let failed_attempts = total_attempts - successful_attempts;
        
        let success_rate = if total_attempts > 0 {
            (successful_attempts as f64 / total_attempts as f64) * 100.0
        } else {
            0.0
        };

        let stats = ParentalControlStats {
            total_attempts,
            successful_attempts,
            failed_attempts,
            success_rate,
        };

        info!("Parental control stats: {:?}", stats);
        Ok(stats)
    }
}

/// Statistics for parental control validation attempts
#[derive(Debug, Clone)]
pub struct ParentalControlStats {
    pub total_attempts: usize,
    pub successful_attempts: usize,
    pub failed_attempts: usize,
    pub success_rate: f64, // Percentage
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::storage::DbConnection;

    async fn setup_test() -> ParentalControlService {
        let db = Arc::new(DbConnection::init_test().await.expect("Failed to create test database"));
        ParentalControlService::new(db)
    }

    #[tokio::test]
    async fn test_correct_answer_validation() {
        let service = setup_test().await;
        
        let command = ValidateParentalControlCommand {
            answer: "ice cold".to_string(),
        };
        
        let response = service.validate_answer(command).await.unwrap();
        assert!(response.success);
        assert!(response.message.contains("Access granted"));
    }

    #[tokio::test]
    async fn test_case_insensitive_validation() {
        let service = setup_test().await;
        
        let test_cases = vec![
            "ICE COLD",
            "Ice Cold", 
            "ice cold",
            "ICE cold",
            "iCe CoLd",
        ];
        
        for answer in test_cases {
            let command = ValidateParentalControlCommand {
                answer: answer.to_string(),
            };
            
            let response = service.validate_answer(command).await.unwrap();
            assert!(response.success, "Answer '{}' should be accepted", answer);
        }
    }

    #[tokio::test]
    async fn test_whitespace_handling() {
        let service = setup_test().await;
        
        let test_cases = vec![
            "  ice cold  ",
            "\tice cold\t",
            " ice cold",
            "ice cold ",
        ];
        
        for answer in test_cases {
            let command = ValidateParentalControlCommand {
                answer: answer.to_string(),
            };
            
            let response = service.validate_answer(command).await.unwrap();
            assert!(response.success, "Answer '{}' should be accepted", answer);
        }
    }

    #[tokio::test]
    async fn test_incorrect_answer_validation() {
        let service = setup_test().await;
        
        let test_cases = vec![
            "wrong answer",
            "cold ice",
            "freeze",
            "cool",
            "",
            "ice",
            "cold",
        ];
        
        for answer in test_cases {
            let command = ValidateParentalControlCommand {
                answer: answer.to_string(),
            };
            
            let response = service.validate_answer(command).await.unwrap();
            assert!(!response.success, "Answer '{}' should be rejected", answer);
            assert!(response.message.contains("Incorrect answer"));
        }
    }

    #[tokio::test]
    async fn test_attempts_are_recorded() {
        let service = setup_test().await;
        
        // Initially no attempts
        let initial_attempts = service.get_recent_attempts(None).await.unwrap();
        assert_eq!(initial_attempts.len(), 0);
        
        // Make a correct attempt
        let correct_command = ValidateParentalControlCommand {
            answer: "ice cold".to_string(),
        };
        service.validate_answer(correct_command).await.unwrap();
        
        // Make an incorrect attempt
        let incorrect_command = ValidateParentalControlCommand {
            answer: "wrong".to_string(),
        };
        service.validate_answer(incorrect_command).await.unwrap();
        
        // Check attempts were recorded
        let attempts = service.get_recent_attempts(None).await.unwrap();
        assert_eq!(attempts.len(), 2);
        
        // Check the most recent attempt (should be the incorrect one)
        assert_eq!(attempts[0].attempted_value, "wrong");
        assert!(!attempts[0].success);
        
        // Check the first attempt (should be the correct one)
        assert_eq!(attempts[1].attempted_value, "ice cold");
        assert!(attempts[1].success);
    }

    #[tokio::test]
    async fn test_validation_stats() {
        let service = setup_test().await;
        
        // Initially no stats
        let initial_stats = service.get_validation_stats().await.unwrap();
        assert_eq!(initial_stats.total_attempts, 0);
        assert_eq!(initial_stats.success_rate, 0.0);
        
        // Make some attempts
        let requests = vec![
            ("ice cold", true),   // correct
            ("wrong", false),     // incorrect
            ("ICE COLD", true),   // correct
            ("bad", false),       // incorrect
            ("ice cold", true),   // correct
        ];
        
        for (answer, _expected) in requests {
            let command = ValidateParentalControlCommand {
                answer: answer.to_string(),
            };
            service.validate_answer(command).await.unwrap();
        }
        
        // Check stats
        let stats = service.get_validation_stats().await.unwrap();
        assert_eq!(stats.total_attempts, 5);
        assert_eq!(stats.successful_attempts, 3);
        assert_eq!(stats.failed_attempts, 2);
        assert_eq!(stats.success_rate, 60.0); // 3/5 * 100
    }

    #[tokio::test]
    async fn test_custom_answer() {
        let db = Arc::new(DbConnection::init_test().await.expect("Failed to create test database"));
        let service = ParentalControlService::with_answer(db, "custom answer".to_string());
        
        // Test correct custom answer
        let correct_command = ValidateParentalControlCommand {
            answer: "custom answer".to_string(),
        };
        let response = service.validate_answer(correct_command).await.unwrap();
        assert!(response.success);
        
        // Test default answer should fail
        let default_command = ValidateParentalControlCommand {
            answer: "ice cold".to_string(),
        };
        let response = service.validate_answer(default_command).await.unwrap();
        assert!(!response.success);
    }
} 