use axum::{
    Json, Router,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde_json::json;

use crate::state::SharedState;

mod admin;
mod auth;
mod events;
mod hosts;
mod metrics;
mod stream;
mod targets;
mod tokens;

pub use hosts::action_by_name as hosts_action;

#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub message: String,
}

impl ApiError {
    pub fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }
    pub fn not_found(what: &str) -> Self {
        Self::new(StatusCode::NOT_FOUND, format!("{what} not found"))
    }
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, msg)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({"error": self.message}))).into_response()
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        tracing::error!("internal error: {e:#}");
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal error")
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        anyhow::Error::from(e).into()
    }
}

pub type ApiResult<T> = Result<T, ApiError>;

pub fn router() -> Router<SharedState> {
    Router::new()
        .route("/auth/me", get(auth::me))
        .route("/auth/setup", post(auth::setup))
        .route("/auth/login", post(auth::login))
        .route("/auth/logout", post(auth::logout))
        .route("/auth/oidc/login", get(crate::oidc::login))
        .route("/auth/oidc/callback", get(crate::oidc::callback))
        .route("/overview", get(hosts::overview))
        .route("/hosts", get(hosts::list))
        .route(
            "/hosts/{id}",
            get(hosts::detail)
                .patch(hosts::update)
                .delete(hosts::remove),
        )
        .route("/hosts/{id}/test", post(hosts::test))
        .route("/hosts/{id}/action", post(hosts::bulk_action))
        .route("/hosts/{id}/services/{name}", get(hosts::service))
        .route("/hosts/{id}/services/{name}/action", post(hosts::action))
        .route("/metrics", get(metrics::query))
        .route("/events", get(events::list))
        .route("/events/kinds", get(events::kinds))
        .route("/events/ack", post(events::ack_many))
        .route("/events/{id}/ack", post(events::ack))
        .route("/stream", get(stream::stream))
        .route("/users", get(admin::users).post(admin::create_user))
        .route("/users/me", axum::routing::patch(admin::update_me))
        .route("/push/key", get(crate::push::key))
        .route(
            "/push/subscriptions",
            get(crate::push::list).post(crate::push::subscribe),
        )
        .route(
            "/push/subscriptions/{id}",
            axum::routing::delete(crate::push::unsubscribe),
        )
        .route("/push/test", post(crate::push::test))
        .route("/tokens", get(tokens::list).post(tokens::create))
        .route("/tokens/{id}", axum::routing::delete(tokens::revoke))
        .route(
            "/users/{id}",
            axum::routing::patch(admin::update_user).delete(admin::delete_user),
        )
        .route(
            "/channels",
            get(admin::channels).post(admin::create_channel),
        )
        .route(
            "/channels/{id}",
            axum::routing::patch(admin::update_channel).delete(admin::delete_channel),
        )
        .route("/channels/{id}/test", post(admin::test_channel))
        .route("/targets", get(targets::list).post(targets::create))
        .route("/targets/test", post(targets::test))
        .route(
            "/targets/{id}",
            axum::routing::patch(targets::update).delete(targets::remove),
        )
        .route("/targets/{id}/poll", post(targets::poll_now))
        .route(
            "/settings",
            get(admin::settings).patch(admin::update_settings),
        )
        .fallback(|| async { ApiError::not_found("endpoint") })
}
