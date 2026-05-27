use serde::{Deserialize, Serialize};

// ── Workspace ──

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceStatus {
    Downloading,
    Starting,
    Ready,
    Stopped,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceDescriptor {
    pub id: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub status: WorkspaceStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    pub proxy_path: String,
    pub binary_id: String,
    pub binary_label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binary_version: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceCreateRequest {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

pub type WorkspaceCreateResponse = WorkspaceDescriptor;
pub type WorkspaceListResponse = Vec<WorkspaceDescriptor>;
pub type WorkspaceDetailResponse = WorkspaceDescriptor;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceDeleteResponse {
    pub id: String,
    pub status: WorkspaceStatus,
}

// ── Worktree ──

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WorktreeKind {
    Root,
    Worktree,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeDescriptor {
    pub slug: String,
    pub directory: String,
    pub kind: WorktreeKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeListResponse {
    pub worktrees: Vec<WorktreeDescriptor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_git_repo: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeCreateRequest {
    pub slug: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeMap {
    pub version: u8,
    pub default_worktree_slug: String,
    pub parent_session_worktree_slug: std::collections::HashMap<String, String>,
}

// ── Git ──

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum GitChangeKind {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    Untracked,
    Unmerged,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeGitStatusEntry {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_path: Option<String>,
    pub staged_status: Option<GitChangeKind>,
    pub staged_additions: u64,
    pub staged_deletions: u64,
    pub unstaged_status: Option<GitChangeKind>,
    pub unstaged_additions: u64,
    pub unstaged_deletions: u64,
}

pub type WorktreeGitStatusResponse = Vec<WorktreeGitStatusEntry>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WorktreeGitDiffScope {
    Staged,
    Unstaged,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeGitPathsRequest {
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeGitMutationResponse {
    pub ok: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeGitCommitRequest {
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeGitCommitResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_sha: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeGitDiffResponse {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_path: Option<String>,
    pub scope: WorktreeGitDiffScope,
    pub before: String,
    pub after: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_binary: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeGitDiffRequest {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_path: Option<String>,
    pub scope: WorktreeGitDiffScope,
}

// ── Logging ──

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceLogEntry {
    pub workspace_id: String,
    pub timestamp: String,
    pub level: LogLevel,
    pub message: String,
}

// ── File System ──

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSystemEntry {
    pub name: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub absolute_path: Option<String>,
    #[serde(rename = "type")]
    pub entry_type: FileSystemEntryType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FileSystemEntryType {
    File,
    Directory,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FileSystemScope {
    Restricted,
    Unrestricted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FileSystemPathKind {
    Relative,
    Absolute,
    Drives,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSystemListingMetadata {
    pub scope: FileSystemScope,
    pub current_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_path: Option<String>,
    pub root_path: String,
    pub home_path: String,
    pub display_path: String,
    pub path_kind: FileSystemPathKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSystemListResponse {
    pub entries: Vec<FileSystemEntry>,
    pub metadata: FileSystemListingMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSystemCreateFolderRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_path: Option<String>,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSystemCreateFolderResponse {
    pub path: String,
    pub absolute_path: String,
}

pub const WINDOWS_DRIVES_ROOT: &str = "__drives__";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceFileResponse {
    pub workspace_id: String,
    pub relative_path: String,
    pub contents: String,
}

pub type WorkspaceFileSearchResponse = Vec<FileSystemEntry>;

// ── Instance ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceData {
    pub message_history: Vec<String>,
    pub agent_model_selections: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum InstanceStreamStatus {
    Connecting,
    Connected,
    Error,
    Disconnected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceStreamEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<std::collections::HashMap<String, serde_json::Value>>,
}

// ── SideCar ──

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SideCarKind {
    Port,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SideCarPrefixMode {
    Strip,
    Preserve,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SideCarStatus {
    Running,
    Stopped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SideCar {
    pub id: String,
    pub kind: SideCarKind,
    pub name: String,
    pub port: u16,
    pub insecure: bool,
    pub prefix_mode: SideCarPrefixMode,
    pub status: SideCarStatus,
    pub created_at: String,
    pub updated_at: String,
}

// ── Binary ──

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BinaryRecord {
    pub id: String,
    pub path: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub is_default: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_validated_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation_error: Option<String>,
}

pub type SettingsOwner = String;
pub type SettingsBucket = serde_json::Value;
pub type SettingsDoc = serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinaryListResponse {
    pub binaries: Vec<BinaryRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BinaryCreateRequest {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub make_default: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BinaryUpdateRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub make_default: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinaryValidationResult {
    pub valid: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

// ── Speech ──

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechSegment {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechCapabilitiesResponse {
    pub available: bool,
    pub configured: bool,
    pub provider: String,
    pub supports_stt: bool,
    pub supports_tts: bool,
    pub supports_streaming_tts: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    pub stt_model: String,
    pub tts_model: String,
    pub tts_voice: String,
    pub tts_formats: Vec<String>,
    pub streaming_tts_formats: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechTranscriptionResponse {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub segments: Option<Vec<SpeechSegment>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechSynthesisResponse {
    pub audio_base64: String,
    pub mime_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceModeStateResponse {
    pub enabled: bool,
}

// ── Remote Server ──

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteServerProfile {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub skip_tls_verify: bool,
    pub created_at: String,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_connected_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteServerProbeRequest {
    pub base_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skip_tls_verify: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteServerProbeResponse {
    pub ok: bool,
    pub reachable: bool,
    pub normalized_url: String,
    pub skip_tls_verify: bool,
    pub requires_auth: bool,
    pub authenticated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteProxySessionCreateRequest {
    pub base_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skip_tls_verify: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteProxySessionCreateResponse {
    pub session_id: String,
    pub window_url: String,
}

// ── Events ──

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceEventType {
    WorkspaceCreated,
    WorkspaceUpdate,
    WorkspaceStarted,
    WorkspaceError,
    WorkspaceStopped,
    WorkspaceLog,
    SidecarUpdated,
    SidecarRemoved,
    StorageConfigChanged,
    StorageStateChanged,
    InstanceDataChanged,
    InstanceEvent,
    InstanceEventStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum WorkspaceEventPayload {
    WorkspaceCreated {
        #[serde(rename = "type")]
        event_type: String,
        workspace: WorkspaceDescriptor,
    },
    WorkspaceUpdate {
        #[serde(rename = "type")]
        event_type: String,
        workspace: WorkspaceDescriptor,
    },
    WorkspaceStarted {
        #[serde(rename = "type")]
        event_type: String,
        workspace: WorkspaceDescriptor,
    },
    WorkspaceError {
        #[serde(rename = "type")]
        event_type: String,
        workspace: WorkspaceDescriptor,
    },
    WorkspaceStopped {
        #[serde(rename = "type")]
        event_type: String,
        #[serde(rename = "workspaceId")]
        workspace_id: String,
    },
    WorkspaceLog {
        #[serde(rename = "type")]
        event_type: String,
        entry: WorkspaceLogEntry,
    },
    SidecarUpdated {
        #[serde(rename = "type")]
        event_type: String,
        sidecar: SideCar,
    },
    SidecarRemoved {
        #[serde(rename = "type")]
        event_type: String,
        #[serde(rename = "sidecarId")]
        sidecar_id: String,
    },
    StorageConfigChanged {
        #[serde(rename = "type")]
        event_type: String,
        owner: SettingsOwner,
        value: SettingsBucket,
    },
    StorageStateChanged {
        #[serde(rename = "type")]
        event_type: String,
        owner: SettingsOwner,
        value: SettingsBucket,
    },
    InstanceDataChanged {
        #[serde(rename = "type")]
        event_type: String,
        #[serde(rename = "instanceId")]
        instance_id: String,
        data: InstanceData,
    },
    InstanceEvent {
        #[serde(rename = "type")]
        event_type: String,
        #[serde(rename = "instanceId")]
        instance_id: String,
        event: InstanceStreamEvent,
    },
    InstanceEventStatus {
        #[serde(rename = "type")]
        event_type: String,
        #[serde(rename = "instanceId")]
        instance_id: String,
        status: InstanceStreamStatus,
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
}

// ── Network ──

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkAddress {
    pub ip: String,
    #[serde(rename = "family")]
    pub addr_family: NetworkFamily,
    pub scope: AddressScope,
    pub remote_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum NetworkFamily {
    Ipv4,
    Ipv6,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AddressScope {
    External,
    Internal,
    Loopback,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LatestReleaseInfo {
    pub version: String,
    pub tag: String,
    pub url: String,
    pub channel: ReleaseChannel,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseChannel {
    Stable,
    Dev,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiMeta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(rename = "source")]
    pub ui_source: UiSource,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum UiSource {
    Bundled,
    Downloaded,
    Previous,
    Override,
    DevProxy,
    Missing,
}

impl From<crate::ui::remote_ui::UiSource> for UiSource {
    fn from(s: crate::ui::remote_ui::UiSource) -> Self {
        match s {
            crate::ui::remote_ui::UiSource::Bundled => UiSource::Bundled,
            crate::ui::remote_ui::UiSource::Downloaded => UiSource::Downloaded,
            crate::ui::remote_ui::UiSource::Previous => UiSource::Previous,
            crate::ui::remote_ui::UiSource::Override => UiSource::Override,
            crate::ui::remote_ui::UiSource::DevProxy => UiSource::DevProxy,
            crate::ui::remote_ui::UiSource::Missing => UiSource::Missing,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupportMeta {
    pub supported: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_server_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_server_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_server_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerMeta {
    pub local_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_url: Option<String>,
    pub events_url: String,
    pub host: String,
    pub listening_mode: ListeningMode,
    pub local_port: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_port: Option<u16>,
    pub host_label: String,
    pub workspace_root: String,
    pub addresses: Vec<NetworkAddress>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ui: Option<UiMeta>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub support: Option<SupportMeta>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update: Option<LatestReleaseInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ListeningMode {
    Local,
    All,
}

// ── Background Process ──

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundProcessStatus {
    Running,
    Stopped,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundProcessTerminalReason {
    Finished,
    Failed,
    UserStopped,
    UserTerminated,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundProcess {
    pub id: String,
    pub workspace_id: String,
    pub title: String,
    pub command: String,
    pub cwd: String,
    pub status: BackgroundProcessStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    pub started_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stopped_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_size_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal_reason: Option<BackgroundProcessTerminalReason>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notify_enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackgroundProcessListResponse {
    pub processes: Vec<BackgroundProcess>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundProcessOutputResponse {
    pub id: String,
    pub content: String,
    pub truncated: bool,
    pub size_bytes: u64,
}
