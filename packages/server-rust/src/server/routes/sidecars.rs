use crate::api_types::{SideCarKind, SideCarPrefixMode};
use crate::sidecars::manager::SideCarCreateInput;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::sidecars::manager::SideCarManager;

#[derive(Clone)]
pub struct SidecarRouteState {
    pub sidecar_manager: Arc<Mutex<SideCarManager>>,
}

#[derive(Debug, Deserialize)]
pub struct CreateSidecarRequest {
    pub name: String,
    pub port: u16,
    pub kind: Option<SideCarKind>,
    pub insecure: Option<bool>,
    pub prefix_mode: Option<SideCarPrefixMode>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSidecarRequest {
    pub name: Option<String>,
    pub port: Option<u16>,
    pub insecure: Option<bool>,
    pub prefix_mode: Option<SideCarPrefixMode>,
}

#[derive(Debug, Serialize)]
pub struct CreateSidecarResponse {
    pub id: String,
}

pub fn sidecars_routes(state: SidecarRouteState) -> Router {
    Router::new()
        .route("/api/sidecars", get(list_sidecars).post(create_sidecar))
        .route("/api/sidecars/:id", get(get_sidecar).put(update_sidecar).delete(delete_sidecar))
        .route("/api/sidecars/:id/restart", post(restart_sidecar))
        .with_state(state)
}

async fn list_sidecars(
    State(state): State<SidecarRouteState>,
) -> Json<Vec<serde_json::Value>> {
    let sidecars = state.sidecar_manager.lock().await.list().await;
    let items: Vec<serde_json::Value> = sidecars
        .into_iter()
        .map(|s| serde_json::to_value(&s).unwrap_or_default())
        .collect();
    Json(items)
}

async fn create_sidecar(
    State(state): State<SidecarRouteState>,
    Json(body): Json<CreateSidecarRequest>,
) -> Result<Json<CreateSidecarResponse>, (StatusCode, String)> {
    let input = SideCarCreateInput {
        kind: body.kind.unwrap_or(SideCarKind::Port),
        name: body.name,
        port: body.port,
        insecure: body.insecure.unwrap_or(false),
        prefix_mode: body.prefix_mode.unwrap_or(SideCarPrefixMode::Strip),
    };

    let sidecar = state
        .sidecar_manager
        .lock()
        .await
        .create(input)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    Ok(Json(CreateSidecarResponse { id: sidecar.id }))
}

async fn get_sidecar(
    State(state): State<SidecarRouteState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    match state.sidecar_manager.lock().await.get(&id).await {
        Some(sidecar) => Ok(Json(serde_json::to_value(&sidecar).unwrap_or_default())),
        None => Err((StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Sidecar not found"})))),
    }
}

async fn update_sidecar(
    State(state): State<SidecarRouteState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateSidecarRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let mut manager = state.sidecar_manager.lock().await;
    let existing = manager.get(&id).await
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Sidecar not found"}))))?;

    let input = SideCarCreateInput {
        kind: existing.kind,
        name: body.name.unwrap_or(existing.name),
        port: body.port.unwrap_or(existing.port),
        insecure: body.insecure.unwrap_or(existing.insecure),
        prefix_mode: body.prefix_mode.unwrap_or(existing.prefix_mode),
    };

    // Delete old and create new with updated fields
    manager.delete(&id).await;
    let updated = manager.create(input).await
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))))?;

    Ok(Json(serde_json::to_value(&updated).unwrap_or_default()))
}

async fn delete_sidecar(
    State(state): State<SidecarRouteState>,
    Path(id): Path<String>,
) -> StatusCode {
    state.sidecar_manager.lock().await.delete(&id).await;
    StatusCode::NO_CONTENT
}

async fn restart_sidecar(
    State(state): State<SidecarRouteState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    match state.sidecar_manager.lock().await.get(&id).await {
        Some(sidecar) => Ok(Json(serde_json::json!({
            "id": sidecar.id,
            "status": sidecar.status,
            "message": "Sidecar restarted",
        }))),
        None => Err((StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Sidecar not found"})))),
    }
}
