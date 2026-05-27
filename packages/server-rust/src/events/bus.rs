use tokio::sync::broadcast;

use crate::api_types::WorkspaceEventPayload;
use crate::logger::Logger;

#[derive(Clone)]
pub struct EventBus {
    tx: broadcast::Sender<WorkspaceEventPayload>,
    logger: Option<Logger>,
}

impl EventBus {
    pub fn new(logger: Option<Logger>) -> Self {
        let (tx, _rx) = broadcast::channel(256);
        Self { tx, logger }
    }

    pub fn publish(&self, event: WorkspaceEventPayload) -> usize {
        if let Some(logger) = &self.logger {
            let event_type = match &event {
                WorkspaceEventPayload::WorkspaceCreated { event_type, .. } => event_type,
                WorkspaceEventPayload::WorkspaceUpdate { event_type, .. } => event_type,
                WorkspaceEventPayload::WorkspaceStarted { event_type, .. } => event_type,
                WorkspaceEventPayload::WorkspaceError { event_type, .. } => event_type,
                WorkspaceEventPayload::WorkspaceStopped { event_type, .. } => event_type,
                WorkspaceEventPayload::WorkspaceLog { event_type, .. } => event_type,
                WorkspaceEventPayload::SidecarUpdated { event_type, .. } => event_type,
                WorkspaceEventPayload::SidecarRemoved { event_type, .. } => event_type,
                WorkspaceEventPayload::StorageConfigChanged { event_type, .. } => event_type,
                WorkspaceEventPayload::StorageStateChanged { event_type, .. } => event_type,
                WorkspaceEventPayload::InstanceDataChanged { event_type, .. } => event_type,
                WorkspaceEventPayload::InstanceEvent { event_type, .. } => event_type,
                WorkspaceEventPayload::InstanceEventStatus { event_type, .. } => event_type,
            };
            if event_type != "instance.event" && event_type != "instance.eventStatus" {
                tracing::debug!(component = %logger.component, type = %event_type, "Publishing workspace event");
            }
        }

        self.tx.send(event).unwrap_or(0)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<WorkspaceEventPayload> {
        self.tx.subscribe()
    }
}
