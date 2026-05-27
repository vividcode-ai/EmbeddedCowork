use std::collections::HashMap;
use std::path::PathBuf;
use std::fs;
use tokio::fs as tokio_fs;

use crate::api_types::InstanceData;
use crate::logger::Logger;

pub struct InstanceStore {
    data_dir: PathBuf,
    instances: HashMap<String, InstanceData>,
    #[allow(dead_code)]
    logger: Logger,
}

impl InstanceStore {
    pub fn new(data_dir: PathBuf, logger: Logger) -> Self {
        fs::create_dir_all(&data_dir).ok();
        Self {
            data_dir,
            instances: HashMap::new(),
            logger,
        }
    }

    pub async fn read(&self, id: &str) -> Result<InstanceData, std::io::Error> {
        let file_path = self.resolve_path(id);
        match tokio_fs::read_to_string(&file_path).await {
            Ok(content) => {
                let parsed: InstanceData = serde_json::from_str(&content)?;
                Ok(InstanceData {
                    message_history: if parsed.message_history.is_empty() {
                        Vec::new()
                    } else {
                        parsed.message_history
                    },
                    agent_model_selections: if parsed.agent_model_selections.is_empty() {
                        HashMap::new()
                    } else {
                        parsed.agent_model_selections
                    },
                })
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Ok(InstanceData {
                    message_history: Vec::new(),
                    agent_model_selections: HashMap::new(),
                })
            }
            Err(e) => Err(e),
        }
    }

    pub async fn write(&self, id: &str, data: &InstanceData) -> Result<(), std::io::Error> {
        let file_path = self.resolve_path(id);
        if let Some(parent) = file_path.parent() {
            tokio_fs::create_dir_all(parent).await?;
        }
        let content = serde_json::to_string_pretty(data)?;
        tokio_fs::write(&file_path, content).await
    }

    pub async fn delete(&self, id: &str) -> Result<(), std::io::Error> {
        let file_path = self.resolve_path(id);
        match tokio_fs::remove_file(&file_path).await {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }

    fn resolve_path(&self, id: &str) -> PathBuf {
        let filename = self.sanitize_id(id);
        self.data_dir.join(format!("{}.json", filename))
    }

    fn sanitize_id(&self, id: &str) -> String {
        let sanitized: String = id
            .chars()
            .map(|c| if c == '/' || c == '\\' { '_' } else { c })
            .collect::<String>()
            .chars()
            .map(|c| if c.is_alphanumeric() || c == '_' || c == '.' || c == '-' { c } else { '_' })
            .collect::<String>();
        let collapsed = {
            let mut result = String::new();
            let mut prev_underscore = false;
            for c in sanitized.chars() {
                if c == '_' {
                    if prev_underscore { continue; }
                    prev_underscore = true;
                } else {
                    prev_underscore = false;
                }
                result.push(c);
            }
            result
        };
        let trimmed = collapsed.trim_matches('_').to_string();
        trimmed.to_lowercase()
    }

    pub fn get(&self, instance_id: &str) -> Option<&InstanceData> {
        self.instances.get(instance_id)
    }

    pub fn set(&mut self, instance_id: String, data: InstanceData) {
        self.instances.insert(instance_id, data);
    }

    pub fn remove(&mut self, instance_id: &str) {
        self.instances.remove(instance_id);
    }
}
