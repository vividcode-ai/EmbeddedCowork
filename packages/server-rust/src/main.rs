use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

use clap::Parser;

use embeddedcowork_server::auth::auth_store::AuthStore;
use embeddedcowork_server::auth::manager::AuthManager;
use embeddedcowork_server::auth::manager::AuthManagerInit;
use embeddedcowork_server::auth::session_manager::SessionManager;
use embeddedcowork_server::auth::token_manager::TokenManager;
use embeddedcowork_server::background_processes::manager::BgProcessManager;
use embeddedcowork_server::clients::connection_manager::ClientConnectionManager;
use embeddedcowork_server::config::location::resolve_config_location;
use embeddedcowork_server::events::bus::EventBus;
use embeddedcowork_server::filesystem::browser::FileBrowser;
use embeddedcowork_server::launcher::BrowserLauncher;
use embeddedcowork_server::logger::{init_logger, init_logger_with_destination, Logger};
use embeddedcowork_server::opencode_downloader::OpenCodeDownloader;
use embeddedcowork_server::opencode_paths::get_opencode_binary_path;
use embeddedcowork_server::plugins::channel::PluginChannelManager;
use embeddedcowork_server::plugins::voice_mode::VoiceModeManager;
use embeddedcowork_server::releases::release_monitor::ReleaseMonitor;
use embeddedcowork_server::server::http_server::{build_and_start_server, AppState, ServerConfig};
use embeddedcowork_server::server::network_addresses::{resolve_network_addresses, resolve_remote_addresses};
use embeddedcowork_server::server::proxy::create_proxy_client;
use embeddedcowork_server::server::remote_proxy::RemoteProxyManager;
use embeddedcowork_server::server::tls::{
    resolve_https_options, ResolveHttpsOptionsArgs,
};
use embeddedcowork_server::settings::binaries::BinaryResolver;
use embeddedcowork_server::settings::service::{DocKind, SettingsService};
use embeddedcowork_server::sidecars::manager::SideCarManager;
use embeddedcowork_server::speech::service::SpeechService;
use embeddedcowork_server::storage::instance_store::InstanceStore;
use embeddedcowork_server::ui::remote_ui::{RemoteUIService, ResolveUiArgs};
use embeddedcowork_server::workspaces::instance_events::InstanceEventBridge;
use embeddedcowork_server::workspaces::manager::WorkspaceManager;

#[derive(Parser, Debug)]
#[command(name = "embeddedcowork-server", version, about = "EmbeddedCowork Server")]
struct CliArgs {
    /// Host to bind to
    #[arg(short = 'H', long, default_value = "127.0.0.1", env = "CLI_HOST")]
    host: String,

    /// HTTP port
    #[arg(short = 'P', long, default_value = "18081", env = "CLI_HTTP_PORT")]
    http_port: u16,

    /// HTTPS port
    #[arg(long, default_value = "18443", env = "CLI_HTTPS_PORT")]
    https_port: u16,

    /// Path to config directory
    #[arg(short, long, env = "CLI_CONFIG")]
    config_dir: Option<String>,

    /// Path to data directory
    #[arg(short = 'd', long)]
    data_dir: Option<String>,

    /// Enable HTTPS
    #[arg(long, env = "CLI_HTTPS")]
    https: bool,

    /// Enable HTTP (when HTTPS is also enabled)
    #[arg(long, env = "CLI_HTTP")]
    http: bool,

    /// Path to TLS cert
    #[arg(long, env = "CLI_TLS_CERT")]
    tls_cert: Option<String>,

    /// Path to TLS key
    #[arg(long, env = "CLI_TLS_KEY")]
    tls_key: Option<String>,

    /// Path to TLS CA
    #[arg(long, env = "CLI_TLS_CA")]
    tls_ca: Option<String>,

    /// Directory containing the built UI bundle
    #[arg(long, env = "CLI_UI_DIR")]
    ui_dir: Option<String>,

    /// Proxy UI requests to a running dev server
    #[arg(long, env = "CLI_UI_DEV_SERVER")]
    ui_dev_server_url: Option<String>,

    /// URL to fetch UI update manifest from
    #[arg(long, env = "CLI_UI_MANIFEST_URL")]
    ui_manifest_url: Option<String>,

    /// Enable automatic UI updates (default: true)
    #[arg(long, default_value = "true", env = "CLI_UI_AUTO_UPDATE")]
    ui_auto_update: bool,

    /// Disable UI update download
    #[arg(long, env = "CLI_UI_NO_UPDATE")]
    ui_no_update: bool,

    /// Additional DNS names / IPs for TLS certificate (comma-separated)
    #[arg(long, env = "CLI_TLS_SANS")]
    tls_sans: Option<String>,

    /// Launch browser on start
    #[arg(long, env = "CLI_LAUNCH")]
    launch: bool,

    /// Log level
    #[arg(short, long, default_value = "info", env = "CLI_LOG_LEVEL")]
    log_level: String,

    /// Log destination file (defaults to stdout)
    #[arg(long, env = "CLI_LOG_DESTINATION")]
    log_destination: Option<String>,

    /// Verbose output
    #[arg(short, long)]
    verbose: bool,

    /// Username for server authentication
    #[arg(long, env = "EMBEDDEDCOWORK_SERVER_USERNAME")]
    username: Option<String>,

    /// Password for server authentication
    #[arg(long, env = "EMBEDDEDCOWORK_SERVER_PASSWORD")]
    password: Option<String>,

    /// Emit a one-time bootstrap token for desktop auth
    #[arg(long, env = "EMBEDDEDCOWORK_GENERATE_TOKEN")]
    generate_token: bool,

    /// Disable internal auth (use only behind trusted perimeter)
    #[arg(long, env = "EMBEDDEDCOWORK_SKIP_AUTH")]
    dangerously_skip_auth: bool,

    /// Cookie name for server authentication
    #[arg(long, env = "EMBEDDEDCOWORK_AUTH_COOKIE_NAME")]
    auth_cookie_name: Option<String>,

    /// Allow browsing the full filesystem
    #[arg(long, env = "CLI_UNRESTRICTED_ROOT")]
    unrestricted_root: bool,
}

fn is_loopback_host(host: &str) -> bool {
    host == "127.0.0.1" || host == "::1" || host.starts_with("127.")
}

#[tokio::main]
async fn main() {
    let args = CliArgs::parse();

    let log_level = if args.verbose { "debug" } else { &args.log_level };
    if let Some(ref dest) = args.log_destination {
        init_logger_with_destination(Some(log_level), "server", Some(dest));
    } else {
        init_logger(Some(log_level), "server");
    }

    let logger = Logger::new("server");

    // Initialize config location
    let config_dir = args.config_dir.unwrap_or_else(|| {
        dirs::config_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("EmbeddedCowork")
            .to_string_lossy()
            .to_string()
    });

    let config_location = resolve_config_location(&config_dir);

    // Initialize event bus
    let event_bus = EventBus::new(Some(logger.child("events")));

    // Initialize settings service
    let mut settings = SettingsService::new(
        config_location.clone(),
        Some(event_bus.clone()),
        logger.child("settings"),
    );

    settings.get_doc(&DocKind::Config);

    let settings = Arc::new(Mutex::new(settings));

    // Initialize binary resolver
    let binary_resolver = Arc::new(Mutex::new(BinaryResolver::new(settings.clone())));

    // Initialize auth components
    let auth_store = AuthStore::new(config_location.base_dir.join("auth.json"));
    let session_manager = SessionManager::new();
    let token_manager = Some(TokenManager::new(60000));

    let mut auth_manager = AuthManager::new(
        auth_store,
        session_manager,
        token_manager,
    );

    // Apply CLI-level auth configuration
    auth_manager.apply_init(AuthManagerInit {
        config_path: config_location.config_yaml_path.to_string_lossy().to_string(),
        username: args.username.clone().unwrap_or_default(),
        password: args.password.clone(),
        generate_token: args.generate_token,
        dangerously_skip_auth: args.dangerously_skip_auth,
        cookie_name: args.auth_cookie_name.clone(),
    });

    // Bootstrap token generation
    if args.generate_token {
        if let Some(token) = auth_manager.issue_bootstrap_token() {
            println!("EMBEDDEDCOWORK_BOOTSTRAP_TOKEN:{}", token);
        }
    }

    let auth_manager = Arc::new(Mutex::new(auth_manager));

    // Initialize workspace manager
    let workspace_root = config_location.base_dir.to_string_lossy().to_string();
    let workspace_manager = WorkspaceManager::new(
        workspace_root.clone(),
        SettingsService::new(
            config_location.clone(),
            Some(event_bus.clone()),
            logger.child("settings-ws"),
        ),
        BinaryResolver::new(settings.clone()),
        event_bus.clone(),
        logger.child("workspaces"),
    );
    let workspace_manager = Arc::new(Mutex::new(workspace_manager));

    // Initialize background process manager
    let bg_process_manager = Arc::new(Mutex::new(BgProcessManager::new(
        logger.child("bg-processes"),
    )));

    // Initialize file browser
    let file_browser = Arc::new(Mutex::new(FileBrowser::new(
        vec![config_location.base_dir.to_string_lossy().to_string()],
        logger.child("filesystem"),
    ).with_unrestricted(args.unrestricted_root)));

    // Initialize sidecar manager
    let sidecar_manager = SideCarManager::new(
        logger.child("sidecars"),
        event_bus.clone(),
    ).with_settings(settings.clone()).await;
    let sidecar_manager = Arc::new(Mutex::new(sidecar_manager));

    // Initialize speech service
    let speech_service = Arc::new(Mutex::new(SpeechService::new(
        SettingsService::new(
            config_location.clone(),
            Some(event_bus.clone()),
            logger.child("settings-speech"),
        ),
        logger.child("speech"),
    )));

    // Initialize instance store
    let instance_store = Arc::new(Mutex::new(InstanceStore::new(
        config_location.instances_dir.clone(),
        logger.child("storage"),
    )));

    // Initialize plugin channel
    let plugin_channel = Arc::new(Mutex::new(PluginChannelManager::new(
        logger.child("plugins"),
    )));

    // Initialize client connection manager
    let client_connection_manager = Arc::new(Mutex::new(ClientConnectionManager::new(
        logger.child("client-connections"),
    )));

    // Initialize voice mode manager
    let voice_mode_manager = Arc::new(Mutex::new(VoiceModeManager::new(
        client_connection_manager.clone(),
        plugin_channel.clone(),
        logger.child("voice-mode"),
    )));

    // Initialize instance event bridge
    let instance_event_bridge = InstanceEventBridge::new(
        workspace_manager.clone(),
        event_bus.clone(),
        logger.child("instance-events"),
    );

    // Initialize proxy client
    let proxy_client = create_proxy_client();

    // Initialize remote proxy manager
    let proxy_manager = Arc::new(Mutex::new(RemoteProxyManager::new(
        logger.child("proxy"),
        proxy_client.clone(),
    )));

    // Initialize release monitor
    let _release_monitor = ReleaseMonitor::new(
        logger.child("releases"),
        env!("CARGO_PKG_VERSION").to_string(),
    );

    // Initialize browser launcher
    let launcher = BrowserLauncher::new(logger.child("launcher"));

    // Determine bundled UI directory (fallback if remote resolution fails)
    let bundled_ui_dir = args.ui_dir.clone().or_else(|| {
        let pkg_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let ui_dist = pkg_root.join("..").join("ui").join("dist");
        if ui_dist.exists() {
            Some(ui_dist.to_string_lossy().to_string())
        } else {
            None
        }
    });

    // Resolve UI source via RemoteUIService
    let remote_ui = RemoteUIService::new(settings.clone());
    let ui_resolution = remote_ui
        .resolve(ResolveUiArgs {
            server_version: env!("CARGO_PKG_VERSION").to_string(),
            bundled_ui_dir: bundled_ui_dir.clone(),
            auto_update: args.ui_auto_update,
            no_update: args.ui_no_update,
            override_ui_dir: args.ui_dir,
            ui_dev_server_url: args.ui_dev_server_url,
            manifest_url: args.ui_manifest_url,
            config_dir: PathBuf::from(&config_dir),
        })
        .await;

    let ui_static_dir = ui_resolution.ui_static_dir.or(bundled_ui_dir.clone());
    let ui_dev_server_url = ui_resolution.ui_dev_server_url;

    match ui_resolution.source {
        embeddedcowork_server::ui::remote_ui::UiSource::DevProxy => {
            tracing::info!(component = %logger.component, url = %ui_dev_server_url.as_deref().unwrap_or(""), "Using UI dev server proxy");
        }
        embeddedcowork_server::ui::remote_ui::UiSource::Downloaded => {
            tracing::info!(component = %logger.component, version = ?ui_resolution.ui_version, "Using downloaded remote UI");
        }
        embeddedcowork_server::ui::remote_ui::UiSource::Bundled => {
            tracing::info!(component = %logger.component, dir = ?ui_static_dir, "Using bundled UI");
        }
        embeddedcowork_server::ui::remote_ui::UiSource::Override => {
            tracing::info!(component = %logger.component, dir = ?ui_static_dir, "Using overridden UI directory");
        }
        _ => {
            let msg = match &ui_static_dir {
                Some(dir) => format!("UI static dir: {}", dir),
                None => "UI static dir: not configured (API only)".to_string(),
            };
            tracing::info!(component = %logger.component, "{}", msg);
        }
    }

    if let Some(msg) = &ui_resolution.message {
        tracing::warn!(component = %logger.component, message = %msg, "UI resolution warning");
    }

    // Determine protocol for display
    let has_https = args.https;
    let display_protocol = if has_https { "https" } else { "http" };

    // Resolve HTTPS config (provided or auto-generated)
    let https_config = resolve_https_options(ResolveHttpsOptionsArgs {
        https: args.https,
        tls_cert: args.tls_cert,
        tls_key: args.tls_key,
        tls_ca: args.tls_ca,
        tls_sans: args.tls_sans,
        host: args.host.clone(),
        config_dir: Some(PathBuf::from(&config_dir)),
        logger: Some(logger.child("tls")),
    });

    // Compute URL for browser launch
    let local_address = if is_loopback_host(&args.host) {
        "127.0.0.1".to_string()
    } else {
        args.host.clone()
    };
    let local_url = format!("{}://{}:{}", display_protocol, local_address, args.http_port);

    // Print remote connection URLs
    let remote_access_enabled = args.host == "0.0.0.0" || !is_loopback_host(&args.host);
    if remote_access_enabled {
        let remote_protocol = if has_https { "https" } else { "http" };
        let remote_port = if has_https { args.https_port } else { args.http_port };
        let resolved = resolve_remote_addresses(&args.host, remote_protocol, remote_port);
        if let Some(ref remote_url) = resolved.primary_remote_url {
            println!("Remote Connection URL  : {}", remote_url);
            let additional: Vec<&String> = resolved.user_visible.iter()
                .filter_map(|a| {
                    let url = &a.remote_url;
                    if Some(url) != resolved.primary_remote_url.as_ref() {
                        Some(url)
                    } else {
                        None
                    }
                })
                .collect();
            if !additional.is_empty() {
                println!("Other Accessible URLs:");
                for url in additional {
                    println!("  - {}", url);
                }
            }
        }
    }

    // Build app state
    let app_state = AppState {
        auth_manager,
        workspace_manager: workspace_manager.clone(),
        settings: settings.clone(),
        binary_resolver,
        bg_process_manager,
        file_browser,
        sidecar_manager: sidecar_manager.clone(),
        speech_service,
        instance_store,
        client_connection_manager,
        voice_mode_manager,
        event_bus: event_bus.clone(),
        plugin_channel,
        proxy_manager,
        proxy_client: proxy_client.clone(),
    };

    // Determine if we have remote addresses to show
    if args.host == "0.0.0.0" || !is_loopback_host(&args.host) {
        let remote_addrs = resolve_network_addresses(&args.host, display_protocol, args.http_port);
        for addr in &remote_addrs {
            tracing::info!(
                component = %logger.component,
                "Listening on {}://{}:{}",
                display_protocol, addr.ip, args.http_port
            );
        }
    }

    // Launch browser if requested
    if args.launch {
        if let Err(e) = launcher.launch(&local_url) {
            tracing::warn!(component = %logger.component, error = %e, "Failed to launch browser");
        }
    }

    // OpenCode binary auto-download if not available
    {
        let download_logger = logger.child("opencode-download");
        tokio::spawn(async move {
            // First check if opencode is available on system PATH
            let on_path = std::process::Command::new(if cfg!(windows) { "where" } else { "which" })
                .arg("opencode")
                .output()
                .ok()
                .filter(|o| o.status.success())
                .and_then(|o| {
                    let out = String::from_utf8_lossy(&o.stdout);
                    out.lines().next().map(|s| s.trim().to_string())
                })
                .filter(|s| !s.is_empty());

            if let Some(path) = on_path {
                tracing::info!(%path, "OpenCode binary found on system PATH, skipping download");
                return;
            }

            let platform = if cfg!(windows) { "win32" } else { "linux" };
            let binary_path = get_opencode_binary_path(platform);
            if !binary_path.exists() {
                tracing::info!("OpenCode binary not found, triggering auto-download...");
                let downloader = OpenCodeDownloader::new(download_logger);
                match downloader.get_latest_version().await {
                    Ok(version) => {
                        tracing::info!(%version, "Downloading OpenCode");
                        if let Err(e) = downloader.download_version(&version, &binary_path).await {
                            tracing::warn!(error = %e, "OpenCode auto-download failed");
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to check latest OpenCode version");
                    }
                }
            }
        });
    }

    // Build app state for shutdown signal
    let shutdown_workspace_manager = workspace_manager.clone();
    let shutdown_sidecar_manager = sidecar_manager.clone();
    let shutdown_instance_events = instance_event_bridge;

    // Shutdown signal future
    let shutdown_signal = async move {
        #[cfg(unix)]
        {
            let mut term_signal = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("Failed to register SIGTERM handler");
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {},
                _ = term_signal.recv() => {},
            }
        }
        #[cfg(not(unix))]
        {
            tokio::signal::ctrl_c().await.ok();
        }

        tracing::info!(component = "server", "Shutdown signal received, stopping workspaces and server");

        // Shutdown instance event bridge
        shutdown_instance_events.shutdown();

        // Shutdown sidecars
        shutdown_sidecar_manager.lock().await.shutdown().await;

        // Shutdown workspaces
        shutdown_workspace_manager.lock().await.shutdown().await;
    };

    // Start server(s) with graceful shutdown
    // Convert remote UI source to API type for meta route
    let api_ui_source: embeddedcowork_server::api_types::UiSource = ui_resolution.source.clone().into();

    let server_config = ServerConfig {
        host: args.host.clone(),
        port: args.http_port,
        protocol: display_protocol.to_string(),
        ui_static_dir,
        workspace_root,
        ui_dev_server_url,
        ui_version: ui_resolution.ui_version.clone(),
        ui_source: api_ui_source,
        https_config: https_config.map(|r| r.https_options),
        https_port: args.https_port,
    };

    tracing::info!(
        component = %logger.component,
        "Starting EmbeddedCowork server"
    );

    // Run with graceful shutdown
    build_and_start_server(app_state, server_config, shutdown_signal).await;
}
