use std::sync::Mutex;

use crate::logger::Logger;
use crate::log_error;
use crate::log_info;

pub struct RollbackManager {
    logger: Logger,
    versions: Mutex<Vec<String>>,
}

impl RollbackManager {
    pub fn new(logger: Logger) -> Self {
        Self {
            logger,
            versions: Mutex::new(Vec::new()),
        }
    }

    pub fn record_version(&self, version: String) {
        if let Ok(mut versions) = self.versions.lock() {
            log_info!(self.logger, format!("Recording version for rollback: {}", version));
            versions.push(version);
        } else {
            log_error!(self.logger, "Failed to record version: mutex poisoned");
        }
    }

    pub async fn perform_rollback(&self) -> Result<String, String> {
        let mut versions = self
            .versions
            .lock()
            .map_err(|e| format!("Failed to acquire rollback lock: {}", e))?;

        let version = versions
            .pop()
            .ok_or_else(|| "No previous versions available for rollback".to_string())?;

        log_info!(self.logger, format!("Performing rollback to version: {}", version));

        Ok(version)
    }
}
