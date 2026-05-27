use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

use axum::{
    body::Body,
    extract::Request,
    http::{header, HeaderValue, StatusCode},
    middleware,
    response::Response,
    Router,
};
use hyper_util::{
    rt::{TokioExecutor, TokioIo},
    server::conn::auto::Builder as HyperBuilder,
    service::TowerToHyperService,
};
use std::net::SocketAddr;
use tower::ServiceExt;
use tower_http::cors::{AllowHeaders, AllowMethods, AllowOrigin, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::api_types::UiSource;
use crate::auth::manager::AuthManager;
use crate::background_processes::manager::BgProcessManager;
use crate::background_processes::plugin_manager::PluginBgProcessManager;
use crate::clients::connection_manager::ClientConnectionManager;
use crate::events::bus::EventBus;
use crate::filesystem::browser::FileBrowser;
use crate::plugins::channel::PluginChannelManager;
use crate::plugins::voice_mode::VoiceModeManager;
use crate::server::auth_middleware::{self, AuthMiddlewareState};
use crate::server::proxy::SharedProxyClient;
use crate::server::remote_proxy::RemoteProxyManager;
use crate::server::routes::auth::{auth_routes, AuthRouteState};
use crate::server::routes::auth_pages::{auth_pages_routes, AuthPagesRouteState};
use crate::server::routes::background_processes::{background_processes_routes, BgProcessRouteState};
use crate::server::routes::events::{events_routes, EventsRouteState};
use crate::server::routes::filesystem::{filesystem_routes, FilesystemRouteState};
use crate::server::routes::instance_proxy::{instance_proxy_routes, InstanceProxyRouteState};
use crate::server::routes::meta::{meta_routes, MetaRouteState};
use crate::server::routes::opencode_status::{opencode_status_routes, OpenCodeStatusRouteState};
use crate::server::routes::plugin::{plugin_routes, PluginRouteState};
use crate::server::routes::plugin_background_processes::{plugin_bg_process_routes, PluginBgProcessRouteState};
use crate::server::routes::remote_proxy::{remote_proxy_routes, RemoteProxyRouteState};
use crate::server::routes::remote_servers::{remote_servers_routes, RemoteServerRouteState};
use crate::server::routes::settings::{settings_routes, SettingsRouteState};
use crate::server::routes::sidecar_proxy::{sidecar_proxy_routes, SidecarProxyRouteState};
use crate::server::routes::sidecars::{sidecars_routes, SidecarRouteState};
use crate::server::routes::speech::{speech_routes, SpeechRouteState};
use crate::server::routes::storage::{storage_routes, StorageRouteState};
use crate::server::routes::workspaces::{workspace_routes, WorkspaceRouteState};
use crate::server::routes::worktrees::{worktrees_routes, WorktreeRouteState};
use crate::server::tls::HttpsConfig;
use crate::settings::binaries::BinaryResolver;
use crate::settings::service::SettingsService;
use crate::sidecars::manager::SideCarManager;
use crate::speech::service::SpeechService;
use crate::storage::instance_store::InstanceStore;
use crate::workspaces::manager::WorkspaceManager;

#[derive(Clone)]
pub struct AppState {
    pub auth_manager: Arc<Mutex<AuthManager>>,
    pub workspace_manager: Arc<Mutex<WorkspaceManager>>,
    pub settings: Arc<Mutex<SettingsService>>,
    pub binary_resolver: Arc<Mutex<BinaryResolver>>,
    pub bg_process_manager: Arc<Mutex<BgProcessManager>>,
    pub file_browser: Arc<Mutex<FileBrowser>>,
    pub sidecar_manager: Arc<Mutex<SideCarManager>>,
    pub speech_service: Arc<Mutex<SpeechService>>,
    pub instance_store: Arc<Mutex<InstanceStore>>,
    pub client_connection_manager: Arc<Mutex<ClientConnectionManager>>,
    pub voice_mode_manager: Arc<Mutex<VoiceModeManager>>,
    pub event_bus: EventBus,
    pub plugin_channel: Arc<Mutex<PluginChannelManager>>,
    pub proxy_manager: Arc<Mutex<RemoteProxyManager>>,
    pub proxy_client: SharedProxyClient,
}

pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub protocol: String,
    pub ui_static_dir: Option<String>,
    pub workspace_root: String,
    pub ui_dev_server_url: Option<String>,
    pub ui_version: Option<String>,
    pub ui_source: UiSource,
    pub https_config: Option<HttpsConfig>,
    pub https_port: u16,
}

pub async fn build_and_start_server(
    state: AppState,
    mut config: ServerConfig,
    shutdown_signal: impl std::future::Future<Output = ()> + Send + 'static,
) {
    let has_https = config.https_config.is_some();

    // Step 1: Bind listeners first to support port 0 auto-allocation
    let (http_listener, https_listener, tls_config) = if has_https {
        let https_cfg = config.https_config.clone().unwrap();
        let http_addr: SocketAddr = format!("127.0.0.1:{}", config.port)
            .parse()
            .expect("Invalid HTTP address");
        let https_addr: SocketAddr = format!("{}:{}", config.host, config.https_port)
            .parse()
            .expect("Invalid HTTPS address");

        let h_listener = tokio::net::TcpListener::bind(http_addr).await.unwrap();
        let hs_listener = tokio::net::TcpListener::bind(https_addr).await.unwrap();

        let actual_port = h_listener.local_addr().unwrap().port();
        let actual_https_port = hs_listener.local_addr().unwrap().port();
        config.port = actual_port;
        config.https_port = actual_https_port;

        let tls = build_tls_config(&https_cfg);
        (h_listener, Some(hs_listener), Some(tls))
    } else {
        let addr: SocketAddr = format!("{}:{}", config.host, config.port)
            .parse()
            .expect("Invalid address");
        let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
        config.port = listener.local_addr().unwrap().port();
        (listener, None, None)
    };

    // Step 1b: Set server base URL with the actual port so EMBEDDEDCOWORK_BASE_URL
    // is correct even when port 0 auto-allocation is used (dev mode).
    let server_base_url = format!("http://127.0.0.1:{}", config.port);
    state.workspace_manager.lock().await.set_server_base_url(&server_base_url);

    // Step 2: Build router with actual port (important for meta route)
    let router = build_api_router(
        state.clone(),
        config.host.clone(),
        config.port,
        config.protocol.clone(),
        config.workspace_root.clone(),
        config.ui_version.clone(),
        config.ui_source.clone(),
    );

    let app = build_ui_router(router, &config);

    // CORS
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::mirror_request())
        .allow_methods(AllowMethods::mirror_request())
        .allow_headers(AllowHeaders::mirror_request())
        .allow_credentials(true);

    let app = app
        .layer(TraceLayer::new_for_http())
        .layer(cors);

    // Step 3: Print connection URL with actual port
    let local_host = if config.host == "0.0.0.0" || config.host.starts_with("127.") || config.host == "::1" {
        "127.0.0.1".to_string()
    } else {
        config.host.clone()
    };
    println!("Local Connection URL : {}://{}:{}", config.protocol, local_host, config.port);

    if has_https {
        println!("  HTTP  : http://127.0.0.1:{}", config.port);
        println!("  HTTPS : {}:{}", config.host, config.https_port);
    }

    // Step 4: Serve
    if let (Some(https_listener), Some(tls_config)) = (https_listener, tls_config) {
        let app_https = app.clone();
        let shutdown = std::sync::Arc::new(tokio::sync::Notify::new());
        let s1 = shutdown.clone();
        let s2 = shutdown.clone();

        let http_fut = async move {
            axum::serve(
                http_listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
                .with_graceful_shutdown(async move {
                    shutdown_signal.await;
                    s1.notify_waiters();
                })
                .await
                .unwrap();
        };

        let https_fut = async move {
            tokio::select! {
                _ = s2.notified() => {},
                result = serve_https(https_listener, app_https, tls_config) => {
                    if let Err(e) = result {
                        tracing::error!(component = "server", error = %e, "HTTPS server error");
                    }
                }
            }
        };

        tokio::join!(http_fut, https_fut);
    } else {
        tracing::info!(
            component = "server",
            addr = %http_listener.local_addr().unwrap(),
            protocol = %config.protocol,
            "HTTP server listening"
        );
        axum::serve(
            http_listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
            .with_graceful_shutdown(shutdown_signal)
            .await
            .unwrap();
    }
}

fn build_tls_config(https_config: &HttpsConfig) -> Arc<rustls::ServerConfig> {
    let certs = rustls_pemfile::certs(&mut https_config.cert.as_bytes())
        .collect::<Result<Vec<_>, _>>()
        .expect("Failed to parse TLS certificate");

    let key = rustls_pemfile::private_key(&mut https_config.key.as_bytes())
        .expect("Failed to parse TLS private key")
        .expect("No private key found");

    let config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .expect("Failed to build TLS config");

    Arc::new(config)
}

async fn serve_https(
    listener: tokio::net::TcpListener,
    app: Router,
    tls_config: Arc<rustls::ServerConfig>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use hyper::body::Incoming;

    let acceptor = tokio_rustls::TlsAcceptor::from(tls_config);

    loop {
        let (stream, _peer_addr) = listener.accept().await?;
        let acceptor = acceptor.clone();
        let app = app.clone();

        tokio::spawn(async move {
            match acceptor.accept(stream).await {
                Ok(tls_stream) => {
                    let io = TokioIo::new(tls_stream);

                    let svc = app.map_request(|req: http::Request<Incoming>| {
                        let (parts, body) = req.into_parts();
                        http::Request::from_parts(parts, Body::new(body))
                    });
                    let hyper_svc = TowerToHyperService::new(svc);

                    if let Err(e) = HyperBuilder::new(TokioExecutor::new())
                        .serve_connection_with_upgrades(io, hyper_svc)
                        .await
                    {
                        tracing::warn!(component = "server", error = %e, "HTTPS connection error");
                    }
                }
                Err(e) => {
                    tracing::warn!(component = "server", error = %e, "TLS handshake failed");
                }
            }
        });
    }
}

fn build_ui_router(router: Router, config: &ServerConfig) -> Router {
    if let Some(ref dev_url) = config.ui_dev_server_url {
        let dev_url = dev_url.clone();
        router.fallback(move |req: Request| handle_dev_proxy(req, dev_url.clone()))
    } else if let Some(ref ui_dir) = config.ui_static_dir {
        let ui_dir = ui_dir.clone();
        router.fallback(move |req: Request| handle_spa_or_static(req, ui_dir.clone()))
    } else {
        router.fallback(|_req: Request| async { StatusCode::NOT_FOUND })
    }
}

pub fn build_api_router(
    state: AppState,
    host: String,
    port: u16,
    protocol: String,
    workspace_root: String,
    ui_version: Option<String>,
    ui_source: UiSource,
) -> Router {
    let auth_state = AuthRouteState {
        auth_manager: state.auth_manager.clone(),
    };
    let auth_pages_state = AuthPagesRouteState {
        auth_manager: state.auth_manager.clone(),
    };
    let workspace_state = WorkspaceRouteState {
        workspace_manager: state.workspace_manager.clone(),
        worktree_map: Arc::new(Mutex::new(HashMap::new())),
    };
    let settings_state = SettingsRouteState {
        settings: state.settings.clone(),
    };
    let bg_state = BgProcessRouteState {
        manager: state.bg_process_manager.clone(),
    };
    let fs_state = FilesystemRouteState {
        browser: state.file_browser.clone(),
    };
    let sidecar_state = SidecarRouteState {
        sidecar_manager: state.sidecar_manager.clone(),
    };
    let speech_state = SpeechRouteState {
        speech_service: state.speech_service.clone(),
    };
    let storage_state = StorageRouteState {
        instance_store: state.instance_store.clone(),
        settings: state.settings.clone(),
        event_bus: state.event_bus.clone(),
    };
    let events_state = EventsRouteState {
        event_bus: state.event_bus.clone(),
        client_connection_manager: state.client_connection_manager.clone(),
    };
    let plugin_state = PluginRouteState {
        channel_manager: state.plugin_channel.clone(),
        voice_mode_manager: state.voice_mode_manager.clone(),
        workspace_manager: state.workspace_manager.clone(),
    };
    let proxy_state = RemoteProxyRouteState {
        proxy_manager: state.proxy_manager.clone(),
    };
    let instance_proxy_state = InstanceProxyRouteState {
        workspace_manager: state.workspace_manager.clone(),
        proxy_client: state.proxy_client.clone(),
    };
    let sidecar_proxy_state = SidecarProxyRouteState {
        sidecar_manager: state.sidecar_manager.clone(),
        proxy_client: state.proxy_client.clone(),
    };
    let opencode_status_state = OpenCodeStatusRouteState {
        binary_resolver: state.binary_resolver.clone(),
    };
    let remote_server_state = RemoteServerRouteState {
        servers: Arc::new(Mutex::new(HashMap::new())),
    };
    let worktree_state = WorktreeRouteState {
        worktrees: Arc::new(Mutex::new(HashMap::new())),
    };
    let plugin_bg_state = PluginBgProcessRouteState {
        bg_process_manager: Arc::new(Mutex::new(PluginBgProcessManager::new())),
        workspace_manager: state.workspace_manager.clone(),
        channel_manager: state.plugin_channel.clone(),
    };

    let meta_state = MetaRouteState::new(
        host,
        port,
        protocol,
        workspace_root,
        ui_version,
        ui_source,
        state.event_bus.clone(),
        state.settings.clone(),
        state.binary_resolver.clone(),
        state.workspace_manager.clone(),
    );

    let public_routes = Router::new()
        .merge(auth_routes(auth_state))
        .merge(auth_pages_routes(auth_pages_state))
        .merge(meta_routes(meta_state));

    let protected_routes = Router::new()
        .merge(workspace_routes(workspace_state))
        .merge(settings_routes(settings_state))
        .merge(background_processes_routes(bg_state))
        .merge(filesystem_routes(fs_state))
        .merge(sidecars_routes(sidecar_state))
        .merge(speech_routes(speech_state))
        .merge(storage_routes(storage_state))
        .merge(events_routes(events_state))
        .merge(plugin_routes(plugin_state))
        .merge(plugin_bg_process_routes(plugin_bg_state))
        .merge(remote_proxy_routes(proxy_state))
        .merge(instance_proxy_routes(instance_proxy_state))
        .merge(sidecar_proxy_routes(sidecar_proxy_state))
        .merge(opencode_status_routes(opencode_status_state))
        .merge(remote_servers_routes(remote_server_state))
        .merge(worktrees_routes(worktree_state))
        .layer(middleware::from_fn_with_state(
            Arc::new(AuthMiddlewareState {
                auth_manager: state.auth_manager.clone(),
                workspace_manager: state.workspace_manager.clone(),
            }),
            auth_middleware::auth_middleware,
        ));

    Router::new()
        .merge(public_routes)
        .merge(protected_routes)
}

async fn handle_dev_proxy(req: Request, dev_url: String) -> Result<Response<Body>, StatusCode> {
    let path = req.uri().path_and_query().map(|pq| pq.as_str()).unwrap_or("/");
    let target_url = format!("{}{}", dev_url.trim_end_matches('/'), path);

    let client = reqwest::Client::new();
    let method = req.method().clone();
    let headers = req.headers().clone();
    let body_bytes = axum::body::to_bytes(req.into_body(), usize::MAX)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let reqwest_method = match method {
        http::Method::GET => reqwest::Method::GET,
        http::Method::POST => reqwest::Method::POST,
        http::Method::PUT => reqwest::Method::PUT,
        http::Method::DELETE => reqwest::Method::DELETE,
        http::Method::PATCH => reqwest::Method::PATCH,
        http::Method::HEAD => reqwest::Method::HEAD,
        _ => reqwest::Method::GET,
    };

    let mut rb = client.request(reqwest_method, &target_url);
    for (key, value) in headers.iter() {
        if key != "host" {
            rb = rb.header(key, value);
        }
    }
    if !body_bytes.is_empty() {
        rb = rb.body(body_bytes.to_vec());
    }

    let resp = rb.send().await.map_err(|_| StatusCode::BAD_GATEWAY)?;
    let status = resp.status();
    let resp_headers = resp.headers().clone();
    let resp_bytes = resp.bytes().await.map_err(|_| StatusCode::BAD_GATEWAY)?;

    let mut response = Response::new(Body::from(resp_bytes));
    *response.status_mut() = status;
    for (key, value) in resp_headers.iter() {
        let lower = key.as_str().to_lowercase();
        if is_hop_by_hop(&lower) || lower == "content-length" || lower == "content-encoding" {
            continue;
        }
        response.headers_mut().insert(key.clone(), value.clone());
    }

    Ok(response)
}

async fn handle_spa_or_static(req: Request, ui_dir: String) -> Result<Response<Body>, StatusCode> {
    let path = req.uri().path();
    let file_path = Path::new(&ui_dir).join(path.trim_start_matches('/'));

    if file_path.is_file() {
        let content = tokio::fs::read(&file_path).await.map_err(|_| StatusCode::NOT_FOUND)?;
        let mime = guess_mime_type(&file_path);
        let mut res = Response::new(Body::from(content));
        res.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_str(mime).unwrap_or(HeaderValue::from_static("application/octet-stream")),
        );
        return Ok(res);
    }

    let index_path = Path::new(&ui_dir).join("index.html");
    if index_path.is_file() {
        let content = tokio::fs::read(&index_path).await.map_err(|_| StatusCode::NOT_FOUND)?;
        let mut res = Response::new(Body::from(content));
        res.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/html; charset=utf-8"),
        );
        return Ok(res);
    }

    Err(StatusCode::NOT_FOUND)
}

fn guess_mime_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" => "application/javascript",
        "mjs" => "application/javascript",
        "json" => "application/json",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "wasm" => "application/wasm",
        "txt" => "text/plain; charset=utf-8",
        "map" => "application/json",
        "webp" => "image/webp",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        _ => "application/octet-stream",
    }
}

fn is_hop_by_hop(name: &str) -> bool {
    matches!(
        name,
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
    )
}
