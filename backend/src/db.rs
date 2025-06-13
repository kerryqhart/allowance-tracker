use anyhow::Result;
use shared::KeyValue;
use sqlx::{migrate::MigrateDatabase, Pool, Row, Sqlite, SqlitePool};

pub const DATABASE_URL: &str = "sqlite:keyvalue.db";

/// Initialize the database.
/// Creates the database if it doesn't exist and sets up the schema.
pub async fn init_db() -> Result<Pool<Sqlite>> {
    // Create database if it doesn't exist
    if !Sqlite::database_exists(DATABASE_URL).await.unwrap_or(false) {
        Sqlite::create_database(DATABASE_URL).await?;
    }

    // Connect to the database
    let pool = SqlitePool::connect(DATABASE_URL).await?;

    // Setup database schema
    setup_schema(&pool).await?;

    Ok(pool)
}

/// Set up the required database schema
async fn setup_schema(pool: &SqlitePool) -> Result<()> {
    // Create our database table if it doesn't exist
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS key_values (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// Store a key-value pair in the database.
/// This will overwrite any existing value for the same key.
pub async fn put_value(pool: &SqlitePool, kv: &KeyValue) -> Result<()> {
    sqlx::query(
        "INSERT OR REPLACE INTO key_values (key, value) VALUES (?, ?)",
    )
    .bind(&kv.key)
    .bind(&kv.value)
    .execute(pool)
    .await?;

    Ok(())
}

/// Retrieve a value by its key
pub async fn get_value(pool: &SqlitePool, key: &str) -> Result<Option<KeyValue>> {
    let result = sqlx::query("SELECT value FROM key_values WHERE key = ?")
        .bind(key)
        .map(|row: sqlx::sqlite::SqliteRow| {
            row.get::<String, _>("value")
        })
        .fetch_optional(pool)
        .await?;

    // Convert the result to a KeyValue if found
    Ok(result.map(|value| KeyValue {
        key: key.to_string(),
        value,
    }))
}

/// Delete a value by its key
pub async fn delete_value(pool: &SqlitePool, key: &str) -> Result<bool> {
    let result = sqlx::query("DELETE FROM key_values WHERE key = ?")
        .bind(key)
        .execute(pool)
        .await?;
    
    // Return whether any row was affected (i.e., key existed)
    Ok(result.rows_affected() > 0)
}

/// List all keys in the database
pub async fn list_keys(pool: &SqlitePool) -> Result<Vec<String>> {
    let rows = sqlx::query("SELECT key FROM key_values ORDER BY key")
        .map(|row: sqlx::sqlite::SqliteRow| {
            row.get::<String, _>("key")
        })
        .fetch_all(pool)
        .await?;
    
    Ok(rows)
}
