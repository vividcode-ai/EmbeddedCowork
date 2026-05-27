use std::process::Command;

use crate::logger::Logger;

pub struct BrowserLauncher {
    #[allow(dead_code)]
    logger: Logger,
}

impl BrowserLauncher {
    pub fn new(logger: Logger) -> Self {
        Self { logger }
    }

    pub fn launch(&self, url: &str) -> Result<(), String> {
        let os = std::env::consts::OS;

        match os {
            "windows" => {
                Command::new("cmd")
                    .args(["/c", "start", "", url])
                    .spawn()
                    .map_err(|e| e.to_string())?;
            }
            "macos" => {
                Command::new("open")
                    .arg(url)
                    .spawn()
                    .map_err(|e| e.to_string())?;
            }
            _ => {
                Command::new("xdg-open")
                    .arg(url)
                    .spawn()
                    .map_err(|e| e.to_string())?;
            }
        }

        Ok(())
    }
}
