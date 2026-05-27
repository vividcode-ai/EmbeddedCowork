use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::Json,
    routing::get,
    Router,
};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::settings::service::SettingsService;

#[derive(Clone)]
pub struct SettingsRouteState {
    pub settings: Arc<Mutex<SettingsService>>,
}

#[derive(Debug, Deserialize)]
pub struct SettingsQuery {
    pub paths: Option<String>,
}

pub fn settings_routes(state: SettingsRouteState) -> Router {
    Router::new()
        .route("/api/settings", get(get_settings).put(update_settings))
        .route("/api/settings/paths", get(get_settings_paths))
        .with_state(state)
}

async fn get_settings(
    State(state): State<SettingsRouteState>,
    Query(_query): Query<SettingsQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let settings = state.settings.lock().await;
    let public = settings.to_public();
    Ok(Json(serde_json::to_value(&public).unwrap_or_default()))
}

async fn update_settings(
    State(state): State<SettingsRouteState>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let mut settings = state.settings.lock().await;
    let merged = settings.deep_merge(&body);
    match settings.save(merged) {
        Ok(_) => {
            let public = settings.to_public();
            Ok(Json(serde_json::to_value(&public).unwrap_or_default()))
        }
        Err(e) => Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e})))),
    }
}

async fn get_settings_paths(
    State(state): State<SettingsRouteState>,
) -> Json<serde_json::Value> {
    let settings = state.settings.lock().await;
    Json(serde_json::json!({
        "config": settings.config_path(),
        "data": settings.data_path(),
    }))
}
