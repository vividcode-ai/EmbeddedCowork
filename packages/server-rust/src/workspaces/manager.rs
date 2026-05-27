use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};

use sha2::{Digest, Sha256};

use crate::api_types::{
    FileSystemEntry, FileSystemEntryType, WorkspaceDescriptor, WorkspaceEventPayload,
    WorkspaceFileResponse, WorkspaceStatus,
};
use crate::events::bus::EventBus;
use crate::logger::Logger;
use crate::opencode_config::OpenCodeConfig;
use crate::settings::binaries::BinaryResolver;
use crate::workspaces::opencode_auth::{
    build_opencode_basic_auth_header, generate_opencode_server_password,
    DEFAULT_OPENCODE_USERNAME, OPENCODE_SERVER_PASSWORD_ENV, OPENCODE_SERVER_USERNAME_ENV,
};
use crate::workspaces::runtime::{LaunchOptions, WorkspaceRuntime};

pub struct WorkspaceManager {
    workspaces: Arc<Mutex<HashMap<String, WorkspaceRecord>>>,
    runtime: WorkspaceRuntime,
    root_dir: String,
    event_bus: EventBus,
    binary_resolver: BinaryResolver,
    #[allow(dead_code)]
    settings: SettingsService,
    #[allow(dead_code)]
    logger: Logger,
    opencode_auth: Arc<Mutex<HashMap<String, OpenCodeAuthEntry>>>,
    /// The server's own base URL (e.g. http://127.0.0.1:18081).
    /// Set after the server starts listening. Uses std::sync::Mutex because
    /// it is set once during startup from a sync context and only read later.
    server_base_url: Arc<std::sync::Mutex<Option<String>>>,
}

use crate::settings::service::SettingsService;

#[derive(Clone)]
struct WorkspaceRecord {
    id: String,
    path: String,
    name: Option<String>,
    status: WorkspaceStatus,
    pid: Option<u32>,
    port: Option<u16>,
    proxy_path: String,
    binary_id: String,
    binary_label: String,
    binary_version: Option<String>,
    created_at: String,
    updated_at: String,
    error: Option<String>,
}

#[derive(Clone)]
struct OpenCodeAuthEntry {
    #[allow(dead_code)]
    username: String,
    #[allow(dead_code)]
    password: String,
    authorization: String,
}

impl WorkspaceManager {
    pub fn new(
        root_dir: String,
        settings: SettingsService,
        binary_resolver: BinaryResolver,
        event_bus: EventBus,
        logger: Logger,
    ) -> Self {
        let runtime = WorkspaceRuntime::new(event_bus.clone(), logger.child("runtime"));

        Self {
            workspaces: Arc::new(Mutex::new(HashMap::new())),
            runtime,
            root_dir,
            event_bus,
            binary_resolver,
            settings,
            logger,
            opencode_auth: Arc::new(Mutex::new(HashMap::new())),
            server_base_url: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    /// Set the server's base URL after the server starts listening.
    /// This is used to set EMBEDDEDCOWORK_BASE_URL for the OpenCode process.
    pub fn set_server_base_url(&self, url: &str) {
        if let Ok(mut stored) = self.server_base_url.lock() {
            *stored = Some(url.to_string());
        }
    }

    pub async fn list(&self) -> Vec<WorkspaceDescriptor> {
        let workspaces = self.workspaces.lock().await;
        workspaces.values().map(|w| w.to_descriptor()).collect()
    }

    pub async fn get(&self, id: &str) -> Option<WorkspaceDescriptor> {
        let workspaces = self.workspaces.lock().await;
        workspaces.get(id).map(|w| w.to_descriptor())
    }

    pub async fn get_instance_port(&self, id: &str) -> Option<u16> {
        let workspaces = self.workspaces.lock().await;
        workspaces.get(id).and_then(|w| w.port)
    }

    pub async fn get_instance_authorization_header(&self, id: &str) -> Option<String> {
        let auth = self.opencode_auth.lock().await;
        auth.get(id).map(|e| e.authorization.clone())
    }

    pub async fn create(
        &mut self,
        folder: &str,
        name: Option<String>,
    ) -> Result<WorkspaceDescriptor, String> {
        let workspace_path = if Path::new(folder).is_absolute() {
            folder.to_string()
        } else {
            Path::new(&self.root_dir)
                .join(folder)
                .to_string_lossy()
                .to_string()
        };

        let id = compute_workspace_id(&workspace_path);
        let binary = self.binary_resolver.resolve_default().await;

        // Check that binary exists
        let binary_path = binary.path.clone();
        if !Path::new(&binary_path).exists() {
            return Err(format!(
                "OpenCode binary not found at: {}. Please configure the correct path in settings.",
                binary_path
            ));
        }

        let proxy_path = format!("/workspaces/{}/worktrees/root/instance", id);

        // ── Step 1: Create the workspace record ──
        let record = WorkspaceRecord {
            id: id.clone(),
            path: workspace_path.clone(),
            name,
            status: WorkspaceStatus::Starting,
            pid: None,
            port: None,
            proxy_path,
            binary_id: binary.path.clone(),
            binary_label: binary.label,
            binary_version: binary.version,
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
            error: None,
        };

        self.workspaces
            .lock()
            .await
            .insert(id.clone(), record.clone());

        self.event_bus.publish(WorkspaceEventPayload::WorkspaceCreated {
            event_type: "workspace.created".to_string(),
            workspace: record.to_descriptor(),
        });

        // ── Step 2: Set up OpenCode basic auth ──
        let username = DEFAULT_OPENCODE_USERNAME.to_string();
        let password = generate_opencode_server_password();
        let authorization =
            build_opencode_basic_auth_header(&username, &password)
                .ok_or_else(|| "Failed to build OpenCode auth header".to_string())?;

        self.opencode_auth.lock().await.insert(
            id.clone(),
            OpenCodeAuthEntry {
                username: username.clone(),
                password: password.clone(),
                authorization: authorization.clone(),
            },
        );

        // ── Step 3: Build environment variables for OpenCode ──
        let mut env = HashMap::new();

        // OpenCode auth
        env.insert(
            OPENCODE_SERVER_USERNAME_ENV.to_string(),
            username,
        );
        env.insert(
            OPENCODE_SERVER_PASSWORD_ENV.to_string(),
            password,
        );

        // OpenCode config directory
        // NOTE: Must use forward slashes on Windows — OpenCode constructs
        // file:// URLs from this path and backslashes produce invalid URLs
        // like "file://%3F\D:\...".
        let opencode_cfg = OpenCodeConfig::new();
        let config_dir_str = opencode_cfg
            .config_dir
            .to_string_lossy()
            .to_string()
            .replace('\\', "/");
        println!(
            "[WORKSPACE] OPENCODE_CONFIG_DIR={}",
            config_dir_str,
        );
        env.insert(
            "OPENCODE_CONFIG_DIR".to_string(),
            config_dir_str,
        );

        // Session database path (project-local mode)
        let db_path = Path::new(&workspace_path)
            .join(".embeddedcowork")
            .join("session")
            .join("data.db");
        if let Some(parent) = db_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        env.insert("OPENCODE_DB".to_string(), db_path.to_string_lossy().to_string());

        // Instance identity
        env.insert("EMBEDDEDCOWORK_INSTANCE_ID".to_string(), id.clone());

        // Server base URL (for callbacks)
        let base_url = self.server_base_url.lock().ok().and_then(|u| u.clone());
        if let Some(ref url) = base_url {
            env.insert("EMBEDDEDCOWORK_BASE_URL".to_string(), url.clone());
        }

        // ── Step 4: Launch OpenCode process ──
        let result = self
            .runtime
            .launch(LaunchOptions {
                workspace_id: id.clone(),
                folder: workspace_path.clone(),
                binary_path,
                environment: Some(env),
                log_level: None,
            })
            .await;

        // ── Step 5: Handle launch result ──
        match result {
            Ok(launch) => {
                // ── Step 5a: Health probe + stability delay ──
                // Aligns with TS server behavior (waitForPortAvailability +
                // waitForInstanceHealth + STARTUP_STABILITY_DELAY_MS).
                let auth_header = self
                    .opencode_auth
                    .lock()
                    .await
                    .get(&id)
                    .map(|e| e.authorization.clone());

                let healthy = Self::probe_instance(launch.port, &auth_header).await;
                sleep(Duration::from_millis(1500)).await;

                let mut workspaces = self.workspaces.lock().await;
                if let Some(record) = workspaces.get_mut(&id) {
                    record.pid = Some(launch.pid);
                    record.port = Some(launch.port);
                    record.status = if healthy {
                        WorkspaceStatus::Ready
                    } else {
                        WorkspaceStatus::Error
                    };
                    record.updated_at = chrono::Utc::now().to_rfc3339();
                    let descriptor = record.to_descriptor();
                    drop(workspaces);

                    if healthy {
                        self.event_bus.publish(
                            WorkspaceEventPayload::WorkspaceStarted {
                                event_type: "workspace.started".to_string(),
                                workspace: descriptor.clone(),
                            },
                        );

                        tracing::info!(
                            component = %self.logger.component,
                            workspace_id = %id,
                            port = %launch.port,
                            pid = %launch.pid,
                            "Workspace ready"
                        );

                        Ok(descriptor)
                    } else {
                        self.event_bus.publish(
                            WorkspaceEventPayload::WorkspaceError {
                                event_type: "workspace.error".to_string(),
                                workspace: descriptor.clone(),
                            },
                        );

                        Err("Instance health check failed".to_string())
                    }
                } else {
                    Err("Workspace was deleted before it finished starting".to_string())
                }
            }
            Err(error) => {
                tracing::error!(
                    component = %self.logger.component,
                    workspace_id = %id,
                    error = %error,
                    "Failed to start OpenCode"
                );

                // Update workspace status to error
                let mut workspaces = self.workspaces.lock().await;
                if let Some(record) = workspaces.get_mut(&id) {
                    record.status = WorkspaceStatus::Error;
                    record.error = Some(error.clone());
                    record.updated_at = chrono::Utc::now().to_rfc3339();
                    let descriptor = record.to_descriptor();
                    drop(workspaces);

                    self.event_bus.publish(WorkspaceEventPayload::WorkspaceError {
                        event_type: "workspace.error".to_string(),
                        workspace: descriptor,
                    });
                }

                Err(error)
            }
        }
    }

    pub async fn delete(&mut self, id: &str) -> Option<WorkspaceDescriptor> {
        let mut workspaces = self.workspaces.lock().await;
        let workspace = workspaces.remove(id)?;

        // Stop the runtime process
        let _ = self.runtime.stop(id).await;

        self.opencode_auth.lock().await.remove(id);

        self.event_bus.publish(WorkspaceEventPayload::WorkspaceStopped {
            event_type: "workspace.stopped".to_string(),
            workspace_id: id.to_string(),
        });

        Some(workspace.to_descriptor())
    }

    pub async fn shutdown(&self) {
        tracing::info!(component = %self.logger.component, "Shutting down all workspaces");
        self.runtime.shutdown().await;
        self.workspaces.lock().await.clear();
        self.opencode_auth.lock().await.clear();
    }

    pub async fn list_files(
        &self,
        workspace_id: &str,
        relative_path: &str,
    ) -> Result<Vec<FileSystemEntry>, String> {
        let workspace_path = {
            let workspaces = self.workspaces.lock().await;
            workspaces
                .get(workspace_id)
                .map(|w| w.path.clone())
                .ok_or_else(|| "Workspace not found".to_string())?
        };

        let dir_path = Path::new(&workspace_path).join(relative_path);
        let mut entries = tokio::fs::read_dir(&dir_path)
            .await
            .map_err(|e| format!("Failed to read directory: {}", e))?;

        let mut result = Vec::new();
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| format!("Failed to read entry: {}", e))?
        {
            let metadata = entry
                .metadata()
                .await
                .map_err(|e| format!("Failed to read metadata: {}", e))?;

            let entry_path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            let is_dir = metadata.is_dir();

            let rel_path = entry_path
                .strip_prefix(Path::new(&workspace_path))
                .unwrap_or(&entry_path)
                .to_string_lossy()
                .to_string();

            let absolute_path = entry_path.to_string_lossy().to_string();

            let modified_at = metadata.modified().ok().and_then(|t| {
                let duration = t.duration_since(std::time::UNIX_EPOCH).ok()?;
                chrono::DateTime::from_timestamp(
                    duration.as_secs() as i64,
                    duration.subsec_nanos(),
                )
                .map(|dt| dt.to_rfc3339())
            });

            result.push(FileSystemEntry {
                name,
                path: rel_path,
                absolute_path: Some(absolute_path),
                entry_type: if is_dir {
                    FileSystemEntryType::Directory
                } else {
                    FileSystemEntryType::File
                },
                size: if metadata.is_file() {
                    Some(metadata.len())
                } else {
                    None
                },
                modified_at,
            });
        }

        Ok(result)
    }

    pub async fn read_file(
        &self,
        workspace_id: &str,
        relative_path: &str,
    ) -> Result<WorkspaceFileResponse, String> {
        let workspace_path = {
            let workspaces = self.workspaces.lock().await;
            workspaces
                .get(workspace_id)
                .map(|w| w.path.clone())
                .ok_or_else(|| "Workspace not found".to_string())?
        };

        let full_path = Path::new(&workspace_path).join(relative_path);
        let contents = tokio::fs::read_to_string(&full_path)
            .await
            .map_err(|e| format!("Failed to read file: {}", e))?;

        Ok(WorkspaceFileResponse {
            workspace_id: workspace_id.to_string(),
            relative_path: relative_path.to_string(),
            contents,
        })
    }

    pub async fn write_file(
        &self,
        workspace_id: &str,
        relative_path: &str,
        contents: &str,
    ) -> Result<(), String> {
        let workspace_path = {
            let workspaces = self.workspaces.lock().await;
            workspaces
                .get(workspace_id)
                .map(|w| w.path.clone())
                .ok_or_else(|| "Workspace not found".to_string())?
        };

        let full_path = Path::new(&workspace_path).join(relative_path);
        if let Some(parent) = full_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| format!("Failed to create parent directories: {}", e))?;
        }

        tokio::fs::write(&full_path, contents)
            .await
            .map_err(|e| format!("Failed to write file: {}", e))?;

        Ok(())
    }

    /// Probe OpenCode instance health endpoint, matching TS server probeInstance().
    /// Returns `true` if the instance responds with `{ healthy: true }`.
    async fn probe_instance(port: u16, auth_header: &Option<String>) -> bool {
        let url = format!("http://127.0.0.1:{}/global/health", port);

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .ok();

        let client = match client {
            Some(c) => c,
            None => return false,
        };

        let mut req = client.get(&url);
        if let Some(auth) = auth_header {
            req = req.header("Authorization", auth);
        }

        let resp = match req.send().await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(%port, error = %e, "Health probe request failed");
                return false;
            }
        };

        if !resp.status().is_success() {
            tracing::warn!(%port, status = %resp.status().as_u16(), "Health probe returned non-success");
            return false;
        }

        let payload: serde_json::Value = match resp.json().await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(%port, error = %e, "Health probe response was not valid JSON");
                return false;
            }
        };

        let healthy = payload
            .get("healthy")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if !healthy {
            tracing::warn!(%port, payload = %payload, "Health probe returned unhealthy");
        }

        healthy
    }
}

fn compute_workspace_id(folder: &str) -> String {
    let abs_path =
        std::path::absolute(folder).unwrap_or_else(|_| Path::new(folder).to_path_buf());
    let hash = Sha256::digest(abs_path.to_string_lossy().as_bytes());
    hex::encode(&hash[..6])
}

impl WorkspaceRecord {
    fn to_descriptor(&self) -> WorkspaceDescriptor {
        WorkspaceDescriptor {
            id: self.id.clone(),
            path: self.path.clone(),
            name: self.name.clone(),
            status: self.status.clone(),
            pid: self.pid,
            port: self.port,
            proxy_path: self.proxy_path.clone(),
            binary_id: self.binary_id.clone(),
            binary_label: self.binary_label.clone(),
            binary_version: self.binary_version.clone(),
            created_at: self.created_at.clone(),
            updated_at: self.updated_at.clone(),
            error: self.error.clone(),
        }
    }
}
