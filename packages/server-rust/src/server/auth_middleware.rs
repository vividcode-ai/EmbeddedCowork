use std::sync::Arc;
use tokio::sync::Mutex;

use axum::{
    body::Body,
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::auth::http_auth::wants_html;
use crate::auth::manager::{AuthManager, HttpHeaders};
use crate::workspaces::manager::WorkspaceManager;

pub struct AuthMiddlewareState {
    pub auth_manager: Arc<Mutex<AuthManager>>,
    pub workspace_manager: Arc<Mutex<WorkspaceManager>>,
}

pub async fn auth_middleware(
    State(state): State<Arc<AuthMiddlewareState>>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let path = request.uri().path();

    // Public API paths
    let public_api = [
        "/api/auth/login",
        "/api/auth/token",
        "/api/auth/status",
        "/api/auth/logout",
    ];
    if public_api.contains(&path) {
        return Ok(next.run(request).await);
    }

    // Check if this path needs auth
    let needs_auth = path.starts_with("/api/")
        || path.starts_with("/workspaces/")
        || path.starts_with("/sidecars/");
    if !needs_auth {
        return Ok(next.run(request).await);
    }

    // Check session cookie
    let cookie_header = request
        .headers()
        .get("cookie")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let is_authenticated = {
        let mut auth_mgr = state.auth_manager.lock().await;
        auth_mgr
            .get_session_from_headers(&HttpHeaders {
                cookie: cookie_header,
            })
            .is_some()
    };

    if is_authenticated {
        return Ok(next.run(request).await);
    }

    // Allow OpenCode plugin -> EmbeddedCowork calls with per-instance basic auth
    let plugin_match = path.strip_prefix("/workspaces/")
        .and_then(|rest| rest.split_once('/'))
        .and_then(|(workspace_id, suffix)| {
            if suffix == "plugin" || suffix.starts_with("plugin/") {
                Some(workspace_id.to_string())
            } else {
                None
            }
        });

    if let Some(workspace_id) = plugin_match {
        let expected = state.workspace_manager.lock().await
            .get_instance_authorization_header(&workspace_id)
            .await;
        let provided = request
            .headers()
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        if let (Some(expected), Some(provided)) = (expected, provided) {
            if provided == expected {
                return Ok(next.run(request).await);
            }
        }
    }

    // Loopback remote proxy DELETE bypass
    if request.method() == "DELETE"
        && path.starts_with("/api/remote-proxy/sessions/")
    {
        let is_loopback = request
            .headers()
            .get("x-forwarded-for")
            .or_else(|| request.headers().get("remote-addr"))
            .and_then(|v| v.to_str().ok())
            .map(|addr| {
                addr == "127.0.0.1"
                    || addr == "::1"
                    || addr == "::ffff:127.0.0.1"
            })
            .unwrap_or(false);

        if is_loopback {
            return Ok(next.run(request).await);
        }
    }

    // Not authenticated
    let accept = request
        .headers()
        .get("accept")
        .and_then(|v| v.to_str().ok());

    if wants_html(accept) {
        let response = (
            StatusCode::FOUND,
            [("Location", "/login")],
            Body::empty(),
        );
        Ok(response.into_response())
    } else {
        let response = (
            StatusCode::UNAUTHORIZED,
            [("content-type", "application/json")],
            Body::from(r#"{"error":"Unauthorized"}"#),
        );
        Ok(response.into_response())
    }
}
