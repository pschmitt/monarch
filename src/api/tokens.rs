//! Personal API tokens: bearer credentials that act as their owner.

use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use super::{ApiError, ApiResult};
use crate::{
    auth::{self, User},
    state::{SharedState, now},
};

#[derive(Serialize, FromRow)]
pub struct TokenInfo {
    id: i64,
    name: String,
    prefix: String,
    created_at: i64,
    last_used: Option<i64>,
    expires_at: Option<i64>,
}

#[derive(Deserialize)]
pub struct NewToken {
    name: String,
    /// Lifetime in days; omitted = never expires.
    expires_days: Option<i64>,
}

#[derive(Serialize)]
pub struct CreatedToken {
    #[serde(flatten)]
    info: TokenInfo,
    /// The token itself, shown only once.
    token: String,
}

/// Tokens are managed with a browser session only, so a leaked token cannot mint more.
fn forbid_token_auth(headers: &HeaderMap) -> ApiResult<()> {
    if auth::bearer_token(headers).is_some() {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "API tokens cannot be managed with an API token",
        ));
    }
    Ok(())
}

pub async fn list(
    State(state): State<SharedState>,
    user: User,
    headers: HeaderMap,
) -> ApiResult<Json<Vec<TokenInfo>>> {
    forbid_token_auth(&headers)?;
    Ok(Json(
        sqlx::query_as(
            "SELECT id, name, prefix, created_at, last_used, expires_at FROM api_tokens
             WHERE user_id = ? ORDER BY created_at DESC",
        )
        .bind(user.id)
        .fetch_all(&state.db)
        .await?,
    ))
}

pub async fn create(
    State(state): State<SharedState>,
    user: User,
    headers: HeaderMap,
    Json(b): Json<NewToken>,
) -> ApiResult<Json<CreatedToken>> {
    forbid_token_auth(&headers)?;
    let name = b.name.trim();
    if name.is_empty() || name.len() > 64 {
        return Err(ApiError::bad_request("name must be 1-64 characters"));
    }
    if b.expires_days.is_some_and(|d| d <= 0) {
        return Err(ApiError::bad_request("expires_days must be positive"));
    }
    let token = auth::generate_api_token();
    let ts = now();
    let expires_at = b.expires_days.map(|d| ts + d * 86400);
    // Shown in the list to recognise a token: the prefix plus the first characters.
    let prefix: String = token
        .chars()
        .take(auth::API_TOKEN_PREFIX.len() + 4)
        .collect();
    let id = sqlx::query(
        "INSERT INTO api_tokens (user_id, name, token_hash, prefix, created_at, expires_at)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(user.id)
    .bind(name)
    .bind(auth::api_token_hash(&token))
    .bind(&prefix)
    .bind(ts)
    .bind(expires_at)
    .execute(&state.db)
    .await?
    .last_insert_rowid();
    Ok(Json(CreatedToken {
        info: TokenInfo {
            id,
            name: name.to_owned(),
            prefix,
            created_at: ts,
            last_used: None,
            expires_at,
        },
        token,
    }))
}

pub async fn revoke(
    State(state): State<SharedState>,
    user: User,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    forbid_token_auth(&headers)?;
    let n = sqlx::query("DELETE FROM api_tokens WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(user.id)
        .execute(&state.db)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(ApiError::not_found("token"));
    }
    Ok(StatusCode::NO_CONTENT)
}
