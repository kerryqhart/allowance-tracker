use aws_sdk_dynamodb::Client;

pub struct DynamoStore {
    #[allow(dead_code)]
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
}
