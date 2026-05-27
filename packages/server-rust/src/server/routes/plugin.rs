use axum::{
    extract::State,
    http::StatusCode,
    response::{
        sse::{Event, Sse},
        Json,
    },
    routing::{get, post},
    Router,
};
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use std::{
    convert::Infallible,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};
use tokio::sync::Mutex;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;

use crate::plugins::channel::PluginChannelManager;
use crate::plugins::voice_mode::VoiceModeManager;
use crate::workspaces::manager::WorkspaceManager;

#[derive(Clone)]
pub struct PluginRouteState {
    pub channel_manager: Arc<Mutex<PluginChannelManager>>,
    pub voice_mode_manager: Arc<Mutex<VoiceModeManager>>,
    pub workspace_manager: Arc<Mutex<WorkspaceManager>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PluginMessage {
    pub plugin_id: String,
    pub event: String,
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct VoiceModePayload {
    pub enabled: bool,
    pub client_id: Option<String>,
    pub connection_id: Option<String>,
}

pub fn plugin_routes(state: PluginRouteState) -> Router {
    Router::new()
        .route("/api/plugins/channel", get(plugin_channel))
        .route("/api/plugins/message", post(send_message))
        .route("/workspaces/:id/plugin/voice-mode", post(voice_mode_handler))
        .route("/workspaces/:id/plugin/events", get(workspace_events_handler))
        .route("/workspaces/:id/plugin/event", post(plugin_event_handler))
        .with_state(state)
}

async fn plugin_channel(
    State(state): State<PluginRouteState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.channel_manager.lock().await.subscribe();
    let stream = PluginEventStream::new(rx);
    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("keep-alive"),
    )
}

struct PluginEventStream {
    inner: BroadcastStream<serde_json::Value>,
}

impl PluginEventStream {
    fn new(rx: broadcast::Receiver<serde_json::Value>) -> Self {
        Self {
            inner: BroadcastStream::new(rx),
        }
    }
}

impl Stream for PluginEventStream {
    type Item = Result<Event, Infallible>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::new(&mut self.inner).poll_next(cx) {
            Poll::Ready(Some(Ok(msg))) => {
                let data = serde_json::to_string(&msg).unwrap_or_default();
                Poll::Ready(Some(Ok(Event::default().data(data))))
            }
            Poll::Ready(Some(Err(_))) => Poll::Ready(None),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

async fn send_message(
    State(state): State<PluginRouteState>,
    Json(body): Json<PluginMessage>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    // Handle voice mode toggling
    if body.event == "embeddedcowork.voiceMode" {
        if let Some(ref data) = body.data {
            if let Ok(payload) = serde_json::from_value::<VoiceModePayload>(data.clone()) {
                let instance_id = &body.plugin_id;
                if let (Some(client_id), Some(connection_id)) = (payload.client_id, payload.connection_id) {
                    let connection = crate::clients::connection_manager::ClientConnectionRef {
                        client_id,
                        connection_id,
                    };
                    let mut vm = state.voice_mode_manager.lock().await;
                    vm.set_enabled(instance_id, &connection, payload.enabled).await;
                }
            }
        }
    }

    // Always publish to channel
    state.channel_manager.lock().await.publish(serde_json::to_value(&body).unwrap_or_default());
    Ok(Json(serde_json::json!({"ok": true})))
}

#[derive(Debug, Deserialize)]
pub struct VoiceModeRequest {
    #[serde(rename = "clientId")]
    pub client_id: String,
    #[serde(rename = "connectionId")]
    pub connection_id: String,
    pub enabled: bool,
}

async fn voice_mode_handler(
    State(state): State<PluginRouteState>,
    axum::extract::Path(id): axum::extract::Path<String>,
    Json(body): Json<VoiceModeRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    // Verify workspace exists
    let ws_exists = state.workspace_manager.lock().await.get(&id).await.is_some();
    if !ws_exists {
        return Err((StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Workspace not found"}))));
    }

    let connection = crate::clients::connection_manager::ClientConnectionRef {
        client_id: body.client_id,
        connection_id: body.connection_id,
    };

    let mut vm = state.voice_mode_manager.lock().await;
    let applied = vm.set_enabled(&id, &connection, body.enabled).await;

    if body.enabled && !applied {
        return Err((StatusCode::CONFLICT, Json(serde_json::json!({"error": "Client connection not active for voice mode enable"}))));
    }

    Ok(Json(serde_json::json!({"enabled": body.enabled})))
}

#[derive(Debug, Deserialize)]
pub struct PluginEventBody {
    pub r#type: String,
    pub properties: Option<serde_json::Value>,
}

async fn workspace_events_handler(
    State(state): State<PluginRouteState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<
    Sse<impl Stream<Item = Result<Event, Infallible>>>,
    (StatusCode, Json<serde_json::Value>),
> {
    // Verify workspace exists
    let ws_exists = state.workspace_manager.lock().await.get(&id).await.is_some();
    if !ws_exists {
        return Err((StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Workspace not found"}))));
    }

    // Register with channel manager
    state.channel_manager.lock().await.register(&id);

    // Sync voice mode state
    state.voice_mode_manager.lock().await.sync_instance(&id).await;

    // Subscribe to broadcast channel and filter by workspace_id
    let rx = state.channel_manager.lock().await.subscribe();
    let stream = WorkspaceEventStream::new(rx, id);

    Ok(Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("keep-alive"),
    ))
}

struct WorkspaceEventStream {
    inner: BroadcastStream<serde_json::Value>,
    workspace_id: String,
}

impl WorkspaceEventStream {
    fn new(rx: broadcast::Receiver<serde_json::Value>, workspace_id: String) -> Self {
        Self {
            inner: BroadcastStream::new(rx),
            workspace_id,
        }
    }
}

impl Stream for WorkspaceEventStream {
    type Item = Result<Event, Infallible>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            match Pin::new(&mut self.inner).poll_next(cx) {
                Poll::Ready(Some(Ok(msg))) => {
                    // Filter by workspace_id
                    let matches = msg
                        .get("workspaceId")
                        .and_then(|v| v.as_str())
                        .map(|wid| wid == self.workspace_id)
                        .unwrap_or(false);

                    if !matches {
                        continue;
                    }

                    let data = serde_json::to_string(&msg).unwrap_or_default();
                    return Poll::Ready(Some(Ok(Event::default().data(data))));
                }
                Poll::Ready(Some(Err(_))) => return Poll::Ready(None),
                Poll::Ready(None) => return Poll::Ready(None),
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

async fn plugin_event_handler(
    State(state): State<PluginRouteState>,
    axum::extract::Path(id): axum::extract::Path<String>,
    Json(body): Json<PluginEventBody>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    // Verify workspace exists
    let ws_exists = state.workspace_manager.lock().await.get(&id).await.is_some();
    if !ws_exists {
        return Err((StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Workspace not found"}))));
    }

    match body.r#type.as_str() {
        "embeddedcowork.pong" => {
            // Just acknowledge the pong
            Ok(StatusCode::NO_CONTENT)
        }
        _ => {
            tracing::debug!(
                component = "plugin",
                workspace_id = %id,
                event_type = %body.r#type,
                "Unhandled plugin event"
            );
            Ok(StatusCode::NO_CONTENT)
        }
    }
}
