use std::{path::Path, str::FromStr, time::Duration};

use anyhow::{Context, Result};
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
};

use crate::config::Settings;

pub async fn connect(path: &Path) -> Result<SqlitePool> {
    if let Some(dir) = path.parent()
        && !dir.as_os_str().is_empty()
    {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    let opts = SqliteConnectOptions::from_str(&format!("sqlite://{}", path.display()))?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(10));
    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(opts)
        .await
        .with_context(|| format!("opening database {}", path.display()))?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}

pub async fn load_settings(db: &SqlitePool) -> Result<Option<Settings>> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT value FROM settings WHERE key = 'settings'")
            .fetch_optional(db)
            .await?;
    Ok(row.and_then(|(v,)| serde_json::from_str(&v).ok()))
}

pub async fn save_settings(db: &SqlitePool, s: &Settings) -> Result<()> {
    sqlx::query(
        "INSERT INTO settings (key, value) VALUES ('settings', ?)
         ON CONFLICT (key) DO UPDATE SET value = excluded.value",
    )
    .bind(serde_json::to_string(s)?)
    .execute(db)
    .await?;
    Ok(())
}
