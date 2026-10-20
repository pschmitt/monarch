//! Profile pictures. The browser crops and scales them before uploading, so
//! the server only checks the size and that the bytes are a raster image
//! (never SVG, which could carry script). Users without one can fall back to
//! Gravatar, fetched by the server so viewers never contact gravatar.com.

use std::time::{Duration, Instant};

use axum::{
    Json,
    body::Bytes,
    extract::{Path, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use sha2::{Digest, Sha256};

use super::{ApiError, ApiResult};
use crate::{
    auth::{Role, USER_SELECT, User},
    state::{SharedState, now},
};

pub const MAX_BYTES: usize = 512 * 1024;
const GRAVATAR_TTL: Duration = Duration::from_secs(6 * 3600);

/// Image type from the leading magic bytes.
pub fn sniff(data: &[u8]) -> Option<&'static str> {
    if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if data.starts_with(b"\xff\xd8\xff") {
        Some("image/jpeg")
    } else if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if data.len() >= 12 && &data[..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

/// Gravatar's key for an address: SHA-256 of the trimmed, lowercased email.
pub fn gravatar_hash(email: &str) -> String {
    hex::encode(Sha256::digest(email.trim().to_lowercase().as_bytes()))
}

fn image(mime: &str, data: impl IntoResponse, cache: &'static str) -> Response {
    let mut res = data.into_response();
    let h = res.headers_mut();
    if let Ok(v) = HeaderValue::from_str(mime) {
        h.insert(header::CONTENT_TYPE, v);
    }
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    res
}

async fn fetch_gravatar(state: &SharedState, email: &str) -> Option<(&'static str, Bytes)> {
    let hash = gravatar_hash(email);
    if let Some((at, cached)) = state.gravatars.lock().unwrap().get(&hash)
        && at.elapsed() < GRAVATAR_TTL
    {
        return cached.clone();
    }
    // d=404: no generated placeholder, the UI shows initials instead.
    let url = format!("https://gravatar.com/avatar/{hash}?s=256&d=404");
    let fetched = async {
        let res = state
            .http
            .get(&url)
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .ok()?;
        if !res.status().is_success() {
            return None;
        }
        let body = res.bytes().await.ok()?;
        let mime = sniff(&body).filter(|_| body.len() <= MAX_BYTES)?;
        Some((mime, body))
    }
    .await;
    state
        .gravatars
        .lock()
        .unwrap()
        .insert(hash, (Instant::now(), fetched.clone()));
    fetched
}

pub async fn get(
    State(state): State<SharedState>,
    _user: User,
    Path(id): Path<i64>,
) -> ApiResult<Response> {
    let row: Option<(String, Vec<u8>)> =
        sqlx::query_as("SELECT mime, data FROM user_avatars WHERE user_id = ?")
            .bind(id)
            .fetch_optional(&state.db)
            .await?;
    if let Some((mime, data)) = row {
        // URLs carry the version (`?v=<updated_at>`), so they never go stale.
        return Ok(image(&mime, data, "private, max-age=31536000, immutable"));
    }
    if state.settings.read().await.gravatar {
        let email: Option<(Option<String>,)> =
            sqlx::query_as("SELECT email FROM users WHERE id = ?")
                .bind(id)
                .fetch_optional(&state.db)
                .await?;
        if let Some((Some(email),)) =
            email.filter(|(e,)| e.as_deref().is_some_and(|e| !e.is_empty()))
            && let Some((mime, data)) = fetch_gravatar(&state, &email).await
        {
            return Ok(image(mime, data, "private, max-age=3600"));
        }
    }
    Err(ApiError::not_found("avatar"))
}

async fn user_json(state: &SharedState, id: i64) -> ApiResult<Json<User>> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!("{USER_SELECT} WHERE id = ?")))
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::not_found("user"))
}

async fn store(state: &SharedState, id: i64, body: &[u8]) -> ApiResult<Json<User>> {
    // 404 for unknown users rather than a foreign key error.
    let _ = user_json(state, id).await?;
    if body.len() > MAX_BYTES {
        return Err(ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            format!("profile picture must be at most {} KiB", MAX_BYTES / 1024),
        ));
    }
    let mime = sniff(body)
        .ok_or_else(|| ApiError::bad_request("expected a PNG, JPEG, WebP or GIF image"))?;
    sqlx::query(
        "INSERT INTO user_avatars (user_id, mime, data, updated_at) VALUES (?, ?, ?, ?)
         ON CONFLICT (user_id) DO UPDATE SET mime = excluded.mime, data = excluded.data,
         updated_at = excluded.updated_at",
    )
    .bind(id)
    .bind(mime)
    .bind(body)
    .bind(now())
    .execute(&state.db)
    .await?;
    user_json(state, id).await
}

async fn clear(state: &SharedState, id: i64) -> ApiResult<Json<User>> {
    sqlx::query("DELETE FROM user_avatars WHERE user_id = ?")
        .bind(id)
        .execute(&state.db)
        .await?;
    user_json(state, id).await
}

pub async fn put_me(
    State(state): State<SharedState>,
    user: User,
    body: Bytes,
) -> ApiResult<Json<User>> {
    store(&state, user.id, &body).await
}

pub async fn delete_me(State(state): State<SharedState>, user: User) -> ApiResult<Json<User>> {
    clear(&state, user.id).await
}

/// Admins set pictures for other accounts, e.g. collector or service users
/// that never sign in.
pub async fn put_user(
    State(state): State<SharedState>,
    user: User,
    Path(id): Path<i64>,
    body: Bytes,
) -> ApiResult<Json<User>> {
    user.require(Role::Admin)?;
    store(&state, id, &body).await
}

pub async fn delete_user(
    State(state): State<SharedState>,
    user: User,
    Path(id): Path<i64>,
) -> ApiResult<Json<User>> {
    user.require(Role::Admin)?;
    clear(&state, id).await
}

#[cfg(test)]
mod tests {
    use super::{gravatar_hash, sniff};

    #[test]
    fn sniffs_raster_images_only() {
        assert_eq!(sniff(b"\x89PNG\r\n\x1a\n...."), Some("image/png"));
        assert_eq!(sniff(b"\xff\xd8\xff\xe0...."), Some("image/jpeg"));
        assert_eq!(sniff(b"GIF89a...."), Some("image/gif"));
        assert_eq!(sniff(b"RIFF\0\0\0\0WEBPVP8 "), Some("image/webp"));
        assert_eq!(sniff(b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>"), None);
        assert_eq!(sniff(b""), None);
    }

    #[test]
    fn hashes_normalized_email() {
        assert_eq!(
            gravatar_hash(" MyEmailAddress@example.com "),
            "84059b07d4be67b806386c0aad8070a23f18836bbaae342275dc0a83414c32ee"
        );
    }
}
