use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};

use crate::api_types::{BackgroundProcessStatus, BackgroundProcessTerminalReason};
use crate::logger::Logger;

const OUTPUT_BUFFER_MAX: usize = 10000;

#[derive(Clone, Debug, serde::Serialize)]
pub struct BgProcessOutputEvent {
    pub id: String,
    pub data: String,
    pub is_stderr: bool,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct BgProcessExitEvent {
    pub id: String,
    pub exit_code: Option<i32>,
    pub terminal_reason: Option<BackgroundProcessTerminalReason>,
}

#[derive(Clone, Debug)]
pub struct BgProcessInfo {
    pub id: String,
    pub command: String,
    pub pid: Option<u32>,
    pub status: String,
    pub created_at: String,
}

pub struct BgProcessManager {
    processes: Arc<Mutex<HashMap<String, BgProcessManaged>>>,
    output_tx: broadcast::Sender<BgProcessOutputEvent>,
    exit_tx: broadcast::Sender<BgProcessExitEvent>,
    #[allow(dead_code)]
    logger: Logger,
}

struct BgProcessManaged {
    id: String,
    command: String,
    #[allow(dead_code)]
    args: Vec<String>,
    #[allow(dead_code)]
    cwd: Option<String>,
    pid: Option<u32>,
    status: BackgroundProcessStatus,
    created_at: String,
    #[allow(dead_code)]
    stopped_at: Option<String>,
    #[allow(dead_code)]
    exit_code: Option<i32>,
    #[allow(dead_code)]
    terminal_reason: Option<BackgroundProcessTerminalReason>,
    #[allow(dead_code)]
    output_buffer: Vec<String>,
}

impl BgProcessInfo {
    fn from_managed(m: &BgProcessManaged) -> Self {
        let status = match m.status {
            BackgroundProcessStatus::Running => "running",
            BackgroundProcessStatus::Stopped => "stopped",
            BackgroundProcessStatus::Error => "error",
        };
        Self {
            id: m.id.clone(),
            command: m.command.clone(),
            pid: m.pid,
            status: status.to_string(),
            created_at: m.created_at.clone(),
        }
    }
}

impl BgProcessManager {
    pub fn new(logger: Logger) -> Self {
        let (output_tx, _) = broadcast::channel(256);
        let (exit_tx, _) = broadcast::channel(64);
        Self {
            processes: Arc::new(Mutex::new(HashMap::new())),
            output_tx,
            exit_tx,
            logger,
        }
    }

    pub async fn list(&self) -> Vec<BgProcessInfo> {
        let guard = self.processes.lock().await;
        guard.values().map(BgProcessInfo::from_managed).collect()
    }

    pub async fn get(&self, id: &str) -> Option<BgProcessInfo> {
        let guard = self.processes.lock().await;
        guard.get(id).map(BgProcessInfo::from_managed)
    }

    pub fn subscribe_output(&self) -> broadcast::Receiver<BgProcessOutputEvent> {
        self.output_tx.subscribe()
    }

    pub fn subscribe_exit(&self) -> broadcast::Receiver<BgProcessExitEvent> {
        self.exit_tx.subscribe()
    }

    pub async fn execute(
        &self,
        id: &str,
        command: &str,
        args: &[String],
        cwd: Option<&str>,
    ) -> Result<u32, String> {
        use tokio::io::AsyncBufReadExt;

        let mut cmd = tokio::process::Command::new(command);
        cmd.args(args);
        if let Some(dir) = cwd {
            cmd.current_dir(dir);
        }
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| format!("Failed to spawn process: {}", e))?;
        let pid = child.id().ok_or("Failed to get PID")?;

        {
            let mut guard = self.processes.lock().await;
            guard.insert(
                id.to_string(),
                BgProcessManaged {
                    id: id.to_string(),
                    command: command.to_string(),
                    args: args.to_vec(),
                    cwd: cwd.map(|s| s.to_string()),
                    pid: Some(pid),
                    status: BackgroundProcessStatus::Running,
                    created_at: chrono::Utc::now().to_rfc3339(),
                    stopped_at: None,
                    exit_code: None,
                    terminal_reason: None,
                    output_buffer: Vec::new(),
                },
            );
        }

        // -- stdout reader task --
        if let Some(stdout) = child.stdout.take() {
            let id_task = id.to_string();
            let processes = self.processes.clone();
            let tx = self.output_tx.clone();
            tokio::spawn(async move {
                let reader = tokio::io::BufReader::new(stdout);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let _ = tx.send(BgProcessOutputEvent {
                        id: id_task.clone(),
                        data: line.clone(),
                        is_stderr: false,
                    });
                    let mut guard = processes.lock().await;
                    if let Some(managed) = guard.get_mut(&id_task) {
                        managed.output_buffer.push(line);
                        if managed.output_buffer.len() > OUTPUT_BUFFER_MAX {
                            managed.output_buffer.remove(0);
                        }
                    }
                }
            });
        }

        // -- stderr reader task --
        if let Some(stderr) = child.stderr.take() {
            let id_task = id.to_string();
            let processes = self.processes.clone();
            let tx = self.output_tx.clone();
            tokio::spawn(async move {
                let reader = tokio::io::BufReader::new(stderr);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let _ = tx.send(BgProcessOutputEvent {
                        id: id_task.clone(),
                        data: line.clone(),
                        is_stderr: true,
                    });
                    let mut guard = processes.lock().await;
                    if let Some(managed) = guard.get_mut(&id_task) {
                        managed.output_buffer.push(line);
                        if managed.output_buffer.len() > OUTPUT_BUFFER_MAX {
                            managed.output_buffer.remove(0);
                        }
                    }
                }
            });
        }

        // -- wait / exit task --
        let id_task = id.to_string();
        let processes = self.processes.clone();
        let tx = self.exit_tx.clone();
        tokio::spawn(async move {
            let result = child.wait().await;
            let exit_code = result.ok().and_then(|s| s.code());
            let terminal_reason = if exit_code.map_or(true, |c| c != 0) {
                Some(BackgroundProcessTerminalReason::Failed)
            } else {
                Some(BackgroundProcessTerminalReason::Finished)
            };

            let mut guard = processes.lock().await;
            if let Some(managed) = guard.get_mut(&id_task) {
                managed.status = BackgroundProcessStatus::Stopped;
                managed.stopped_at = Some(chrono::Utc::now().to_rfc3339());
                managed.exit_code = exit_code;
                managed.terminal_reason = terminal_reason.clone();
            }

            let _ = tx.send(BgProcessExitEvent {
                id: id_task,
                exit_code,
                terminal_reason,
            });
        });

        Ok(pid)
    }

    pub async fn kill(&self, id: &str) -> bool {
        let pid_to_kill = {
            let guard = self.processes.lock().await;
            guard.get(id).and_then(|p| p.pid)
        };

        if let Some(pid) = pid_to_kill {
            #[cfg(unix)]
            let _ = std::process::Command::new("kill")
                .arg(pid.to_string())
                .spawn();
            #[cfg(windows)]
            let _ = std::process::Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/F"])
                .spawn();

            let terminal_reason = Some(BackgroundProcessTerminalReason::UserTerminated);
            let id_str = id.to_string();

            {
                let mut guard = self.processes.lock().await;
                if let Some(managed) = guard.get_mut(id) {
                    managed.status = BackgroundProcessStatus::Stopped;
                    managed.stopped_at = Some(chrono::Utc::now().to_rfc3339());
                    managed.exit_code = None;
                    managed.terminal_reason = terminal_reason.clone();
                }
            }

            let _ = self.exit_tx.send(BgProcessExitEvent {
                id: id_str,
                exit_code: None,
                terminal_reason,
            });
        }

        self.processes.lock().await.contains_key(id)
    }

    pub async fn shutdown(&self) {
        let ids: Vec<String> = {
            let guard = self.processes.lock().await;
            guard.keys().cloned().collect()
        };
        for id in &ids {
            self.kill(id).await;
        }
        self.processes.lock().await.clear();
    }
}
