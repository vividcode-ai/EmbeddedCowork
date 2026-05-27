use std::path::Path;

use crate::api_types::{FileSystemEntry, FileSystemEntryType};
use crate::logger::Logger;

pub struct FileBrowser {
    root_dirs: Vec<String>,
    unrestricted: bool,
    #[allow(dead_code)]
    logger: Logger,
}

impl FileBrowser {
    pub fn new(root_dirs: Vec<String>, logger: Logger) -> Self {
        Self { root_dirs, unrestricted: false, logger }
    }

    pub fn with_unrestricted(mut self, unrestricted: bool) -> Self {
        self.unrestricted = unrestricted;
        self
    }

    pub async fn browse(&self, path: &str, _depth: u32, _show_hidden: bool) -> Result<Vec<FileSystemEntry>, String> {
        let abs_path = if Path::new(path).is_absolute() {
            path.to_string()
        } else {
            return Err("Relative path not supported".to_string());
        };

        if !self.unrestricted && !abs_path.starts_with("\\\\?\\") && !self.root_dirs.iter().any(|r| abs_path.starts_with(r)) {
            return Err("Access denied: path outside allowed directories".to_string());
        }

        let entries = std::fs::read_dir(&abs_path).map_err(|e| e.to_string())?;
        let mut result = Vec::new();

        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            let file_type = entry.file_type().map_err(|e| e.to_string())?;
            let entry_type = if file_type.is_dir() {
                FileSystemEntryType::Directory
            } else {
                FileSystemEntryType::File
            };
            result.push(FileSystemEntry {
                name: entry.file_name().to_string_lossy().to_string(),
                path: entry.path().to_string_lossy().to_string(),
                absolute_path: None,
                entry_type,
                size: None,
                modified_at: None,
            });
        }

        Ok(result)
    }

    pub async fn read_file(&self, path: &str) -> Result<String, String> {
        tokio::fs::read_to_string(path).await.map_err(|e| e.to_string())
    }

    pub async fn path_exists(&self, path: &str) -> bool {
        Path::new(path).exists()
    }

    pub fn default_root(&self) -> Option<&str> {
        self.root_dirs.first().map(|s| s.as_str())
    }
}
