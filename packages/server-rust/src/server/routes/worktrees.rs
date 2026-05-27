use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::get,
    Router,
};
use serde::Deserialize;

use crate::api_types::WorktreeDescriptor;

#[derive(Clone)]
pub struct WorktreeRouteState {
    pub worktrees: Arc<Mutex<HashMap<String, WorktreeDescriptor>>>,
}

#[derive(Debug, Deserialize)]
pub struct CreateWorktreeRequest {
    pub path: String,
    pub branch: Option<String>,
    pub commit: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct WorktreeQuery {
    pub workspace_id: Option<String>,
}

pub fn worktrees_routes(state: WorktreeRouteState) -> Router {
    Router::new()
        .route("/api/worktrees", get(list_worktrees).post(create_worktree))
        .route("/api/worktrees/:id", get(get_worktree).delete(delete_worktree))
        .with_state(state)
}

async fn list_worktrees(
    State(state): State<WorktreeRouteState>,
    Query(_query): Query<WorktreeQuery>,
) -> Json<Vec<serde_json::Value>> {
    let worktrees = state.worktrees.lock().await;
    let items: Vec<serde_json::Value> = worktrees
        .values()
        .map(|w| serde_json::to_value(w).unwrap_or_default())
        .collect();
    Json(items)
}

async fn create_worktree(
    State(state): State<WorktreeRouteState>,
    Json(body): Json<CreateWorktreeRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let slug = body
        .path
        .split(|c: char| c == '/' || c == '\\')
        .last()
        .unwrap_or(&body.path)
        .to_string()
        .to_lowercase()
        .replace(' ', "-");

    let descriptor = WorktreeDescriptor {
        slug: slug.clone(),
        directory: body.path,
        kind: crate::api_types::WorktreeKind::Worktree,
        branch: body.branch,
    };

    let mut worktrees = state.worktrees.lock().await;
    worktrees.insert(slug.clone(), descriptor);

    Ok(Json(serde_json::json!({
        "slug": slug,
        "message": "Worktree created",
    })))
}

async fn get_worktree(
    State(state): State<WorktreeRouteState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let worktrees = state.worktrees.lock().await;
    match worktrees.get(&id) {
        Some(w) => Ok(Json(serde_json::to_value(w).unwrap_or_default())),
        None => Err((StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Worktree not found"})))),
    }
}

async fn delete_worktree(
    State(state): State<WorktreeRouteState>,
    Path(id): Path<String>,
) -> StatusCode {
    let mut worktrees = state.worktrees.lock().await;
    worktrees.remove(&id);
    StatusCode::NO_CONTENT
}
