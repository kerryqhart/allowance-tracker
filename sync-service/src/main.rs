use sync_service::create_app;
use sync_service::storage::TableConfig;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    let config = TableConfig::from_prefix("");
    let app = create_app(config).await?;
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3030").await?;
    log::info!("sync-service listening on 0.0.0.0:3030");
    axum::serve(listener, app).await?;
    Ok(())
}
