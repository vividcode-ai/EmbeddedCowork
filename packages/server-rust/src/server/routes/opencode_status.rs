use axum::{
    extract::State,
    response::Json,
    routing::get,
    Router,
};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::settings::binaries::BinaryResolver;

#[derive(Clone)]
pub struct OpenCodeStatusRouteState {
    pub binary_resolver: Arc<Mutex<BinaryResolver>>,
}

pub fn opencode_status_routes(state: OpenCodeStatusRouteState) -> Router {
    Router::new()
        .route("/api/opencode/status", get(get_opencode_status))
        .route("/api/opencode/binaries", get(list_binaries))
        .with_state(state)
}

async fn get_opencode_status(
    State(state): State<OpenCodeStatusRouteState>,
) -> Json<serde_json::Value> {
    let resolved = state.binary_resolver.lock().await.resolve_default().await;
    Json(serde_json::json!({
        "available": !resolved.path.is_empty(),
        "version": resolved.version,
        "path": resolved.path,
        "label": resolved.label,
    }))
}

async fn list_binaries(
    State(state): State<OpenCodeStatusRouteState>,
) -> Json<serde_json::Value> {
    let entries = state.binary_resolver.lock().await.list().await;
    let items: Vec<serde_json::Value> = entries
        .into_iter()
        .map(|e| {
            serde_json::json!({
                "path": e.path,
                "version": e.version,
                "last_used": e.last_used,
                "label": e.label,
            })
        })
        .collect();
    Json(serde_json::json!(items))
}
