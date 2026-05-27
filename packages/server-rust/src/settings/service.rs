use crate::config::location::ConfigLocation;
use crate::api_types::WorkspaceEventPayload;
use crate::logger::Logger;
use crate::settings::migrate::migrate_settings_layout;
use crate::settings::public_config::sanitize_config_owner;
use crate::settings::yaml_doc_store::{SettingsDoc, YamlDocStore};

use crate::events::bus::EventBus;

pub enum DocKind {
    Config,
    State,
}

pub struct SettingsService {
    location: ConfigLocation,
    event_bus: Option<EventBus>,
    config_store: YamlDocStore,
    state_store: YamlDocStore,
    #[allow(dead_code)]
    logger: Logger,
}

impl std::fmt::Debug for SettingsService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SettingsService")
            .field("location", &self.location)
            .finish()
    }
}

fn is_plain_object(value: &SettingsDoc) -> bool {
    matches!(value, serde_json::Value::Object(_))
}

fn is_deep_equal(a: &SettingsDoc, b: &SettingsDoc) -> bool {
    a == b
}

fn normalize_server_config_owner(value: &SettingsDoc) -> SettingsDoc {
    if !is_plain_object(value) {
        return serde_json::Value::Object(serde_json::Map::new());
    }

    let mut next = value.clone();
    if let Some(log_level) = next.get("logLevel").and_then(|v| v.as_str()) {
        let upper = log_level.trim().to_uppercase();
        match upper.as_str() {
            "DEBUG" | "INFO" | "WARN" | "ERROR" => {
                next.as_object_mut()
                    .map(|m| m.insert("logLevel".to_string(), serde_json::Value::String(upper)));
            }
            _ => {
                next.as_object_mut()
                    .map(|m| m.insert("logLevel".to_string(), serde_json::Value::String("DEBUG".to_string())));
            }
        }
    }
    next
}

fn normalize_config_doc(doc: &SettingsDoc) -> SettingsDoc {
    if !is_plain_object(doc) {
        return serde_json::Value::Object(serde_json::Map::new());
    }

    if !is_plain_object(doc.get("server").unwrap_or(&serde_json::Value::Null)) {
        return doc.clone();
    }

    let mut result = doc.clone();
    if let Some(server) = result.get("server") {
        let normalized = normalize_server_config_owner(server);
        result.as_object_mut().map(|m| m.insert("server".to_string(), normalized));
    }
    result
}

impl SettingsService {
    pub fn new(location: ConfigLocation, event_bus: Option<EventBus>, logger: Logger) -> Self {
        migrate_settings_layout(&location, &logger);
        let config_store = YamlDocStore::new(
            location.config_yaml_path.clone(),
            logger.child("settings-config"),
        );
        let state_store = YamlDocStore::new(
            location.state_yaml_path.clone(),
            logger.child("settings-state"),
        );
        Self {
            location,
            event_bus,
            config_store,
            state_store,
            logger,
        }
    }

    pub fn config_path(&self) -> String {
        self.location.config_yaml_path.to_string_lossy().to_string()
    }

    pub fn data_path(&self) -> String {
        self.location.base_dir.to_string_lossy().to_string()
    }

    pub fn to_public(&self) -> serde_json::Value {
        let doc = self.config_store.peek();
        let server = doc.get("server").cloned().unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
        serde_json::json!({
            "server": sanitize_config_owner("server", server),
        })
    }

    pub fn deep_merge(&mut self, patch: &serde_json::Value) -> serde_json::Value {
        self.config_store.merge_patch(patch)
    }

    pub fn save(&mut self, value: serde_json::Value) -> Result<(), String> {
        self.config_store.replace(value);
        Ok(())
    }

    pub fn get_doc(&mut self, kind: &DocKind) -> SettingsDoc {
        match kind {
            DocKind::State => self.state_store.get(),
            DocKind::Config => {
                let current = self.config_store.get();
                let normalized = normalize_config_doc(&current);
                if !is_deep_equal(&current, &normalized) {
                    self.config_store.replace(normalized.clone());
                }
                normalized
            }
        }
    }

    pub fn merge_patch_doc(&mut self, kind: &DocKind, patch: &SettingsDoc) -> SettingsDoc {
        let updated = match kind {
            DocKind::Config => {
                let merged = self.config_store.merge_patch(patch);
                self.config_store.replace(normalize_config_doc(&merged));
                merged
            }
            DocKind::State => self.state_store.merge_patch(patch),
        };
        self.publish(kind, "*", None);
        updated
    }

    pub fn get_owner(&mut self, kind: &DocKind, owner: &str) -> SettingsDoc {
        match kind {
            DocKind::State => self.state_store.get_owner(owner),
            DocKind::Config => {
                if owner == "server" {
                    normalize_server_config_owner(
                        &self.get_doc(&DocKind::Config)
                            .get("server")
                            .cloned()
                            .unwrap_or(serde_json::Value::Object(serde_json::Map::new())),
                    )
                } else {
                    self.get_doc(&DocKind::Config)
                        .get(owner)
                        .cloned()
                        .unwrap_or(serde_json::Value::Object(serde_json::Map::new()))
                }
            }
        }
    }

    pub fn merge_patch_owner(&mut self, kind: &DocKind, owner: &str, patch: &SettingsDoc) -> SettingsDoc {
        let updated = match kind {
            DocKind::Config => {
                if owner == "server" {
                    let merged = self.config_store.merge_patch_owner(owner, patch);
                    self.config_store.replace_owner(owner, &normalize_server_config_owner(&merged));
                    merged
                } else {
                    self.config_store.merge_patch_owner(owner, patch)
                }
            }
            DocKind::State => self.state_store.merge_patch_owner(owner, patch),
        };
        self.publish(kind, owner, Some(&updated));
        updated
    }

    fn publish(&mut self, kind: &DocKind, owner: &str, value: Option<&SettingsDoc>) {
        let event_bus = match &self.event_bus {
            Some(bus) => bus.clone(),
            None => return,
        };

        let event_type = match kind {
            DocKind::Config => "storage.configChanged",
            DocKind::State => "storage.stateChanged",
        };

        let next_value = match value {
            Some(v) => v.clone(),
            None => self.get_owner(kind, owner),
        };

        let payload = WorkspaceEventPayload::StorageConfigChanged {
            event_type: event_type.to_string(),
            owner: owner.to_string(),
            value: if matches!(kind, DocKind::Config) {
                sanitize_config_owner(owner, next_value)
            } else {
                next_value
            },
        };

        event_bus.publish(payload);
    }
}
