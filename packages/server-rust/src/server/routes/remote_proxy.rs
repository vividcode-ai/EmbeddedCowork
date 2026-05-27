use axum::{
    body::Body,
    extract::{Path, Request, State},
    http::StatusCode,
    response::{Json, Response},
    routing::get,
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::server::remote_proxy::RemoteProxyManager;

#[derive(Clone)]
pub struct RemoteProxyRouteState {
    pub proxy_manager: Arc<Mutex<RemoteProxyManager>>,
}

#[derive(Debug, Deserialize)]
pub struct CreateSessionRequest {
    pub target_url: String,
    pub allowed_origins: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
pub struct CreateSessionResponse {
    pub session_id: String,
    pub proxy_url: String,
}

pub fn remote_proxy_routes(state: RemoteProxyRouteState) -> Router {
    Router::new()
        .route("/api/remote-proxy/sessions", get(list_sessions).post(create_session))
        .route("/api/remote-proxy/sessions/:id", get(get_session).delete(delete_session))
        .nest(
            "/api/proxy",
            Router::new().fallback(proxy_fallback),
        )
        .with_state(state)
}

async fn proxy_fallback(
    State(state): State<RemoteProxyRouteState>,
    req: Request,
) -> Result<Response<Body>, StatusCode> {
    // Extract session ID and remaining path from URI: /api/proxy/{id}/{*path}
    let uri_path = req.uri().path();
    let rest = uri_path
        .strip_prefix("/api/proxy/")
        .unwrap_or("")
        .trim_start_matches('/');

    let (id, path) = match rest.split_once('/') {
        Some((id, path)) => (id.to_string(), path.to_string()),
        None => (rest.to_string(), String::new()),
    };

    if id.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let method = req.method().clone();
    let headers = req.headers().clone();
    let body_bytes = axum::body::to_bytes(req.into_body(), usize::MAX)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    state
        .proxy_manager
        .lock()
        .await
        .forward(&id, method, &path, &headers, body_bytes.to_vec())
        .await
}

async fn list_sessions(
    State(state): State<RemoteProxyRouteState>,
) -> Json<Vec<serde_json::Value>> {
    let sessions = state.proxy_manager.lock().await.list().await;
    let items: Vec<serde_json::Value> = sessions
        .into_iter()
        .map(|s| {
            serde_json::json!({
                "id": s.id,
                "target_url": s.target_url,
                "proxy_url": s.proxy_url,
                "allowed_origins": s.allowed_origins,
                "created_at": s.created_at,
            })
        })
        .collect();
    Json(items)
}

async fn create_session(
    State(state): State<RemoteProxyRouteState>,
    Json(body): Json<CreateSessionRequest>,
) -> Result<Json<CreateSessionResponse>, (StatusCode, String)> {
    let allowed = body.allowed_origins.unwrap_or_default();
    let session = state
        .proxy_manager
        .lock()
        .await
        .create(&body.target_url, allowed)
        .await;

    Ok(Json(CreateSessionResponse {
        session_id: session.id,
        proxy_url: session.proxy_url,
    }))
}

async fn get_session(
    State(state): State<RemoteProxyRouteState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    match state.proxy_manager.lock().await.get(&id).await {
        Some(s) => Ok(Json(serde_json::json!({
            "id": s.id,
            "target_url": s.target_url,
            "proxy_url": s.proxy_url,
            "allowed_origins": s.allowed_origins,
            "created_at": s.created_at,
        }))),
        None => Err((StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Session not found"})))),
    }
}

async fn delete_session(
    State(state): State<RemoteProxyRouteState>,
    Path(id): Path<String>,
) -> StatusCode {
    state.proxy_manager.lock().await.delete(&id).await;
    StatusCode::NO_CONTENT
}

