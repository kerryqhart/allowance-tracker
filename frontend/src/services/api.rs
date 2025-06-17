use gloo::net::http::Request;
use shared::{
    AddMoneyRequest, AddMoneyResponse, SpendMoneyRequest, SpendMoneyResponse,
    CalendarMonth, TransactionTableResponse, DeleteTransactionsRequest, DeleteTransactionsResponse,
    ParentalControlRequest, ParentalControlResponse
};

/// API client for communicating with the backend server
#[derive(Clone)]
pub struct ApiClient {
    base_url: String,
}

impl ApiClient {
    /// Create a new API client with the default base URL
    pub fn new() -> Self {
        Self {
            base_url: "http://localhost:3000".to_string(),
        }
    }

    /// Create a new API client with a custom base URL
    pub fn with_base_url(base_url: String) -> Self {
        Self { base_url }
    }

    /// Test connection to the backend
    pub async fn test_connection(&self) -> Result<(), String> {
        match Request::get(&format!("{}/api/transactions/table?limit=1", self.base_url))
            .send()
            .await
        {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("Connection failed: {}", e)),
        }
    }

    /// Get formatted transactions from the backend
    pub async fn get_transactions(&self, limit: Option<u32>) -> Result<TransactionTableResponse, String> {
        let limit_param = limit.map(|l| format!("?limit={}", l)).unwrap_or_default();
        let url = format!("{}/api/transactions/table{}", self.base_url, limit_param);
        
        match Request::get(&url).send().await {
            Ok(response) => {
                match response.json::<TransactionTableResponse>().await {
                    Ok(data) => Ok(data),
                    Err(e) => Err(format!("Failed to parse transactions: {}", e)),
                }
            }
            Err(e) => Err(format!("Failed to fetch transactions: {}", e)),
        }
    }

    /// Get calendar data for a specific month/year
    pub async fn get_calendar_month(&self, month: u32, year: u32) -> Result<CalendarMonth, String> {
        let url = format!("{}/api/calendar/month?month={}&year={}", self.base_url, month, year);
        
        match Request::get(&url).send().await {
            Ok(response) => {
                match response.json::<CalendarMonth>().await {
                    Ok(data) => Ok(data),
                    Err(e) => Err(format!("Failed to parse calendar data: {}", e)),
                }
            }
            Err(e) => Err(format!("Failed to fetch calendar data: {}", e)),
        }
    }

    /// Add money transaction
    pub async fn add_money(&self, request: AddMoneyRequest) -> Result<AddMoneyResponse, String> {
        let url = format!("{}/api/money/add", self.base_url);
        
        match Request::post(&url)
            .json(&request)
            .map_err(|e| format!("Failed to serialize request: {}", e))?
            .send()
            .await
        {
            Ok(response) => {
                if response.ok() {
                    match response.json::<AddMoneyResponse>().await {
                        Ok(data) => Ok(data),
                        Err(e) => Err(format!("Failed to parse response: {}", e)),
                    }
                } else {
                    let error_text = response.text().await
                        .unwrap_or_else(|_| "Unknown error".to_string());
                    Err(error_text)
                }
            }
            Err(e) => Err(format!("Network error: {}", e)),
        }
    }

    /// Spend money transaction
    pub async fn spend_money(&self, request: SpendMoneyRequest) -> Result<SpendMoneyResponse, String> {
        let url = format!("{}/api/money/spend", self.base_url);
        
        match Request::post(&url)
            .json(&request)
            .map_err(|e| format!("Failed to serialize request: {}", e))?
            .send()
            .await
        {
            Ok(response) => {
                if response.ok() {
                    match response.json::<SpendMoneyResponse>().await {
                        Ok(data) => Ok(data),
                        Err(e) => Err(format!("Failed to parse response: {}", e)),
                    }
                } else {
                    let error_text = response.text().await
                        .unwrap_or_else(|_| "Unknown error".to_string());
                    Err(error_text)
                }
            }
            Err(e) => Err(format!("Network error: {}", e)),
        }
    }

    /// Delete multiple transactions
    pub async fn delete_transactions(&self, request: DeleteTransactionsRequest) -> Result<DeleteTransactionsResponse, String> {
        let url = format!("{}/api/transactions", self.base_url);
        
        match Request::delete(&url)
            .json(&request)
            .map_err(|e| format!("Failed to serialize request: {}", e))?
            .send()
            .await
        {
            Ok(response) => {
                if response.ok() {
                    match response.json::<DeleteTransactionsResponse>().await {
                        Ok(data) => Ok(data),
                        Err(e) => Err(format!("Failed to parse response: {}", e)),
                    }
                } else {
                    let error_text = response.text().await
                        .unwrap_or_else(|_| "Unknown error".to_string());
                    Err(error_text)
                }
            }
            Err(e) => Err(format!("Network error: {}", e)),
        }
    }

    /// Validate parental control answer
    pub async fn validate_parental_control(&self, answer: &str) -> Result<ParentalControlResponse, String> {
        let url = format!("{}/api/parental-control/validate", self.base_url);
        
        let request_body = ParentalControlRequest {
            answer: answer.to_string(),
        };

        match Request::post(&url)
            .json(&request_body)
            .map_err(|e| format!("Failed to serialize request: {}", e))?
            .send()
            .await
        {
            Ok(response) => {
                if response.ok() {
                    match response.json::<ParentalControlResponse>().await {
                        Ok(data) => Ok(data),
                        Err(e) => Err(format!("Failed to parse response: {}", e)),
                    }
                } else {
                    let status = response.status();
                    let error_text = response.text().await
                        .unwrap_or_else(|_| "Unknown error".to_string());
                    Err(format!("Server error {}: {}", status, error_text))
                }
            }
            Err(e) => Err(format!("Network error: {}", e)),
        }
    }
}

impl Default for ApiClient {
    fn default() -> Self {
        Self::new()
    }
} 