use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;

use axum::{
    extract::State,
    response::Json,
    routing::get,
    Router,
};

use crate::api_types::*;
use crate::events::bus::EventBus;
use crate::server::network_addresses::resolve_network_addresses;
use crate::settings::binaries::BinaryResolver;
use crate::settings::service::{DocKind, SettingsService};
use crate::workspaces::manager::WorkspaceManager;

#[derive(Clone)]
pub struct MetaRouteState {
    pub start_time: Arc<Mutex<Instant>>,
    pub host: String,
    pub port: u16,
    pub protocol: String,
    pub workspace_root: String,
    pub server_version: String,
    pub ui_version: Option<String>,
    pub ui_source: UiSource,
    pub event_bus: EventBus,
    pub settings: Arc<Mutex<SettingsService>>,
    pub binary_resolver: Arc<Mutex<BinaryResolver>>,
    pub workspace_manager: Arc<Mutex<WorkspaceManager>>,
}

impl MetaRouteState {
    pub fn new(
        host: String,
        port: u16,
        protocol: String,
        workspace_root: String,
        ui_version: Option<String>,
        ui_source: UiSource,
        event_bus: EventBus,
        settings: Arc<Mutex<SettingsService>>,
        binary_resolver: Arc<Mutex<BinaryResolver>>,
        workspace_manager: Arc<Mutex<WorkspaceManager>>,
    ) -> Self {
        Self {
            start_time: Arc::new(Mutex::new(Instant::now())),
            host,
            port,
            protocol,
            workspace_root,
            server_version: env!("CARGO_PKG_VERSION").to_string(),
            ui_version,
            ui_source,
            event_bus,
            settings,
            binary_resolver,
            workspace_manager,
        }
    }
}

pub fn meta_routes(state: MetaRouteState) -> Router {
    Router::new()
        .route("/api/meta", get(get_full_meta))
        .route("/api/meta/version", get(get_version))
        .route("/api/meta/ping", get(ping))
        .route("/api/meta/health", get(health))
        .with_state(state)
}

async fn get_version() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "name": "embeddedcowork-server",
    }))
}

async fn ping() -> Json<serde_json::Value> {
    Json(serde_json::json!({"pong": true}))
}

async fn health(
    State(state): State<MetaRouteState>,
) -> Json<serde_json::Value> {
    let uptime = state.start_time.lock().await.elapsed().as_secs();
    Json(serde_json::json!({
        "status": "ok",
        "uptime_secs": uptime,
        "uptime": format_duration(uptime),
    }))
}

async fn get_full_meta(
    State(state): State<MetaRouteState>,
) -> Json<serde_json::Value> {
    let uptime = state.start_time.lock().await.elapsed().as_secs();
    let local_url = format!("{}://{}:{}", state.protocol, state.host, state.port);
    let events_url = format!("{}/api/events", local_url);

    let addresses = resolve_network_addresses(&state.host, &state.protocol, state.port);

    // Determine listening mode
    let listening_mode = if state.host == "0.0.0.0" {
        ListeningMode::All
    } else {
        ListeningMode::Local
    };

    // Get host label from settings
    let host_label = {
        let mut settings = state.settings.lock().await;
        let server = settings.get_owner(&DocKind::Config, "server");
        server
            .get("hostLabel")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| state.host.clone())
    };

    // Get binary info for opencode status
    let (binary_available, _binary_version, _binary_path) = {
        let mut resolver = state.binary_resolver.lock().await;
        let resolved = resolver.resolve_default().await;
        (
            !resolved.path.is_empty(),
            resolved.version.clone(),
            resolved.path.clone(),
        )
    };

    // Build OpenCode support meta
    let support = SupportMeta {
        supported: binary_available,
        message: if binary_available { None } else { Some("No OpenCode binary configured".to_string()) },
        min_server_version: None,
        latest_server_version: None,
        latest_server_url: None,
    };

    // UI info
    let ui = UiMeta {
        version: state.ui_version.clone(),
        ui_source: state.ui_source.clone(),
    };

    // Update info (from release monitor - None for now, populated by the release monitor)
    let update: Option<LatestReleaseInfo> = None;

    Json(serde_json::json!({
        "localUrl": local_url,
        "eventsUrl": events_url,
        "host": state.host,
        "listeningMode": listening_mode,
        "localPort": state.port,
        "hostLabel": host_label,
        "workspaceRoot": state.workspace_root,
        "addresses": addresses,
        "serverVersion": state.server_version,
        "uptimeSecs": uptime,
        "uptime": format_duration(uptime),
        "ui": ui,
        "support": support,
        "update": update,
    }))
}

fn format_duration(secs: u64) -> String {
    let hours = secs / 3600;
    let minutes = (secs % 3600) / 60;
    let seconds = secs % 60;
    if hours > 0 {
        format!("{}h {}m {}s", hours, minutes, seconds)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, seconds)
    } else {
        format!("{}s", seconds)
    }
}
