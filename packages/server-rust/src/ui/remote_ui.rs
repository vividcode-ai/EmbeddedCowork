use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::settings::service::SettingsService;

const DEFAULT_MANIFEST_URL: &str = "https://ui.embeddedcowork.vividcode.ai/version.json";
const MANIFEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
const ZIP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

#[derive(Debug, Clone, serde::Deserialize)]
pub struct RemoteUIManifest {
    #[serde(rename = "minServerVersion")]
    pub min_server_version: String,
    #[serde(rename = "latestUIVersion")]
    pub latest_ui_version: String,
    #[serde(rename = "uiPackageURL")]
    pub ui_package_url: String,
    pub sha256: String,
    #[serde(rename = "latestServerVersion")]
    pub latest_server_version: Option<String>,
    #[serde(rename = "latestServerUrl")]
    pub latest_server_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UiSource {
    Bundled,
    Downloaded,
    Previous,
    Override,
    DevProxy,
    Missing,
}

#[derive(Debug, Clone)]
pub struct UiResolution {
    pub ui_static_dir: Option<String>,
    pub ui_dev_server_url: Option<String>,
    pub source: UiSource,
    pub ui_version: Option<String>,
    pub supported: bool,
    pub message: Option<String>,
    pub latest_server_version: Option<String>,
    pub latest_server_url: Option<String>,
    pub min_server_version: Option<String>,
}

pub struct ResolveUiArgs {
    pub server_version: String,
    pub bundled_ui_dir: Option<String>,
    pub auto_update: bool,
    pub no_update: bool,
    pub override_ui_dir: Option<String>,
    pub ui_dev_server_url: Option<String>,
    pub manifest_url: Option<String>,
    pub config_dir: PathBuf,
}

pub struct RemoteUIService;

impl RemoteUIService {
    pub fn new(_settings: Arc<Mutex<SettingsService>>) -> Self {
        Self
    }

    pub async fn resolve(&self, args: ResolveUiArgs) -> UiResolution {
        // Dev proxy takes highest precedence
        if let Some(ref dev_url) = args.ui_dev_server_url {
            return UiResolution {
                ui_dev_server_url: Some(dev_url.clone()),
                source: UiSource::DevProxy,
                supported: true,
                ..Default::default()
            };
        }

        // Override dir takes next precedence
        if let Some(ref override_dir) = args.override_ui_dir {
            let resolved = resolve_static_ui_dir(override_dir).await;
            let ui_version = read_ui_version(resolved.as_deref().unwrap_or(override_dir)).await;
            return UiResolution {
                ui_static_dir: resolved.or_else(|| Some(override_dir.clone())),
                source: UiSource::Override,
                ui_version,
                supported: true,
                ..Default::default()
            };
        }

        let ui_root = resolve_ui_cache_root(Some(&args.config_dir));
        let current_dir = ui_root.join("current");
        let previous_dir = ui_root.join("previous");

        // If auto-update is disabled or --no-update, use cached/bundled only
        if !args.auto_update || args.no_update {
            return resolve_from_cache_or_bundled(
                &args.bundled_ui_dir,
                &current_dir,
                &previous_dir,
                true,
                None,
                None,
                None,
                None,
            )
            .await;
        }

        // Fetch remote manifest
        let manifest_url = args.manifest_url.as_deref().unwrap_or(DEFAULT_MANIFEST_URL);
        let manifest = fetch_manifest(manifest_url).await;

        let manifest = match manifest {
            Ok(m) => m,
            Err(_) => {
                return resolve_from_cache_or_bundled(
                    &args.bundled_ui_dir,
                    &current_dir,
                    &previous_dir,
                    true,
                    None,
                    None,
                    None,
                    None,
                )
                .await;
            }
        };

        // Check if server version meets minimum
        let supported = compare_semver_core(&args.server_version, &manifest.min_server_version) >= 0;
        if !supported {
            let message = Some("Upgrade App to use latest features".to_string());
            return resolve_from_cache_or_bundled(
                &args.bundled_ui_dir,
                &current_dir,
                &previous_dir,
                false,
                message,
                manifest.latest_server_version.clone(),
                manifest.latest_server_url.clone(),
                Some(manifest.min_server_version.clone()),
            )
            .await;
        }

        // Pick best local UI
        let best_local = pick_best_local_ui(&args.bundled_ui_dir, &current_dir, &previous_dir).await;

        let remote_is_newer = match &best_local {
            None => true,
            Some(local) => {
                compare_semver_maybe(Some(&manifest.latest_ui_version), local.ui_version.as_deref()) > 0
            }
        };

        if !remote_is_newer {
            return resolve_from_cache_or_bundled(
                &args.bundled_ui_dir,
                &current_dir,
                &previous_dir,
                true,
                None,
                manifest.latest_server_version.clone(),
                manifest.latest_server_url.clone(),
                Some(manifest.min_server_version.clone()),
            )
            .await;
        }

        // Try to download and install remote UI
        match install_remote_ui(&manifest, &ui_root, &current_dir, &previous_dir).await {
            Ok(()) => {
                if let Some(installed) = resolve_static_ui_dir(&current_dir).await {
                    let ui_version = read_ui_version(&installed).await;
                    return UiResolution {
                        ui_static_dir: Some(installed),
                        source: UiSource::Downloaded,
                        ui_version,
                        supported: true,
                        latest_server_version: manifest.latest_server_version,
                        latest_server_url: manifest.latest_server_url,
                        min_server_version: Some(manifest.min_server_version),
                        ..Default::default()
                    };
                }
            }
            Err(e) => {
                tracing::warn!(component = "remote-ui", error = %e, "Failed to install remote UI, falling back");
            }
        }

        resolve_from_cache_or_bundled(
            &args.bundled_ui_dir,
            &current_dir,
            &previous_dir,
            true,
            None,
            manifest.latest_server_version,
            manifest.latest_server_url,
            Some(manifest.min_server_version),
        )
        .await
    }
}

impl Default for UiResolution {
    fn default() -> Self {
        Self {
            ui_static_dir: None,
            ui_dev_server_url: None,
            source: UiSource::Missing,
            ui_version: None,
            supported: true,
            message: None,
            latest_server_version: None,
            latest_server_url: None,
            min_server_version: None,
        }
    }
}

fn resolve_ui_cache_root(config_dir: Option<&PathBuf>) -> PathBuf {
    config_dir
        .map(|d| d.join("ui"))
        .unwrap_or_else(|| {
            dirs::config_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("EmbeddedCowork")
                .join("ui")
        })
}

async fn resolve_from_cache_or_bundled(
    bundled_ui_dir: &Option<String>,
    current_dir: &Path,
    previous_dir: &Path,
    supported: bool,
    message: Option<String>,
    latest_server_version: Option<String>,
    latest_server_url: Option<String>,
    min_server_version: Option<String>,
) -> UiResolution {
    let best_local = pick_best_local_ui(bundled_ui_dir, current_dir, previous_dir).await;

    if let Some(best) = best_local {
        return UiResolution {
            ui_static_dir: Some(best.ui_static_dir),
            source: best.source,
            ui_version: best.ui_version,
            supported,
            message,
            latest_server_version,
            latest_server_url,
            min_server_version,
            ..Default::default()
        };
    }

    tracing::warn!(component = "remote-ui", bundled = ?bundled_ui_dir, "No UI assets found");
    UiResolution {
        ui_static_dir: bundled_ui_dir.clone(),
        source: UiSource::Missing,
        supported,
        message,
        latest_server_version,
        latest_server_url,
        min_server_version,
        ..Default::default()
    }
}

struct LocalUiCandidate {
    ui_static_dir: String,
    source: UiSource,
    ui_version: Option<String>,
    priority: i32,
}

async fn pick_best_local_ui(
    bundled_ui_dir: &Option<String>,
    current_dir: &Path,
    previous_dir: &Path,
) -> Option<LocalUiCandidate> {
    let mut candidates: Vec<LocalUiCandidate> = Vec::new();

    // Check downloaded "current" dir
    if let Some(resolved) = resolve_static_ui_dir(current_dir).await {
        let version = read_ui_version(&resolved).await;
        candidates.push(LocalUiCandidate {
            ui_static_dir: resolved,
            source: UiSource::Downloaded,
            ui_version: version,
            priority: 1,
        });
    }

    // Check bundled dir
    if let Some(ref bundled) = bundled_ui_dir {
        if let Some(resolved) = resolve_static_ui_dir(bundled).await {
            let version = read_ui_version(&resolved).await;
            candidates.push(LocalUiCandidate {
                ui_static_dir: resolved,
                source: UiSource::Bundled,
                ui_version: version,
                priority: 2,
            });
        }
    }

    // Check previous dir
    if let Some(resolved) = resolve_static_ui_dir(previous_dir).await {
        let version = read_ui_version(&resolved).await;
        candidates.push(LocalUiCandidate {
            ui_static_dir: resolved,
            source: UiSource::Previous,
            ui_version: version,
            priority: 0,
        });
    }

    if candidates.is_empty() {
        return None;
    }

    // Sort: highest version first, then by priority
    candidates.sort_by(|a, b| {
        let version_cmp = compare_semver_maybe(
            a.ui_version.as_deref(),
            b.ui_version.as_deref(),
        );
        // Reverse: higher version sorts first
        let ordering = match version_cmp {
            -1 => Ordering::Greater,
            0 => Ordering::Equal,
            _ => Ordering::Less,
        };
        ordering.then(b.priority.cmp(&a.priority))
    });

    candidates.into_iter().next()
}

async fn resolve_static_ui_dir(dir: impl AsRef<Path>) -> Option<String> {
    let dir = dir.as_ref();
    let index_path = dir.join("index.html");
    tokio::fs::metadata(&index_path).await.ok().filter(|m| m.is_file())?;
    Some(dir.to_string_lossy().to_string())
}

async fn read_ui_version(ui_dir: impl AsRef<Path>) -> Option<String> {
    let version_path = ui_dir.as_ref().join("ui-version.json");
    let content = tokio::fs::read_to_string(version_path).await.ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&content).ok()?;
    parsed.get("uiVersion")
        .or_else(|| parsed.get("version"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

async fn fetch_manifest(url: &str) -> Result<RemoteUIManifest, Box<dyn std::error::Error + Send + Sync>> {
    let client = reqwest::Client::builder()
        .timeout(MANIFEST_TIMEOUT)
        .build()?;

    let resp = client
        .get(url)
        .header("Accept", "application/json")
        .header("User-Agent", "EmbeddedCowork-CLI")
        .send()
        .await?;

    if !resp.status().is_success() {
        return Err(format!("Manifest responded with {}", resp.status()).into());
    }

    let manifest: RemoteUIManifest = resp.json().await?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}

fn validate_manifest(manifest: &RemoteUIManifest) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if manifest.min_server_version.trim().is_empty() {
        return Err("Manifest missing minServerVersion".into());
    }
    if manifest.latest_ui_version.trim().is_empty() {
        return Err("Manifest missing latestUIVersion".into());
    }
    if manifest.ui_package_url.trim().is_empty() {
        return Err("Manifest missing uiPackageURL".into());
    }
    if !manifest.ui_package_url.starts_with("https://") {
        return Err("uiPackageURL must be https".into());
    }
    if manifest.sha256.trim().len() != 64 || !manifest.sha256.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("sha256 must be 64 hex chars".into());
    }
    Ok(())
}

async fn install_remote_ui(
    manifest: &RemoteUIManifest,
    ui_root: &Path,
    current_dir: &Path,
    previous_dir: &Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tokio::fs::create_dir_all(ui_root).await?;

    let tmp_dir = ui_root.join(format!("tmp-{}", std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()));
    let zip_path = ui_root.join(format!("ui-{}.zip", manifest.latest_ui_version));

    // Ensure cleanup on failure
    let result = install_remote_ui_inner(manifest, &tmp_dir, &zip_path, current_dir, previous_dir).await;
    // Cleanup temp files
    let _ = tokio::fs::remove_dir_all(&tmp_dir).await;
    let _ = tokio::fs::remove_file(&zip_path).await;
    result
}

async fn install_remote_ui_inner(
    manifest: &RemoteUIManifest,
    tmp_dir: &Path,
    zip_path: &Path,
    current_dir: &Path,
    previous_dir: &Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Download zip
    download_file(&manifest.ui_package_url, zip_path).await?;

    // Verify SHA256
    let digest = sha256_file(zip_path).await?;
    if !digest.eq_ignore_ascii_case(&manifest.sha256) {
        return Err(format!(
            "SHA256 mismatch for UI zip (expected {}, got {})",
            manifest.sha256, digest
        )
        .into());
    }

    // Extract zip
    extract_zip(zip_path, tmp_dir).await?;

    // Verify index.html exists
    if !tokio::fs::metadata(tmp_dir.join("index.html")).await.is_ok() {
        return Err("Extracted UI missing index.html".into());
    }

    // Rotate current -> previous
    rotate_dirs(current_dir, previous_dir).await?;

    // Move new -> current
    if tokio::fs::metadata(current_dir).await.is_ok() {
        tokio::fs::remove_dir_all(current_dir).await?;
    }
    tokio::fs::rename(tmp_dir, current_dir).await?;

    Ok(())
}

async fn download_file(url: &str, target_path: &Path) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let client = reqwest::Client::builder()
        .timeout(ZIP_TIMEOUT)
        .build()?;

    let resp = client
        .get(url)
        .header("Accept", "application/octet-stream")
        .header("User-Agent", "EmbeddedCowork-CLI")
        .send()
        .await?;

    if !resp.status().is_success() {
        return Err(format!("UI zip download failed with {}", resp.status()).into());
    }

    if let Some(parent) = target_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let bytes = resp.bytes().await?;
    tokio::fs::write(target_path, &bytes).await?;

    tracing::debug!(component = "remote-ui", %url, path = %target_path.display(), "Downloaded remote UI bundle");
    Ok(())
}

async fn sha256_file(path: &Path) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    use sha2::Digest;
    let data = tokio::fs::read(path).await?;
    let hash = sha2::Sha256::digest(&data);
    Ok(format!("{:x}", hash))
}

async fn extract_zip(zip_path: &Path, target_dir: &Path) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tokio::fs::create_dir_all(target_dir).await?;

    // ZIP extraction is CPU-bound, run on blocking pool
    let zip_path = zip_path.to_path_buf();
    let target_dir = target_dir.to_path_buf();

    tokio::task::spawn_blocking(move || -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let file = std::fs::File::open(&zip_path)?;
        let mut archive = zip::ZipArchive::new(file)?;

        let root = std::fs::canonicalize(&target_dir)?;

        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;
            let entry_path = entry.name().replace('\\', "/");

            // Zip-slip protection
            if entry_path.contains("..") || std::path::Path::new(&entry_path).is_absolute() {
                return Err(format!("Invalid zip entry path: {}", entry.name()).into());
            }

            let destination = target_dir.join(&entry_path);
            let canonical_dest = std::fs::canonicalize(destination.parent().unwrap_or(&target_dir))?;
            if !canonical_dest.starts_with(&root) {
                return Err(format!("Zip entry escapes target dir: {}", entry.name()).into());
            }

            if entry.is_dir() {
                std::fs::create_dir_all(&destination)?;
            } else {
                if let Some(parent) = destination.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                let mut output = std::fs::File::create(&destination)?;
                std::io::copy(&mut entry, &mut output)?;
            }
        }

        Ok(())
    })
    .await?
}

async fn rotate_dirs(current_dir: &Path, previous_dir: &Path) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Remove previous dir
    if tokio::fs::metadata(previous_dir).await.is_ok() {
        tokio::fs::remove_dir_all(previous_dir).await?;
    }
    // Rename current -> previous
    if tokio::fs::metadata(current_dir).await.is_ok() {
        tokio::fs::rename(current_dir, previous_dir).await?;
    }
    Ok(())
}

fn compare_semver_core(a: &str, b: &str) -> i32 {
    let pa = parse_semver_core(a);
    let pb = parse_semver_core(b);
    if pa.major != pb.major {
        return if pa.major > pb.major { 1 } else { -1 };
    }
    if pa.minor != pb.minor {
        return if pa.minor > pb.minor { 1 } else { -1 };
    }
    if pa.patch != pb.patch {
        return if pa.patch > pb.patch { 1 } else { -1 };
    }
    0
}

fn compare_semver_maybe(a: Option<&str>, b: Option<&str>) -> i32 {
    match (a, b) {
        (None, None) => 0,
        (None, Some(_)) => -1,
        (Some(_), None) => 1,
        (Some(a), Some(b)) => compare_semver_core(a, b),
    }
}

fn parse_semver_core(value: &str) -> SemverCore {
    let core = value
        .trim()
        .trim_start_matches('v')
        .split('-')
        .next()
        .unwrap_or("0.0.0");
    let parts: Vec<&str> = core.split('.').collect();

    let parse_part = |s: Option<&&str>| -> u64 {
        s.and_then(|s| {
            let cleaned: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
            cleaned.parse::<u64>().ok()
        })
        .unwrap_or(0)
    };

    SemverCore {
        major: parse_part(parts.get(0)),
        minor: parse_part(parts.get(1)),
        patch: parse_part(parts.get(2)),
    }
}

struct SemverCore {
    major: u64,
    minor: u64,
    patch: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compare_semver_core() {
        assert_eq!(compare_semver_core("1.0.0", "1.0.0"), 0);
        assert_eq!(compare_semver_core("2.0.0", "1.0.0"), 1);
        assert_eq!(compare_semver_core("1.0.0", "2.0.0"), -1);
        assert_eq!(compare_semver_core("1.1.0", "1.0.0"), 1);
        assert_eq!(compare_semver_core("1.0.1", "1.0.0"), 1);
        assert_eq!(compare_semver_core("v1.0.0", "1.0.0"), 0);
        assert_eq!(compare_semver_core("1.0.0-beta", "1.0.0"), 0);
    }
}
