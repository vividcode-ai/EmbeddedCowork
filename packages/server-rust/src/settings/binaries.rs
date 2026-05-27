use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::settings::service::{DocKind, SettingsService};
use serde_json::Value;

/// Resolve a binary name to an absolute path using `where`/`which`.
fn resolve_on_path(name: &str) -> Option<String> {
    let cmd = if cfg!(windows) { "where" } else { "which" };
    let output = std::process::Command::new(cmd)
        .arg(name)
        .output()
        .ok()
        .filter(|o| o.status.success())?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let candidates: Vec<&str> = stdout
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && Path::new(l).is_absolute())
        .collect();

    if cfg!(windows) {
        // On Windows, `where` returns results in PATH order. The first result
        // is often the npm wrapper script (no .exe/.cmd extension) which
        // cannot be executed directly by Command::new(). Prefer .exe, then
        // .cmd/.bat — only fall back to extensionless files if nothing else
        // is available.
        let exe = candidates.iter().find(|p| p.ends_with(".exe"));
        if let Some(path) = exe {
            return Some((*path).to_string());
        }
        let cmd_bat = candidates.iter().find(|p| p.ends_with(".cmd") || p.ends_with(".bat"));
        if let Some(path) = cmd_bat {
            return Some((*path).to_string());
        }
    }

    // Return first absolute-line result (or None if empty).
    candidates.into_iter().next().map(|s| s.to_string())
}

#[derive(Debug, Clone)]
pub struct OpenCodeBinaryEntry {
    pub path: String,
    pub version: Option<String>,
    pub last_used: Option<u64>,
    pub label: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ResolvedBinary {
    pub path: String,
    pub label: String,
    pub version: Option<String>,
}

fn pretty_label(p: &str) -> String {
    let path = Path::new(p);
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| p.to_string())
}

fn read_ui_binaries(settings: &mut SettingsService) -> Vec<OpenCodeBinaryEntry> {
    let ui = settings.get_owner(&DocKind::State, "ui");
    let list = ui.get("opencodeBinaries");
    match list {
        Some(Value::Array(arr)) => arr
            .iter()
            .filter(|item| item.is_object() && item.get("path").and_then(|p| p.as_str()).is_some())
            .map(|item| OpenCodeBinaryEntry {
                path: item["path"].as_str().unwrap_or("").to_string(),
                version: item["version"].as_str().map(String::from),
                last_used: item["last_used"].as_u64(),
                label: item["label"].as_str().map(String::from),
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn read_default_binary_path(settings: &mut SettingsService) -> Option<String> {
    let server = settings.get_owner(&DocKind::Config, "server");
    server
        .get("opencodeBinary")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

#[derive(Debug, Clone)]
pub struct BinaryResolver {
    settings: Arc<Mutex<SettingsService>>,
}

impl BinaryResolver {
    pub fn new(settings: Arc<Mutex<SettingsService>>) -> Self {
        Self { settings }
    }

    pub async fn list(&mut self) -> Vec<OpenCodeBinaryEntry> {
        let mut settings = self.settings.lock().await;
        read_ui_binaries(&mut settings)
    }

    pub async fn resolve_default(&mut self) -> ResolvedBinary {
        let mut settings = self.settings.lock().await;
        let binaries = read_ui_binaries(&mut settings);
        let configured_default = read_default_binary_path(&mut settings);
        let fallback = binaries.first().map(|b| b.path.clone());
        let raw = configured_default.or(fallback).unwrap_or_else(|| "opencode".to_string());

        // Resolve non-absolute paths (e.g. "opencode") to absolute via PATH lookup
        let path = if Path::new(&raw).is_absolute() {
            raw.clone()
        } else {
            resolve_on_path(&raw).unwrap_or(raw.clone())
        };

        let entry = binaries.iter().find(|b| b.path == raw);
        ResolvedBinary {
            label: entry
                .and_then(|e| e.label.clone())
                .unwrap_or_else(|| pretty_label(&raw)),
            version: entry.and_then(|e| e.version.clone()),
            path,
        }
    }
}
