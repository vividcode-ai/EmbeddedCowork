use std::collections::HashMap;
use std::net::TcpStream;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

use crate::api_types::{SideCar, SideCarKind, SideCarPrefixMode, SideCarStatus};
use crate::events::bus::EventBus;
use crate::logger::Logger;
use crate::settings::service::{DocKind, SettingsService};

const SIDECARS_STATE_KEY: &str = "sidecars";

pub struct SideCarManager {
    configs: HashMap<String, SideCarConfigRecord>,
    runtime: HashMap<String, SideCarRuntimeRecord>,
    event_bus: EventBus,
    #[allow(dead_code)]
    logger: Logger,
    settings: Option<Arc<Mutex<SettingsService>>>,
}

#[derive(Debug, Clone)]
struct SideCarConfigRecord {
    id: String,
    kind: SideCarKind,
    name: String,
    port: u16,
    insecure: bool,
    prefix_mode: SideCarPrefixMode,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone)]
struct SideCarRuntimeRecord {
    status: SideCarStatus,
}

impl SideCarManager {
    pub fn new(logger: Logger, event_bus: EventBus) -> Self {
        let mut mgr = Self {
            configs: HashMap::new(),
            runtime: HashMap::new(),
            event_bus,
            logger,
            settings: None,
        };

        let ids: Vec<String> = mgr.configs.keys().cloned().collect();
        for id in ids {
            mgr.refresh_port_sidecar(&id);
        }

        mgr
    }

    pub async fn with_settings(mut self, settings: Arc<Mutex<SettingsService>>) -> Self {
        self.load_configs(&settings).await;
        self.settings = Some(settings);
        let ids: Vec<String> = self.configs.keys().cloned().collect();
        for id in ids {
            self.refresh_port_sidecar(&id);
        }
        self
    }

    pub async fn list(&mut self) -> Vec<SideCar> {
        self.refresh_port_statuses();
        self.configs.values().map(|r| self.to_sidecar(r)).collect()
    }

    pub async fn get(&mut self, id: &str) -> Option<SideCar> {
        if !self.configs.contains_key(id) {
            return None;
        }
        self.refresh_port_sidecar(id);
        self.configs.get(id).map(|r| self.to_sidecar(r))
    }

    pub async fn create(&mut self, input: SideCarCreateInput) -> Result<SideCar, String> {
        let normalized_name = input.name.trim().to_string();
        let id = self.build_sidecar_id(&normalized_name)?;
        if self.configs.contains_key(&id) {
            return Err(format!("SideCar '{}' already exists", id));
        }

        let now = chrono::Utc::now().to_rfc3339();
        let record = SideCarConfigRecord {
            id: id.clone(),
            kind: input.kind,
            name: normalized_name,
            port: input.port,
            insecure: input.insecure,
            prefix_mode: input.prefix_mode,
            created_at: now.clone(),
            updated_at: now,
        };

        self.configs.insert(record.id.clone(), record.clone());
        self.runtime.insert(record.id.clone(), SideCarRuntimeRecord { status: SideCarStatus::Stopped });
        self.persist_configs().await;
        self.refresh_port_sidecar(&record.id);
        Ok(self.to_sidecar(&record))
    }

    pub async fn update(&mut self, id: &str, input: SideCarUpdateInput) -> Result<SideCar, String> {
        let mut record = self.configs.get(id).cloned().ok_or_else(|| "SideCar not found".to_string())?;

        if let Some(name) = input.name {
            record.name = name.trim().to_string();
        }
        if let Some(port) = input.port {
            record.port = port;
        }
        if let Some(insecure) = input.insecure {
            record.insecure = insecure;
        }
        if let Some(prefix_mode) = input.prefix_mode {
            record.prefix_mode = prefix_mode;
        }
        record.updated_at = chrono::Utc::now().to_rfc3339();

        self.configs.insert(id.to_string(), record.clone());
        self.persist_configs().await;
        self.refresh_port_sidecar(id);
        Ok(self.to_sidecar(&record))
    }

    pub async fn delete(&mut self, id: &str) -> bool {
        if self.configs.remove(id).is_none() {
            return false;
        }
        self.runtime.remove(id);
        self.persist_configs().await;
        self.event_bus.publish(
            crate::api_types::WorkspaceEventPayload::SidecarRemoved {
                event_type: "sidecar.removed".to_string(),
                sidecar_id: id.to_string(),
            },
        );
        true
    }

    pub async fn shutdown(&self) {}

    pub fn build_target_origin(&self, sidecar: &SideCar) -> String {
        let protocol = if sidecar.insecure { "http" } else { "https" };
        format!("{}://127.0.0.1:{}", protocol, sidecar.port)
    }

    pub fn build_proxy_base_path(&self, id: &str) -> String {
        format!("/sidecars/{}", urlencoding(id))
    }

    pub fn build_target_path(&self, id: &str, incoming_path: &str, search: &str) -> String {
        let record = match self.configs.get(id) {
            Some(r) => r,
            None => return String::new(),
        };
        let public_base = self.build_proxy_base_path(id);
        let normalized_path = if incoming_path.is_empty() { &public_base } else { incoming_path };

        if record.prefix_mode == SideCarPrefixMode::Preserve {
            return format!("{}{}", normalized_path, search);
        }

        let stripped = if normalized_path.starts_with(&public_base) {
            normalized_path[public_base.len()..].to_string()
        } else {
            normalized_path.to_string()
        };

        let stripped = if stripped.is_empty() || stripped == "/" {
            "/".to_string()
        } else if !stripped.starts_with('/') {
            format!("/{}", stripped)
        } else {
            stripped
        };

        format!("{}{}", stripped, search)
    }

    fn refresh_port_statuses(&mut self) {
        let ids: Vec<String> = self.configs.keys().cloned().collect();
        for id in ids {
            self.refresh_port_sidecar(&id);
        }
    }

    fn refresh_port_sidecar(&mut self, id: &str) {
        let record = match self.configs.get(id) {
            Some(r) => r.clone(),
            None => return,
        };

        let is_available = is_port_available(record.port);
        let current = self.runtime.get(id);
        let next_status = if is_available { SideCarStatus::Running } else { SideCarStatus::Stopped };

        if current.map(|r| &r.status) == Some(&next_status) {
            return;
        }

        self.runtime.insert(id.to_string(), SideCarRuntimeRecord { status: next_status });
        if let Some(record) = self.configs.get_mut(id) {
            record.updated_at = chrono::Utc::now().to_rfc3339();
            self.publish(id);
        }
    }

    fn publish(&self, id: &str) {
        if let Some(record) = self.configs.get(id) {
            self.event_bus.publish(
                crate::api_types::WorkspaceEventPayload::SidecarUpdated {
                    event_type: "sidecar.updated".to_string(),
                    sidecar: self.to_sidecar(record),
                },
            );
        }
    }

    fn to_sidecar(&self, record: &SideCarConfigRecord) -> SideCar {
        let runtime = self.runtime.get(&record.id);
        SideCar {
            id: record.id.clone(),
            kind: record.kind.clone(),
            name: record.name.clone(),
            port: record.port,
            insecure: record.insecure,
            prefix_mode: record.prefix_mode.clone(),
            status: runtime.map(|r| r.status.clone()).unwrap_or(SideCarStatus::Stopped),
            created_at: record.created_at.clone(),
            updated_at: record.updated_at.clone(),
        }
    }

    async fn persist_configs(&mut self) {
        let settings = match &self.settings {
            Some(s) => s,
            None => return,
        };

        let configs_json: Vec<serde_json::Value> = self.configs.values().map(|r| {
            serde_json::json!({
                "id": r.id,
                "kind": r.kind,
                "name": r.name,
                "port": r.port,
                "insecure": r.insecure,
                "prefix_mode": r.prefix_mode,
                "created_at": r.created_at,
                "updated_at": r.updated_at,
            })
        }).collect();

        let patch = serde_json::json!({
            SIDECARS_STATE_KEY: serde_json::Value::Array(configs_json)
        });

        let mut settings_lock = settings.lock().await;
        settings_lock.merge_patch_doc(&DocKind::State, &patch);
    }

    async fn load_configs(&mut self, settings: &Arc<Mutex<SettingsService>>) {
        let mut settings_lock = settings.lock().await;
        let doc = settings_lock.get_owner(&DocKind::State, SIDECARS_STATE_KEY);
        let arr = match doc.as_array() {
            Some(a) => a,
            None => return,
        };

        for item in arr {
            let id = item.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if id.is_empty() { continue; }

            let record = SideCarConfigRecord {
                id: id.clone(),
                kind: item.get("kind").and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or(SideCarKind::Port),
                name: item.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                port: item.get("port").and_then(|v| v.as_u64()).unwrap_or(0) as u16,
                insecure: item.get("insecure").and_then(|v| v.as_bool()).unwrap_or(false),
                prefix_mode: item.get("prefix_mode").and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or(SideCarPrefixMode::Strip),
                created_at: item.get("created_at").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                updated_at: item.get("updated_at").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            };

            self.configs.insert(id, record);
        }
    }

    fn build_sidecar_id(&self, name: &str) -> Result<String, String> {
        let normalized: String = name
            .to_lowercase()
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '-' })
            .collect::<String>()
            .chars()
            .fold(String::new(), |mut acc, c| {
                if c == '-' && acc.ends_with('-') {
                    // skip duplicate hyphens
                } else {
                    acc.push(c);
                }
                acc
            })
            .trim_matches('-')
            .to_string();

        if normalized.is_empty() {
            return Err("SideCar name must include letters or numbers".to_string());
        }

        Ok(normalized)
    }
}

#[derive(Debug)]
pub struct SideCarCreateInput {
    pub kind: SideCarKind,
    pub name: String,
    pub port: u16,
    pub insecure: bool,
    pub prefix_mode: SideCarPrefixMode,
}

#[derive(Debug)]
pub struct SideCarUpdateInput {
    pub name: Option<String>,
    pub port: Option<u16>,
    pub insecure: Option<bool>,
    pub prefix_mode: Option<SideCarPrefixMode>,
}

fn is_port_available(port: u16) -> bool {
    TcpStream::connect_timeout(
        &format!("127.0.0.1:{}", port).parse().unwrap(),
        Duration::from_millis(500),
    )
    .is_ok()
}

fn urlencoding(input: &str) -> String {
    percent_encoding::utf8_percent_encode(input, percent_encoding::NON_ALPHANUMERIC).to_string()
}
