use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Model Preferences ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPreference {
    pub provider_id: String,
    pub model_id: String,
}

pub type AgentModelSelection = HashMap<String, ModelPreference>;
pub type AgentModelSelections = HashMap<String, AgentModelSelection>;

// ── Preferences ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preferences {
    #[serde(default)]
    pub show_thinking_blocks: bool,
    #[serde(default = "default_thinking_expansion")]
    pub thinking_blocks_expansion: String,
    #[serde(default = "default_true")]
    pub show_timeline_tools: bool,
    #[serde(default)]
    pub prompt_submit_on_enter: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used_binary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    #[serde(default)]
    pub environment_variables: HashMap<String, String>,
    #[serde(default)]
    pub model_recents: Vec<ModelPreference>,
    #[serde(default)]
    pub model_favorites: Vec<ModelPreference>,
    #[serde(default)]
    pub model_thinking_selections: HashMap<String, String>,
    #[serde(default = "default_split")]
    pub diff_view_mode: String,
    #[serde(default = "default_expanded")]
    pub tool_output_expansion: String,
    #[serde(default = "default_expanded")]
    pub diagnostics_expansion: String,
    #[serde(default = "default_true")]
    pub show_usage_metrics: bool,
    #[serde(default = "default_true")]
    pub auto_cleanup_blank_sessions: bool,
    #[serde(default = "default_local")]
    pub listening_mode: String,
    #[serde(default = "default_debug")]
    pub log_level: String,
    #[serde(default = "default_project")]
    pub session_storage_mode: String,
    #[serde(default)]
    pub os_notifications_enabled: bool,
    #[serde(default)]
    pub os_notifications_allow_when_visible: bool,
    #[serde(default = "default_true")]
    pub notify_on_needs_input: bool,
    #[serde(default = "default_true")]
    pub notify_on_idle: bool,
}

fn default_thinking_expansion() -> String { "expanded".to_string() }
fn default_true() -> bool { true }
fn default_split() -> String { "split".to_string() }
fn default_expanded() -> String { "expanded".to_string() }
fn default_local() -> String { "local".to_string() }
fn default_debug() -> String { "DEBUG".to_string() }
fn default_project() -> String { "project".to_string() }

impl Default for Preferences {
    fn default() -> Self {
        Self {
            show_thinking_blocks: false,
            thinking_blocks_expansion: "expanded".to_string(),
            show_timeline_tools: true,
            prompt_submit_on_enter: false,
            last_used_binary: None,
            locale: None,
            environment_variables: HashMap::new(),
            model_recents: Vec::new(),
            model_favorites: Vec::new(),
            model_thinking_selections: HashMap::new(),
            diff_view_mode: "split".to_string(),
            tool_output_expansion: "expanded".to_string(),
            diagnostics_expansion: "expanded".to_string(),
            show_usage_metrics: true,
            auto_cleanup_blank_sessions: true,
            listening_mode: "local".to_string(),
            log_level: "DEBUG".to_string(),
            session_storage_mode: "project".to_string(),
            os_notifications_enabled: false,
            os_notifications_allow_when_visible: false,
            notify_on_needs_input: true,
            notify_on_idle: true,
        }
    }
}

// ── RecentFolder ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentFolder {
    pub path: String,
    pub last_accessed: u64,
}

// ── OpenCodeBinary ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenCodeBinary {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default)]
    pub last_used: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

// ── ConfigFile ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigFile {
    #[serde(default)]
    pub preferences: Preferences,
    #[serde(default)]
    pub recent_folders: Vec<RecentFolder>,
    #[serde(default)]
    pub opencode_binaries: Vec<OpenCodeBinary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
}

impl Default for ConfigFile {
    fn default() -> Self {
        Self {
            preferences: Preferences::default(),
            recent_folders: Vec::new(),
            opencode_binaries: Vec::new(),
            theme: None,
        }
    }
}

// ── ConfigYamlFile (persisted config, no recent_folders) ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigYamlFile {
    #[serde(default)]
    pub preferences: Preferences,
    #[serde(default)]
    pub opencode_binaries: Vec<OpenCodeBinary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
}

impl Default for ConfigYamlFile {
    fn default() -> Self {
        Self {
            preferences: Preferences::default(),
            opencode_binaries: Vec::new(),
            theme: None,
        }
    }
}

// ── StateFile ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateFile {
    #[serde(default)]
    pub recent_folders: Vec<RecentFolder>,
}

impl Default for StateFile {
    fn default() -> Self {
        Self {
            recent_folders: Vec::new(),
        }
    }
}
