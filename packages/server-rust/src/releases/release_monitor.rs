use crate::logger::Logger;
use crate::log_info;
use crate::log_warn;

pub struct ReleaseMonitor {
    logger: Logger,
    current_version: String,
    client: reqwest::Client,
}

impl ReleaseMonitor {
    pub fn new(logger: Logger, current_version: String) -> Self {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::USER_AGENT,
            reqwest::header::HeaderValue::from_static("EmbeddedCowork-Server"),
        );
        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            logger,
            current_version,
            client,
        }
    }

    pub fn current_version(&self) -> &str {
        &self.current_version
    }

    pub async fn check_for_updates(&self) -> Result<Option<String>, String> {
        let url = "https://api.github.com/repos/vividcode-ai/opencode/releases/latest";

        let response = match self.client.get(url).send().await {
            Ok(resp) => resp,
            Err(e) => {
                log_warn!(self.logger, format!("Failed to fetch latest release: {}", e));
                return Ok(None);
            }
        };

        if !response.status().is_success() {
            let status_code = response.status().as_u16();
            log_warn!(self.logger, format!("GitHub API returned non-success status: {}", status_code));
            return Ok(None);
        }

        #[derive(serde::Deserialize)]
        struct GitHubRelease {
            tag_name: String,
            #[allow(dead_code)]
            html_url: String,
            #[allow(dead_code)]
            published_at: Option<String>,
            #[allow(dead_code)]
            body: Option<String>,
        }

        let release: GitHubRelease = match response.json().await {
            Ok(r) => r,
            Err(e) => {
                log_warn!(self.logger, format!("Failed to parse GitHub release response: {}", e));
                return Ok(None);
            }
        };

        let fetched_version = release.tag_name.strip_prefix('v').unwrap_or(&release.tag_name).to_string();

        log_info!(self.logger, format!("Checked for updates: current={}, latest={}", self.current_version, fetched_version));

        if fetched_version != self.current_version {
            Ok(Some(fetched_version))
        } else {
            Ok(None)
        }
    }
}
