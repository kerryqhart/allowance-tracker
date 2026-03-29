use aws_sdk_dynamodb::Client;
use sync_service::storage::table_definitions;
use uuid::Uuid;

pub struct DynamoTestContext {
    pub client: Client,
    pub table_prefix: String,
}

impl DynamoTestContext {
    pub async fn new(port: u16) -> Self {
        let client = sync_service::create_local_dynamo_client(port)
            .await
            .expect("Failed to create DynamoDB Local client. Is DynamoDB Local running?");

        let table_prefix = format!(
            "test_{}_",
            Uuid::new_v4()
                .to_string()
                .replace('-', "")[..8]
                .to_string()
        );

        table_definitions::create_all_tables(&client, &table_prefix)
            .await
            .expect("Failed to create test tables");

        Self { client, table_prefix }
    }

    pub fn table_name(&self, base: &str) -> String {
        format!("{}{}", self.table_prefix, base)
    }

    pub async fn cleanup(&self) {
        let _ = table_definitions::delete_all_tables(&self.client, &self.table_prefix).await;
    }
}

impl Drop for DynamoTestContext {
    fn drop(&mut self) {
        // Best-effort cleanup. Call cleanup() explicitly since Drop can't run async.
    }
}

pub const DYNAMO_LOCAL_PORT: u16 = 8000;

pub async fn is_dynamo_local_available(port: u16) -> bool {
    let client = match sync_service::create_local_dynamo_client(port).await {
        Ok(c) => c,
        Err(_) => return false,
    };
    client.list_tables().send().await.is_ok()
}
