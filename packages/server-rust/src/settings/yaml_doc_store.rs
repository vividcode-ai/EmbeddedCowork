use std::fs;
use std::path::PathBuf;

use crate::logger::Logger;

pub type SettingsDoc = serde_json::Value;

pub struct YamlDocStore {
    path: PathBuf,
    cached: Option<SettingsDoc>,
    #[allow(dead_code)]
    logger: Logger,
}

impl YamlDocStore {
    pub fn new(path: PathBuf, logger: Logger) -> Self {
        Self {
            path,
            cached: None,
            logger,
        }
    }

    pub fn peek(&self) -> SettingsDoc {
        self.cached.clone().unwrap_or_else(|| {
            let doc = self.load();
            doc
        })
    }

    pub fn get(&mut self) -> SettingsDoc {
        if let Some(ref cached) = self.cached {
            return cached.clone();
        }

        let doc = self.load();
        self.cached = Some(doc.clone());
        doc
    }

    pub fn replace(&mut self, doc: SettingsDoc) -> SettingsDoc {
        self.persist(&doc);
        self.cached = Some(doc.clone());
        doc
    }

    pub fn merge_patch(&mut self, patch: &SettingsDoc) -> SettingsDoc {
        let current = self.get();
        let merged = deep_merge(current, patch.clone());
        self.replace(merged)
    }

    pub fn get_owner(&mut self, owner: &str) -> SettingsDoc {
        let doc = self.get();
        doc.get(owner).cloned().unwrap_or(serde_json::Value::Object(serde_json::Map::new()))
    }

    pub fn merge_patch_owner(&mut self, owner: &str, patch: &SettingsDoc) -> SettingsDoc {
        let current = self.get();
        let mut doc_map = match current {
            serde_json::Value::Object(map) => map,
            _ => serde_json::Map::new(),
        };

        let existing = doc_map.get(owner).cloned().unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
        let merged = deep_merge(existing, patch.clone());

        doc_map.insert(owner.to_string(), merged.clone());
        let new_doc = serde_json::Value::Object(doc_map);
        self.replace(new_doc);
        merged
    }

    pub fn replace_owner(&mut self, owner: &str, doc: &SettingsDoc) -> SettingsDoc {
        let current = self.get();
        let mut doc_map = match current {
            serde_json::Value::Object(map) => map,
            _ => serde_json::Map::new(),
        };

        doc_map.insert(owner.to_string(), doc.clone());
        let new_doc = serde_json::Value::Object(doc_map);
        self.replace(new_doc);
        doc.clone()
    }

    fn load(&self) -> SettingsDoc {
        if !self.path.exists() {
            return serde_json::Value::Object(serde_json::Map::new());
        }

        match fs::read_to_string(&self.path) {
            Ok(content) => {
                if self.path.to_string_lossy().to_lowercase().ends_with(".yaml")
                    || self.path.to_string_lossy().to_lowercase().ends_with(".yml")
                {
                    serde_yaml::from_str(&content).unwrap_or_else(|_| {
                        serde_json::Value::Object(serde_json::Map::new())
                    })
                } else {
                    serde_json::from_str(&content).unwrap_or_else(|_| {
                        serde_json::Value::Object(serde_json::Map::new())
                    })
                }
            }
            Err(_) => serde_json::Value::Object(serde_json::Map::new()),
        }
    }

    fn persist(&self, doc: &SettingsDoc) {
        if let Some(parent) = self.path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let content = if self.path.to_string_lossy().to_lowercase().ends_with(".yaml")
            || self.path.to_string_lossy().to_lowercase().ends_with(".yml")
        {
            serde_yaml::to_string(doc).unwrap_or_default()
        } else {
            serde_json::to_string_pretty(doc).unwrap_or_default()
        };

        let _ = fs::write(&self.path, &content);
    }
}

fn deep_merge(a: SettingsDoc, b: SettingsDoc) -> SettingsDoc {
    match (a, b) {
        (serde_json::Value::Object(a_map), serde_json::Value::Object(b_map)) => {
            let mut merged = a_map;
            for (key, b_val) in b_map {
                if let Some(a_val) = merged.remove(&key) {
                    merged.insert(key, deep_merge(a_val, b_val));
                } else {
                    merged.insert(key, b_val);
                }
            }
            serde_json::Value::Object(merged)
        }
        (_a, b) => b,
    }
}
