use sync_service::create_app;
use sync_service::storage::TableConfig;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let is_lambda = std::env::var("AWS_LAMBDA_RUNTIME_API").is_ok();

    let config = if is_lambda {
        TableConfig::from_env()?
    } else {
        TableConfig::from_prefix("")
    };

    let app = create_app(config).await?;

    if is_lambda {
        lambda_http::run(app).await.map_err(|e| anyhow::anyhow!("Lambda runtime error: {}", e))?;
    } else {
        env_logger::init();
        let listener = tokio::net::TcpListener::bind("0.0.0.0:3030").await?;
        log::info!("sync-service listening on 0.0.0.0:3030");
        axum::serve(listener, app).await?;
    }

    Ok(())
}
