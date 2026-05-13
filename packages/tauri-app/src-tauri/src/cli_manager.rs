use dirs::home_dir;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::VecDeque;
use std::env;
#[cfg(windows)]
use std::ffi::c_void;
use std::ffi::OsStr;
use std::fs;
use std::io::{BufRead, BufReader};
#[cfg(windows)]
use std::mem::{size_of, zeroed};

#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{webview::cookie::Cookie, AppHandle, Emitter, Manager};
use url::Url;

#[cfg(windows)]
use std::os::windows::io::AsRawHandle;
#[cfg(windows)]
use std::os::windows::process::CommandExt;
#[cfg(windows)]
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
#[cfg(windows)]
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
    SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;
const MISSING_NODE_PREFIX: &str = "CODENOMAD_MISSING_NODE:";

#[cfg(windows)]
#[derive(Debug)]
struct WindowsJobObject {
    // The desktop wrapper may observe only a short-lived Node wrapper PID while the real
    // server and workspace descendants continue running below it. KILL_ON_JOB_CLOSE gives
    // Tauri an OS-owned handle for the whole subtree instead of relying on a single PID.
    handle: HANDLE,
}

#[cfg(windows)]
impl WindowsJobObject {
    fn create() -> anyhow::Result<Self> {
        let handle = unsafe { CreateJobObjectW(std::ptr::null_mut(), std::ptr::null()) };
        if handle.is_null() {
            return Err(anyhow::anyhow!(
                "CreateJobObjectW failed: {}",
                std::io::Error::last_os_error()
            ));
        }

        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

        let ok = unsafe {
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                &mut info as *mut _ as *mut c_void,
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        if ok == 0 {
            let err = std::io::Error::last_os_error();
            unsafe {
                CloseHandle(handle);
            }
            return Err(anyhow::anyhow!("SetInformationJobObject failed: {}", err));
        }

        Ok(Self { handle })
    }

    fn assign_child(&self, child: &Child) -> anyhow::Result<()> {
        let process_handle = child.as_raw_handle() as HANDLE;
        let ok = unsafe { AssignProcessToJobObject(self.handle, process_handle) };
        if ok == 0 {
            return Err(anyhow::anyhow!(
                "AssignProcessToJobObject failed: {}",
                std::io::Error::last_os_error()
            ));
        }

        Ok(())
    }
}

#[cfg(windows)]
impl Drop for WindowsJobObject {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe {
                CloseHandle(self.handle);
            }
        }
    }
}

#[cfg(windows)]
unsafe impl Send for WindowsJobObject {}

#[cfg(windows)]
unsafe impl Sync for WindowsJobObject {}

fn log_line(message: &str) {
    println!("[tauri-cli] {message}");
}

#[cfg(windows)]
fn configure_spawn(command: &mut Command) {
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn configure_spawn(_command: &mut Command) {}

fn workspace_root() -> Option<PathBuf> {
    std::env::current_dir().ok().and_then(|mut dir| {
        for _ in 0..3 {
            if let Some(parent) = dir.parent() {
                dir = parent.to_path_buf();
            }
        }
        Some(dir)
    })
}

fn launch_cwd() -> Option<PathBuf> {
    std::env::current_dir().ok()
}

const CLI_STOP_GRACE_SECS: u64 = 30;
#[cfg(windows)]
const CLI_WINDOWS_FORCE_GRACE_MS: u64 = 2_000;

#[cfg(unix)]
fn configure_posix_process_group(command: &mut Command) {
    // Ensure the CLI runs in its own process group so we can terminate wrapper
    // processes (login shell/tsx) without leaving the server orphaned.
    unsafe {
        command.pre_exec(|| {
            if libc::setpgid(0, 0) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

#[cfg(windows)]
fn kill_process_tree_windows(pid: u32, force: bool) -> bool {
    let mut args = vec!["/PID".to_string(), pid.to_string(), "/T".to_string()];
    if force {
        args.push("/F".to_string());
    }

    let mut command = Command::new("taskkill");
    command.args(&args);
    configure_spawn(&mut command);

    match command.output() {
        Ok(output) => {
            if output.status.success() {
                return true;
            }

            // If the PID is already gone, treat it as success.
            let stdout = String::from_utf8_lossy(&output.stdout).to_lowercase();
            let stderr = String::from_utf8_lossy(&output.stderr).to_lowercase();
            let combined = format!("{stdout}\n{stderr}");
            combined.contains("not found") || combined.contains("no running instance")
        }
        Err(_) => false,
    }
}
fn navigate_main(app: &AppHandle, url: &str) {
    if let Some(win) = app.webview_windows().get("main") {
        let mut display = url.to_string();
        if let Some(hash_index) = display.find('#') {
            display.replace_range(hash_index + 1.., "[REDACTED]");
        }
        log_line(&format!("navigating main to {display}"));
        if let Ok(parsed) = Url::parse(url) {
            let _ = win.navigate(parsed);
        } else {
            log_line("failed to parse URL for navigation");
        }
    } else {
        log_line("main window not found for navigation");
    }
}

fn find_free_port() -> anyhow::Result<u16> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    // Drop the listener so the port becomes available immediately.
    drop(listener);
    Ok(port)
}

fn generate_password() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let pid = std::process::id();
    format!("{:016x}_{:08x}", nanos, pid)
}

fn login_and_set_cookie(
    app: &AppHandle,
    base_url: &str,
    password: &str,
) -> anyhow::Result<bool> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;

    let login_body = serde_json::json!({
        "username": "embeddedcowork",
        "password": password,
    });

    let login_url = format!("{}/api/auth/login", base_url);
    let resp = client.post(&login_url).json(&login_body).send()?;

    if !resp.status().is_success() {
        return Ok(false);
    }

    // Extract Set-Cookie header from the response
    let set_cookie_header = resp
        .headers()
        .get(reqwest::header::SET_COOKIE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let Some(cookie_str) = set_cookie_header else {
        return Ok(false);
    };

    // Parse cookie name=value from Set-Cookie
    let cookie_kv = cookie_str.split(';').next().unwrap_or("").trim();
    if cookie_kv.is_empty() {
        return Ok(false);
    }

    let eq_idx = cookie_kv.find('=').ok_or_else(|| anyhow::anyhow!("invalid Set-Cookie: {}", cookie_kv))?;
    let cookie_name = cookie_kv[..eq_idx].trim().to_string();
    let cookie_value = cookie_kv[eq_idx + 1..].trim().to_string();

    // Set cookie on the webview
    if let Some(win) = app.webview_windows().get("main") {
        let parsed = Url::parse(base_url)?;
        let domain = parsed.host_str().unwrap_or("127.0.0.1").to_string();

        let cookie = Cookie::build((cookie_name, cookie_value))
            .domain(domain)
            .path("/")
            .http_only(true)
            .same_site(tauri::webview::cookie::SameSite::Lax)
            .build();
        win.set_cookie(cookie)?;
    }

    Ok(true)
}

fn health_check_and_login(
    app: &AppHandle,
    status: &Arc<Mutex<CliStatus>>,
    ready: &Arc<AtomicBool>,
    base_url: &str,
    password: &str,
) {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();

    let health_url = format!("{}/api/health", base_url);
    let started = Instant::now();
    let timeout = Duration::from_secs(60);

    loop {
        if started.elapsed() > timeout {
            log_line("health check timed out");
            let mut locked = status.lock();
            locked.state = CliState::Error;
            locked.error = Some("Server did not become ready in time".to_string());
            let _ = app.emit("cli:error", json!({"message": "Server did not become ready in time"}));
            return;
        }

        // Also check if the process has exited
        if ready.load(Ordering::SeqCst) {
            return;
        }

        match client.get(&health_url).send() {
            Ok(resp) if resp.status().is_success() => {
                log_line(&format!("health check passed on {base_url}"));

                // Now login with the known password
                match login_and_set_cookie(app, base_url, password) {
                    Ok(true) => {
                        log_line("password login successful");
                        ready.store(true, Ordering::SeqCst);
                        let mut locked = status.lock();
                        locked.state = CliState::Ready;
                        locked.url = Some(base_url.to_string());
                        locked.error = None;
                        let snapshot = locked.clone();
                        drop(locked);
                        navigate_main(app, base_url);
                        let _ = app.emit("cli:ready", snapshot);
                        // Background auto-update check
                        thread::spawn(move || {
                            check_and_download_update();
                        });
                        return;
                    }
                    Ok(false) => {
                        log_line("password login failed (invalid credentials)");
                        ready.store(true, Ordering::SeqCst);
                        let mut locked = status.lock();
                        locked.state = CliState::Ready;
                        locked.url = Some(base_url.to_string());
                        locked.error = None;
                        let snapshot = locked.clone();
                        drop(locked);
                        navigate_main(app, &format!("{base_url}/login"));
                        let _ = app.emit("cli:ready", snapshot);
                        return;
                    }
                    Err(err) => {
                        log_line(&format!("password login error: {err}, retrying..."));
                    }
                }
            }
            Ok(resp) => {
                log_line(&format!("health check returned {}, retrying...", resp.status()));
            }
            Err(err) => {
                log_line(&format!("health check failed: {err}, retrying..."));
            }
        }

        thread::sleep(Duration::from_millis(200));
    }
}

const DEFAULT_CONFIG_PATH: &str = "~/.config/embeddedcowork/config.json";

#[derive(Debug, Deserialize)]
struct PreferencesConfig {
    #[serde(rename = "listeningMode")]
    listening_mode: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ServerConfig {
    #[serde(rename = "listeningMode")]
    listening_mode: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AppConfig {
    preferences: Option<PreferencesConfig>,
    server: Option<ServerConfig>,
}

fn resolve_config_locations() -> (PathBuf, PathBuf) {
    let raw = env::var("CLI_CONFIG")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_CONFIG_PATH.to_string());

    let expanded = expand_home(&raw);
    let lower = raw.trim().to_lowercase();

    if lower.ends_with(".yaml") || lower.ends_with(".yml") {
        let base = expanded
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| expanded.clone());
        return (expanded, base.join("config.json"));
    }

    if lower.ends_with(".json") {
        let base = expanded
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| expanded.clone());
        return (base.join("config.yaml"), expanded);
    }

    // Treat as directory.
    (expanded.join("config.yaml"), expanded.join("config.json"))
}

fn expand_home(path: &str) -> PathBuf {
    if path.starts_with("~/") {
        if let Some(home) = home_dir().or_else(|| env::var("HOME").ok().map(PathBuf::from)) {
            return home.join(path.trim_start_matches("~/"));
        }
    }
    PathBuf::from(path)
}

fn resolve_listening_mode() -> String {
    let (yaml_path, json_path) = resolve_config_locations();

    if let Ok(content) = fs::read_to_string(&yaml_path) {
        if let Ok(config) = serde_yaml::from_str::<AppConfig>(&content) {
            let mode = config
                .server
                .as_ref()
                .and_then(|srv| srv.listening_mode.as_ref())
                .or_else(|| {
                    config
                        .preferences
                        .as_ref()
                        .and_then(|prefs| prefs.listening_mode.as_ref())
                });

            if let Some(mode) = mode {
                if mode == "local" {
                    return "local".to_string();
                }
                if mode == "all" {
                    return "all".to_string();
                }
            }
        }
    }

    // Legacy fallback.
    if let Ok(content) = fs::read_to_string(&json_path) {
        if let Ok(config) = serde_json::from_str::<AppConfig>(&content) {
            let mode = config
                .server
                .as_ref()
                .and_then(|srv| srv.listening_mode.as_ref())
                .or_else(|| {
                    config
                        .preferences
                        .as_ref()
                        .and_then(|prefs| prefs.listening_mode.as_ref())
                });
            if let Some(mode) = mode {
                if mode == "local" {
                    return "local".to_string();
                }
                if mode == "all" {
                    return "all".to_string();
                }
            }
        }
    }
    "local".to_string()
}

fn resolve_listening_host() -> String {
    let mode = resolve_listening_mode();
    if mode == "local" {
        "127.0.0.1".to_string()
    } else {
        "0.0.0.0".to_string()
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CliState {
    Starting,
    Ready,
    Error,
    Stopped,
}

#[derive(Debug, Clone, Serialize)]
pub struct CliStatus {
    pub state: CliState,
    pub pid: Option<u32>,
    pub port: Option<u16>,
    pub url: Option<String>,
    pub error: Option<String>,
}

impl Default for CliStatus {
    fn default() -> Self {
        Self {
            state: CliState::Stopped,
            pid: None,
            port: None,
            url: None,
            error: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CliProcessManager {
    status: Arc<Mutex<CliStatus>>,
    child: Arc<Mutex<Option<Child>>>,
    #[cfg(windows)]
    job: Arc<Mutex<Option<WindowsJobObject>>>,
    ready: Arc<AtomicBool>,
}

impl CliProcessManager {
    pub fn new() -> Self {
        Self {
            status: Arc::new(Mutex::new(CliStatus::default())),
            child: Arc::new(Mutex::new(None)),
            #[cfg(windows)]
            job: Arc::new(Mutex::new(None)),
            ready: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn start(&self, app: AppHandle, dev: bool) -> anyhow::Result<()> {
        log_line(&format!("start requested (dev={dev})"));
        self.stop()?;
        self.ready.store(false, Ordering::SeqCst);
        {
            let mut status = self.status.lock();
            status.state = CliState::Starting;
            status.port = None;
            status.url = None;
            status.error = None;
            status.pid = None;
        }
        Self::emit_status(&app, &self.status.lock());

        let status_arc = self.status.clone();
        let child_arc = self.child.clone();
        #[cfg(windows)]
        let job_arc = self.job.clone();
        let ready_flag = self.ready.clone();
        thread::spawn(move || {
            if let Err(err) = Self::spawn_cli(
                app.clone(),
                status_arc.clone(),
                child_arc,
                #[cfg(windows)]
                job_arc,
                ready_flag,
                dev,
            ) {
                log_line(&format!("cli spawn failed: {err}"));
                let mut locked = status_arc.lock();
                locked.state = CliState::Error;
                locked.error = Some(err.to_string());
                let snapshot = locked.clone();
                drop(locked);
                let _ = app.emit("cli:error", json!({"message": err.to_string()}));
                let _ = app.emit("cli:status", snapshot);
            }
        });

        Ok(())
    }

    pub fn stop(&self) -> anyhow::Result<()> {
        #[cfg(windows)]
        let _job = self.job.lock().take();

        let mut child_opt = self.child.lock();
        if let Some(mut child) = child_opt.take() {
            log_line(&format!("stopping CLI pid={}", child.id()));
            #[cfg(unix)]
            unsafe {
                let pid = child.id() as i32;
                // Prefer signaling the process group to avoid orphaning children
                // when the CLI was launched via a wrapper shell.
                let group_res = libc::kill(-pid, libc::SIGTERM);
                if group_res != 0 {
                    let _ = libc::kill(pid, libc::SIGTERM);
                }
            }
            #[cfg(windows)]
            {
                let _ = kill_process_tree_windows(child.id(), false);
            }

            let start = Instant::now();
            loop {
                match child.try_wait() {
                    Ok(Some(_)) => break,
                    Ok(None) => {
                        #[cfg(windows)]
                        if start.elapsed() > Duration::from_millis(CLI_WINDOWS_FORCE_GRACE_MS) {
                            log_line(&format!(
                                "regular Windows shutdown still running after {}ms; escalating pid={}",
                                CLI_WINDOWS_FORCE_GRACE_MS,
                                child.id()
                            ));
                            if !kill_process_tree_windows(child.id(), true) {
                                let _ = child.kill();
                            }
                            break;
                        }

                        if start.elapsed() > Duration::from_secs(CLI_STOP_GRACE_SECS) {
                            log_line(&format!(
                                "stop timed out after {}s; sending SIGKILL pid={}",
                                CLI_STOP_GRACE_SECS,
                                child.id()
                            ));
                            #[cfg(unix)]
                            unsafe {
                                let pid = child.id() as i32;
                                let group_res = libc::kill(-pid, libc::SIGKILL);
                                if group_res != 0 {
                                    let _ = libc::kill(pid, libc::SIGKILL);
                                }
                            }
                            #[cfg(windows)]
                            {
                                if !kill_process_tree_windows(child.id(), true) {
                                    let _ = child.kill();
                                }
                            }
                            break;
                        }
                        thread::sleep(Duration::from_millis(50));
                    }
                    Err(_) => break,
                }
            }
        } else {
            #[cfg(windows)]
            log_line("tracked CLI process already exited; dropping Windows job object to reap descendants");
        }

        let mut status = self.status.lock();
        status.state = CliState::Stopped;
        status.pid = None;
        status.port = None;
        status.url = None;
        status.error = None;

        Ok(())
    }

    pub fn status(&self) -> CliStatus {
        self.status.lock().clone()
    }

    fn spawn_cli(
        app: AppHandle,
        status: Arc<Mutex<CliStatus>>,
        child_holder: Arc<Mutex<Option<Child>>>,
        #[cfg(windows)] job_holder: Arc<Mutex<Option<WindowsJobObject>>>,
        ready: Arc<AtomicBool>,
        dev: bool,
    ) -> anyhow::Result<()> {
        log_line("resolving CLI entry");
        let resolution = CliEntry::resolve(&app, dev)?;
        let host = resolve_listening_host();
        let port = find_free_port()?;
        let password = generate_password();
        log_line(&format!(
            "resolved CLI entry runner={:?} entry={} host={} port={}",
            resolution.runner, resolution.entry, host, port
        ));
        let args = resolution.build_args(dev, &host, port, &password);
        log_line(&format!("CLI args: {:?}", args));
        if dev {
            log_line("development mode: will prefer tsx + source if present");
        }

        let cwd = launch_cwd();
        if let Some(ref c) = cwd {
            log_line(&format!("using cwd={}", c.display()));
        }

        let use_user_shell = supports_user_shell();

        if resolution.runner == Runner::Tsx
            && !use_user_shell
            && which::which(&resolution.node_binary).is_err()
        {
            return Err(anyhow::anyhow!(
                "Node binary '{}' not found. EmbeddedCowork development mode requires Node.js installed on the system, or set NODE_BINARY to a valid runtime path.",
                resolution.node_binary
            ));
        }

        let command_info = if use_user_shell {
            log_line("spawning via user shell");
            ShellCommandType::UserShell(build_shell_command_string(&resolution, &args)?)
        } else {
            log_line(if resolution.runner == Runner::Standalone || resolution.runner == Runner::System {
                "spawning directly with standalone executable"
            } else {
                "spawning directly with node"
            });
            ShellCommandType::Direct(DirectCommand {
                program: if resolution.runner == Runner::Standalone || resolution.runner == Runner::System {
                    resolution.entry.clone()
                } else {
                    resolution.node_binary.clone()
                },
                args: resolution.runner_args(&args),
            })
        };

        let child = match &command_info {
            ShellCommandType::UserShell(cmd) => {
                log_line(&format!("spawn command: {} {:?}", cmd.shell, cmd.args));
                let mut c = Command::new(&cmd.shell);
                c.args(&cmd.args)
                    .env_remove("npm_config_prefix")
                    .env_remove("NPM_CONFIG_PREFIX")
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped());
                if resolution.runner != Runner::Standalone && resolution.runner != Runner::System {
                    c.env("ELECTRON_RUN_AS_NODE", "1");
                }
                configure_spawn(&mut c);
                if let Some(ref cwd) = cwd {
                    c.current_dir(cwd);
                }
                #[cfg(unix)]
                configure_posix_process_group(&mut c);
                c.spawn()?
            }
            ShellCommandType::Direct(cmd) => {
                log_line(&format!("spawn command: {} {:?}", cmd.program, cmd.args));
                let mut c = Command::new(&cmd.program);
                c.args(&cmd.args)
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped());
                if resolution.runner != Runner::Standalone && resolution.runner != Runner::System {
                    c.env("ELECTRON_RUN_AS_NODE", "1");
                }
                configure_spawn(&mut c);
                if let Some(ref cwd) = cwd {
                    c.current_dir(cwd);
                }
                #[cfg(unix)]
                configure_posix_process_group(&mut c);
                c.spawn()?
            }
        };

        let pid = child.id();
        log_line(&format!("spawned pid={pid}"));
        #[cfg(windows)]
        match WindowsJobObject::create().and_then(|job| {
            job.assign_child(&child)?;
            Ok(job)
        }) {
            Ok(job) => {
                log_line(&format!("attached pid={pid} to Windows job object"));
                *job_holder.lock() = Some(job);
            }
            Err(err) => {
                log_line(&format!(
                    "failed to attach pid={pid} to Windows job object; falling back to taskkill-only cleanup: {err}"
                ));
            }
        }

        {
            let mut locked = status.lock();
            locked.pid = Some(pid);
        }
        Self::emit_status(&app, &status.lock());

        {
            let mut holder = child_holder.lock();
            *holder = Some(child);
        }

        let child_clone = child_holder.clone();
        let base_url = format!("http://127.0.0.1:{}", port);

        thread::spawn(move || {
            let stdout = child_clone
                .lock()
                .as_mut()
                .and_then(|c| c.stdout.take())
                .map(BufReader::new);
            let stderr = child_clone
                .lock()
                .as_mut()
                .and_then(|c| c.stderr.take())
                .map(BufReader::new);

            if let Some(reader) = stdout {
                let _ = status_clone.clone();
                thread::spawn(move || {
                    Self::process_stream(reader, "stdout");
                });
            }

            if let Some(reader) = stderr {
                thread::spawn(move || {
                    Self::process_stream(reader, "stderr");
                });
            }
        });

        // Health check + login thread
        let app_hc = app.clone();
        let status_hc = status.clone();
        let ready_hc = ready.clone();
        let base_url_hc = base_url.clone();
        let password_hc = password.clone();
        let child_holder_hc = child_holder.clone();
        thread::spawn(move || {
            health_check_and_login(&app_hc, &status_hc, &ready_hc, &base_url_hc, &password_hc);
            if !ready_hc.load(Ordering::SeqCst) {
                // Health check timed out or failed; kill the child process
                if let Some(child) = child_holder_hc.lock().as_mut() {
                    #[cfg(unix)]
                    unsafe {
                        let pid = child.id() as i32;
                        let group_res = libc::kill(-pid, libc::SIGKILL);
                        if group_res != 0 {
                            let _ = libc::kill(pid, libc::SIGKILL);
                        }
                    }
                    #[cfg(windows)]
                    {
                        if !kill_process_tree_windows(child.id(), true) {
                            let _ = child.kill();
                        }
                    }
                    #[cfg(not(any(unix, windows)))]
                    {
                        let _ = child.kill();
                    }
                }
                let _ = app_hc.emit("cli:error", json!({"message": "CLI did not start in time"}));
            }
        });

        let status_exit = status.clone();
        let app_exit = app.clone();
        thread::spawn(move || {
            let code = loop {
                let maybe_exited = {
                    let mut guard = child_holder.lock();
                    if guard.is_none() {
                        return;
                    }
                    match guard
                        .as_mut()
                        .and_then(|child| child.try_wait().ok().flatten())
                    {
                        Some(status) => {
                            *guard = None;
                            #[cfg(windows)]
                            {
                                let _ = job_holder.lock().take();
                            }
                            Some(status)
                        }
                        None => None,
                    }
                };

                if let Some(status) = maybe_exited {
                    break Some(status);
                }
                thread::sleep(Duration::from_millis(100));
            };

            let mut locked = status_exit.lock();
            let failed = locked.state != CliState::Ready;
            let err_msg = if failed {
                Some(match code {
                    Some(status) => format!("CLI exited early: {status}"),
                    None => "CLI exited early".to_string(),
                })
            } else {
                None
            };

            if failed {
                locked.state = CliState::Error;
                if locked.error.is_none() {
                    locked.error = err_msg.clone();
                }
                log_line(&format!(
                    "cli process exited before ready: {:?}",
                    locked.error
                ));
                let _ = app_exit.emit(
                    "cli:error",
                    json!({"message": locked.error.clone().unwrap_or_default()}),
                );
            } else {
                locked.state = CliState::Stopped;
                log_line("cli process stopped cleanly");
            }

            Self::emit_status(&app_exit, &locked);
        });

        Ok(())
    }

    fn process_stream<R: BufRead>(mut reader: R, stream: &str) {
        let mut buffer = String::new();
        loop {
            buffer.clear();
            match reader.read_line(&mut buffer) {
                Ok(0) => break,
                Ok(_) => {
                    let line = buffer.trim_end();
                    if !line.is_empty() {
                        if line.starts_with(MISSING_NODE_PREFIX) {
                            continue;
                        }
                        log_line(&format!("[cli][{}] {}", stream, line));
                    }
                }
                Err(_) => break,
            }
        }
    }

    fn emit_status(app: &AppHandle, status: &CliStatus) {
        let _ = app.emit("cli:status", status.clone());
    }
}

fn supports_user_shell() -> bool {
    cfg!(unix)
}

#[derive(Debug)]
struct ShellCommand {
    shell: String,
    args: Vec<String>,
}

#[derive(Debug)]
struct DirectCommand {
    program: String,
    args: Vec<String>,
}

#[derive(Debug)]
enum ShellCommandType {
    UserShell(ShellCommand),
    Direct(DirectCommand),
}

#[derive(Debug)]
struct CliEntry {
    entry: String,
    runner: Runner,
    runner_path: Option<String>,
    node_binary: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Runner {
    Standalone,
    Tsx,
    System,
}

impl CliEntry {
    fn resolve(app: &AppHandle, dev: bool) -> anyhow::Result<Self> {
        let node_binary = std::env::var("NODE_BINARY").unwrap_or_else(|_| "node".to_string());

        if dev {
            if let Some(tsx_path) = resolve_tsx(app) {
                if let Some(entry) = resolve_dev_entry(app) {
                    return Ok(Self {
                        entry,
                        runner: Runner::Tsx,
                        runner_path: Some(tsx_path),
                        node_binary,
                    });
                }
            }
        }

        // Prod: 三步检测链
        if let Some(entry) = resolve_system_entry() {
            return Ok(Self {
                entry,
                runner: Runner::System,
                runner_path: None,
                node_binary: String::new(),
            });
        }

        if let Some(entry) = resolve_standalone_entry(app) {
            return Ok(Self {
                entry,
                runner: Runner::Standalone,
                runner_path: None,
                node_binary: String::new(),
            });
        }

        Err(anyhow::anyhow!(
            "Unable to locate the packaged EmbeddedCowork standalone server. Please rebuild the desktop bundle."
        ))
    }

    fn build_args(&self, dev: bool, host: &str, port: u16, password: &str) -> Vec<String> {
        let mut args = vec![
            "serve".to_string(),
            "--host".to_string(),
            host.to_string(),
            "--password".to_string(),
            password.to_string(),
            "--http-port".to_string(),
            port.to_string(),
            "--http".to_string(),
            "true".to_string(),
            "--unrestricted-root".to_string(),
        ];

        if dev {
            let ui_dev_server = std::env::var("VITE_DEV_SERVER_URL")
                .ok()
                .filter(|value| !value.trim().is_empty())
                .or_else(|| {
                    std::env::var("ELECTRON_RENDERER_URL")
                        .ok()
                        .filter(|value| !value.trim().is_empty())
                })
                .unwrap_or_else(|| "http://localhost:3000".to_string());
            let log_level = std::env::var("CLI_LOG_LEVEL")
                .ok()
                .map(|value| value.trim().to_lowercase())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "info".to_string());

            args.push("--https".to_string());
            args.push("true".to_string());
            args.push("--ui-dev-server".to_string());
            args.push(ui_dev_server);
            args.push("--log-level".to_string());
            args.push(log_level);
        } else {
            args.push("--https".to_string());
            args.push("false".to_string());
        }
        args
    }

    fn runner_args(&self, cli_args: &[String]) -> Vec<String> {
        if self.runner == Runner::Standalone || self.runner == Runner::System {
            return cli_args.to_vec();
        }

        let mut args = VecDeque::new();
        if self.runner == Runner::Tsx {
            if let Some(path) = &self.runner_path {
                args.push_back(path.clone());
            }
        }
        args.push_back(self.entry.clone());
        for arg in cli_args {
            args.push_back(arg.clone());
        }
        args.into_iter().collect()
    }
}

fn resolve_system_entry() -> Option<String> {
    // Step 1: 系统 PATH → embeddedCowork
    if let Ok(path) = which::which("embeddedCowork") {
        return Some(path.to_string_lossy().to_string());
    }

    // Step 2: ~/.embeddedcowork/bin/embeddedcowork-server
    let binary_name = if cfg!(windows) {
        "embeddedcowork-server.exe"
    } else {
        "embeddedcowork-server"
    };
    let installed = home_dir()
        .or_else(|| std::env::var("HOME").ok().map(PathBuf::from))
        .map(|h| h.join(".embeddedcowork").join("bin").join(binary_name))
        .filter(|p| p.exists())?;
    Some(installed.to_string_lossy().to_string())
}

fn resolve_tsx(_app: &AppHandle) -> Option<String> {
    let cwd = std::env::current_dir().ok();
    let workspace = workspace_root();
    let mut candidates = vec![
        cwd.as_ref()
            .map(|p| p.join("node_modules/tsx/dist/cli.mjs")),
        cwd.as_ref()
            .map(|p| p.join("node_modules/tsx/dist/cli.cjs")),
        cwd.as_ref().map(|p| p.join("node_modules/tsx/dist/cli.js")),
        cwd.as_ref()
            .map(|p| p.join("../node_modules/tsx/dist/cli.mjs")),
        cwd.as_ref()
            .map(|p| p.join("../node_modules/tsx/dist/cli.cjs")),
        cwd.as_ref()
            .map(|p| p.join("../node_modules/tsx/dist/cli.js")),
        cwd.as_ref()
            .map(|p| p.join("../../node_modules/tsx/dist/cli.mjs")),
        cwd.as_ref()
            .map(|p| p.join("../../node_modules/tsx/dist/cli.cjs")),
        cwd.as_ref()
            .map(|p| p.join("../../node_modules/tsx/dist/cli.js")),
        workspace
            .as_ref()
            .map(|p| p.join("node_modules/tsx/dist/cli.mjs")),
        workspace
            .as_ref()
            .map(|p| p.join("node_modules/tsx/dist/cli.cjs")),
        workspace
            .as_ref()
            .map(|p| p.join("node_modules/tsx/dist/cli.js")),
    ];

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(Some(dir.join("../node_modules/tsx/dist/cli.mjs")));
            candidates.push(Some(dir.join("../node_modules/tsx/dist/cli.cjs")));
            candidates.push(Some(dir.join("../node_modules/tsx/dist/cli.js")));
        }
    }

    first_existing(candidates)
}

fn resolve_dev_entry(_app: &AppHandle) -> Option<String> {
    let cwd = std::env::current_dir().ok();
    let workspace = workspace_root();
    let candidates = vec![
        workspace
            .as_ref()
            .map(|p| p.join("packages/server/src/index.ts")),
        cwd.as_ref().map(|p| p.join("packages/server/src/index.ts")),
        cwd.as_ref().map(|p| p.join("../server/src/index.ts")),
        cwd.as_ref().map(|p| p.join("../../server/src/index.ts")),
    ];

    first_existing(candidates)
}

fn resolve_standalone_entry(_app: &AppHandle) -> Option<String> {
    let executable_name = if cfg!(windows) {
        "embeddedcowork-server.exe"
    } else {
        "embeddedcowork-server"
    };

    // Production sidecar: externalBin places the binary next to the app executable
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let sidecar_path = dir.join(&executable_name);
            if sidecar_path.exists() {
                return Some(normalize_path(sidecar_path));
            }

            // macOS: sidecar may be in ../Resources/ alongside the .app bundle
            let resources = dir.join("../Resources");
            let resources_sidecar = resources.join(&executable_name);
            if resources_sidecar.exists() {
                return Some(normalize_path(resources_sidecar));
            }

            // Fallback: check the old resources/server/dist path
            let legacy = dir.join("resources/server/dist").join(&executable_name);
            if legacy.exists() {
                return Some(normalize_path(legacy));
            }
        }
    }

    // Dev mode fallback: check workspace dist
    let base = workspace_root();
    if let Some(ws) = base {
        let dev_path = ws.join("packages/server/dist").join(&executable_name);
        if dev_path.exists() {
            return Some(normalize_path(dev_path));
        }
    }

    None
}

fn build_shell_command_string(
    entry: &CliEntry,
    cli_args: &[String],
) -> anyhow::Result<ShellCommand> {
    let shell = default_shell();
    let mut quoted: Vec<String> = Vec::new();
    let command = if entry.runner == Runner::Standalone || entry.runner == Runner::System {
        quoted.push(shell_escape(&entry.entry));
        for arg in cli_args {
            quoted.push(shell_escape(arg));
        }
        format!("exec {}", quoted.join(" "))
    } else {
        quoted.push(shell_escape(&entry.node_binary));
        for arg in entry.runner_args(cli_args) {
            quoted.push(shell_escape(&arg));
        }
        format!(
            "if command -v {} >/dev/null 2>&1; then ELECTRON_RUN_AS_NODE=1 exec {}; else printf '%s%s\\n' '{}' {} >&2; exit 127; fi",
            shell_escape(&entry.node_binary),
            quoted.join(" "),
            MISSING_NODE_PREFIX,
            shell_escape(&entry.node_binary),
        )
    };
    let wrapped_command = wrap_command_for_shell(&command, &shell);
    let args = build_shell_args(&shell, &wrapped_command);
    log_line(&format!("user shell command: {} {:?}", shell, args));
    Ok(ShellCommand { shell, args })
}

fn wrap_command_for_shell(command: &str, shell: &str) -> String {
    let shell_name = std::path::Path::new(shell)
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or("")
        .to_lowercase();

    if shell_name.contains("bash") {
        return format!(
            "if [ -f ~/.bashrc ]; then source ~/.bashrc >/dev/null 2>&1; fi; {}",
            command
        );
    }

    if shell_name.contains("zsh") {
        return format!(
            "if [ -f ~/.zshrc ]; then source ~/.zshrc >/dev/null 2>&1; fi; {}",
            command
        );
    }

    command.to_string()
}

fn default_shell() -> String {
    if let Ok(shell) = std::env::var("SHELL") {
        if !shell.trim().is_empty() {
            return shell;
        }
    }
    if cfg!(target_os = "macos") {
        "/bin/zsh".to_string()
    } else {
        "/bin/bash".to_string()
    }
}

fn shell_escape(input: &str) -> String {
    if input.is_empty() {
        "''".to_string()
    } else if !input
        .chars()
        .any(|c| matches!(c, ' ' | '"' | '\'' | '$' | '`' | '!'))
    {
        input.to_string()
    } else {
        let escaped = input.replace('\'', "'\\''");
        format!("'{}'", escaped)
    }
}

fn build_shell_args(shell: &str, command: &str) -> Vec<String> {
    let shell_name = std::path::Path::new(shell)
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or("")
        .to_lowercase();

    if shell_name.contains("zsh") {
        vec!["-l".into(), "-i".into(), "-c".into(), command.into()]
    } else {
        vec!["-l".into(), "-c".into(), command.into()]
    }
}

fn first_existing(paths: Vec<Option<PathBuf>>) -> Option<String> {
    paths
        .into_iter()
        .flatten()
        .find(|p| p.exists())
        .map(|p| normalize_path(p))
}

fn normalize_path(path: PathBuf) -> String {
    let resolved = if let Ok(clean) = path.canonicalize() {
        clean
    } else {
        path
    };

    let rendered = resolved.to_string_lossy().to_string();
    if let Some(stripped) = rendered.strip_prefix("\\\\?\\UNC\\") {
        format!("\\\\{}", stripped)
    } else if let Some(stripped) = rendered.strip_prefix("\\\\?\\") {
        stripped.to_string()
    } else {
        rendered
    }
}

fn check_and_download_update() {
    if let Err(_) = try_download_update() {
        // 静默失败
    }
}

fn download_with_retry(client: &reqwest::blocking::Client, url: &str, dest: &std::path::Path, max_retries: u32) -> anyhow::Result<()> {
    for attempt in 1..=max_retries {
        match client.get(url).send() {
            Ok(mut response) => {
                if !response.status().is_success() {
                    let err_msg = format!("HTTP {}", response.status());
                    if attempt == max_retries {
                        anyhow::bail!(err_msg);
                    }
                    let delay = std::cmp::min(1000 * (1 << (attempt - 1)), 30000);
                    thread::sleep(Duration::from_millis(delay));
                    continue;
                }

                let result = (|| -> anyhow::Result<()> {
                    let mut file = fs::File::create(dest)?;
                    response.copy_to(&mut file)?;
                    Ok(())
                })();

                match result {
                    Ok(()) => return Ok(()),
                    Err(e) => {
                        let _ = fs::remove_file(dest);
                        if attempt == max_retries {
                            return Err(e);
                        }
                        let delay = std::cmp::min(1000 * (1 << (attempt - 1)), 30000);
                        thread::sleep(Duration::from_millis(delay));
                    }
                }
            }
            Err(e) => {
                if attempt == max_retries {
                    return Err(e.into());
                }
                let delay = std::cmp::min(1000 * (1 << (attempt - 1)), 30000);
                thread::sleep(Duration::from_millis(delay));
            }
        }
    }
    anyhow::bail!("download failed after {max_retries} attempts")
}

fn try_download_update() -> anyhow::Result<()> {
    let current_version = env!("CARGO_PKG_VERSION");

    let client = reqwest::blocking::Client::builder()
        .user_agent("EmbeddedCowork")
        .connect_timeout(Duration::from_secs(10))
        .build()?;

    let resp = client
        .get("https://api.github.com/repos/vividcode-ai/EmbeddedCowork/releases/latest")
        .header("Accept", "application/vnd.github.v3+json")
        .send()?;

    if !resp.status().is_success() {
        return Ok(());
    }

    let data: serde_json::Value = resp.json()?;
    let tag_name = data
        .get("tag_name")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let latest_version = tag_name.strip_prefix('v').unwrap_or(tag_name);

    if compare_versions(latest_version, current_version) <= 0 {
        return Ok(());
    }

    let platform = get_platform_key();
    if platform.is_empty() {
        return Ok(());
    }

    let ext = if cfg!(windows) { ".exe" } else { "" };
    let asset_name = format!("embeddedcowork-server-{latest_version}-{platform}{ext}");
    let download_url = format!(
        "https://github.com/vividcode-ai/EmbeddedCowork/releases/download/{tag_name}/{asset_name}"
    );

    let bin_dir = home_dir()
        .or_else(|| std::env::var("HOME").ok().map(PathBuf::from))
        .map(|h| h.join(".embeddedcowork").join("bin"))
        .ok_or_else(|| anyhow::anyhow!("cannot determine home directory"))?;

    let binary_name = if cfg!(windows) {
        "embeddedcowork-server.exe"
    } else {
        "embeddedcowork-server"
    };
    let target_path = bin_dir.join(binary_name);

    fs::create_dir_all(&bin_dir)?;

    download_with_retry(&client, &download_url, &target_path, 5)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&target_path, fs::Permissions::from_mode(0o755))?;
    }

    log_line(&format!(
        "server updated to v{latest_version} → {}",
        target_path.display()
    ));

    Ok(())
}

fn compare_versions(a: &str, b: &str) -> i32 {
    let va: Vec<i32> = a.split('.').filter_map(|s| s.parse().ok()).collect();
    let vb: Vec<i32> = b.split('.').filter_map(|s| s.parse().ok()).collect();
    for i in 0..3 {
        let diff = va.get(i).unwrap_or(&0) - vb.get(i).unwrap_or(&0);
        if diff != 0 {
            return diff;
        }
    }
    0
}

fn get_platform_key() -> String {
    let os = if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(windows) {
        "win32"
    } else {
        "linux"
    };
    let arch = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "x64"
    };
    format!("{os}-{arch}")
}
