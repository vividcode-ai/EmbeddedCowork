use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};
use tokio_util::sync::CancellationToken;

use futures::StreamExt;

use crate::api_types::{
    InstanceStreamEvent, InstanceStreamStatus, WorkspaceEventPayload,
};
use crate::events::bus::EventBus;
use crate::logger::Logger;
use crate::workspaces::manager::WorkspaceManager;

const INSTANCE_HOST: &str = "127.0.0.1";
const RECONNECT_DELAY_MS: u64 = 1000;

struct ActiveStream {
    cancel: CancellationToken,
}

pub struct InstanceEventBridge {
    workspace_manager: Arc<Mutex<WorkspaceManager>>,
    event_bus: EventBus,
    logger: Logger,
    shutdown_token: CancellationToken,
    streams: Arc<Mutex<HashMap<String, ActiveStream>>>,
}

impl InstanceEventBridge {
    pub fn new(
        workspace_manager: Arc<Mutex<WorkspaceManager>>,
        event_bus: EventBus,
        logger: Logger,
    ) -> Self {
        let shutdown_token = CancellationToken::new();
        let streams: Arc<Mutex<HashMap<String, ActiveStream>>> =
            Arc::new(Mutex::new(HashMap::new()));

        let bridge = Self {
            workspace_manager,
            event_bus: event_bus.clone(),
            logger,
            shutdown_token,
            streams,
        };

        // Start the lifecycle listener
        bridge.spawn_lifecycle_listener();

        bridge
    }

    fn spawn_lifecycle_listener(&self) {
        let mut rx = self.event_bus.subscribe();
        let streams = self.streams.clone();
        let ws_manager = self.workspace_manager.clone();
        let event_bus = self.event_bus.clone();
        let logger = self.logger.child("lifecycle");
        let shutdown_token = self.shutdown_token.clone();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = shutdown_token.cancelled() => break,
                    result = rx.recv() => {
                        match result {
                            Ok(event) => {
                                match &event {
                                    WorkspaceEventPayload::WorkspaceStarted { workspace, .. } => {
                                        let id = workspace.id.clone();
                                        tracing::info!(
                                            component = %logger.component,
                                            workspace_id = %id,
                                            "Starting instance event stream"
                                        );

                                        let cancel = CancellationToken::new();
                                        {
                                            let mut guard = streams.lock().await;
                                            if let Some(existing) = guard.remove(&id) {
                                                existing.cancel.cancel();
                                            }
                                            guard.insert(id.clone(), ActiveStream { cancel: cancel.clone() });
                                        }

                                        let ws_mgr = ws_manager.clone();
                                        let eb = event_bus.clone();
                                        let log = logger.child(&id);
                                        let sd = shutdown_token.clone();

                                        tokio::spawn(async move {
                                            run_event_stream(id, ws_mgr, eb, log, cancel, sd).await;
                                        });
                                    }
                                    WorkspaceEventPayload::WorkspaceStopped { workspace_id, .. } => {
                                        let id = workspace_id.clone();
                                        let mut guard = streams.lock().await;
                                        if let Some(active) = guard.remove(&id) {
                                            tracing::info!(
                                                component = %logger.component,
                                                workspace_id = %id,
                                                "Stopping instance event stream (workspace stopped)"
                                            );
                                            active.cancel.cancel();
                                        }
                                        publish_status(&event_bus, &id, InstanceStreamStatus::Disconnected, Some("workspace stopped"));
                                    }
                                    WorkspaceEventPayload::WorkspaceError { workspace, .. } => {
                                        let id = workspace.id.clone();
                                        let mut guard = streams.lock().await;
                                        if let Some(active) = guard.remove(&id) {
                                            tracing::info!(
                                                component = %logger.component,
                                                workspace_id = %id,
                                                "Stopping instance event stream (workspace error)"
                                            );
                                            active.cancel.cancel();
                                        }
                                        publish_status(&event_bus, &id, InstanceStreamStatus::Disconnected, Some("workspace error"));
                                    }
                                    _ => {}
                                }
                            }
                            Err(broadcast::error::RecvError::Closed) => break,
                            Err(broadcast::error::RecvError::Lagged(n)) => {
                                tracing::warn!(
                                    component = %logger.component,
                                    lagged = %n,
                                    "Instance event bridge lifecycle listener lagged"
                                );
                            }
                        }
                    }
                }
            }

            tracing::info!(
                component = %logger.component,
                "Instance event bridge lifecycle listener stopped"
            );
        });
    }

    pub fn shutdown(&self) {
        tracing::info!(
            component = %self.logger.component,
            "Shutting down instance event bridge"
        );
        self.shutdown_token.cancel();

        let streams = self.streams.clone();
        let logger = self.logger.clone();
        tokio::spawn(async move {
            let mut guard = streams.lock().await;
            for (id, active) in guard.drain() {
                tracing::debug!(
                    component = %logger.component,
                    workspace_id = %id,
                    "Cancelling instance event stream"
                );
                active.cancel.cancel();
            }
        });
    }
}

async fn run_event_stream(
    workspace_id: String,
    workspace_manager: Arc<Mutex<WorkspaceManager>>,
    event_bus: EventBus,
    logger: Logger,
    cancel: CancellationToken,
    shutdown_token: CancellationToken,
) {
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()
        .expect("Failed to create reqwest client for instance event bridge");

    while !cancel.is_cancelled() && !shutdown_token.is_cancelled() {
        // Get port from workspace manager
        let port = {
            let manager = workspace_manager.lock().await;
            manager.get_instance_port(&workspace_id).await
        };

        let port = match port {
            Some(p) => p,
            None => {
                tracing::debug!(
                    component = %logger.component,
                    workspace_id = %workspace_id,
                    "No port yet for workspace, waiting..."
                );
                tokio::select! {
                    _ = tokio::time::sleep(std::time::Duration::from_millis(RECONNECT_DELAY_MS)) => {},
                    _ = cancel.cancelled() => return,
                    _ = shutdown_token.cancelled() => return,
                }
                continue;
            }
        };

        // Get auth header
        let auth_header = {
            let manager = workspace_manager.lock().await;
            manager.get_instance_authorization_header(&workspace_id).await
        };

        // Publish "connecting" status
        publish_status(&event_bus, &workspace_id, InstanceStreamStatus::Connecting, None);

        let url = format!("http://{}:{}/global/event", INSTANCE_HOST, port);
        tracing::info!(
            component = %logger.component,
            workspace_id = %workspace_id,
            %url,
            "Connecting to instance event stream"
        );

        let mut req = client.get(&url);
        if let Some(ref auth) = auth_header {
            tracing::debug!(
                component = %logger.component,
                workspace_id = %workspace_id,
                auth_header = %auth,
                "Using auth header for instance event stream"
            );
            req = req.header("Authorization", auth);
        } else {
            tracing::warn!(
                component = %logger.component,
                workspace_id = %workspace_id,
                "No auth header available for instance event stream"
            );
        }

        let response = match req.send().await {
            Ok(resp) => resp,
            Err(e) => {
                tracing::warn!(
                    component = %logger.component,
                    workspace_id = %workspace_id,
                    error = %e,
                    error_debug = ?e,
                    "Failed to connect to instance event stream"
                );
                publish_status(
                    &event_bus,
                    &workspace_id,
                    InstanceStreamStatus::Error,
                    Some(&format!("{:#}", e)),
                );
                tokio::select! {
                    _ = tokio::time::sleep(std::time::Duration::from_millis(RECONNECT_DELAY_MS)) => {},
                    _ = cancel.cancelled() => return,
                    _ = shutdown_token.cancelled() => return,
                }
                continue;
            }
        };

        if !response.status().is_success() {
            tracing::warn!(
                component = %logger.component,
                workspace_id = %workspace_id,
                status = %response.status(),
                "Instance event stream unavailable"
            );
            publish_status(
                &event_bus,
                &workspace_id,
                InstanceStreamStatus::Error,
                Some(&format!("HTTP {}", response.status())),
            );
            tokio::select! {
                _ = tokio::time::sleep(std::time::Duration::from_millis(RECONNECT_DELAY_MS)) => {},
                _ = cancel.cancelled() => return,
                _ = shutdown_token.cancelled() => return,
            }
            continue;
        }

        // Connected!
        publish_status(&event_bus, &workspace_id, InstanceStreamStatus::Connected, None);
        tracing::info!(
            component = %logger.component,
            workspace_id = %workspace_id,
            "Connected to instance event stream"
        );

        // Consume SSE events
        if let Err(e) = consume_sse(response, &event_bus, &workspace_id, &logger, &cancel, &shutdown_token).await {
            if !cancel.is_cancelled() && !shutdown_token.is_cancelled() {
                tracing::warn!(
                    component = %logger.component,
                    workspace_id = %workspace_id,
                    error = %e,
                    "Instance event stream error, will reconnect"
                );
                publish_status(
                    &event_bus,
                    &workspace_id,
                    InstanceStreamStatus::Error,
                    Some(&e),
                );
            }
        }

        if cancel.is_cancelled() || shutdown_token.is_cancelled() {
            return;
        }

        // Wait before reconnecting
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_millis(RECONNECT_DELAY_MS)) => {},
            _ = cancel.cancelled() => return,
            _ = shutdown_token.cancelled() => return,
        }
    }
}

async fn consume_sse(
    response: reqwest::Response,
    event_bus: &EventBus,
    workspace_id: &str,
    logger: &Logger,
    cancel: &CancellationToken,
    shutdown_token: &CancellationToken,
) -> Result<(), String> {
    let mut stream = response.bytes_stream();
    let mut buffer: Vec<u8> = Vec::new();

    loop {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => return Ok(()),
            _ = shutdown_token.cancelled() => return Ok(()),
            chunk = stream.next() => {
                match chunk {
                    Some(Ok(bytes)) => {
                        buffer.extend_from_slice(&bytes);

                        // Process all complete SSE events in buffer
                        loop {
                            match extract_sse_event(&mut buffer) {
                                SseExtract::Event(data) => {
                                    process_sse_payload(&data, event_bus, workspace_id, logger);
                                }
                                SseExtract::NeedMore => break,
                            }
                        }

                        // Cap buffer to prevent OOM on malformed stream
                        if buffer.len() > 1024 * 1024 {
                            tracing::warn!(
                                component = %logger.component,
                                workspace_id = %workspace_id,
                                "SSE buffer exceeded 1MB, clearing"
                            );
                            buffer.clear();
                        }
                    }
                    Some(Err(e)) => {
                        return Err(format!("SSE read error: {}", e));
                    }
                    None => {
                        // Stream ended
                        return Ok(());
                    }
                }
            }
        }
    }
}

enum SseExtract {
    Event(String),
    NeedMore,
}

/// Extract a single complete SSE event from the buffer.
/// Searches for `\n\n` or `\r\n\r\n` as the event separator,
/// then extracts the `data:` lines and concatenates them.
fn extract_sse_event(buffer: &mut Vec<u8>) -> SseExtract {
    // Find event boundary: \n\n or \r\n\r\n
    let boundary_pos = find_event_boundary(buffer);

    let boundary_pos = match boundary_pos {
        Some(pos) => pos,
        None => return SseExtract::NeedMore,
    };

    let event_bytes = buffer[..boundary_pos].to_vec();
    // Remove the event including the boundary from buffer
    let event_len = boundary_pos + if buffer[boundary_pos..].starts_with(b"\r\n\r\n") {
        4
    } else {
        2
    };
    buffer.drain(..event_len);

    if event_bytes.is_empty() {
        return SseExtract::NeedMore;
    }

    let event_str = String::from_utf8_lossy(&event_bytes);

    // Extract data: lines
    let mut data_lines: Vec<&str> = Vec::new();
    for line in event_str.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("data:") {
            data_lines.push(trimmed[5..].trim_start());
        }
        // Lines starting with ":" are comments, skip them
    }

    if data_lines.is_empty() {
        return SseExtract::NeedMore;
    }

    let payload = data_lines.join("\n").trim().to_string();
    if payload.is_empty() {
        return SseExtract::NeedMore;
    }

    SseExtract::Event(payload)
}

fn find_event_boundary(buffer: &[u8]) -> Option<usize> {
    for i in 0..buffer.len().saturating_sub(1) {
        if buffer[i] == b'\n' && buffer[i + 1] == b'\n' {
            return Some(i);
        }
        if i + 3 < buffer.len() && buffer[i] == b'\r' && buffer[i + 1] == b'\n'
            && buffer[i + 2] == b'\r' && buffer[i + 3] == b'\n'
        {
            return Some(i);
        }
    }
    None
}

fn process_sse_payload(
    payload: &str,
    event_bus: &EventBus,
    workspace_id: &str,
    logger: &Logger,
) {
    let parsed: serde_json::Value = match serde_json::from_str(payload) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(
                component = %logger.component,
                workspace_id = %workspace_id,
                error = %e,
                payload = %payload,
                "Failed to parse instance SSE event as JSON"
            );
            return;
        }
    };

    // OpenCode SSE payload shapes vary.
    // Common variants:
    // - { type, properties, ... }
    // - { payload: { type, properties, ... }, directory: "/abs/path" }
    // - { payload: { type, properties, ... } }
    let base = if let Some(payload_obj) = parsed.get("payload").and_then(|v| v.as_object()) {
        serde_json::Value::Object(payload_obj.clone())
    } else {
        parsed.clone()
    };

    // Ensure it has a "type" field
    if !base.get("type").and_then(|v| v.as_str()).map_or(false, |s| !s.is_empty()) {
        tracing::warn!(
            component = %logger.component,
            workspace_id = %workspace_id,
            payload = %payload,
            "Instance SSE event missing type field"
        );
        return;
    }

    // Build InstanceStreamEvent
    let event = InstanceStreamEvent {
        event_type: base.get("type").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        properties: base.get("properties")
            .and_then(|v| v.as_object())
            .map(|obj| {
                obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
            }),
    };

    // Attach directory from outer payload if available
    let mut enriched_event = event.clone();
    if let Some(dir) = parsed.get("directory").and_then(|v| v.as_str()) {
        let mut props = enriched_event.properties.unwrap_or_default();
        props.insert("directory".to_string(), serde_json::Value::String(dir.to_string()));
        enriched_event.properties = Some(props);
    }

    tracing::trace!(
        component = %logger.component,
        workspace_id = %workspace_id,
        event_type = %enriched_event.event_type,
        "Instance SSE event received"
    );

    event_bus.publish(WorkspaceEventPayload::InstanceEvent {
        event_type: "instance.event".to_string(),
        instance_id: workspace_id.to_string(),
        event: enriched_event,
    });
}

fn publish_status(
    event_bus: &EventBus,
    instance_id: &str,
    status: InstanceStreamStatus,
    reason: Option<&str>,
) {
    event_bus.publish(WorkspaceEventPayload::InstanceEventStatus {
        event_type: "instance.eventStatus".to_string(),
        instance_id: instance_id.to_string(),
        status,
        reason: reason.map(|s| s.to_string()),
    });
}
