use axum::{
    extract::{
        connect_info::ConnectInfo,
        State,
    },
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Json, Response},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::auth::manager::{AuthManager, HttpHeaders};

#[derive(Clone)]
pub struct AuthRouteState {
    pub auth_manager: Arc<Mutex<AuthManager>>,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct TokenRequest {
    pub token: String,
}

#[derive(Debug, Deserialize)]
pub struct PasswordRequest {
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct AuthStatusResponse {
    pub authenticated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password_user_provided: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub ok: bool,
}

pub fn auth_routes(state: AuthRouteState) -> Router {
    Router::new()
        .route("/api/auth/status", get(get_auth_status))
        .route("/api/auth/login", post(post_login))
        .route("/api/auth/logout", post(post_logout))
        .route("/api/auth/token", post(post_token))
        .route("/api/auth/password", post(post_password))
        .with_state(state)
}

async fn get_auth_status(
    State(state): State<AuthRouteState>,
    headers: HeaderMap,
) -> Json<AuthStatusResponse> {
    let mut auth_mgr = state.auth_manager.lock().await;

    // Check session cookie
    let cookie = headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let session = auth_mgr.get_session_from_headers(&HttpHeaders { cookie });

    if let Some(_session) = session {
        let status = auth_mgr.get_status();
        Json(AuthStatusResponse {
            authenticated: true,
            username: Some(status.username),
            password_user_provided: Some(status.password_user_provided),
        })
    } else {
        Json(AuthStatusResponse {
            authenticated: false,
            username: None,
            password_user_provided: None,
        })
    }
}

fn build_cookie(name: &str, value: &str, is_secure: bool) -> String {
    let mut base = format!("{}={}; HttpOnly; SameSite=Lax; Path=/", name, value);
    if is_secure {
        base.push_str("; Secure");
    }
    base
}

async fn post_login(
    State(state): State<AuthRouteState>,
    headers: HeaderMap,
    Json(body): Json<LoginRequest>,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let is_secure = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .map(|v| v == "https")
        .unwrap_or(false);

    let (cookie_name, session_id) = {
        let mut auth_mgr = state.auth_manager.lock().await;
        let ok = auth_mgr.validate_login(&body.username, &body.password);
        if !ok {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "Invalid credentials"})),
            ));
        }
        let session = auth_mgr.create_session(&body.username);
        let cookie_name = auth_mgr.get_cookie_name().to_string();
        (cookie_name, session.id)
    };

    let mut response = Json(serde_json::json!({"ok": true})).into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&build_cookie(&cookie_name, &session_id, is_secure)).unwrap(),
    );
    Ok(response)
}

async fn post_logout(
    State(state): State<AuthRouteState>,
) -> Response {
    let cookie_name = state.auth_manager.lock().await.get_cookie_name().to_string();
    let mut response = Json(serde_json::json!({"ok": true})).into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&format!(
            "{}=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0",
            cookie_name
        )).unwrap(),
    );
    response
}

async fn post_token(
    State(state): State<AuthRouteState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<TokenRequest>,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let (enabled, is_loopback) = {
        let auth_mgr = state.auth_manager.lock().await;
        let enabled = auth_mgr.is_token_bootstrap_enabled();
        // Check actual TCP socket address first, fall back to headers
        let is_from_loopback = addr.ip().is_loopback()
            || auth_mgr.is_loopback_request(
                headers
                    .get("x-forwarded-for")
                    .or_else(|| headers.get("remote-addr"))
                    .and_then(|v| v.to_str().ok()),
            );
        (enabled, is_from_loopback)
    };

    if !enabled {
        return Err((StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Not found"}))));
    }

    if !is_loopback {
        return Err((StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Not found"}))));
    }

    let is_secure = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .map(|v| v == "https")
        .unwrap_or(false);

    let consumed = state.auth_manager.lock().await.consume_bootstrap_token(&body.token);
    if !consumed {
        return Err((StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "Invalid token"}))));
    }

    let (cookie_name, session_id) = {
        let mut auth_mgr = state.auth_manager.lock().await;
        let session = auth_mgr.create_session("admin");
        (auth_mgr.get_cookie_name().to_string(), session.id)
    };

    let mut response = Json(serde_json::json!({"ok": true})).into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&build_cookie(&cookie_name, &session_id, is_secure)).unwrap(),
    );
    Ok(response)
}

async fn post_password(
    State(state): State<AuthRouteState>,
    headers: HeaderMap,
    Json(body): Json<PasswordRequest>,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    // Check authentication
    let is_auth = {
        let cookie_header = headers
            .get(header::COOKIE)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        let mut auth_mgr = state.auth_manager.lock().await;
        auth_mgr
            .get_session_from_headers(&HttpHeaders {
                cookie: cookie_header,
            })
            .is_some()
    };

    if !is_auth {
        return Err((StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "Unauthorized"}))));
    }

    if body.password.len() < 8 {
        return Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "Password must be at least 8 characters"}))));
    }

    let mut auth_mgr = state.auth_manager.lock().await;
    match auth_mgr.set_password(&body.password) {
        Ok(status) => Ok(Json(serde_json::json!({
            "ok": true,
            "username": status.username,
            "passwordUserProvided": status.password_user_provided,
        })).into_response()),
        Err(e) => Err((StatusCode::CONFLICT, Json(serde_json::json!({"error": e})))),
    }
}
