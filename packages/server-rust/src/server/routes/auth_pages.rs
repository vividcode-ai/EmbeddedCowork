use axum::{
    extract::State,
    http::{header, StatusCode},
    response::{Html, IntoResponse, Redirect},
    routing::get,
    Router,
};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::auth::manager::{AuthManager, HttpHeaders};

#[derive(Clone)]
pub struct AuthPagesRouteState {
    pub auth_manager: Arc<Mutex<AuthManager>>,
}

const LOGIN_PAGE: &str = include_str!("auth-pages/login.html");
const TOKEN_PAGE: &str = include_str!("auth-pages/token.html");

pub fn auth_pages_routes(state: AuthPagesRouteState) -> Router {
    Router::new()
        .route("/login", get(login_page))
        .route("/auth/token", get(token_page))
        .with_state(state)
}

async fn login_page(
    State(state): State<AuthPagesRouteState>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    // If session exists, redirect to /
    let has_session = {
        let cookie = headers
            .get(header::COOKIE)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        state.auth_manager.lock().await
            .get_session_from_headers(&HttpHeaders { cookie })
            .is_some()
    };

    if has_session {
        return Redirect::to("/").into_response();
    }

    (StatusCode::OK, [("content-type", "text/html; charset=utf-8")], Html(LOGIN_PAGE)).into_response()
}

async fn token_page(
    State(state): State<AuthPagesRouteState>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    let (enabled, is_loopback) = {
        let auth_mgr = state.auth_manager.lock().await;
        let enabled = auth_mgr.is_token_bootstrap_enabled();
        let remote_addr = headers
            .get("x-forwarded-for")
            .or_else(|| headers.get("remote-addr"))
            .and_then(|v| v.to_str().ok());
        let is_loopback = auth_mgr.is_loopback_request(remote_addr);
        (enabled, is_loopback)
    };

    if !enabled || !is_loopback {
        return (StatusCode::NOT_FOUND, [("content-type", "text/plain; charset=utf-8")], "Not found").into_response();
    }

    (StatusCode::OK, [("content-type", "text/html; charset=utf-8")], Html(TOKEN_PAGE)).into_response()
}
