use anyhow::Result;
use sqlx::{migrate::MigrateDatabase, Sqlite, SqlitePool};
use std::sync::Arc;

// The database URL for the production database
const DATABASE_URL: &str = "sqlite:keyvalue.db";

/// DbConnection manages database operations
#[derive(Clone)]
pub struct DbConnection {
    pool: Arc<SqlitePool>,
}

impl DbConnection {
    /// Create a new database connection
    pub async fn new(url: &str) -> Result<Self> {
        // Create database if it doesn't exist
        if !Sqlite::database_exists(url).await.unwrap_or(false) {
            Sqlite::create_database(url).await?
        }

        // Connect to the database
        let pool = SqlitePool::connect(url).await?;

        // Setup database schema
        Self::setup_schema(&pool).await?;

        Ok(Self { pool: Arc::new(pool) })
    }
    
    /// Initialize the standard database
    pub async fn init() -> Result<Self> {
        Self::new(DATABASE_URL).await
    }
    
    /// Initialize a test database with a unique name
    #[cfg(test)]
    pub async fn init_test() -> Result<Self> {
        // Generate a unique database name for tests
        let test_id = uuid::Uuid::new_v4().to_string();
        let db_url = format!("file:memdb_{}?mode=memory&cache=shared", test_id);
        
        Self::new(&db_url).await
    }

    /// Get a reference to the connection pool
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    // All temporary backward compatibility methods have been removed.
    // Services now use their respective repositories directly.

    /// Set up the required database schema
    async fn setup_schema(pool: &SqlitePool) -> Result<()> {
        // Create transactions table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS transactions (
                id TEXT PRIMARY KEY,
                child_id TEXT NOT NULL,
                date TEXT NOT NULL,
                description TEXT NOT NULL,
                amount REAL NOT NULL,
                balance REAL NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (child_id) REFERENCES children (id) ON DELETE CASCADE
            );
            "#,
        )
        .execute(pool)
        .await?;

        // Create index for ordering by created_at (for pagination)
        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_transactions_created_at 
            ON transactions(created_at DESC);
            "#,
        )
        .execute(pool)
        .await?;

        // Create index for child_id filtering
        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_transactions_child_id 
            ON transactions(child_id);
            "#,
        )
        .execute(pool)
        .await?;

        // Create children table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS children (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                birthdate TEXT NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
            );
            "#,
        )
        .execute(pool)
        .await?;

        // Create index for ordering children by name
        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_children_name 
            ON children(name);
            "#,
        )
        .execute(pool)
        .await?;

        // Create parental_control_attempts table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS parental_control_attempts (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                attempted_value TEXT NOT NULL,
                timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
                success BOOLEAN NOT NULL DEFAULT FALSE
            );
            "#,
        )
        .execute(pool)
        .await?;

        // Create index for ordering attempts by id
        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_parental_control_attempts_id 
            ON parental_control_attempts(id DESC);
            "#,
        )
        .execute(pool)
        .await?;

        // Create active_child table (single row to track the currently active child)
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS active_child (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                child_id TEXT NOT NULL,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (child_id) REFERENCES children (id) ON DELETE CASCADE
            );
            "#,
        )
        .execute(pool)
        .await?;

        // Create allowance_configs table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS allowance_configs (
                id TEXT PRIMARY KEY,
                child_id TEXT NOT NULL UNIQUE,
                amount REAL NOT NULL,
                day_of_week INTEGER NOT NULL CHECK (day_of_week >= 0 AND day_of_week <= 6),
                is_active BOOLEAN NOT NULL DEFAULT TRUE,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (child_id) REFERENCES children (id) ON DELETE CASCADE
            );
            "#,
        )
        .execute(pool)
        .await?;

        // Create index for child_id lookup
        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_allowance_configs_child_id 
            ON allowance_configs(child_id);
            "#,
        )
        .execute(pool)
        .await?;

        // Create index for active configs
        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_allowance_configs_active 
            ON allowance_configs(is_active);
            "#,
        )
        .execute(pool)
        .await?;

        Ok(())
    }
} 