use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};

const OUTPUT_BUFFER_MAX_LINES: usize = 10000;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct BgProcessNotification {
    pub session_id: String,
    pub directory: String,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct PluginBgProcess {
    pub id: String,
    pub workspace_id: String,
    pub title: String,
    pub command: String,
    pub cwd: String,
    pub status: String,
    pub pid: Option<u32>,
    pub started_at: String,
    pub stopped_at: Option<String>,
    pub exit_code: Option<i32>,
    pub terminal_reason: Option<String>,
    pub output_size_bytes: usize,
    pub notify: bool,
    pub notification: Option<BgProcessNotification>,
}

struct PluginBgProcessInternal {
    info: PluginBgProcess,
    output_buffer: Vec<String>,
    output_tx: broadcast::Sender<String>,
}

pub struct PluginBgProcessManager {
    processes: Arc<Mutex<HashMap<String, PluginBgProcessInternal>>>,
    workspace_index: Arc<Mutex<HashMap<String, Vec<String>>>>,
}

impl PluginBgProcessManager {
    pub fn new() -> Self {
        Self {
            processes: Arc::new(Mutex::new(HashMap::new())),
            workspace_index: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn list(&self, workspace_id: &str) -> Vec<PluginBgProcess> {
        let guard = self.workspace_index.lock().await;
        let processes = self.processes.lock().await;
        guard
            .get(workspace_id)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| processes.get(id))
                    .map(|p| p.info.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub async fn get(&self, id: &str) -> Option<PluginBgProcess> {
        self.processes.lock().await.get(id).map(|p| p.info.clone())
    }

    pub async fn start(
        &self,
        id: &str,
        workspace_id: &str,
        title: &str,
        command: &str,
        cwd: &str,
        notify: bool,
        notification: Option<BgProcessNotification>,
        notify_tx: Option<broadcast::Sender<serde_json::Value>>,
    ) -> Result<PluginBgProcess, String> {
        let mut shell_cmd = if cfg!(windows) {
            let comspec = std::env::var("ComSpec").unwrap_or_else(|_| "cmd.exe".to_string());
            let mut c = tokio::process::Command::new(&comspec);
            c.args(["/d", "/s", "/c", command]);
            c
        } else {
            let mut c = tokio::process::Command::new("bash");
            c.args(["-c", command]);
            c
        };

        let mut child = shell_cmd
            .current_dir(cwd)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to spawn process: {}", e))?;

        let pid = child.id();
        let started_at = chrono::Utc::now().to_rfc3339();

        let (output_tx, _) = broadcast::channel(256);

        let info = PluginBgProcess {
            id: id.to_string(),
            workspace_id: workspace_id.to_string(),
            title: title.to_string(),
            command: command.to_string(),
            cwd: cwd.to_string(),
            status: "running".to_string(),
            pid,
            started_at,
            stopped_at: None,
            exit_code: None,
            terminal_reason: None,
            output_size_bytes: 0,
            notify,
            notification: notification.clone(),
        };

        let internal = PluginBgProcessInternal {
            info: info.clone(),
            output_buffer: Vec::new(),
            output_tx: output_tx.clone(),
        };

        let proc_id = id.to_string();
        {
            let mut guard = self.processes.lock().await;
            guard.insert(proc_id.clone(), internal);
        }
        {
            let mut guard = self.workspace_index.lock().await;
            guard.entry(workspace_id.to_string()).or_default().push(proc_id);
        }

        // stdout reader
        if let Some(stdout) = child.stdout.take() {
            let task_id = id.to_string();
            let processes = self.processes.clone();
            let tx = output_tx.clone();
            tokio::spawn(async move {
                use tokio::io::AsyncBufReadExt;
                let reader = tokio::io::BufReader::new(stdout);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let size = line.len();
                    let _ = tx.send(line.clone());
                    let mut guard = processes.lock().await;
                    if let Some(p) = guard.get_mut(&task_id) {
                        p.output_buffer.push(line);
                        if p.output_buffer.len() > OUTPUT_BUFFER_MAX_LINES {
                            p.output_buffer.remove(0);
                        }
                        p.info.output_size_bytes += size;
                    } else {
                        break;
                    }
                }
            });
        }

        // stderr reader
        if let Some(stderr) = child.stderr.take() {
            let task_id = id.to_string();
            let processes = self.processes.clone();
            let tx = output_tx.clone();
            tokio::spawn(async move {
                use tokio::io::AsyncBufReadExt;
                let reader = tokio::io::BufReader::new(stderr);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let stderr_line = format!("[stderr] {}", line);
                    let size = stderr_line.len();
                    let _ = tx.send(stderr_line.clone());
                    let mut guard = processes.lock().await;
                    if let Some(p) = guard.get_mut(&task_id) {
                        p.output_buffer.push(stderr_line);
                        if p.output_buffer.len() > OUTPUT_BUFFER_MAX_LINES {
                            p.output_buffer.remove(0);
                        }
                        p.info.output_size_bytes += size;
                    } else {
                        break;
                    }
                }
            });
        }

        // exit handler
        let exit_id = id.to_string();
        let processes = self.processes.clone();
        let ws_id = workspace_id.to_string();
        let proc_title = title.to_string();
        let proc_cmd = command.to_string();
        let proc_notify = notify;
        let proc_notification = notification.clone();
        tokio::spawn(async move {
            let result = child.wait().await;
            let exit_code = result.ok().and_then(|s| s.code());
            let terminal_reason = if exit_code.map_or(true, |c| c != 0) {
                Some("failed")
            } else {
                Some("finished")
            };
            {
                let mut guard = processes.lock().await;
                if let Some(p) = guard.get_mut(&exit_id) {
                    p.info.status = "stopped".to_string();
                    p.info.stopped_at = Some(chrono::Utc::now().to_rfc3339());
                    p.info.exit_code = exit_code;
                    p.info.terminal_reason = terminal_reason.map(|s| s.to_string());
                }
            }
            // Send notification if configured
            if proc_notify {
                if let Some(ref tx) = notify_tx {
                    if let Some(ref notif) = proc_notification {
                        let payload = serde_json::json!({
                            "workspaceId": ws_id,
                            "event": "embeddedcowork.notification",
                            "properties": {
                                "sessionID": notif.session_id,
                                "directory": notif.directory,
                                "title": proc_title,
                                "command": proc_cmd,
                                "exitCode": exit_code,
                                "terminalReason": terminal_reason,
                            }
                        });
                        let _ = tx.send(payload);
                    }
                }
            }
        });

        Ok(info)
    }

    pub async fn update_status(&self, id: &str, status: &str, terminal_reason: &str) -> bool {
        let mut guard = self.processes.lock().await;
        if let Some(p) = guard.get_mut(id) {
            p.info.status = status.to_string();
            p.info.stopped_at = Some(chrono::Utc::now().to_rfc3339());
            p.info.terminal_reason = Some(terminal_reason.to_string());
            true
        } else {
            false
        }
    }

    pub async fn subscribe_output(&self, id: &str) -> Option<broadcast::Receiver<String>> {
        let guard = self.processes.lock().await;
        guard.get(id).map(|p| p.output_tx.subscribe())
    }

    pub async fn get_output_tx(&self, id: &str) -> Option<broadcast::Sender<String>> {
        let guard = self.processes.lock().await;
        guard.get(id).map(|p| p.output_tx.clone())
    }

    pub async fn read_output(
        &self,
        id: &str,
        method: &str,
        pattern: Option<&str>,
        line_count: Option<usize>,
        max_bytes: Option<usize>,
    ) -> Result<(String, bool, usize), String> {
        let guard = self.processes.lock().await;
        let p = guard.get(id).ok_or_else(|| "Process not found".to_string())?;

        let raw = p.output_buffer.join("\n");
        let total_size = p.info.output_size_bytes;
        let lines = line_count.unwrap_or(10);
        let max_b = max_bytes.unwrap_or(usize::MAX);

        let truncated = total_size > max_b;
        let content = if total_size > max_b {
            let bytes = raw.as_bytes();
            let start = bytes.len().saturating_sub(max_b);
            String::from_utf8_lossy(&bytes[start..]).to_string()
        } else {
            raw
        };

        let result = match method {
            "head" => content.lines().take(lines).collect::<Vec<_>>().join("\n"),
            "tail" => {
                let all_lines: Vec<&str> = content.lines().collect();
                let len = all_lines.len();
                let start = len.saturating_sub(lines);
                if start < len {
                    all_lines[start..].join("\n")
                } else {
                    String::new()
                }
            }
            "grep" => {
                let pat = pattern.unwrap_or("");
                if pat.is_empty() {
                    return Err("Pattern is required for grep output".to_string());
                }
                content
                    .lines()
                    .filter(|line| line.contains(pat))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
            _ => content,
        };

        Ok((result, truncated, total_size))
    }
}
