use std::collections::HashMap;
use std::time::Instant;

pub struct SearchCache {
    entries: HashMap<String, CachedEntry>,
    max_size: usize,
}

struct CachedEntry {
    results: Vec<String>,
    #[allow(dead_code)]
    cached_at: Instant,
}

impl SearchCache {
    pub fn new(max_size: usize) -> Self {
        Self {
            entries: HashMap::new(),
            max_size,
        }
    }

    pub fn get(&self, key: &str) -> Option<Vec<String>> {
        self.entries.get(key).map(|e| e.results.clone())
    }

    pub fn set(&mut self, key: String, results: Vec<String>) {
        if self.entries.len() >= self.max_size {
            self.entries.clear();
        }
        self.entries.insert(key, CachedEntry {
            results,
            cached_at: Instant::now(),
        });
    }

    pub fn invalidate(&mut self, key: &str) {
        self.entries.remove(key);
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}
