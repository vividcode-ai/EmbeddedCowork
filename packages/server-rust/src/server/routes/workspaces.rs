use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post, delete},
    Router,
};
use serde::{Deserialize, Serialize};

use crate::api_types::WorktreeDescriptor;
use crate::workspaces::manager::WorkspaceManager;

#[derive(Clone)]
pub struct WorkspaceRouteState {
    pub workspace_manager: Arc<Mutex<WorkspaceManager>>,
    pub worktree_map: Arc<Mutex<HashMap<String, WorktreeMap>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeMap {
    pub version: u32,
    pub default_worktree_slug: String,
    #[serde(default)]
    pub parent_session_worktree_slug: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateWorkspaceRequest {
    pub path: String,
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct WorkspaceFilesQuery {
    pub path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct WorkspaceFileSearchQuery {
    pub q: Option<String>,
    pub limit: Option<u32>,
    pub r#type: Option<String>,
    pub refresh: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct CreateWorktreeRequest {
    pub slug: String,
    pub branch: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct WorktreeGitDiffQuery {
    pub path: String,
    pub scope: String,
    #[serde(default)]
    pub original_path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct WorktreeGitPathsRequest {
    pub paths: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct WorktreeGitCommitRequest {
    pub message: String,
}

pub fn workspace_routes(state: WorkspaceRouteState) -> Router {
    Router::new()
        // Workspace CRUD
        .route("/api/workspaces", get(list_workspaces).post(create_workspace))
        .route("/api/workspaces/:id", get(get_workspace).delete(delete_workspace))
        // Workspace files
        .route("/api/workspaces/:id/files", get(list_workspace_files))
        .route("/api/workspaces/:id/files/search", get(search_workspace_files))
        .route("/api/workspaces/:id/files/content", get(read_workspace_file).put(write_workspace_file))
        // Worktrees
        .route("/api/workspaces/:id/worktrees", get(list_worktrees).post(create_worktree))
        .route("/api/workspaces/:id/worktrees/map", get(get_worktree_map).put(put_worktree_map))
        .route("/api/workspaces/:id/worktrees/:slug", delete(delete_worktree))
        // Git operations
        .route("/api/workspaces/:id/worktrees/:slug/git-status", get(git_status))
        .route("/api/workspaces/:id/worktrees/:slug/git-diff", get(git_diff))
        .route("/api/workspaces/:id/worktrees/:slug/git-stage", post(git_stage))
        .route("/api/workspaces/:id/worktrees/:slug/git-unstage", post(git_unstage))
        .route("/api/workspaces/:id/worktrees/:slug/git-commit", post(git_commit))
        .with_state(state)
}

// ============ Workspace CRUD ============

async fn list_workspaces(
    State(state): State<WorkspaceRouteState>,
) -> Json<Vec<serde_json::Value>> {
    let workspaces = state.workspace_manager.lock().await.list().await;
    Json(workspaces.into_iter().map(|w| serde_json::to_value(&w).unwrap_or_default()).collect())
}

async fn create_workspace(
    State(state): State<WorkspaceRouteState>,
    Json(body): Json<CreateWorkspaceRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let descriptor = state
        .workspace_manager
        .lock()
        .await
        .create(&body.path, body.name)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    Ok(Json(serde_json::to_value(&descriptor).unwrap_or_default()))
}

async fn get_workspace(
    State(state): State<WorkspaceRouteState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    match state.workspace_manager.lock().await.get(&id).await {
        Some(ws) => Ok(Json(serde_json::to_value(&ws).unwrap_or_default())),
        None => Err((StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Workspace not found"})))),
    }
}

async fn delete_workspace(
    State(state): State<WorkspaceRouteState>,
    Path(id): Path<String>,
) -> StatusCode {
    state.workspace_manager.lock().await.delete(&id).await;
    // Also clean up worktree map
    state.worktree_map.lock().await.remove(&id);
    StatusCode::NO_CONTENT
}

// ============ Workspace Files ============

async fn list_workspace_files(
    State(state): State<WorkspaceRouteState>,
    Path(id): Path<String>,
    Query(query): Query<WorkspaceFilesQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let path = query.path.unwrap_or_else(|| ".".to_string());
    match state.workspace_manager.lock().await.list_files(&id, &path).await {
        Ok(files) => Ok(Json(serde_json::to_value(&files).unwrap_or_default())),
        Err(e) => Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e})))),
    }
}

async fn read_workspace_file(
    State(state): State<WorkspaceRouteState>,
    Path(id): Path<String>,
    Query(query): Query<WorkspaceFilesQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let path = query.path.unwrap_or_default();
    match state.workspace_manager.lock().await.read_file(&id, &path).await {
        Ok(content) => Ok(Json(serde_json::to_value(&content).unwrap_or_default())),
        Err(e) => Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e})))),
    }
}

async fn write_workspace_file(
    State(state): State<WorkspaceRouteState>,
    Path(id): Path<String>,
    Query(query): Query<WorkspaceFilesQuery>,
    Json(body): Json<serde_json::Value>,
) -> Result<StatusCode, (StatusCode, String)> {
    let path = query.path.unwrap_or_default();
    let contents = body.get("contents").and_then(|v| v.as_str()).unwrap_or("");
    state
        .workspace_manager
        .lock()
        .await
        .write_file(&id, &path, contents)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    Ok(StatusCode::NO_CONTENT)
}

async fn search_workspace_files(
    State(state): State<WorkspaceRouteState>,
    Path(id): Path<String>,
    Query(query): Query<WorkspaceFileSearchQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ws = state.workspace_manager.lock().await.get(&id).await;
    let workspace_path = match ws {
        Some(ref w) => w.path.clone(),
        None => return Err((StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Workspace not found"})))),
    };

    let query_text = query.q.unwrap_or_default().trim().to_string();
    if query_text.is_empty() {
        return Ok(Json(serde_json::json!([])));
    }

    let limit = query.limit.unwrap_or(50) as usize;
    let file_type_filter = query.r#type.as_deref().unwrap_or("all");

    let mut results = Vec::new();
    let mut stack = vec![std::path::PathBuf::from(&workspace_path)];

    while let Some(dir) = stack.pop() {
        if results.len() >= limit {
            break;
        }
        let mut entries = match tokio::fs::read_dir(&dir).await {
            Ok(e) => e,
            Err(_) => continue,
        };
        loop {
            if results.len() >= limit {
                break;
            }
            let entry = match entries.next_entry().await {
                Ok(Some(e)) => e,
                Ok(None) => break,
                Err(_) => continue,
            };

            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with('.') {
                continue;
            }

            let is_dir = entry.file_type().await.map(|ft| ft.is_dir()).unwrap_or(false);
            if file_type_filter == "file" && is_dir { continue; }
            if file_type_filter == "directory" && !is_dir { continue; }

            if name_str.to_lowercase().contains(&query_text.to_lowercase()) {
                let path = entry.path();
                let rel_path = path.strip_prefix(&workspace_path).unwrap_or(&path);
                results.push(serde_json::json!({
                    "name": name_str.as_ref(),
                    "path": rel_path.to_string_lossy(),
                    "type": if is_dir { "directory" } else { "file" },
                }));
            }

            if is_dir {
                stack.push(entry.path());
            }
        }
    }

    Ok(Json(serde_json::json!(results)))
}

// ============ Worktree Operations ============

async fn list_worktrees(
    State(state): State<WorkspaceRouteState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ws = state.workspace_manager.lock().await.get(&id).await;
    let workspace_path = match ws {
        Some(ref w) => w.path.clone(),
        None => return Err((StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Workspace not found"})))),
    };

    // Check if it's a git repo
    let is_git_repo = std::path::Path::new(&workspace_path).join(".git").exists();

    // List worktrees by reading .git/worktrees
    let mut worktrees_list: Vec<serde_json::Value> = Vec::new();
    let worktrees_dir = std::path::Path::new(&workspace_path).join(".git").join("worktrees");
    if worktrees_dir.exists() {
        let mut dir = tokio::fs::read_dir(&worktrees_dir).await.ok();
        if let Some(ref mut d) = dir {
            while let Ok(Some(entry)) = d.next_entry().await {
                if entry.file_type().await.map(|ft| ft.is_dir()).unwrap_or(false) {
                    let slug = entry.file_name().to_string_lossy().to_string();
                    worktrees_list.push(serde_json::json!({
                        "slug": slug,
                        "directory": workspace_path.clone(),
                        "kind": "worktree",
                    }));
                }
            }
        }
    }

    Ok(Json(serde_json::json!({
        "worktrees": worktrees_list,
        "isGitRepo": is_git_repo,
    })))
}

async fn create_worktree(
    State(state): State<WorkspaceRouteState>,
    Path(id): Path<String>,
    Json(body): Json<CreateWorktreeRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let ws = state.workspace_manager.lock().await.get(&id).await;
    let workspace_path = match ws {
        Some(ref w) => w.path.clone(),
        None => return Err((StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Workspace not found"})))),
    };

    let slug = body.slug.trim().to_string();
    if slug.is_empty() || slug == "root" {
        return Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "Invalid slug"}))));
    }

    if !std::path::Path::new(&workspace_path).join(".git").exists() {
        return Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "Not a git repository"}))));
    }

    let worktree_path = std::path::Path::new(&workspace_path).join("..").join(&slug);
    let mut cmd = tokio::process::Command::new("git");
    cmd.args(["worktree", "add"])
        .arg(&worktree_path)
        .current_dir(&workspace_path);

    if let Some(ref branch) = body.branch {
        cmd.arg(branch);
    }

    let output = cmd.output().await.map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()})))
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": stderr}))));
    }

    let descriptor = WorktreeDescriptor {
        slug: slug.clone(),
        directory: worktree_path.to_string_lossy().to_string(),
        kind: crate::api_types::WorktreeKind::Worktree,
        branch: body.branch.clone(),
    };

    Ok((StatusCode::CREATED, Json(serde_json::to_value(&descriptor).unwrap_or_default())))
}

async fn delete_worktree(
    State(state): State<WorkspaceRouteState>,
    Path((id, slug)): Path<(String, String)>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    let ws = state.workspace_manager.lock().await.get(&id).await;
    let workspace_path = match ws {
        Some(ref w) => w.path.clone(),
        None => return Err((StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Workspace not found"})))),
    };

    let force = params.get("force").map(|v| v == "true").unwrap_or(false);

    let mut cmd = tokio::process::Command::new("git");
    cmd.args(["worktree", "remove"])
        .current_dir(&workspace_path);

    if force {
        cmd.arg("--force");
    }
    cmd.arg(&slug);

    let output = cmd.output().await.map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()})))
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": stderr}))));
    }

    Ok(StatusCode::NO_CONTENT)
}

// ============ Worktree Map ============

async fn get_worktree_map(
    State(state): State<WorkspaceRouteState>,
    Path(id): Path<String>,
) -> Result<Json<WorktreeMap>, (StatusCode, Json<serde_json::Value>)> {
    let ws = state.workspace_manager.lock().await.get(&id).await;
    match ws {
        Some(_) => {
            let map = state.worktree_map.lock().await;
            let entry = map.get(&id).cloned().unwrap_or(WorktreeMap {
                version: 1,
                default_worktree_slug: "root".to_string(),
                parent_session_worktree_slug: HashMap::new(),
            });
            Ok(Json(entry))
        }
        None => Err((StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Workspace not found"})))),
    }
}

async fn put_worktree_map(
    State(state): State<WorkspaceRouteState>,
    Path(id): Path<String>,
    Json(body): Json<WorktreeMap>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    let ws = state.workspace_manager.lock().await.get(&id).await;
    match ws {
        Some(_) => {
            state.worktree_map.lock().await.insert(id, body);
            Ok(StatusCode::NO_CONTENT)
        }
        None => Err((StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Workspace not found"})))),
    }
}

// ============ Git Operations ============

async fn git_status(
    State(state): State<WorkspaceRouteState>,
    Path((id, slug)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ws = state.workspace_manager.lock().await.get(&id).await;
    let workspace_path = match ws {
        Some(ref w) => w.path.clone(),
        None => return Err((StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Workspace not found"})))),
    };

    let worktree_dir = if slug == "root" {
        workspace_path.clone()
    } else {
        std::path::Path::new(&workspace_path).join("..").join(&slug).to_string_lossy().to_string()
    };

    let output = tokio::process::Command::new("git")
        .args(["-c", "color.ui=false", "status", "--porcelain", "--ignore-submodules"])
        .current_dir(&worktree_dir)
        .output()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))))?;

    if !output.status.success() {
        return Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "Git status failed"}))));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut entries: Vec<serde_json::Value> = Vec::new();

    for line in stdout.lines() {
        if line.len() < 3 { continue; }
        let staged = &line[0..1];
        let unstaged = &line[1..2];
        let path = &line[3..];

        let staged_status = match staged {
            "M" => "modified",
            "A" => "added",
            "D" => "deleted",
            "R" => "renamed",
            "C" => "copied",
            "?" => "untracked",
            _ => "",
        };
        let unstaged_status = match unstaged {
            "M" => "modified",
            "D" => "deleted",
            "?" => "untracked",
            _ => "",
        };

        entries.push(serde_json::json!({
            "path": path.trim(),
            "stagedStatus": staged_status,
            "unstagedStatus": unstaged_status,
        }));
    }

    Ok(Json(serde_json::json!(entries)))
}

async fn git_diff(
    State(state): State<WorkspaceRouteState>,
    Path((id, slug)): Path<(String, String)>,
    Query(query): Query<WorktreeGitDiffQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ws = state.workspace_manager.lock().await.get(&id).await;
    let workspace_path = match ws {
        Some(ref w) => w.path.clone(),
        None => return Err((StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Workspace not found"})))),
    };

    let worktree_dir = if slug == "root" {
        workspace_path.clone()
    } else {
        std::path::Path::new(&workspace_path).join("..").join(&slug).to_string_lossy().to_string()
    };

    let diff_target = match query.scope.as_str() {
        "staged" => "--staged",
        _ => "",
    };

    let path = if let Some(ref orig) = query.original_path {
        orig.as_str()
    } else {
        query.path.as_str()
    };

    let output = tokio::process::Command::new("git")
        .args(["-c", "color.ui=false", "diff", diff_target, "--", path])
        .current_dir(&worktree_dir)
        .output()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let is_binary = stdout.contains("Binary files") || stdout.contains("-\u{1}");

    Ok(Json(serde_json::json!({
        "path": query.path,
        "scope": query.scope,
        "before": stdout,
        "after": stdout,
        "isBinary": is_binary,
    })))
}

async fn git_stage(
    State(state): State<WorkspaceRouteState>,
    Path((id, slug)): Path<(String, String)>,
    Json(body): Json<WorktreeGitPathsRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ws = state.workspace_manager.lock().await.get(&id).await;
    let workspace_path = match ws {
        Some(ref w) => w.path.clone(),
        None => return Err((StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Workspace not found"})))),
    };

    let worktree_dir = if slug == "root" {
        workspace_path.clone()
    } else {
        std::path::Path::new(&workspace_path).join("..").join(&slug).to_string_lossy().to_string()
    };

    let mut cmd = tokio::process::Command::new("git");
    cmd.arg("add").current_dir(&worktree_dir);
    for p in &body.paths {
        cmd.arg(p);
    }

    let output = cmd.output().await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": stderr}))));
    }

    Ok(Json(serde_json::json!({"ok": true})))
}

async fn git_unstage(
    State(state): State<WorkspaceRouteState>,
    Path((id, slug)): Path<(String, String)>,
    Json(body): Json<WorktreeGitPathsRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ws = state.workspace_manager.lock().await.get(&id).await;
    let workspace_path = match ws {
        Some(ref w) => w.path.clone(),
        None => return Err((StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Workspace not found"})))),
    };

    let worktree_dir = if slug == "root" {
        workspace_path.clone()
    } else {
        std::path::Path::new(&workspace_path).join("..").join(&slug).to_string_lossy().to_string()
    };

    let mut cmd = tokio::process::Command::new("git");
    cmd.args(["restore", "--staged"]).current_dir(&worktree_dir);
    for p in &body.paths {
        cmd.arg(p);
    }

    let output = cmd.output().await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": stderr}))));
    }

    Ok(Json(serde_json::json!({"ok": true})))
}

async fn git_commit(
    State(state): State<WorkspaceRouteState>,
    Path((id, slug)): Path<(String, String)>,
    Json(body): Json<WorktreeGitCommitRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ws = state.workspace_manager.lock().await.get(&id).await;
    let workspace_path = match ws {
        Some(ref w) => w.path.clone(),
        None => return Err((StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Workspace not found"})))),
    };

    let worktree_dir = if slug == "root" {
        workspace_path.clone()
    } else {
        std::path::Path::new(&workspace_path).join("..").join(&slug).to_string_lossy().to_string()
    };

    let output = tokio::process::Command::new("git")
        .args(["-c", "user.useConfigOnly=true", "commit", "-m", &body.message])
        .current_dir(&worktree_dir)
        .output()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": stderr}))));
    }

    // Extract commit SHA
    let sha = String::from_utf8_lossy(&output.stdout)
        .lines()
        .find(|l| l.contains("commit"))
        .map(|l| l.trim().to_string());

    Ok(Json(serde_json::json!({
        "ok": true,
        "commitSha": sha,
    })))
}
