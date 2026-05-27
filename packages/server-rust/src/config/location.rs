use std::path::{Path, PathBuf};
use std::env;

#[derive(Debug, Clone)]
pub struct ConfigLocation {
    pub base_dir: PathBuf,
    pub config_yaml_path: PathBuf,
    pub state_yaml_path: PathBuf,
    pub legacy_json_path: PathBuf,
    pub instances_dir: PathBuf,
}

fn is_yaml_path(path: &str) -> bool {
    let lower = path.to_lowercase();
    lower.ends_with(".yaml") || lower.ends_with(".yml")
}

fn is_json_path(path: &str) -> bool {
    path.to_lowercase().ends_with(".json")
}

fn resolve_path(input: &str) -> PathBuf {
    if let Some(rest) = input.strip_prefix("~/") {
        let home = env::var("HOME")
            .or_else(|_| env::var("USERPROFILE"))
            .unwrap_or_else(|_| ".".to_string());
        return Path::new(&home).join(rest);
    }
    std::path::absolute(input).unwrap_or_else(|_| PathBuf::from(input))
}

pub fn resolve_config_location(raw: &str) -> ConfigLocation {
    let trimmed = raw.trim();
    let fallback = "~/.config/EmbeddedCowork/config.json";
    let input = if trimmed.is_empty() { fallback } else { trimmed };

    let resolved_input = resolve_path(input);
    let input_str = resolved_input.to_string_lossy().to_string();

    if is_yaml_path(&input_str) {
        let base_dir = resolved_input.parent().unwrap_or(Path::new("."));
        ConfigLocation {
            base_dir: base_dir.to_path_buf(),
            config_yaml_path: resolved_input.clone(),
            state_yaml_path: base_dir.join("state.yaml"),
            legacy_json_path: base_dir.join("config.json"),
            instances_dir: base_dir.join("instances"),
        }
    } else if is_json_path(&input_str) {
        let base_dir = resolved_input.parent().unwrap_or(Path::new("."));
        ConfigLocation {
            base_dir: base_dir.to_path_buf(),
            config_yaml_path: base_dir.join("config.yaml"),
            state_yaml_path: base_dir.join("state.yaml"),
            legacy_json_path: resolved_input.clone(),
            instances_dir: base_dir.join("instances"),
        }
    } else {
        let base_dir = &resolved_input;
        ConfigLocation {
            base_dir: base_dir.to_path_buf(),
            config_yaml_path: base_dir.join("config.yaml"),
            state_yaml_path: base_dir.join("state.yaml"),
            legacy_json_path: base_dir.join("config.json"),
            instances_dir: base_dir.join("instances"),
        }
    }
}
