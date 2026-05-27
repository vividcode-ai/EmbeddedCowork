use std::convert::Infallible;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::sse::{Event, Sse},
    response::Json,
    routing::{delete, get},
    Router,
};
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tokio_stream::wrappers::BroadcastStream;

use crate::background_processes::manager::{
    BgProcessExitEvent, BgProcessManager, BgProcessOutputEvent,
};

#[derive(Clone)]
pub struct BgProcessRouteState {
    pub manager: Arc<Mutex<BgProcessManager>>,
}

#[derive(Debug, Deserialize)]
pub struct ExecuteCommandRequest {
    pub command: String,
    pub args: Option<Vec<String>>,
    pub cwd: Option<String>,
    pub env: Option<std::collections::HashMap<String, String>>,
}

#[derive(Debug, Serialize)]
pub struct ExecuteCommandResponse {
    pub process_id: String,
}

#[derive(Debug, Deserialize)]
pub struct StreamQuery {
    pub ids: Option<String>,
}

pub fn background_processes_routes(state: BgProcessRouteState) -> Router {
    Router::new()
        .route("/api/processes", get(list_processes).post(execute_command))
        .route("/api/processes/stream", get(stream_processes))
        .route("/api/processes/:id", get(get_process))
        .route("/api/processes/:id", delete(kill_process))
        .with_state(state)
}

async fn list_processes(
    State(state): State<BgProcessRouteState>,
) -> Json<Vec<serde_json::Value>> {
    let processes = state.manager.lock().await.list().await;
    let items: Vec<serde_json::Value> = processes
        .into_iter()
        .map(|p| {
            serde_json::json!({
                "id": p.id,
                "command": p.command,
                "pid": p.pid,
                "status": p.status,
                "created_at": p.created_at,
            })
        })
        .collect();
    Json(items)
}

async fn execute_command(
    State(state): State<BgProcessRouteState>,
    Json(body): Json<ExecuteCommandRequest>,
) -> Result<Json<ExecuteCommandResponse>, (StatusCode, String)> {
    let id = uuid::Uuid::new_v4().to_string();
    let args = body.args.unwrap_or_default();

    let _pid = state
        .manager
        .lock()
        .await
        .execute(&id, &body.command, &args, body.cwd.as_deref())
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    Ok(Json(ExecuteCommandResponse {
        process_id: id,
    }))
}

async fn get_process(
    State(state): State<BgProcessRouteState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    match state.manager.lock().await.get(&id).await {
        Some(p) => Ok(Json(serde_json::json!({
            "id": p.id,
            "command": p.command,
            "pid": p.pid,
            "status": p.status,
            "created_at": p.created_at,
        }))),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Process not found"})),
        )),
    }
}

async fn kill_process(
    State(state): State<BgProcessRouteState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> StatusCode {
    state.manager.lock().await.kill(&id).await;
    StatusCode::NO_CONTENT
}

// ── SSE streaming ──

/// SSE endpoint that streams process output and exit events.
/// Optional `?ids=id1,id2` query parameter filters to only the given processes.
async fn stream_processes(
    State(state): State<BgProcessRouteState>,
    Query(query): Query<StreamQuery>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let ids: Option<Vec<String>> = query
        .ids
        .map(|s| s.split(',').map(|part| part.trim().to_string()).collect());

    let output_rx = state.manager.lock().await.subscribe_output();
    let exit_rx = state.manager.lock().await.subscribe_exit();

    let stream = ProcessEventStream::new(output_rx, exit_rx, ids);

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("keep-alive"),
    )
}

/// Merged stream of output events and exit events.
struct ProcessEventStream {
    output: BroadcastStream<BgProcessOutputEvent>,
    exit: BroadcastStream<BgProcessExitEvent>,
    ids: Option<Vec<String>>,
}

impl ProcessEventStream {
    fn new(
        output_rx: tokio::sync::broadcast::Receiver<BgProcessOutputEvent>,
        exit_rx: tokio::sync::broadcast::Receiver<BgProcessExitEvent>,
        ids: Option<Vec<String>>,
    ) -> Self {
        Self {
            output: BroadcastStream::new(output_rx),
            exit: BroadcastStream::new(exit_rx),
            ids,
        }
    }

    fn id_is_wanted(&self, id: &str) -> bool {
        self.ids
            .as_ref()
            .map_or(true, |ids| ids.iter().any(|allowed| allowed == id))
    }
}

impl Stream for ProcessEventStream {
    type Item = Result<Event, Infallible>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // Drain matching output events
        loop {
            match Pin::new(&mut self.output).poll_next(cx) {
                Poll::Ready(Some(Ok(ev))) => {
                    if self.id_is_wanted(&ev.id) {
                        let data = serde_json::to_string(&ev).unwrap_or_default();
                        return Poll::Ready(Some(Ok(Event::default().data(data))));
                    }
                    // non-matching id: skip and continue
                    continue;
                }
                Poll::Ready(Some(Err(_))) => {
                    // lagged or closed receiver entry: skip
                    continue;
                }
                Poll::Ready(None) => break,
                Poll::Pending => break,
            }
        }

        // Drain matching exit events
        loop {
            match Pin::new(&mut self.exit).poll_next(cx) {
                Poll::Ready(Some(Ok(ev))) => {
                    if self.id_is_wanted(&ev.id) {
                        let data = serde_json::to_string(&ev).unwrap_or_default();
                        return Poll::Ready(Some(Ok(Event::default().data(data))));
                    }
                    continue;
                }
                Poll::Ready(Some(Err(_))) => {
                    continue;
                }
                Poll::Ready(None) => break,
                Poll::Pending => break,
            }
        }

        Poll::Pending
    }
}
