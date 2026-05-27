use std::sync::Arc;
use tokio::sync::Mutex;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{
        sse::{Event, Sse},
        Json,
    },
    routing::{get, post},
    Router,
};
use futures::stream::Stream;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use tokio_stream::wrappers::BroadcastStream;

use crate::background_processes::plugin_manager::PluginBgProcessManager;
use crate::plugins::channel::PluginChannelManager;
use crate::workspaces::manager::WorkspaceManager;

#[derive(Clone)]
pub struct PluginBgProcessRouteState {
    pub bg_process_manager: Arc<Mutex<PluginBgProcessManager>>,
    pub workspace_manager: Arc<Mutex<WorkspaceManager>>,
    pub channel_manager: Arc<Mutex<PluginChannelManager>>,
}

#[derive(Debug, Deserialize)]
pub struct StartProcessRequest {
    pub title: String,
    pub command: String,
    #[serde(default)]
    pub notify: bool,
    pub notification: Option<ProcessNotification>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ProcessNotification {
    #[serde(rename = "sessionID")]
    pub session_id: String,
    pub directory: String,
}

#[derive(Debug, Deserialize)]
pub struct OutputQuery {
    pub method: Option<String>,
    pub mode: Option<String>,
    pub pattern: Option<String>,
    pub lines: Option<u32>,
    #[serde(rename = "maxBytes")]
    pub max_bytes: Option<usize>,
}

pub fn plugin_bg_process_routes(state: PluginBgProcessRouteState) -> Router {
    Router::new()
        .route(
            "/workspaces/:id/plugin/background-processes",
            get(list_processes).post(start_process),
        )
        .route(
            "/workspaces/:id/plugin/background-processes/:processId/stop",
            post(stop_process),
        )
        .route(
            "/workspaces/:id/plugin/background-processes/:processId/terminate",
            post(terminate_process),
        )
        .route(
            "/workspaces/:id/plugin/background-processes/:processId/output",
            get(read_process_output),
        )
        .route(
            "/workspaces/:id/plugin/background-processes/:processId/stream",
            get(stream_process_output),
        )
        .with_state(state)
}

async fn list_processes(
    State(state): State<PluginBgProcessRouteState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    // Verify workspace exists
    let ws_exists = state.workspace_manager.lock().await.get(&id).await.is_some();
    if !ws_exists {
        return Err((StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Workspace not found"}))));
    }

    let processes = state.bg_process_manager.lock().await.list(&id).await;
    Ok(Json(serde_json::json!({ "processes": processes })))
}

async fn start_process(
    State(state): State<PluginBgProcessRouteState>,
    Path(id): Path<String>,
    Json(body): Json<StartProcessRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let ws = state.workspace_manager.lock().await.get(&id).await;
    let ws_path = match ws {
        Some(ref w) => w.path.clone(),
        None => return Err((StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Workspace not found"})))),
    };

    let uuid_str = uuid::Uuid::new_v4().to_string();
    let short_id = &uuid_str[..12];
    let proc_id = format!("proc_{}", short_id);

    let notify_tx = if body.notify {
        Some(state.channel_manager.lock().await.sender())
    } else {
        None
    };
    let manager = state.bg_process_manager.lock().await;
    let notification = body.notification.map(|n| crate::background_processes::plugin_manager::BgProcessNotification {
        session_id: n.session_id,
        directory: n.directory,
    });
    let process = manager
        .start(&proc_id, &id, &body.title, &body.command, &ws_path, body.notify, notification, notify_tx)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))))?;

    Ok((StatusCode::CREATED, Json(serde_json::to_value(&process).unwrap_or_default())))
}

async fn stop_process(
    State(state): State<PluginBgProcessRouteState>,
    Path((_id, process_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let mgr = state.bg_process_manager.lock().await;
    let proc = mgr.get(&process_id).await;
    let process = match proc {
        Some(p) => p,
        None => return Err((StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Process not found"})))),
    };

    if process.status != "running" {
        return Ok(Json(serde_json::to_value(&process).unwrap_or_default()));
    }

    // Graceful stop: send SIGTERM
    if let Some(pid) = process.pid {
        #[cfg(unix)]
        let _ = std::process::Command::new("kill")
            .arg(pid.to_string())
            .spawn();
        #[cfg(windows)]
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string()])
            .spawn();
    }

    mgr.update_status(&process_id, "stopped", "user_stopped").await;
    let updated = mgr.get(&process_id).await.unwrap_or(process);
    Ok(Json(serde_json::to_value(&updated).unwrap_or_default()))
}

async fn terminate_process(
    State(state): State<PluginBgProcessRouteState>,
    Path((_id, process_id)): Path<(String, String)>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    let mgr = state.bg_process_manager.lock().await;
    let proc = mgr.get(&process_id).await;
    match proc {
        Some(p) => {
            if let Some(pid) = p.pid {
                #[cfg(unix)]
                let _ = std::process::Command::new("kill")
                    .arg("-9")
                    .arg(pid.to_string())
                    .spawn();
                #[cfg(windows)]
                let _ = std::process::Command::new("taskkill")
                    .args(["/PID", &pid.to_string(), "/F"])
                    .spawn();
            }
            mgr.update_status(&process_id, "stopped", "user_terminated").await;
            Ok(StatusCode::NO_CONTENT)
        }
        None => Err((StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Process not found"})))),
    }
}

async fn read_process_output(
    State(state): State<PluginBgProcessRouteState>,
    Path((_id, process_id)): Path<(String, String)>,
    Query(query): Query<OutputQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let method = query.method.or(query.mode).unwrap_or_else(|| "full".to_string());

    match state
        .bg_process_manager
        .lock()
        .await
        .read_output(
            &process_id,
            &method,
            query.pattern.as_deref(),
            query.lines.map(|l| l as usize),
            query.max_bytes,
        )
        .await
    {
        Ok((content, truncated, size_bytes)) => Ok(Json(serde_json::json!({
            "id": process_id,
            "content": content,
            "truncated": truncated,
            "sizeBytes": size_bytes,
        }))),
        Err(e) => Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e})))),
    }
}

async fn stream_process_output(
    State(state): State<PluginBgProcessRouteState>,
    Path((_id, process_id)): Path<(String, String)>,
) -> Result<
    Sse<impl Stream<Item = Result<Event, Infallible>>>,
    (StatusCode, Json<serde_json::Value>),
> {
    let rx = state
        .bg_process_manager
        .lock()
        .await
        .subscribe_output(&process_id)
        .await
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Process not found"}))))?;

    let stream = BroadcastStream::new(rx);
    let mapped = stream.filter_map(|result| async move {
        match result {
            Ok(line) => Some(Ok(Event::default().data(
                serde_json::json!({ "type": "chunk", "content": line }).to_string(),
            ))),
            Err(_) => None,
        }
    });

    Ok(Sse::new(mapped).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("keep-alive"),
    ))
}
