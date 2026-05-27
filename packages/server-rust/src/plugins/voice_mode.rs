use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::clients::connection_manager::{ClientConnectionRef, ClientConnectionManager};
use crate::logger::Logger;
use crate::plugins::channel::{PluginChannelManager, PluginOutboundEvent};

pub struct VoiceModeManager {
    enabled_connections_by_instance: HashMap<String, HashSet<String>>,
    aggregate_by_instance: HashMap<String, bool>,
    connections: Arc<Mutex<ClientConnectionManager>>,
    channel: Arc<Mutex<PluginChannelManager>>,
    logger: Logger,
}

impl VoiceModeManager {
    pub fn new(
        connections: Arc<Mutex<ClientConnectionManager>>,
        channel: Arc<Mutex<PluginChannelManager>>,
        logger: Logger,
    ) -> Self {
        Self {
            enabled_connections_by_instance: HashMap::new(),
            aggregate_by_instance: HashMap::new(),
            connections,
            channel,
            logger,
        }
    }

    pub async fn set_enabled(
        &mut self,
        instance_id: &str,
        connection: &ClientConnectionRef,
        enabled: bool,
    ) -> bool {
        if enabled && !self.connections.lock().await.is_connected(connection) {
            tracing::debug!(
                component = %self.logger.component,
                instance_id = %instance_id,
                "Ignoring voice mode enable for disconnected client"
            );
            return false;
        }

        let key = get_connection_key(connection);
        let current = self.enabled_connections_by_instance
            .entry(instance_id.to_string())
            .or_insert_with(HashSet::new);

        if enabled {
            current.insert(key);
        } else {
            current.remove(&key);
            if current.is_empty() {
                self.enabled_connections_by_instance.remove(instance_id);
            }
        }

        self.publish_if_changed(instance_id).await;
        true
    }

    pub async fn sync_instance(&self, instance_id: &str) {
        let enabled = self.is_enabled(instance_id);
        let event = build_voice_mode_event(enabled);
        self.channel.lock().await.send(instance_id, &event);
    }

    pub fn is_enabled(&self, instance_id: &str) -> bool {
        self.aggregate_by_instance.get(instance_id) == Some(&true)
    }

    async fn publish_if_changed(&mut self, instance_id: &str) {
        let enabled = self.enabled_connections_by_instance
            .get(instance_id)
            .map(|s| !s.is_empty())
            .unwrap_or(false);

        let previous = self.aggregate_by_instance.get(instance_id) == Some(&true);
        if enabled == previous {
            return;
        }

        if enabled {
            self.aggregate_by_instance.insert(instance_id.to_string(), true);
        } else {
            self.aggregate_by_instance.remove(instance_id);
        }

        let event = build_voice_mode_event(enabled);
        self.channel.lock().await.send(instance_id, &event);
    }
}

fn build_voice_mode_event(enabled: bool) -> PluginOutboundEvent {
    let mut properties = std::collections::HashMap::new();
    properties.insert("enabled".to_string(), serde_json::Value::Bool(enabled));
    properties.insert("formatVersion".to_string(), serde_json::Value::String("v1".to_string()));

    PluginOutboundEvent {
        event_type: "embeddedcowork.voiceMode".to_string(),
        properties: Some(properties),
    }
}

fn get_connection_key(connection: &ClientConnectionRef) -> String {
    format!("{}:{}", connection.client_id, connection.connection_id)
}
