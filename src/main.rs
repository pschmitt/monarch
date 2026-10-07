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
mod monit;
mod notify;
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

    for u in &config.ensure_users {
        ensure_user(&pool, u).await?;
    }

    let listen = config.listen;
    let state = state::AppState::new(pool, config, settings)?;
    tasks::spawn(state.clone());

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
async fn ensure_user(pool: &sqlx::SqlitePool, u: &config::EnsureUser) -> Result<()> {
    let role = auth::Role::parse(&u.role)
        .with_context(|| format!("invalid role {} for {}", u.role, u.username))?;
    let password = read_secret(&u.password_file)?;
    if password.len() < 8 {
        bail!("password for {} must be at least 8 characters", u.username);
    }
    let existing: Option<(i64,)> = sqlx::query_as("SELECT id FROM users WHERE username = ?")
        .bind(&u.username)
        .fetch_optional(pool)
        .await?;
    match existing {
        None => {
            auth::create_user(pool, &u.username, &password, role).await?;
            tracing::info!(user = %u.username, role = %u.role, "created managed user");
        }
        Some((id,)) => {
            if auth::check_credentials(pool, &u.username, &password)
                .await?
                .is_none()
            {
                sqlx::query("UPDATE users SET password_hash = ? WHERE id = ?")
                    .bind(auth::hash_password(&password)?)
                    .bind(id)
                    .execute(pool)
                    .await?;
                tracing::info!(user = %u.username, "updated password of managed user");
            }
            sqlx::query("UPDATE users SET role = ? WHERE id = ?")
                .bind(&u.role)
                .bind(id)
                .execute(pool)
                .await?;
        }
    }
    Ok(())
}
