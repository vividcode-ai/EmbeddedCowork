use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::get,
    Router,
};
use serde::Deserialize;

use crate::api_types::RemoteServerProfile;

#[derive(Clone)]
pub struct RemoteServerRouteState {
    pub servers: Arc<Mutex<HashMap<String, RemoteServerProfile>>>,
}

#[derive(Debug, Deserialize)]
pub struct CreateRemoteServerRequest {
    pub name: String,
    pub url: String,
    pub api_key: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ProbeRequest {
    pub base_url: String,
    pub skip_tls_verify: Option<bool>,
}

pub fn remote_servers_routes(state: RemoteServerRouteState) -> Router {
    Router::new()
        .route("/api/remote-servers", get(list_remote_servers).post(add_remote_server))
        .route("/api/remote-servers/probe", axum::routing::post(probe_remote_server))
        .route("/api/remote-servers/:id", get(get_remote_server).put(update_remote_server).delete(remove_remote_server))
        .with_state(state)
}

async fn probe_remote_server(
    Json(body): Json<ProbeRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let base_url = body.base_url.trim_end_matches('/').to_string();
    if !base_url.starts_with("http://") && !base_url.starts_with("https://") {
        return Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "Invalid URL"}))));
    }

    let skip_tls = body.skip_tls_verify.unwrap_or(false);
    let status_url = format!("{}/api/auth/status", base_url);

    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(skip_tls)
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))))?;

    let resp = match client.get(&status_url).send().await {
        Ok(r) => r,
        Err(e) => {
            let error_code = if e.is_timeout() { "timeout" }
                else if e.is_connect() { "connection_refused" }
                else { "probe_failed" };

            return Ok(Json(serde_json::json!({
                "ok": false,
                "reachable": false,
                "normalizedUrl": base_url,
                "skipTlsVerify": skip_tls,
                "requiresAuth": false,
                "authenticated": false,
                "error": e.to_string(),
                "errorCode": error_code,
            })));
        }
    };

    let status = resp.status();
    let body_text = resp.text().await.unwrap_or_default();
    let parsed: serde_json::Value = serde_json::from_str(&body_text).unwrap_or_default();
    let authenticated = parsed.get("authenticated").and_then(|v| v.as_bool()).unwrap_or(false);

    Ok(Json(serde_json::json!({
        "ok": status.is_success(),
        "reachable": true,
        "normalizedUrl": base_url,
        "skipTlsVerify": skip_tls,
        "requiresAuth": true,
        "authenticated": authenticated,
        "error": if status.is_success() { serde_json::Value::Null } else { serde_json::json!(status.to_string()) },
        "errorCode": if status.is_success() { serde_json::Value::Null } else { serde_json::json!("http_error") },
    })))
}

async fn list_remote_servers(
    State(state): State<RemoteServerRouteState>,
) -> Json<Vec<serde_json::Value>> {
    let servers = state.servers.lock().await;
    let items: Vec<serde_json::Value> = servers
        .values()
        .map(|s| serde_json::to_value(s).unwrap_or_default())
        .collect();
    Json(items)
}

async fn add_remote_server(
    State(state): State<RemoteServerRouteState>,
    Json(body): Json<CreateRemoteServerRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let profile = RemoteServerProfile {
        id: id.clone(),
        name: body.name,
        base_url: body.url,
        skip_tls_verify: false,
        created_at: now.clone(),
        updated_at: now,
        last_connected_at: None,
    };

    let mut servers = state.servers.lock().await;
    servers.insert(id.clone(), profile);

    Ok(Json(serde_json::json!({
        "id": id,
        "message": "Remote server created",
    })))
}

async fn get_remote_server(
    State(state): State<RemoteServerRouteState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let servers = state.servers.lock().await;
    match servers.get(&id) {
        Some(profile) => Ok(Json(serde_json::to_value(profile).unwrap_or_default())),
        None => Err((StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Remote server not found"})))),
    }
}

async fn update_remote_server(
    State(state): State<RemoteServerRouteState>,
    Path(id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let mut servers = state.servers.lock().await;
    match servers.get_mut(&id) {
        Some(profile) => {
            if let Some(name) = body.get("name").and_then(|v| v.as_str()) {
                profile.name = name.to_string();
            }
            if let Some(url) = body.get("url").and_then(|v| v.as_str()) {
                profile.base_url = url.to_string();
            }
            if let Some(skip) = body.get("skip_tls_verify").and_then(|v| v.as_bool()) {
                profile.skip_tls_verify = skip;
            }
            profile.updated_at = chrono::Utc::now().to_rfc3339();
            Ok(Json(serde_json::to_value(profile).unwrap_or_default()))
        }
        None => Err((StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Remote server not found"})))),
    }
}

async fn remove_remote_server(
    State(state): State<RemoteServerRouteState>,
    Path(id): Path<String>,
) -> StatusCode {
    let mut servers = state.servers.lock().await;
    servers.remove(&id);
    StatusCode::NO_CONTENT
}
