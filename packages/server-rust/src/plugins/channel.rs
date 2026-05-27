use std::collections::{HashSet, HashMap};
use tokio::sync::broadcast;

use crate::logger::Logger;

#[derive(Debug, Clone)]
pub struct PluginOutboundEvent {
    pub event_type: String,
    pub properties: Option<HashMap<String, serde_json::Value>>,
}

pub struct PluginChannelManager {
    clients: HashSet<String>,
    tx: broadcast::Sender<serde_json::Value>,
    logger: Logger,
}

impl PluginChannelManager {
    pub fn new(logger: Logger) -> Self {
        let (tx, _) = broadcast::channel(256);
        Self {
            clients: HashSet::new(),
            tx,
            logger,
        }
    }

    pub fn register(&mut self, workspace_id: &str) {
        self.clients.insert(workspace_id.to_string());
        tracing::debug!(component = %self.logger.component, workspace_id = %workspace_id, "Plugin SSE client connected");
    }

    pub fn unregister(&mut self, workspace_id: &str) {
        self.clients.remove(workspace_id);
        tracing::debug!(component = %self.logger.component, workspace_id = %workspace_id, "Plugin SSE client disconnected");
    }

    pub fn send(&self, workspace_id: &str, event: &PluginOutboundEvent) {
        if self.clients.contains(workspace_id) {
            let payload = serde_json::json!({
                "workspaceId": workspace_id,
                "event": event.event_type,
                "properties": event.properties,
            });
            let _ = self.tx.send(payload);
            tracing::debug!(
                component = %self.logger.component,
                workspace_id = %workspace_id,
                event_type = %event.event_type,
                "Sending plugin event"
            );
        }
    }

    pub fn broadcast(&self, event: &PluginOutboundEvent) {
        if !self.clients.is_empty() {
            let payload = serde_json::json!({
                "event": event.event_type,
                "properties": event.properties,
            });
            let _ = self.tx.send(payload);
            tracing::debug!(
                component = %self.logger.component,
                client_count = self.clients.len(),
                event_type = %event.event_type,
                "Broadcasting plugin event"
            );
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<serde_json::Value> {
        self.tx.subscribe()
    }

    pub fn publish(&mut self, value: serde_json::Value) -> usize {
        self.tx.send(value).unwrap_or(0)
    }

    pub fn sender(&self) -> broadcast::Sender<serde_json::Value> {
        self.tx.clone()
    }
}
