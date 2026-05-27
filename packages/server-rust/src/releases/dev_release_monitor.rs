use crate::logger::Logger;
use crate::log_info;
use crate::log_warn;

pub struct DevReleaseMonitor {
    logger: Logger,
    client: reqwest::Client,
}

impl DevReleaseMonitor {
    pub fn new(logger: Logger) -> Self {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::USER_AGENT,
            reqwest::header::HeaderValue::from_static("EmbeddedCowork-Server"),
        );
        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { logger, client }
    }

    pub async fn check_for_dev_updates(&self) -> Result<Option<String>, String> {
        let url = "https://api.github.com/repos/vividcode-ai/opencode/releases?per_page=5";

        let response = match self.client.get(url).send().await {
            Ok(resp) => resp,
            Err(e) => {
                log_warn!(self.logger, format!("Failed to fetch dev releases: {}", e));
                return Ok(None);
            }
        };

        if !response.status().is_success() {
            let status_code = response.status().as_u16();
            log_warn!(self.logger, format!("GitHub API returned non-success status for dev releases: {}", status_code));
            return Ok(None);
        }

        #[derive(serde::Deserialize)]
        struct GitHubRelease {
            tag_name: String,
            prerelease: bool,
        }

        let releases: Vec<GitHubRelease> = match response.json().await {
            Ok(r) => r,
            Err(e) => {
                log_warn!(self.logger, format!("Failed to parse GitHub releases response: {}", e));
                return Ok(None);
            }
        };

        for release in &releases {
            let tag_lower = release.tag_name.to_lowercase();
            if release.prerelease
                || tag_lower.contains("dev")
                || tag_lower.contains("beta")
                || tag_lower.contains("alpha")
            {
                let version = release
                    .tag_name
                    .strip_prefix('v')
                    .unwrap_or(&release.tag_name)
                    .to_string();
                log_info!(self.logger, format!("Found dev release: version={}, tag={}", version, release.tag_name));
                return Ok(Some(version));
            }
        }

        log_info!(self.logger, "No dev release found");
        Ok(None)
    }
}
