//! Single sign-on with OpenID Connect (authorization code flow with PKCE).

use anyhow::{Context, Result, bail};
use axum::{
    extract::{Query, State},
    http::{HeaderMap, header},
    response::{IntoResponse, Redirect, Response},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::RngCore;
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::sync::OnceCell;

use crate::{
    auth::{self, Role, User},
    config::OidcConfig,
    state::{SharedState, now},
};

const STATE_TTL: i64 = 600;

#[derive(Debug, Deserialize)]
struct Discovery {
    issuer: String,
    authorization_endpoint: String,
    token_endpoint: String,
    userinfo_endpoint: Option<String>,
}

static DISCOVERY: OnceCell<Discovery> = OnceCell::const_new();

async fn discovery(state: &SharedState, cfg: &OidcConfig) -> Result<&'static Discovery> {
    DISCOVERY
        .get_or_try_init(|| async {
            let url = format!(
                "{}/.well-known/openid-configuration",
                cfg.issuer.trim_end_matches('/')
            );
            let d: Discovery = state
                .http
                .get(&url)
                .send()
                .await
                .with_context(|| format!("fetching {url}"))?
                .error_for_status()?
                .json()
                .await
                .context("parsing OIDC discovery document")?;
            Ok::<_, anyhow::Error>(d)
        })
        .await
}

fn random(n: usize) -> String {
    let mut b = vec![0u8; n];
    rand::rng().fill_bytes(&mut b);
    URL_SAFE_NO_PAD.encode(b)
}

fn redirect_uri(public_url: &str) -> String {
    format!(
        "{}/api/auth/oidc/callback",
        public_url.trim_end_matches('/')
    )
}

/// Only allow local paths as post-login targets.
fn safe_next(next: Option<&str>) -> String {
    match next {
        Some(n) if n.starts_with('/') && !n.starts_with("//") && !n.contains('\\') => n.to_owned(),
        _ => "/".to_owned(),
    }
}

fn error_redirect(msg: &str) -> Response {
    let q = serde_urlencoded::to_string([("sso_error", msg)]).unwrap_or_default();
    Redirect::to(&format!("/login?{q}")).into_response()
}

#[derive(Deserialize)]
pub struct LoginQuery {
    next: Option<String>,
}

pub async fn login(State(state): State<SharedState>, Query(q): Query<LoginQuery>) -> Response {
    let Some(cfg) = state.config.oidc.clone() else {
        return error_redirect("single sign-on is not configured");
    };
    match start(&state, &cfg, q.next.as_deref()).await {
        Ok(url) => Redirect::to(&url).into_response(),
        Err(e) => {
            tracing::error!("oidc login: {e:#}");
            error_redirect("the identity provider is not reachable")
        }
    }
}

async fn start(state: &SharedState, cfg: &OidcConfig, next: Option<&str>) -> Result<String> {
    let d = discovery(state, cfg).await?;
    let st = random(24);
    let verifier = random(48);
    let nonce = random(24);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    sqlx::query("DELETE FROM oidc_states WHERE created_at < ?")
        .bind(now() - STATE_TTL)
        .execute(&state.db)
        .await?;
    sqlx::query(
        "INSERT INTO oidc_states (state, verifier, nonce, next, created_at) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&st)
    .bind(&verifier)
    .bind(&nonce)
    .bind(safe_next(next))
    .bind(now())
    .execute(&state.db)
    .await?;
    let public_url = state.settings.read().await.public_url.clone();
    let query = serde_urlencoded::to_string([
        ("response_type", "code"),
        ("client_id", cfg.client_id.as_str()),
        ("redirect_uri", redirect_uri(&public_url).as_str()),
        ("scope", cfg.scopes.join(" ").as_str()),
        ("state", st.as_str()),
        ("nonce", nonce.as_str()),
        ("code_challenge", challenge.as_str()),
        ("code_challenge_method", "S256"),
    ])?;
    let sep = if d.authorization_endpoint.contains('?') {
        '&'
    } else {
        '?'
    };
    Ok(format!("{}{sep}{query}", d.authorization_endpoint))
}

#[derive(Deserialize)]
pub struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

pub async fn callback(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(q): Query<CallbackQuery>,
) -> Response {
    if let Some(e) = q.error {
        return error_redirect(q.error_description.as_deref().unwrap_or(&e));
    }
    let Some(cfg) = state.config.oidc.clone() else {
        return error_redirect("single sign-on is not configured");
    };
    let (Some(code), Some(st)) = (q.code, q.state) else {
        return error_redirect("invalid sign-in response");
    };
    match finish(&state, &cfg, &code, &st).await {
        Ok((user, next)) => match auth::create_session(&state, user.id).await {
            Ok(token) => {
                tracing::info!(user = %user.username, role = %user.role, "signed in via OIDC");
                let cookie = auth::session_cookie(&state, auth::is_secure(&headers), &token);
                ([(header::SET_COOKIE, cookie)], Redirect::to(&next)).into_response()
            }
            Err(e) => {
                tracing::error!("oidc session: {e:#}");
                error_redirect("internal error")
            }
        },
        Err(e) => {
            tracing::warn!("oidc callback: {e:#}");
            error_redirect(&format!("{e}"))
        }
    }
}

fn read_secret(cfg: &OidcConfig) -> Result<Option<String>> {
    cfg.client_secret_file
        .as_ref()
        .map(|p| {
            std::fs::read_to_string(p)
                .map(|s| s.trim_end_matches(['\n', '\r']).to_owned())
                .with_context(|| format!("reading {}", p.display()))
        })
        .transpose()
}

fn jwt_claims(token: &str) -> Result<Value> {
    let payload = token.split('.').nth(1).context("malformed id_token")?;
    let raw = URL_SAFE_NO_PAD
        .decode(payload.trim_end_matches('='))
        .context("malformed id_token")?;
    Ok(serde_json::from_slice(&raw)?)
}

fn claim_strings(v: &Value) -> Vec<String> {
    match v {
        Value::Array(a) => a
            .iter()
            .filter_map(|x| x.as_str().map(str::to_owned))
            .collect(),
        Value::String(s) => s.split(',').map(|s| s.trim().to_owned()).collect(),
        _ => vec![],
    }
}

fn role_for(cfg: &OidcConfig, groups: &[String]) -> Option<Role> {
    let any = |list: &[String]| list.iter().any(|g| groups.contains(g));
    if any(&cfg.admin_groups) {
        Some(Role::Admin)
    } else if any(&cfg.operator_groups) {
        Some(Role::Operator)
    } else {
        Role::parse(&cfg.default_role).filter(|r| *r != Role::Collector)
    }
}

fn role_str(r: Role) -> &'static str {
    match r {
        Role::Admin => "admin",
        Role::Operator => "operator",
        Role::Viewer => "viewer",
        Role::Collector => "collector",
    }
}

async fn finish(
    state: &SharedState,
    cfg: &OidcConfig,
    code: &str,
    st: &str,
) -> Result<(User, String)> {
    let row: Option<(String, String, String, i64)> = sqlx::query_as(
        "DELETE FROM oidc_states WHERE state = ? RETURNING verifier, nonce, next, created_at",
    )
    .bind(st)
    .fetch_optional(&state.db)
    .await?;
    let Some((verifier, nonce, next, created)) = row else {
        bail!("sign-in session expired, please try again");
    };
    if now() - created > STATE_TTL {
        bail!("sign-in session expired, please try again");
    }
    let d = discovery(state, cfg).await?;
    let public_url = state.settings.read().await.public_url.clone();
    let redirect = redirect_uri(&public_url);
    let mut form = vec![
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect.as_str()),
        ("code_verifier", verifier.as_str()),
    ];
    let secret = read_secret(cfg)?;
    let mut req = state.http.post(&d.token_endpoint);
    match &secret {
        Some(s) => req = req.basic_auth(&cfg.client_id, Some(s)),
        None => form.push(("client_id", cfg.client_id.as_str())),
    }
    let res = req
        .form(&form)
        .send()
        .await
        .context("token request failed")?;
    if !res.status().is_success() {
        let body = res.text().await.unwrap_or_default();
        bail!(
            "the identity provider rejected the sign-in: {}",
            body.chars().take(200).collect::<String>()
        );
    }
    let tokens: Value = res.json().await?;
    let id_token = tokens["id_token"]
        .as_str()
        .context("no id_token in token response")?;
    // The token came straight from the token endpoint over TLS, which OIDC
    // Core (3.1.3.7) accepts in lieu of signature validation for this flow.
    let mut claims = jwt_claims(id_token)?;
    if claims["iss"].as_str().map(|i| i.trim_end_matches('/'))
        != Some(d.issuer.trim_end_matches('/'))
    {
        bail!("id_token has an unexpected issuer");
    }
    let aud_ok = match &claims["aud"] {
        Value::String(a) => a == &cfg.client_id,
        Value::Array(a) => a.iter().any(|x| x.as_str() == Some(cfg.client_id.as_str())),
        _ => false,
    };
    if !aud_ok {
        bail!("id_token is not meant for this client");
    }
    if claims["nonce"].as_str() != Some(nonce.as_str()) {
        bail!("id_token nonce mismatch");
    }
    if claims["exp"].as_i64().is_some_and(|e| e < now() - 60) {
        bail!("id_token expired");
    }
    // Merge userinfo (groups etc. are often only there).
    if let (Some(ui), Some(at)) = (&d.userinfo_endpoint, tokens["access_token"].as_str())
        && let Ok(r) = state.http.get(ui).bearer_auth(at).send().await
        && r.status().is_success()
        && let Ok(Value::Object(extra)) = r.json::<Value>().await
        && let Value::Object(c) = &mut claims
    {
        for (k, v) in extra {
            c.entry(k).or_insert(v);
        }
    }
    let subject = claims["sub"]
        .as_str()
        .context("id_token has no subject")?
        .to_owned();
    let username = claims[&cfg.username_claim]
        .as_str()
        .or(claims["email"].as_str())
        .unwrap_or(&subject)
        .to_owned();
    let groups = claim_strings(&claims[&cfg.groups_claim]);
    let Some(role) = role_for(cfg, &groups) else {
        bail!("your account is not allowed to use Monarch");
    };
    let email = claims["email"]
        .as_str()
        .map(str::trim)
        .filter(|e| !e.is_empty())
        .map(str::to_owned);
    let user = upsert_user(state, cfg, &subject, &username, email.as_deref(), role).await?;
    Ok((user, next))
}

async fn upsert_user(
    state: &SharedState,
    cfg: &OidcConfig,
    subject: &str,
    username: &str,
    email: Option<&str>,
    role: Role,
) -> Result<User> {
    let db = &state.db;
    let existing: Option<(i64, String)> =
        sqlx::query_as("SELECT id, auth_source FROM users WHERE oidc_subject = ?")
            .bind(subject)
            .fetch_optional(db)
            .await?;
    // A linked local account keeps its own role; only SSO-created ones follow the IdP groups.
    let linkable: Option<(i64,)> = if existing.is_none() && cfg.link_local_users {
        sqlx::query_as(
            "SELECT id FROM users WHERE username = ? AND auth_source = 'local'
               AND role != 'collector' AND oidc_subject IS NULL",
        )
        .bind(username)
        .fetch_optional(db)
        .await?
    } else {
        None
    };
    let id = match (existing, linkable) {
        (Some((id, source)), _) => {
            if source == "oidc" {
                sqlx::query("UPDATE users SET role = ? WHERE id = ?")
                    .bind(role_str(role))
                    .bind(id)
                    .execute(db)
                    .await?;
            }
            if let Some(e) = email {
                // The IdP is authoritative for SSO accounts; a linked local account
                // keeps an address it already has.
                sqlx::query(
                    "UPDATE users SET email = ? WHERE id = ? AND (auth_source = 'oidc' OR email IS NULL)",
                )
                .bind(e)
                .bind(id)
                .execute(db)
                .await?;
            }
            id
        }
        (None, Some((id,))) => {
            sqlx::query("UPDATE users SET oidc_subject = ? WHERE id = ?")
                .bind(subject)
                .bind(id)
                .execute(db)
                .await?;
            tracing::info!(%username, "linked single sign-on identity to the local account");
            if let Some(e) = email {
                sqlx::query("UPDATE users SET email = ? WHERE id = ? AND email IS NULL")
                    .bind(e)
                    .bind(id)
                    .execute(db)
                    .await?;
            }
            id
        }
        (None, None) => {
            // Never take over a local account with the same name.
            let mut name = username.to_owned();
            let taken: Option<(i64,)> = sqlx::query_as("SELECT id FROM users WHERE username = ?")
                .bind(&name)
                .fetch_optional(db)
                .await?;
            if taken.is_some() {
                name = format!("{username}-sso");
            }
            sqlx::query(
                "INSERT INTO users (username, password_hash, role, created_at, auth_source, oidc_subject, email)
                 VALUES (?, '!', ?, ?, 'oidc', ?, ?)",
            )
            .bind(&name)
            .bind(role_str(role))
            .bind(now())
            .bind(subject)
            .bind(email)
            .execute(db)
            .await?
            .last_insert_rowid()
        }
    };
    Ok(sqlx::query_as(
        "SELECT id, username, role, created_at, last_login, auth_source, (oidc_subject IS NOT NULL) AS sso, email FROM users WHERE id = ?",
    )
    .bind(id)
    .fetch_one(db)
    .await?)
}
