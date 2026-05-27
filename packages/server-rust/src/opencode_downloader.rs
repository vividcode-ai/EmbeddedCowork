use std::path::Path;

use crate::logger::Logger;

pub struct OpenCodeDownloader {
    client: reqwest::Client,
    logger: Logger,
}

impl OpenCodeDownloader {
    pub fn new(logger: Logger) -> Self {
        let client = reqwest::Client::builder()
            .user_agent("EmbeddedCowork-Server")
            .build()
            .unwrap_or_default();
        Self { client, logger }
    }

    pub async fn download_version(&self, version: &str, dest: &Path) -> Result<(), String> {
        let url = crate::opencode_paths::get_opencode_download_url(
            version,
            std::env::consts::OS,
            std::env::consts::ARCH,
        );

        tracing::info!(component = %self.logger.component, url = %url, "Downloading OpenCode binary");

        let response = self.client.get(&url)
            .send()
            .await
            .map_err(|e| format!("Download request failed: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("Download failed with status: {}", response.status()));
        }

        let bytes = response.bytes()
            .await
            .map_err(|e| format!("Failed to read response body: {}", e))?;

        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent).await
                .map_err(|e| format!("Failed to create parent directory: {}", e))?;
        }

        tokio::fs::write(dest, &bytes).await
            .map_err(|e| format!("Failed to write file: {}", e))?;

        tracing::info!(component = %self.logger.component, path = %dest.display(), "Downloaded OpenCode binary");

        Ok(())
    }

    pub async fn get_latest_version(&self) -> Result<String, String> {
        let url = "https://api.github.com/repos/vividcode-ai/opencode/releases/latest";
        let response = self.client.get(url)
            .header("Accept", "application/vnd.github.v3+json")
            .send()
            .await
            .map_err(|e| format!("Failed to fetch latest release: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("GitHub API responded with: {}", response.status()));
        }

        let json: serde_json::Value = response.json().await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        let tag = json.get("tag_name")
            .and_then(|v| v.as_str())
            .ok_or("Missing tag_name in response")?;

        Ok(tag.trim_start_matches('v').to_string())
    }
}
