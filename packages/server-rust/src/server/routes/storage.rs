use std::sync::Arc;
use tokio::sync::Mutex;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use serde::Deserialize;

use crate::events::bus::EventBus;
use crate::settings::service::{DocKind, SettingsService};
use crate::storage::instance_store::InstanceStore;

#[derive(Clone)]
pub struct StorageRouteState {
    pub instance_store: Arc<Mutex<InstanceStore>>,
    pub settings: Arc<Mutex<SettingsService>>,
    pub event_bus: EventBus,
}

#[derive(Debug, Deserialize)]
pub struct ValidateBinaryRequest {
    pub path: String,
}

pub fn storage_routes(state: StorageRouteState) -> Router {
    Router::new()
        // Config (full document)
        .route("/api/storage/config", get(get_config).patch(patch_config))
        // Config owner
        .route("/api/storage/config/:owner", get(get_config_owner).patch(patch_config_owner))
        // State (full document)
        .route("/api/storage/state", get(get_state).patch(patch_state))
        // State owner
        .route("/api/storage/state/:owner", get(get_state_owner).patch(patch_state_owner))
        // Instances
        .route("/api/storage/instances/:id", get(get_instance).put(put_instance).delete(delete_instance))
        // Binary validation
        .route("/api/storage/binaries/validate", post(validate_binary))
        .with_state(state)
}

async fn get_config(
    State(state): State<StorageRouteState>,
) -> Json<serde_json::Value> {
    let mut settings = state.settings.lock().await;
    let doc = settings.get_doc(&DocKind::Config);
    Json(doc)
}

async fn patch_config(
    State(state): State<StorageRouteState>,
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let mut settings = state.settings.lock().await;
    let updated = settings.merge_patch_doc(&DocKind::Config, &body);
    Json(updated)
}

async fn get_state(
    State(state): State<StorageRouteState>,
) -> Json<serde_json::Value> {
    let mut settings = state.settings.lock().await;
    let doc = settings.get_doc(&DocKind::State);
    Json(doc)
}

async fn patch_state(
    State(state): State<StorageRouteState>,
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let mut settings = state.settings.lock().await;
    let updated = settings.merge_patch_doc(&DocKind::State, &body);
    Json(updated)
}

async fn get_config_owner(
    State(state): State<StorageRouteState>,
    Path(owner): Path<String>,
) -> Json<serde_json::Value> {
    let mut settings = state.settings.lock().await;
    let doc = settings.get_owner(&DocKind::Config, &owner);
    Json(doc)
}

async fn patch_config_owner(
    State(state): State<StorageRouteState>,
    Path(owner): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let mut settings = state.settings.lock().await;
    let updated = settings.merge_patch_owner(&DocKind::Config, &owner, &body);
    Ok(Json(updated))
}

async fn get_state_owner(
    State(state): State<StorageRouteState>,
    Path(owner): Path<String>,
) -> Json<serde_json::Value> {
    let mut settings = state.settings.lock().await;
    let doc = settings.get_owner(&DocKind::State, &owner);
    Json(doc)
}

async fn patch_state_owner(
    State(state): State<StorageRouteState>,
    Path(owner): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let mut settings = state.settings.lock().await;
    let updated = settings.merge_patch_owner(&DocKind::State, &owner, &body);
    Ok(Json(updated))
}

async fn get_instance(
    State(state): State<StorageRouteState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    match state.instance_store.lock().await.read(&id).await {
        Ok(data) => Ok(Json(serde_json::to_value(&data).unwrap_or_default())),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()})))),
    }
}

async fn put_instance(
    State(state): State<StorageRouteState>,
    Path(id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    let data: crate::api_types::InstanceData = match serde_json::from_value(body) {
        Ok(d) => d,
        Err(e) => return Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e.to_string()})))),
    };
    match state.instance_store.lock().await.write(&id, &data).await {
        Ok(_) => {
            state.event_bus.publish(crate::api_types::WorkspaceEventPayload::InstanceDataChanged {
                event_type: "instance.dataChanged".to_string(),
                instance_id: id.clone(),
                data: data.clone(),
            });
            Ok(StatusCode::NO_CONTENT)
        }
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()})))),
    }
}

async fn delete_instance(
    State(state): State<StorageRouteState>,
    Path(id): Path<String>,
) -> StatusCode {
    state.instance_store.lock().await.delete(&id).await.ok();
    state.event_bus.publish(crate::api_types::WorkspaceEventPayload::InstanceDataChanged {
        event_type: "instance.dataChanged".to_string(),
        instance_id: id,
        data: crate::api_types::InstanceData {
            message_history: Vec::new(),
            agent_model_selections: std::collections::HashMap::new(),
        },
    });
    StatusCode::NO_CONTENT
}

async fn validate_binary(
    State(_state): State<StorageRouteState>,
    Json(body): Json<ValidateBinaryRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    // If path is not absolute, try to resolve via PATH lookup first
    let resolved_path = if std::path::Path::new(&body.path).is_absolute() {
        body.path.clone()
    } else {
        let cmd = if cfg!(windows) { "where" } else { "which" };
        std::process::Command::new(cmd)
            .arg(&body.path)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| {
                let out = String::from_utf8_lossy(&o.stdout);
                out.lines().next().map(|s| s.trim().to_string())
            })
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| body.path.clone())
    };

    let path = std::path::Path::new(&resolved_path);

    if !path.exists() {
        return Ok(Json(serde_json::json!({
            "valid": false,
            "error": "Binary not found at path",
        })));
    }

    // Try to get version by running `{path} --version`
    let version = tokio::process::Command::new(&resolved_path)
        .arg("--version")
        .output()
        .await
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if s.is_empty() { None } else { Some(s) }
        });

    Ok(Json(serde_json::json!({
        "valid": true,
        "version": version,
    })))
}
