use std::{io::Read, net::SocketAddr, path::PathBuf};

use anyhow::{Context, Result, bail};
use axum::{Router, extract::DefaultBodyLimit, routing::post};
use clap::{Parser, Subcommand};
use tower_http::{
    compression::CompressionLayer, decompression::RequestDecompressionLayer, trace::TraceLayer,
};
use tracing_subscriber::EnvFilter;

mod api;
mod auth;
mod collector;
mod compat;
mod config;
mod db;
mod mail_html;
mod monit;
mod notify;
mod oidc;
mod pull;
mod push;
mod state;
mod tasks;
mod views;
mod web;

use config::{Config, Settings};

#[derive(Parser)]
#[command(
    version,
    about = "Monarch – a modern central dashboard for Monit agents"
)]
struct Cli {
    /// Path to a TOML configuration file.
    #[arg(short, long, env = "MONARCH_CONFIG", global = true)]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Run the server (default).
    Serve,
    /// Create a user. The password is read from stdin.
    UserAdd {
        username: String,
        #[arg(long, default_value = "admin", value_parser = ["admin", "operator", "viewer", "collector"])]
        role: String,
    },
    /// Set a user's password, read from stdin.
    Passwd { username: String },
}

fn read_password() -> Result<String> {
    let mut s = String::new();
    std::io::stdin().read_to_string(&mut s)?;
    let p = s.trim_end_matches(['\n', '\r']).to_owned();
    if p.len() < 8 {
        bail!("password must be at least 8 characters");
    }
    Ok(p)
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_env("MONARCH_LOG")
                .unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn")),
        )
        .init();
    let cli = Cli::parse();
    let config = Config::load(cli.config.as_ref())?;
    let pool = db::connect(&config.database).await?;

    match cli.command.unwrap_or(Command::Serve) {
        Command::UserAdd { username, role } => {
            api_validate(&username)?;
            let role = auth::Role::parse(&role).context("invalid role")?;
            let u = auth::create_user(&pool, &username, &read_password()?, role).await?;
            println!("created user {} ({})", u.username, u.role);
            Ok(())
        }
        Command::Passwd { username } => {
            let hash = auth::hash_password(&read_password()?)?;
            let n = sqlx::query("UPDATE users SET password_hash = ? WHERE username = ?")
                .bind(hash)
                .bind(&username)
                .execute(&pool)
                .await?
                .rows_affected();
            if n == 0 {
                bail!("no such user {username}");
            }
            println!("password updated");
            Ok(())
        }
        Command::Serve => serve(config, pool).await,
    }
}

fn api_validate(username: &str) -> Result<()> {
    if username.is_empty() || username.contains(':') {
        bail!("invalid username");
    }
    Ok(())
}

async fn serve(config: Config, pool: sqlx::SqlitePool) -> Result<()> {
    let mut settings = db::load_settings(&pool).await?.unwrap_or_else(|| Settings {
        public_url: format!("http://{}", config.listen),
        ..Settings::default()
    });
    if let Some(u) = &config.public_url {
        settings.public_url = u.trim_end_matches('/').to_owned();
    }
    if let Some(t) = &config.action_url_template {
        let t = t.trim();
        if !t.is_empty()
            && let Err(e) = views::validate_url_template(t)
        {
            bail!("invalid action_url_template: {e}");
        }
        settings.action_url_template = t.to_owned();
    }
    db::save_settings(&pool, &settings).await?;

    if let (Some(user), Some(file)) = (
        &config.initial_admin_user,
        &config.initial_admin_password_file,
    ) && auth::user_count(&pool).await? == 0
    {
        let password =
            std::fs::read_to_string(file).with_context(|| format!("reading {}", file.display()))?;
        auth::create_user(
            &pool,
            user,
            password.trim_end_matches(['\n', '\r']),
            auth::Role::Admin,
        )
        .await?;
        tracing::info!(%user, "created initial admin user");
    }

    let mut managed_tokens = Vec::new();
    for u in &config.ensure_users {
        let user_id = ensure_user(&pool, u).await?;
        managed_tokens.extend(ensure_api_tokens(&pool, user_id, &u.api_tokens).await?);
    }
    prune_managed_tokens(&pool, &managed_tokens).await?;

    let listen = config.listen;
    let state = state::AppState::new(pool, config, settings)?;
    pull::sync_managed(&state).await?;
    tasks::spawn(state.clone());
    pull::spawn(state.clone());

    let app = Router::new()
        .route(
            "/collector",
            post(collector::collector).layer(DefaultBodyLimit::max(32 * 1024 * 1024)),
        )
        .nest("/api", api::router())
        .merge(compat::router())
        .fallback(web::spa)
        .layer(RequestDecompressionLayer::new())
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind(listen)
        .await
        .with_context(|| format!("binding {listen}"))?;
    tracing::info!(
        "monarch {} listening on http://{listen}",
        env!("CARGO_PKG_VERSION")
    );
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown({
        let state = state.clone();
        async move {
            shutdown().await;
            // End SSE streams, otherwise graceful shutdown waits for them forever.
            let _ = state.shutdown.send(true);
        }
    })
    .await?;
    // Send what the grouping windows still hold instead of losing it on restart.
    let _ = tokio::time::timeout(
        std::time::Duration::from_secs(20),
        notify::flush_all(&state),
    )
    .await;
    Ok(())
}

async fn shutdown() {
    #[cfg(unix)]
    {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("installing SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = term.recv() => {}
        }
    }
    #[cfg(not(unix))]
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("shutting down");
}

fn read_secret(path: &std::path::Path) -> Result<String> {
    let s = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(s.trim_end_matches(['\n', '\r']).to_owned())
}

/// Declaratively managed account: create it, or reset role and password.
async fn ensure_user(pool: &sqlx::SqlitePool, u: &config::EnsureUser) -> Result<i64> {
    let username = match (&u.username, &u.username_file) {
        (Some(n), _) => n.clone(),
        (None, Some(f)) => read_secret(f)?,
        (None, None) => bail!("ensure_users entries need username or username_file"),
    };
    let role = auth::Role::parse(&u.role)
        .with_context(|| format!("invalid role {} for {username}", u.role))?;
    let password = read_secret(&u.password_file)?;
    if password.len() < 8 {
        bail!("password for {username} must be at least 8 characters");
    }
    let email = match (&u.email, &u.email_file) {
        (Some(e), _) => Some(e.trim().to_owned()),
        (None, Some(f)) => Some(read_secret(f)?.trim().to_owned()),
        (None, None) => None,
    }
    .filter(|e| !e.is_empty());
    let user_id;
    let existing: Option<(i64,)> = sqlx::query_as("SELECT id FROM users WHERE username = ?")
        .bind(&username)
        .fetch_optional(pool)
        .await?;
    match existing {
        None => {
            let user = auth::create_user(pool, &username, &password, role).await?;
            user_id = user.id;
            if let Some(e) = &email {
                sqlx::query("UPDATE users SET email = ? WHERE id = ?")
                    .bind(e)
                    .bind(user.id)
                    .execute(pool)
                    .await?;
            }
            tracing::info!(user = %username, role = %u.role, "created managed user");
        }
        Some((id,)) => {
            user_id = id;
            if auth::check_credentials(pool, &username, &password)
                .await?
                .is_none()
            {
                sqlx::query("UPDATE users SET password_hash = ? WHERE id = ?")
                    .bind(auth::hash_password(&password)?)
                    .bind(id)
                    .execute(pool)
                    .await?;
                tracing::info!(user = %username, "updated password of managed user");
            }
            sqlx::query("UPDATE users SET role = ? WHERE id = ?")
                .bind(&u.role)
                .bind(id)
                .execute(pool)
                .await?;
            // Only managed when configured: an unset email leaves what the user chose.
            if let Some(e) = &email {
                sqlx::query("UPDATE users SET email = ? WHERE id = ?")
                    .bind(e)
                    .bind(id)
                    .execute(pool)
                    .await?;
            }
        }
    }
    Ok(user_id)
}

/// Upsert the API tokens declared for a user, keyed by (user, name). Returns their ids.
async fn ensure_api_tokens(
    pool: &sqlx::SqlitePool,
    user_id: i64,
    tokens: &[config::EnsureApiToken],
) -> Result<Vec<i64>> {
    let mut ids = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for t in tokens {
        let name = t.name.trim();
        if name.is_empty() || name.len() > 64 {
            bail!("api token names must be 1-64 characters");
        }
        if !seen.insert(name.to_owned()) {
            bail!("duplicate api token name {name:?}");
        }
        // Errors name the file and the problem, never the token.
        let token = read_secret(&t.token_file)?.trim().to_owned();
        let valid = token.len() >= 20
            && token.len() <= 256
            && token.strip_prefix(auth::API_TOKEN_PREFIX).is_some_and(|r| {
                r.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            });
        if !valid {
            bail!(
                "api token {name:?} ({}): must start with {} followed by 16-252 URL-safe characters",
                t.token_file.display(),
                auth::API_TOKEN_PREFIX
            );
        }
        let hash = auth::api_token_hash(&token);
        let prefix: String = token
            .chars()
            .take(auth::API_TOKEN_PREFIX.len() + 4)
            .collect();
        let existing: Option<i64> =
            sqlx::query_scalar("SELECT id FROM api_tokens WHERE user_id = ? AND name = ?")
                .bind(user_id)
                .bind(name)
                .fetch_optional(pool)
                .await?;
        let id = match existing {
            Some(id) => {
                sqlx::query(
                    "UPDATE api_tokens SET token_hash = ?, prefix = ?, managed = 1, expires_at = NULL
                     WHERE id = ? AND (token_hash != ? OR managed = 0 OR expires_at IS NOT NULL)",
                )
                .bind(&hash)
                .bind(&prefix)
                .bind(id)
                .bind(&hash)
                .execute(pool)
                .await
                .with_context(|| format!("updating api token {name:?}"))?;
                id
            }
            None => sqlx::query(
                "INSERT INTO api_tokens (user_id, name, token_hash, prefix, created_at, managed)
                 VALUES (?, ?, ?, ?, ?, 1)",
            )
            .bind(user_id)
            .bind(name)
            .bind(&hash)
            .bind(&prefix)
            .bind(state::now())
            .execute(pool)
            .await
            .with_context(|| format!("creating api token {name:?}"))?
            .last_insert_rowid(),
        };
        ids.push(id);
    }
    Ok(ids)
}

/// Delete managed tokens that are no longer declared.
async fn prune_managed_tokens(pool: &sqlx::SqlitePool, keep: &[i64]) -> Result<()> {
    let managed: Vec<i64> = sqlx::query_scalar("SELECT id FROM api_tokens WHERE managed = 1")
        .fetch_all(pool)
        .await?;
    for id in managed.into_iter().filter(|id| !keep.contains(id)) {
        sqlx::query("DELETE FROM api_tokens WHERE id = ?")
            .bind(id)
            .execute(pool)
            .await?;
        tracing::info!(
            id,
            "removed managed api token no longer in the configuration"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token_file(dir: &std::path::Path, name: &str, content: &str) -> config::EnsureApiToken {
        let f = dir.join(name);
        std::fs::write(&f, content).unwrap();
        config::EnsureApiToken {
            name: name.into(),
            token_file: f,
        }
    }

    #[tokio::test]
    async fn provisions_updates_and_prunes_managed_tokens() {
        let dir = std::env::temp_dir().join(format!("monarch-test-{}", auth::generate_api_token()));
        std::fs::create_dir_all(&dir).unwrap();
        let pool = db::connect(&dir.join("t.db")).await.unwrap();
        let user = auth::create_user(&pool, "bob", "password123", auth::Role::Viewer)
            .await
            .unwrap();
        let a = auth::generate_api_token();
        let b = auth::generate_api_token();

        let ta = token_file(&dir, "a", &format!("{a}\n"));
        let tb = token_file(&dir, "b", &b);
        let ids = ensure_api_tokens(&pool, user.id, &[ta.clone(), tb])
            .await
            .unwrap();
        assert_eq!(ids.len(), 2);
        prune_managed_tokens(&pool, &ids).await.unwrap();
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM api_tokens WHERE managed = 1")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(n, 2);

        // Same name, new token: updated in place.
        let a2 = auth::generate_api_token();
        std::fs::write(&ta.token_file, &a2).unwrap();
        let ids2 = ensure_api_tokens(&pool, user.id, std::slice::from_ref(&ta))
            .await
            .unwrap();
        assert_eq!(ids2[0], ids[0]);
        let hash: String = sqlx::query_scalar("SELECT token_hash FROM api_tokens WHERE id = ?")
            .bind(ids2[0])
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(hash, auth::api_token_hash(&a2));

        // Dropped from the config: pruned. Unmanaged tokens stay.
        sqlx::query("INSERT INTO api_tokens (user_id, name, token_hash, prefix, created_at) VALUES (?, 'manual', 'h', 'mnr_', 0)")
            .bind(user.id)
            .execute(&pool)
            .await
            .unwrap();
        prune_managed_tokens(&pool, &ids2).await.unwrap();
        let names: Vec<String> = sqlx::query_scalar("SELECT name FROM api_tokens ORDER BY name")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(names, ["a", "manual"]);

        // Invalid tokens are refused without echoing them.
        let bad = token_file(&dir, "bad", "not-a-token");
        let err = ensure_api_tokens(&pool, user.id, &[bad]).await.unwrap_err();
        assert!(!format!("{err:#}").contains("not-a-token"));

        std::fs::remove_dir_all(&dir).ok();
    }
}
