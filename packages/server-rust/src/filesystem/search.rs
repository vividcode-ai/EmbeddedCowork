use std::path::Path;
use std::sync::Arc;
use tokio::fs;
use tokio::sync::Mutex;

use crate::filesystem::search_cache::SearchCache;
use crate::logger::Logger;

pub struct FileSearcher {
    cache: Arc<Mutex<SearchCache>>,
    #[allow(dead_code)]
    logger: Logger,
}

impl FileSearcher {
    pub fn new(cache: Arc<Mutex<SearchCache>>, logger: Logger) -> Self {
        Self { cache, logger }
    }

    pub async fn search(&self, root: &str, pattern: &str) -> Vec<String> {
        if pattern.is_empty() {
            return Vec::new();
        }

        let cache_key = format!("{}:{}", root, pattern);

        // Check cache first
        {
            let cache = self.cache.lock().await;
            if let Some(cached) = cache.get(&cache_key) {
                return cached;
            }
        }

        // Walk directory tree
        let mut results = Vec::new();
        let pattern_lower = pattern.to_lowercase();
        let root_path = Path::new(root);

        walk_directory(root_path, &pattern_lower, &mut results).await;

        // Store in cache
        {
            let mut cache = self.cache.lock().await;
            cache.set(cache_key, results.clone());
        }

        results
    }
}

async fn walk_directory(root: &Path, pattern_lower: &str, results: &mut Vec<String>) {
    let mut stack = vec![(root.to_path_buf(), 0u32)];

    while let Some((dir, depth)) = stack.pop() {
        if depth > 10 || results.len() >= 1000 {
            continue;
        }

        let mut entries = match fs::read_dir(&dir).await {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        loop {
            let entry = match entries.next_entry().await {
                Ok(Some(entry)) => entry,
                Ok(None) => break,
                Err(_) => continue,
            };

            if results.len() >= 1000 {
                break;
            }

            let file_name = entry.file_name();
            let file_name_str = file_name.to_string_lossy();

            // Skip hidden entries and .git
            if file_name_str.starts_with('.') {
                continue;
            }

            let path = entry.path();

            // Check if entry is a directory using async metadata
            let is_dir = match entry.file_type().await {
                Ok(ft) => ft.is_dir(),
                Err(_) => false,
            };

            // Check if this entry's name matches the pattern (case-insensitive)
            if file_name_str.to_lowercase().contains(pattern_lower) {
                results.push(path.to_string_lossy().to_string());
            }

            // Recurse into directories
            if is_dir {
                stack.push((path, depth + 1));
            }
        }
    }
}
