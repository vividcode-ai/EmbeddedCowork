use std::fs;

use crate::config::location::ConfigLocation;
use crate::logger::Logger;

pub fn migrate_settings_layout(location: &ConfigLocation, _logger: &Logger) {
    // Check if legacy JSON config exists and YAML doesn't
    if location.legacy_json_path.exists() && !location.config_yaml_path.exists() {
        match fs::read_to_string(&location.legacy_json_path) {
            Ok(content) => {
                if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(parent) = location.config_yaml_path.parent() {
                        let _ = fs::create_dir_all(parent);
                    }
                    if let Ok(yaml) = serde_yaml::to_string(&json_value) {
                        let _ = fs::write(&location.config_yaml_path, &yaml);
                    }
                }
            }
            Err(_) => {}
        }
    }
}
