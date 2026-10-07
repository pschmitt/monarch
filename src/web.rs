//! Serves the embedded single page application.

use axum::{
    http::{HeaderValue, StatusCode, Uri, header},
    response::{IntoResponse, Response},
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "web/dist"]
struct Assets;

fn asset(path: &str) -> Option<Response> {
    let file = Assets::get(path)?;
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    let cache = if path.starts_with("assets/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    let mut res = (StatusCode::OK, file.data.into_owned()).into_response();
    let h = res.headers_mut();
    if let Ok(v) = HeaderValue::from_str(mime.as_ref()) {
        h.insert(header::CONTENT_TYPE, v);
    }
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    Some(res)
}

pub async fn spa(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if !path.is_empty()
        && let Some(res) = asset(path)
    {
        return res;
    }
    // Unknown files with an extension are real 404s; everything else is a client route.
    let last = path.rsplit('/').next().unwrap_or("");
    if last.contains('.') && !last.ends_with(".csp") {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    }
    asset("index.html").unwrap_or_else(|| (StatusCode::NOT_FOUND, "web UI not built").into_response())
}
