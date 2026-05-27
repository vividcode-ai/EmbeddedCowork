use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::filesystem::browser::FileBrowser;

#[derive(Clone)]
pub struct FilesystemRouteState {
    pub browser: Arc<Mutex<FileBrowser>>,
}

#[derive(Debug, Deserialize)]
pub struct BrowseQuery {
    pub path: Option<String>,
    pub include_files: Option<String>, // "true"/"false" as string
}

#[derive(Debug, Deserialize)]
pub struct ReadQuery {
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub path: Option<String>,
    pub pattern: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateFolderRequest {
    pub parent_path: Option<String>,
    pub name: String,
}

pub fn filesystem_routes(state: FilesystemRouteState) -> Router {
    Router::new()
        // Main filesystem browse (matches UI expectation)
        .route("/api/filesystem", get(browse_filesystem))
        // Existing routes
        .route("/api/filesystem/browse", get(browse_filesystem_detailed))
        .route("/api/filesystem/read", get(read_file))
        .route("/api/filesystem/search", get(search_filesystem))
        .route("/api/filesystem/exists", get(check_exists))
        // Create folder
        .route("/api/filesystem/folders", post(create_folder))
        .with_state(state)
}

async fn browse_filesystem(
    State(state): State<FilesystemRouteState>,
    Query(query): Query<BrowseQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let browser = state.browser.lock().await;
    let path = query.path.filter(|p| !p.is_empty() && p != ".").unwrap_or_else(|| {
        browser.default_root().unwrap_or(".").to_string()
    });
    let include_files = query.include_files.as_deref().unwrap_or("false") == "true";
    let depth = if include_files { 1 } else { 1 };

    match browser.browse(&path, depth, false).await {
        Ok(entries) => {
            let items: Vec<serde_json::Value> = entries
                .into_iter()
                .filter(|e| include_files || e.entry_type == crate::api_types::FileSystemEntryType::Directory)
                .map(|e| serde_json::json!({
                    "name": e.name,
                    "path": e.path,
                    "absolutePath": e.path,
                    "type": e.entry_type,
                }))
                .collect();

            let parent_path = std::path::Path::new(&path).parent()
                .map(|p| p.to_string_lossy().to_string());

            Ok(Json(serde_json::json!({
                "entries": items,
                "metadata": {
                    "scope": "allowed",
                    "currentPath": path,
                    "parentPath": parent_path,
                    "rootPath": path,
                    "homePath": path,
                    "displayPath": path,
                    "pathKind": "allowed",
                },
            })))
        }
        Err(e) => Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e})))),
    }
}

async fn browse_filesystem_detailed(
    State(state): State<FilesystemRouteState>,
    Query(query): Query<BrowseQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let browser = state.browser.lock().await;
    let path = query.path.filter(|p| !p.is_empty() && p != ".").unwrap_or_else(|| {
        browser.default_root().unwrap_or(".").to_string()
    });
    let include_files = query.include_files.as_deref().unwrap_or("false") == "true";
    let depth: u32 = 1;

    match browser.browse(&path, depth, false).await {
        Ok(entries) => {
            let items: Vec<serde_json::Value> = entries
                .into_iter()
                .filter(|e| include_files || e.entry_type == crate::api_types::FileSystemEntryType::Directory)
                .map(|e| serde_json::json!({
                    "name": e.name,
                    "path": e.path,
                    "absolutePath": e.path,
                    "type": e.entry_type,
                }))
                .collect();
            Ok(Json(serde_json::json!({
                "entries": items,
                "path": path,
            })))
        }
        Err(e) => Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e})))),
    }
}

async fn create_folder(
    State(_state): State<FilesystemRouteState>,
    Json(body): Json<CreateFolderRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let parent = body.parent_path.unwrap_or_else(|| ".".to_string());
    let folder_path = std::path::Path::new(&parent).join(&body.name);

    if folder_path.exists() {
        return Err((StatusCode::CONFLICT, Json(serde_json::json!({"error": "Folder already exists"}))));
    }

    tokio::fs::create_dir_all(&folder_path).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))))?;

    Ok(Json(serde_json::json!({
        "path": folder_path.to_string_lossy(),
        "absolutePath": folder_path.to_string_lossy(),
    })))
}

async fn read_file(
    State(state): State<FilesystemRouteState>,
    Query(query): Query<ReadQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    match state.browser.lock().await.read_file(&query.path).await {
        Ok(content) => Ok(Json(serde_json::json!({
            "path": query.path,
            "content": content,
        }))),
        Err(e) => Err((StatusCode::NOT_FOUND, Json(serde_json::json!({"error": e})))),
    }
}

async fn search_filesystem(
    State(_state): State<FilesystemRouteState>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let root = query.path.unwrap_or_else(|| ".".to_string());
    let pattern = query.pattern.unwrap_or_default();

    if pattern.is_empty() {
        return Ok(Json(serde_json::json!({
            "entries": [],
            "path": root,
            "pattern": "",
        })));
    }

    let pattern_lower = pattern.to_lowercase();
    let mut results = Vec::new();
    let mut stack = vec![(PathBuf::from(&root), 0u32)];

    while let Some((dir, depth)) = stack.pop() {
        if depth > 10 || results.len() >= 1000 {
            continue;
        }

        let mut entries = match tokio::fs::read_dir(&dir).await {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        loop {
            let entry = match entries.next_entry().await {
                Ok(Some(entry)) => entry,
                Ok(None) => break,
                Err(_) => continue,
            };

            if results.len() >= 1000 {
                break;
            }

            let file_name = entry.file_name();
            let file_name_str = file_name.to_string_lossy();

            if file_name_str.starts_with('.') {
                continue;
            }

            let path = entry.path();
            let is_dir = entry.file_type().await.map(|ft| ft.is_dir()).unwrap_or(false);

            if file_name_str.to_lowercase().contains(&pattern_lower) {
                let metadata = entry.metadata().await.ok();
                let size = metadata.as_ref().map(|m| m.len());
                let modified_at = metadata.and_then(|m| m.modified().ok()).and_then(|t| {
                    let duration = t.duration_since(std::time::UNIX_EPOCH).ok()?;
                    chrono::DateTime::from_timestamp(duration.as_secs() as i64, duration.subsec_nanos())
                        .map(|dt| dt.to_rfc3339())
                });

                results.push(serde_json::json!({
                    "name": file_name_str.as_ref(),
                    "path": path.to_string_lossy(),
                    "absolutePath": path.to_string_lossy(),
                    "type": if is_dir { "directory" } else { "file" },
                    "size": size,
                    "modifiedAt": modified_at,
                }));
            }

            if is_dir {
                stack.push((path, depth + 1));
            }
        }
    }

    Ok(Json(serde_json::json!({
        "entries": results,
        "path": root,
        "pattern": pattern,
    })))
}

async fn check_exists(
    State(state): State<FilesystemRouteState>,
    Query(query): Query<ReadQuery>,
) -> Json<serde_json::Value> {
    let exists = state.browser.lock().await.path_exists(&query.path).await;
    Json(serde_json::json!({"exists": exists}))
}
