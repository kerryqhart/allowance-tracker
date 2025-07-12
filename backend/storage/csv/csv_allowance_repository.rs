//! CSV implementation of the `AllowanceStorage` trait.

use crate::backend::{
    domain::models::allowance::AllowanceConfig as DomainAllowanceConfig,
    storage::{
        csv::connection::CsvConnection,
        traits::AllowanceStorage,
    },
};
use anyhow::Result;
use async_trait::async_trait;
use std::{collections::HashMap, sync::Arc};

const ALLOWANCE_CONFIG_FILE: &str = "allowance_config.csv";

/// CSV-backed implementation of the `AllowanceStorage` trait.
#[derive(Clone)]
pub struct CsvAllowanceRepository {
    connection: Arc<CsvConnection>,
}

impl CsvAllowanceRepository {
    /// Creates a new `AllowanceRepository`.
    pub fn new(connection: Arc<CsvConnection>) -> Self {
        Self { connection }
    }

    /// Reads all allowance configs from the CSV file.
    async fn read_configs(&self) -> Result<HashMap<String, DomainAllowanceConfig>> {
        let conn = self.connection.clone();
        let path = conn.base_directory().join(ALLOWANCE_CONFIG_FILE);
        if !path.exists() {
            return Ok(HashMap::new());
        }

        let mut reader = csv::Reader::from_path(path)?;
        let mut configs = HashMap::new();
        for result in reader.deserialize() {
            let config: DomainAllowanceConfig = result?;
            configs.insert(config.child_id.clone(), config);
        }
        Ok(configs)
    }

    /// Writes all allowance configs to the CSV file.
    async fn write_configs(&self, configs: &HashMap<String, DomainAllowanceConfig>) -> Result<()> {
        let conn = self.connection.clone();
        let path = conn.base_directory().join(ALLOWANCE_CONFIG_FILE);
        let mut writer = csv::Writer::from_path(path)?;
        for config in configs.values() {
            writer.serialize(config)?;
        }
        writer.flush()?;
        Ok(())
    }
}

#[async_trait]
impl AllowanceStorage for CsvAllowanceRepository {
    async fn store_allowance_config(&self, config: &DomainAllowanceConfig) -> Result<()> {
        let mut configs = self.read_configs().await?;
        configs.insert(config.child_id.clone(), config.clone());
        self.write_configs(&configs).await
    }

    async fn get_allowance_config(&self, child_id: &str) -> Result<Option<DomainAllowanceConfig>> {
        let configs = self.read_configs().await?;
        Ok(configs.get(child_id).cloned())
    }

    async fn update_allowance_config(&self, config: &DomainAllowanceConfig) -> Result<()> {
        self.store_allowance_config(config).await
    }

    async fn delete_allowance_config(&self, child_id: &str) -> Result<bool> {
        let mut configs = self.read_configs().await?;
        let was_present = configs.remove(child_id).is_some();
        if was_present {
            self.write_configs(&configs).await?;
        }
        Ok(was_present)
    }

    async fn list_allowance_configs(&self) -> Result<Vec<DomainAllowanceConfig>> {
        let configs = self.read_configs().await?;
        Ok(configs.values().cloned().collect())
    }
}