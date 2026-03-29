pub mod storage;
pub mod routes;

use axum::Router;

pub async fn create_app() -> anyhow::Result<Router> {
    let dynamo_client = create_dynamo_client().await?;
    let store = storage::DynamoStore::new(dynamo_client, "".to_string());
    let app = routes::build_router(store);
    Ok(app)
}

pub async fn create_dynamo_client() -> anyhow::Result<aws_sdk_dynamodb::Client> {
    let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    Ok(aws_sdk_dynamodb::Client::new(&config))
}

/// Create a DynamoDB client pointing at DynamoDB Local for testing.
pub async fn create_local_dynamo_client(port: u16) -> anyhow::Result<aws_sdk_dynamodb::Client> {
    let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .endpoint_url(format!("http://localhost:{}", port))
        .region(aws_config::Region::new("us-east-1"))
        .credentials_provider(aws_sdk_dynamodb::config::Credentials::new(
            "fakeAccessKeyId",
            "fakeSecretAccessKey",
            None,
            None,
            "test",
        ))
        .load()
        .await;
    Ok(aws_sdk_dynamodb::Client::new(&config))
}
