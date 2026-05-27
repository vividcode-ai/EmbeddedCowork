use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::Mutex;

use axum::{
    body::Body,
    extract::{Request, State},
    http::{HeaderName, HeaderValue, StatusCode},
    response::Response,
    Router,
};
use hyper_util::rt::TokioIo;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::api_types::SideCarPrefixMode;
use crate::server::proxy::SharedProxyClient;
use crate::sidecars::manager::SideCarManager;

#[derive(Clone)]
pub struct SidecarProxyRouteState {
    pub sidecar_manager: Arc<Mutex<SideCarManager>>,
    pub proxy_client: SharedProxyClient,
}

pub fn sidecar_proxy_routes(state: SidecarProxyRouteState) -> Router {
    Router::new()
        .nest(
            "/sidecars",
            Router::new().fallback(proxy_handler),
        )
        .with_state(state)
}

async fn proxy_handler(
    State(state): State<SidecarProxyRouteState>,
    req: Request,
) -> Result<Response<Body>, StatusCode> {
    // Extract id and remaining path from URI: /sidecars/{id}/{*path} or /sidecars/{id}
    let uri_path = req.uri().path();
    let rest = uri_path.strip_prefix('/').unwrap_or("");
    let (id, path) = match rest.split_once('/') {
        Some((id, path)) => (id.to_string(), path.to_string()),
        None => (rest.to_string(), String::new()),
    };

    if id.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Check for WebSocket upgrade
    if is_websocket_upgrade(&req) {
        return handle_sidecar_ws_proxy(&state, &id, &path, req).await;
    }

    let sidecar = state
        .sidecar_manager
        .lock()
        .await
        .get(&id)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;

    let target_origin = format!(
        "{}://127.0.0.1:{}",
        if sidecar.insecure { "http" } else { "https" },
        sidecar.port
    );

    // Build target path using sidecar manager utilities
    let public_base = format!("/sidecars/{}", urlencoding(&id));
    let incoming_path = format!("{}/{}", public_base, path);
    let search = req
        .uri()
        .query()
        .map(|q| format!("?{}", q))
        .unwrap_or_default();

    let is_preserve = sidecar.prefix_mode == SideCarPrefixMode::Preserve;
    let target_path = get_sidecar_target_path(
        &id,
        &incoming_path,
        &search,
        is_preserve,
        &public_base,
    );
    let target_url = format!("{}{}", target_origin, target_path);

    let method = req.method().clone();
    let headers = req.headers().clone();

    // Sanitize request headers: remove blocked ones, set origin to target
    let proxy_headers = sanitize_sidecar_request_headers(&headers, &target_origin);

    let body_bytes = axum::body::to_bytes(req.into_body(), usize::MAX)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Forward request
    let mut response = state
        .proxy_client
        .forward_request(method, &target_url, &proxy_headers, body_bytes.to_vec())
        .await?;

    // Rewrite response headers (Location header for strip prefix_mode)
    rewrite_sidecar_response_headers(&mut response, &id, &target_origin, is_preserve);

    Ok(response)
}

fn sanitize_sidecar_request_headers(
    headers: &http::HeaderMap,
    target_origin: &str,
) -> http::HeaderMap {
    let blocked = blocked_sidecar_headers();
    let mut result = http::HeaderMap::new();

    for (key, value) in headers.iter() {
        let lower = key.as_str().to_lowercase();
        if blocked.contains(&lower.as_str()) {
            continue;
        }
        result.insert(key.clone(), value.clone());
    }

    // Set origin header to target origin
    if let Ok(val) = HeaderValue::from_str(target_origin) {
        result.insert(HeaderName::from_static("origin"), val);
    }

    result
}

fn rewrite_sidecar_response_headers(
    response: &mut Response<Body>,
    sidecar_id: &str,
    target_origin: &str,
    is_preserve: bool,
) {
    if is_preserve {
        return;
    }

    let headers = response.headers_mut();
    if let Some(location) = headers.get("location") {
        if let Ok(location_str) = location.to_str() {
            let public_base = format!("/sidecars/{}", urlencoding(sidecar_id));
            let rewritten = if location_str.starts_with('/') {
                format!("{}{}", public_base, location_str)
            } else if let Ok(parsed) = url::Url::parse(location_str) {
                if parsed.origin().ascii_serialization() == target_origin {
                    format!(
                        "{}{}{}{}",
                        public_base,
                        parsed.path(),
                        parsed.query().map(|q| format!("?{}", q)).unwrap_or_default(),
                        parsed.fragment().map(|f| format!("#{}", f)).unwrap_or_default()
                    )
                } else {
                    return; // Don't rewrite external redirects
                }
            } else {
                return; // Can't parse, leave as-is
            };

            if let Ok(val) = HeaderValue::from_str(&rewritten) {
                headers.insert("location", val);
            }
        }
    }
}

fn blocked_sidecar_headers() -> HashSet<&'static str> {
    [
        "host",
        "authorization",
        "proxy-authorization",
        "forwarded",
        "x-forwarded-for",
        "x-forwarded-host",
        "x-forwarded-port",
        "x-forwarded-proto",
    ]
    .into_iter()
    .collect()
}

fn get_sidecar_target_path(
    _id: &str,
    incoming_path: &str,
    search: &str,
    is_preserve: bool,
    public_base: &str,
) -> String {
    if is_preserve {
        return format!("{}{}", incoming_path, search);
    }

    let stripped = if incoming_path.starts_with(public_base) {
        incoming_path[public_base.len()..].to_string()
    } else {
        incoming_path.to_string()
    };

    let stripped = if stripped.is_empty() || stripped == "/" {
        "/".to_string()
    } else if !stripped.starts_with('/') {
        format!("/{}", stripped)
    } else {
        stripped
    };

    format!("{}{}", stripped, search)
}

fn urlencoding(input: &str) -> String {
    percent_encoding::utf8_percent_encode(input, percent_encoding::NON_ALPHANUMERIC).to_string()
}

// ── WebSocket proxy ──

fn is_websocket_upgrade(req: &Request) -> bool {
    let has_upgrade = req
        .headers()
        .get("upgrade")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.to_lowercase().contains("websocket"))
        .unwrap_or(false);

    let has_connection = req
        .headers()
        .get("connection")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.to_lowercase().contains("upgrade"))
        .unwrap_or(false);

    has_upgrade && has_connection
}

async fn handle_sidecar_ws_proxy(
    state: &SidecarProxyRouteState,
    id: &str,
    path: &str,
    mut req: Request,
) -> Result<Response<Body>, StatusCode> {
    let sidecar = state
        .sidecar_manager
        .lock()
        .await
        .get(id)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;

    let target_port = sidecar.port;
    let id_owned = id.to_string();

    // Build target request path
    let public_base = format!("/sidecars/{}", urlencoding(id));
    let incoming_path = format!("{}/{}", public_base, path);
    let search = req
        .uri()
        .query()
        .map(|q| format!("?{}", q))
        .unwrap_or_default();
    let target_request_path = format!("{}{}", incoming_path, search);

    // Extract hyper upgrade extension
    let on_upgrade = req
        .extensions_mut()
        .remove::<hyper::upgrade::OnUpgrade>()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    // Spawn proxy task
    tokio::spawn(async move {
        if let Err(e) = proxy_sidecar_websocket(on_upgrade, target_port, &target_request_path).await {
            tracing::warn!(sidecar_id = %id_owned, error = %e, "Sidecar WebSocket proxy failed");
        }
    });

    Ok(Response::builder()
        .status(StatusCode::SWITCHING_PROTOCOLS)
        .body(Body::empty())
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?)
}

async fn proxy_sidecar_websocket(
    on_upgrade: hyper::upgrade::OnUpgrade,
    target_port: u16,
    target_path: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Wait for the upgraded connection from the client, wrap for tokio IO compat
    let client_stream = TokioIo::new(on_upgrade.await?);

    // Connect to the target sidecar via TCP
    let target_addr = format!("127.0.0.1:{}", target_port);
    let mut target_stream = tokio::net::TcpStream::connect(&target_addr).await?;

    // Send the HTTP upgrade request to the target
    let upgrade_request = format!(
        "GET {} HTTP/1.1\r\n\
         Host: 127.0.0.1:{}\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
         Sec-WebSocket-Version: 13\r\n\
         \r\n",
        target_path, target_port
    );
    target_stream.write_all(upgrade_request.as_bytes()).await?;

    // Read the upgrade response from target
    let mut buf = vec![0u8; 4096];
    let n = target_stream.read(&mut buf).await?;
    let response = String::from_utf8_lossy(&buf[..n]);
    if !response.contains("101") {
        return Err("Target did not return 101 Switching Protocols".into());
    }

    // Pipe bidirectionally
    let (mut client_read, mut client_write) = tokio::io::split(client_stream);
    let (mut target_read, mut target_write) = target_stream.split();

    let client_to_target = tokio::io::copy(&mut client_read, &mut target_write);
    let target_to_client = tokio::io::copy(&mut target_read, &mut client_write);

    tokio::select! {
        _ = client_to_target => {},
        _ = target_to_client => {},
    }

    Ok(())
}
