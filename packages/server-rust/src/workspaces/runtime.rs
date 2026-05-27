use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::io::AsyncBufReadExt;
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, oneshot};

use crate::api_types::{LogLevel, WorkspaceLogEntry};
use crate::events::bus::EventBus;
use crate::logger::Logger;
use crate::workspaces::spawn::build_spawn_spec;

const PORT_DETECTION_TIMEOUT_SECS: u64 = 30;

#[derive(Debug, Clone)]
pub struct ProcessExitInfo {
    pub workspace_id: String,
    pub code: Option<i32>,
    pub signal: Option<String>,
    pub requested: bool,
}

struct ManagedProcess {
    child: Arc<Mutex<Child>>,
    requested_stop: bool,
}

pub struct WorkspaceRuntime {
    processes: Arc<Mutex<HashMap<String, ManagedProcess>>>,
    event_bus: EventBus,
    logger: Logger,
}

impl WorkspaceRuntime {
    pub fn new(event_bus: EventBus, logger: Logger) -> Self {
        Self {
            processes: Arc::new(Mutex::new(HashMap::new())),
            event_bus,
            logger,
        }
    }

    pub async fn launch(
        &self,
        options: LaunchOptions,
    ) -> Result<LaunchResult, String> {
        self.validate_folder(&options.folder)?;

        let log_level = options
            .log_level
            .as_deref()
            .unwrap_or("DEBUG")
            .to_uppercase();

        let args = vec![
            "serve".to_string(),
            "--port".to_string(),
            "0".to_string(),
            "--print-logs".to_string(),
            "--log-level".to_string(),
            log_level,
        ];

        let empty_env = HashMap::new();
        let env = options.environment.as_ref().unwrap_or(&empty_env);

        let spec = build_spawn_spec(
            &options.binary_path,
            &args,
            &options.folder,
            env,
        );

        let command_line = format!("{} {}", spec.command, spec.args.join(" "));
        tracing::info!(
            component = %self.logger.component,
            workspace_id = %options.workspace_id,
            folder = %options.folder,
            binary = %options.binary_path,
            spawn_command = %spec.command,
            command_line = %command_line,
            "Launching OpenCode process"
        );

        let mut cmd = Command::new(&spec.command);
        cmd.args(&spec.args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        if let Some(ref dir) = spec.current_dir {
            cmd.current_dir(dir);
        }
        if let Some(ref extra_env) = spec.env {
            cmd.envs(extra_env);
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Failed to spawn OpenCode process: {}", e))?;

        let pid = child.id().unwrap_or(0);
        let workspace_id = options.workspace_id.clone();

        tracing::info!(
            component = %self.logger.component,
            workspace_id = %workspace_id,
            pid = pid,
            "OpenCode process spawned, waiting for port"
        );

        // Take stdout for port detection
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "Failed to capture stdout from OpenCode process".to_string())?;

        // Spawn background task to read stderr (prevents buffer deadlock)
        if let Some(stderr) = child.stderr.take() {
            let ws_id = workspace_id.clone();
            let logger = self.logger.child("opencode-stderr");
            let event_bus = self.event_bus.clone();
            tokio::spawn(async move {
                let reader = tokio::io::BufReader::new(stderr);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let trimmed = line.trim().to_string();
                    if trimmed.is_empty() {
                        continue;
                    }
                    tracing::debug!(
                        component = %logger.component,
                        workspace_id = %ws_id,
                        "[opencode:stderr] {}",
                        trimmed
                    );
                    event_bus.publish(WorkspaceEventPayload::WorkspaceLog {
                        event_type: "workspace.log".to_string(),
                        entry: WorkspaceLogEntry {
                            workspace_id: ws_id.clone(),
                            timestamp: chrono::Utc::now().to_rfc3339(),
                            level: LogLevel::Error,
                            message: trimmed,
                        },
                    });
                }
            });
        }

        // Port detection from stdout
        let (port_tx, port_rx) = oneshot::channel::<u16>();

        let ws_id_for_stdout = workspace_id.clone();
        let logger_for_stdout = self.logger.child("opencode-stdout");
        let event_bus_for_stdout = self.event_bus.clone();
        tokio::spawn(async move {
            let reader = tokio::io::BufReader::new(stdout);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let trimmed = line.trim().to_string();
                if trimmed.is_empty() {
                    continue;
                }

                tracing::trace!(
                    component = %logger_for_stdout.component,
                    workspace_id = %ws_id_for_stdout,
                    "[opencode:stdout] {}",
                    trimmed
                );

                event_bus_for_stdout.publish(WorkspaceEventPayload::WorkspaceLog {
                    event_type: "workspace.log".to_string(),
                    entry: WorkspaceLogEntry {
                        workspace_id: ws_id_for_stdout.clone(),
                        timestamp: chrono::Utc::now().to_rfc3339(),
                        level: LogLevel::Info,
                        message: trimmed.clone(),
                    },
                });

                // Detect port from: "opencode server listening on http://...:PORT"
                if let Some(pos) = trimmed.to_lowercase().find("opencode server listening on http://") {
                    let rest = &trimmed[pos + 34..]; // skip past "opencode server listening on http://"
                    if let Some(colon_pos) = rest.rfind(':') {
                        let port_str = rest[colon_pos + 1..]
                            .trim_end_matches(|c: char| !c.is_ascii_digit());
                        if let Ok(port) = port_str.parse::<u16>() {
                            tracing::info!(
                                component = %logger_for_stdout.component,
                                workspace_id = %ws_id_for_stdout,
                                port = %port,
                                "Detected OpenCode listening port"
                            );
                            let _ = port_tx.send(port);
                            return;
                        }
                    }
                }
            }
        });

        // Wait for port detection with timeout
        let port = tokio::time::timeout(
            std::time::Duration::from_secs(PORT_DETECTION_TIMEOUT_SECS),
            port_rx,
        )
        .await
        .map_err(|_| {
            format!(
                "OpenCode did not report a listening port within {} seconds",
                PORT_DETECTION_TIMEOUT_SECS
            )
        })?
        .map_err(|_| "Failed to read OpenCode port from stdout (channel closed)".to_string())?;

        let local_host = "127.0.0.1".to_string();

        let managed = ManagedProcess {
            child: Arc::new(Mutex::new(child)),
            requested_stop: false,
        };

        self.processes
            .lock()
            .await
            .insert(options.workspace_id.clone(), managed);

        tracing::info!(
            component = %self.logger.component,
            workspace_id = %workspace_id,
            pid = pid,
            port = port,
            "OpenCode process started successfully"
        );

        Ok(LaunchResult {
            pid,
            port,
            exit_promise: Box::pin(async move { None }),
            get_last_output: Arc::new(|| String::new()),
            local_host,
        })
    }

    pub async fn stop(&self, workspace_id: &str) -> Result<(), String> {
        let mut processes = self.processes.lock().await;
        if let Some(mut managed) = processes.remove(workspace_id) {
            managed.requested_stop = true;
            // Explicitly kill the child process (tokio::process::Child does
            // NOT auto-kill on drop).
            let mut child = managed.child.lock().await;
            let _ = child.start_kill();
        }
        Ok(())
    }

    pub async fn shutdown(&self) {
        let mut processes = self.processes.lock().await;
        for (_, managed) in processes.iter() {
            let mut child = managed.child.lock().await;
            let _ = child.start_kill();
        }
        processes.clear();
    }

    fn validate_folder(&self, folder: &str) -> Result<(), String> {
        let path = Path::new(folder);
        if !path.exists() {
            return Err(format!("Folder does not exist: {}", folder));
        }
        if !path.is_dir() {
            return Err(format!("Path is not a directory: {}", folder));
        }
        Ok(())
    }
}

use crate::api_types::WorkspaceEventPayload;

pub struct LaunchOptions {
    pub workspace_id: String,
    pub folder: String,
    pub binary_path: String,
    pub environment: Option<HashMap<String, String>>,
    pub log_level: Option<String>,
}

pub struct LaunchResult {
    pub pid: u32,
    pub port: u16,
    pub exit_promise: std::pin::Pin<
        Box<dyn std::future::Future<Output = Option<ProcessExitInfo>> + Send>,
    >,
    pub get_last_output: Arc<dyn Fn() -> String + Send + Sync>,
    pub local_host: String,
}
