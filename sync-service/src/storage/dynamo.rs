use aws_sdk_dynamodb::Client;
use super::table_definitions;

pub struct DynamoStore {
    client: Client,
    table_prefix: String,
}

impl DynamoStore {
    pub fn new(client: Client, table_prefix: String) -> Self {
        Self { client, table_prefix }
    }

    pub fn table_name(&self, base: &str) -> String {
        format!("{}{}", self.table_prefix, base)
    }

    pub fn client(&self) -> &Client {
        &self.client
    }

    pub fn table_prefix(&self) -> &str {
        &self.table_prefix
    }

    pub async fn create_tables(&self) -> anyhow::Result<()> {
        table_definitions::create_all_tables(&self.client, &self.table_prefix).await
    }

    pub async fn delete_tables(&self) -> anyhow::Result<()> {
        table_definitions::delete_all_tables(&self.client, &self.table_prefix).await
    }
}
