/// Configuration for DynamoDB table names.
/// Supports both env-var-based (Lambda) and prefix-based (local/test) naming.
pub struct TableConfig {
    pub children: String,
    pub transactions: String,
    pub goals: String,
    pub sync_events: String,
    pub sync_metadata: String,
}

impl TableConfig {
    /// For Lambda: reads table names from environment variables.
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            children: std::env::var("CHILDREN_TABLE")
                .map_err(|_| anyhow::anyhow!("CHILDREN_TABLE not set"))?,
            transactions: std::env::var("TRANSACTIONS_TABLE")
                .map_err(|_| anyhow::anyhow!("TRANSACTIONS_TABLE not set"))?,
            goals: std::env::var("GOALS_TABLE")
                .map_err(|_| anyhow::anyhow!("GOALS_TABLE not set"))?,
            sync_events: std::env::var("SYNC_EVENTS_TABLE")
                .map_err(|_| anyhow::anyhow!("SYNC_EVENTS_TABLE not set"))?,
            sync_metadata: std::env::var("SYNC_METADATA_TABLE")
                .map_err(|_| anyhow::anyhow!("SYNC_METADATA_TABLE not set"))?,
        })
    }

    /// For local dev and tests: constructs names from a prefix.
    pub fn from_prefix(prefix: &str) -> Self {
        Self {
            children: format!("{}children", prefix),
            transactions: format!("{}transactions", prefix),
            goals: format!("{}goals", prefix),
            sync_events: format!("{}sync_events", prefix),
            sync_metadata: format!("{}sync_metadata", prefix),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_prefix_empty() {
        let config = TableConfig::from_prefix("");
        assert_eq!(config.children, "children");
        assert_eq!(config.transactions, "transactions");
        assert_eq!(config.goals, "goals");
        assert_eq!(config.sync_events, "sync_events");
        assert_eq!(config.sync_metadata, "sync_metadata");
    }

    #[test]
    fn test_from_prefix_with_prefix() {
        let config = TableConfig::from_prefix("test_abc_");
        assert_eq!(config.children, "test_abc_children");
        assert_eq!(config.transactions, "test_abc_transactions");
        assert_eq!(config.goals, "test_abc_goals");
        assert_eq!(config.sync_events, "test_abc_sync_events");
        assert_eq!(config.sync_metadata, "test_abc_sync_metadata");
    }

    #[test]
    fn test_from_env_missing_var() {
        // Clear any existing vars to ensure failure
        std::env::remove_var("CHILDREN_TABLE");
        let result = TableConfig::from_env();
        assert!(result.is_err());
    }
}
