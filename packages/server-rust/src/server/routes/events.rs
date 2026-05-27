use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::sse::{Event, Sse},
    routing::get,
    Json, Router,
};
use futures::stream::Stream;
use serde::Deserialize;
use std::{
    convert::Infallible,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};
use tokio::sync::Mutex;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;

use crate::clients::connection_manager::{ClientConnectionManager, ClientConnectionRef};
use crate::events::bus::EventBus;
use crate::api_types::WorkspaceEventPayload;

#[derive(Clone)]
pub struct EventsRouteState {
    pub event_bus: EventBus,
    pub client_connection_manager: Arc<Mutex<ClientConnectionManager>>,
}

#[derive(Debug, Deserialize)]
pub struct EventsQuery {
    #[serde(rename = "clientId")]
    pub client_id: Option<String>,
    #[serde(rename = "connectionId")]
    pub connection_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PongRequest {
    pub client_id: String,
    pub connection_id: String,
    pub ping_ts: Option<u64>,
}

pub fn events_routes(state: EventsRouteState) -> Router {
    Router::new()
        .route("/api/events", get(stream_events))
        .route("/api/client-connections/pong", axum::routing::post(post_pong))
        .with_state(state)
}

async fn stream_events(
    State(state): State<EventsRouteState>,
    Query(query): Query<EventsQuery>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    // Register client connection if clientId and connectionId are provided
    if let (Some(client_id), Some(connection_id)) = (query.client_id, query.connection_id) {
        let conn = ClientConnectionRef {
            client_id,
            connection_id,
        };
        state.client_connection_manager.lock().await.register(conn, || {});
    }

    let rx = state.event_bus.subscribe();
    let stream = EventStream::new(rx);
    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("keep-alive"),
    )
}

async fn post_pong(
    State(state): State<EventsRouteState>,
    Json(body): Json<PongRequest>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    let input = crate::clients::connection_manager::ClientConnectionRef {
        client_id: body.client_id,
        connection_id: body.connection_id,
    };

    let ok = state.client_connection_manager.lock().await.pong(&input);
    if ok {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err((StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Client connection not found"}))))
    }
}

struct EventStream {
    inner: BroadcastStream<WorkspaceEventPayload>,
}

impl EventStream {
    fn new(rx: broadcast::Receiver<WorkspaceEventPayload>) -> Self {
        Self {
            inner: BroadcastStream::new(rx),
        }
    }
}

impl Stream for EventStream {
    type Item = Result<Event, Infallible>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::new(&mut self.inner).poll_next(cx) {
            Poll::Ready(Some(Ok(payload))) => {
                let data = serde_json::to_string(&payload).unwrap_or_default();
                Poll::Ready(Some(Ok(Event::default().data(data))))
            }
            Poll::Ready(Some(Err(_))) => {
                Poll::Ready(None)
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}
