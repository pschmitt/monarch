use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::json;

use super::{ApiError, ApiResult};
use crate::{
    auth::{self, Role},
    state::SharedState,
};

#[derive(Deserialize)]
pub struct Credentials {
    username: String,
    password: String,
}

pub async fn me(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> ApiResult<Json<serde_json::Value>> {
    let user = auth::user_from_headers(&state, &headers)
        .await?
        .filter(|u| u.role() != Role::Collector);
    Ok(Json(json!({
        "user": user,
        "setup_required": auth::user_count(&state.db).await? == 0,
        "version": env!("CARGO_PKG_VERSION"),
    })))
}

fn signed_in(state: &SharedState, headers: &HeaderMap, token: &str, user: auth::User) -> Response {
    let cookie = auth::session_cookie(state, auth::is_secure(headers), token);
    ([(header::SET_COOKIE, cookie)], Json(user)).into_response()
}

pub async fn setup(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(c): Json<Credentials>,
) -> ApiResult<Response> {
    if auth::user_count(&state.db).await? > 0 {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "setup already completed",
        ));
    }
    validate(&c)?;
    let user = auth::create_user(&state.db, c.username.trim(), &c.password, Role::Admin).await?;
    let token = auth::create_session(&state, user.id).await?;
    tracing::info!(user = %user.username, "initial admin created via setup");
    Ok(signed_in(&state, &headers, &token, user))
}

pub fn validate(c: &Credentials) -> ApiResult<()> {
    validate_parts(&c.username, &c.password)
}

pub fn validate_parts(username: &str, password: &str) -> ApiResult<()> {
    let u = username.trim();
    if u.is_empty() || u.len() > 64 || u.contains(':') {
        return Err(ApiError::bad_request(
            "username must be 1-64 characters without ':'",
        ));
    }
    if password.len() < 8 {
        return Err(ApiError::bad_request(
            "password must be at least 8 characters",
        ));
    }
    Ok(())
}

pub async fn login(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(c): Json<Credentials>,
) -> ApiResult<Response> {
    match auth::check_credentials(&state.db, c.username.trim(), &c.password).await? {
        Some(u) if u.role() == Role::Collector => Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "collector accounts cannot sign in",
        )),
        Some(u) => {
            let token = auth::create_session(&state, u.id).await?;
            Ok(signed_in(&state, &headers, &token, u))
        }
        None => Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "invalid username or password",
        )),
    }
}

pub async fn logout(State(state): State<SharedState>, headers: HeaderMap) -> ApiResult<Response> {
    if let Some(token) = auth::session_token(&headers) {
        auth::delete_session(&state.db, &token).await?;
    }
    Ok((
        StatusCode::NO_CONTENT,
        [(header::SET_COOKIE, auth::clear_cookie())],
    )
        .into_response())
}
