use std::collections::HashMap;
use std::path::Path as StdPath;
use std::sync::Arc;
use tokio::sync::Mutex;

use axum::{
    body::Body,
    extract::{Request, State},
    http::StatusCode,
    response::Response,
    routing, Router,
};

use crate::server::proxy::SharedProxyClient;
use crate::workspaces::git_worktrees::is_valid_worktree_slug;
use crate::workspaces::manager::WorkspaceManager;

const INSTANCE_PROXY_HOST: &str = "127.0.0.1";
const OPENCODE_DIR_OVERRIDE_PREFIX: &str = "__dir/";
const OPENCODE_DIR_OVERRIDE_MAX_LEN: usize = 4096;

#[derive(Clone)]
pub struct InstanceProxyRouteState {
    pub workspace_manager: Arc<Mutex<WorkspaceManager>>,
    pub proxy_client: SharedProxyClient,
}

pub fn instance_proxy_routes(state: InstanceProxyRouteState) -> Router {
    Router::new()
        .route(
            "/workspaces/:id/worktrees/:slug/instance",
            routing::any(proxy_handler),
        )
        .route(
            "/workspaces/:id/worktrees/:slug/instance/*rest",
            routing::any(proxy_handler),
        )
        .with_state(state)
}

async fn proxy_handler(
    State(state): State<InstanceProxyRouteState>,
    req: Request,
) -> Result<Response<Body>, StatusCode> {
    // Extract params from full URI path (not stripped by .route())
    let full_path = req.uri().path().to_owned();
    let method = req.method().clone();

    println!(
        "[PROXY] proxy_handler called: {} {}",
        method,
        full_path,
    );

    // Parse /workspaces/{id}/worktrees/{slug}/instance[/{rest}]
    let parts: Vec<&str> = full_path.splitn(6, '/').collect();
    // [ "", "workspaces", "{id}", "worktrees", "{slug}", "instance/..." ]
    let id = parts.get(2).ok_or_else(|| {
        println!("[PROXY] Bad request: cannot extract id from path={}", full_path);
        StatusCode::BAD_REQUEST
    })?.to_string();
    let slug = parts.get(4).ok_or_else(|| {
        println!("[PROXY] Bad request: cannot extract slug from path={}", full_path);
        StatusCode::BAD_REQUEST
    })?.to_string();

    // Validate worktree slug
    if !is_valid_worktree_slug(&slug) {
        println!("[PROXY] Bad request: invalid slug={}", slug);
        return Err(StatusCode::BAD_REQUEST);
    }

    // Extract remaining path after "/workspaces/{id}/worktrees/{slug}/instance"
    let prefix = format!("/workspaces/{}/worktrees/{}/instance", id, slug);
    let path = if full_path.len() > prefix.len() {
        full_path[prefix.len()..].trim_start_matches('/').to_string()
    } else {
        String::new()
    };

    println!("[PROXY] routing: id={} slug={} path={}", id, slug, path);

    // Get workspace and port
    let (port, instance_auth_header, workspace) = {
        let manager = state.workspace_manager.lock().await;
        let port = manager
            .get_instance_port(&id)
            .await
            .ok_or_else(|| {
                println!("[PROXY] Bad gateway: no port for id={}", id);
                StatusCode::BAD_GATEWAY
            })?;
        let auth = manager
            .get_instance_authorization_header(&id)
            .await;
        let ws = manager.get(&id).await;
        (port, auth, ws)
    };

    // Handle OpenCode directory override
    let extracted = extract_opencode_directory_override(&path)
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    let directory = if let Some(ref override_dir) = extracted.override_directory {
        let ws_root = workspace
            .as_ref()
            .map(|w| w.path.clone())
            .unwrap_or_default();
        validate_and_normalize_override_directory(override_dir, &ws_root)
            .map_err(|_| StatusCode::BAD_REQUEST)?
    } else {
        // Resolve worktree directory from workspace + slug
        let ws_path = workspace
            .as_ref()
            .map(|w| w.path.clone())
            .ok_or(StatusCode::NOT_FOUND)?;
        // Use resolved path directly from workspace + slug
        let resolved = resolve_worktree_directory(&id, &ws_path, &slug);
        resolved.ok_or(StatusCode::NOT_FOUND)?
    };

    let forwarded_suffix = extracted.forwarded_suffix.as_deref().unwrap_or("");
    let normalized_suffix = normalize_instance_suffix(forwarded_suffix);
    let query = req
        .uri()
        .query()
        .map(|q| format!("?{}", q))
        .unwrap_or_default();
    let target_url = format!(
        "http://{}:{}{}{}",
        INSTANCE_PROXY_HOST, port, normalized_suffix, query
    );

    let headers = req.headers().clone();

    // Filter and build proxy headers (hop-by-hop removal, auth injection, directory header)
    let proxy_headers = build_instance_proxy_headers(&headers, &instance_auth_header, &directory);

    // Log proxy headers for debugging
    println!("[PROXY] forwarding headers: {:?}", proxy_headers);

    let body_bytes = axum::body::to_bytes(req.into_body(), usize::MAX)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Log POST/PUT/PATCH requests at info level for visibility
    if method != http::Method::GET && method != http::Method::HEAD {
        println!(
            "[PROXY] --> {} {} body={}",
            method,
            target_url,
            String::from_utf8_lossy(&body_bytes),
        );
    }

    // Build new header map for forwarding
    let mut header_map = http::HeaderMap::new();
    for (key, value) in &proxy_headers {
        if let (Ok(name), Ok(val)) = (
            http::HeaderName::from_bytes(key.as_bytes()),
            http::HeaderValue::from_str(value),
        ) {
            header_map.insert(name, val);
        }
    }

    state
        .proxy_client
        .forward_request(method, &target_url, &header_map, body_bytes.to_vec())
        .await
}

struct DirOverrideResult {
    override_directory: Option<String>,
    forwarded_suffix: Option<String>,
}

fn extract_opencode_directory_override(path_suffix: &str) -> Result<DirOverrideResult, String> {
    let trimmed = path_suffix.trim_start_matches('/');
    if !trimmed.starts_with(OPENCODE_DIR_OVERRIDE_PREFIX) {
        return Ok(DirOverrideResult {
            override_directory: None,
            forwarded_suffix: Some(path_suffix.to_string()),
        });
    }

    let rest = &trimmed[OPENCODE_DIR_OVERRIDE_PREFIX.len()..];
    let slash_index = rest.find('/').unwrap_or(rest.len());
    let encoded = rest[..slash_index].trim();
    let remaining = if slash_index < rest.len() {
        &rest[slash_index + 1..]
    } else {
        ""
    };

    if encoded.is_empty() {
        return Err("Missing directory override".to_string());
    }
    if encoded.len() > OPENCODE_DIR_OVERRIDE_MAX_LEN {
        return Err("Directory override too large".to_string());
    }

    let decoded = decode_base64url(encoded)?;
    Ok(DirOverrideResult {
        override_directory: Some(decoded),
        forwarded_suffix: Some(remaining.to_string()),
    })
}

fn decode_base64url(input: &str) -> Result<String, String> {
    // base64url -> base64
    let normalized = input.replace('-', "+").replace('_', "/");
    let padding_len = (4 - normalized.len() % 4) % 4;
    let padded = format!("{}{}", normalized, "=".repeat(padding_len));

    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&padded)
        .map_err(|_| "Invalid base64url encoding".to_string())?;

    String::from_utf8(bytes).map_err(|_| "Invalid UTF-8 in directory override".to_string())
}

fn validate_and_normalize_override_directory(
    raw: &str,
    workspace_root: &str,
) -> Result<String, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err("Override directory is empty".to_string());
    }

    let raw_path = StdPath::new(raw);
    if !raw_path.is_absolute() {
        return Err("Override directory must be an absolute path".to_string());
    }
    if !raw_path.exists() {
        return Err(format!("Override directory does not exist: {}", raw));
    }
    if !raw_path.is_dir() {
        return Err(format!("Override path is not a directory: {}", raw));
    }

    let canonical = std::fs::canonicalize(raw)
        .map_err(|e| format!("Failed to canonicalize path: {}", e))?;
    let canonical_root = std::fs::canonicalize(workspace_root)
        .map_err(|e| format!("Failed to canonicalize workspace root: {}", e))?;

    if !canonical.starts_with(&canonical_root) {
        return Err("Override directory must be within the workspace root".to_string());
    }

    Ok(canonical.to_string_lossy().to_string())
}

fn resolve_worktree_directory(
    _workspace_id: &str,
    workspace_path: &str,
    _slug: &str,
) -> Option<String> {
    // For the default "root" worktree, use the workspace path directly.
    // For other slugs, would need to resolve from git worktree list.
    Some(workspace_path.to_string())
}

fn normalize_instance_suffix(path_suffix: &str) -> String {
    if path_suffix.is_empty() || path_suffix == "/" {
        return "/".to_string();
    }
    let trimmed = path_suffix.trim_start_matches('/');
    if trimmed.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", trimmed)
    }
}

fn build_instance_proxy_headers(
    headers: &http::HeaderMap,
    instance_auth: &Option<String>,
    directory: &str,
) -> HashMap<String, String> {
    let mut result = HashMap::new();

    for (key, value) in headers.iter() {
        let lower = key.as_str().to_lowercase();
        // Skip hop-by-hop, host, and content-length (reqwest sets it from body)
        if is_hop_by_hop_header(&lower) || lower == "host" || lower == "content-length" {
            continue;
        }
        if let Ok(val) = value.to_str() {
            result.insert(key.as_str().to_string(), val.to_string());
        }
    }

    if let Some(auth) = instance_auth {
        result.insert("authorization".to_string(), auth.clone());
    }

    // x-opencode-directory header (URL-encode if non-ASCII)
    let is_non_ascii = directory.bytes().any(|b| b > 0x7F);
    if is_non_ascii {
        let encoded: String = percent_encoding::utf8_percent_encode(
            directory,
            percent_encoding::NON_ALPHANUMERIC,
        )
        .to_string();
        result.insert("x-opencode-directory".to_string(), encoded);
    } else {
        result.insert("x-opencode-directory".to_string(), directory.to_string());
    }

    result
}

fn is_hop_by_hop_header(name: &str) -> bool {
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
