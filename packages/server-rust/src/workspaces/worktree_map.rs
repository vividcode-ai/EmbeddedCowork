use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::api_types::WorktreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeMapFile {
    pub version: u8,
    pub default_worktree_slug: String,
    pub parent_session_worktree_slug: HashMap<String, String>,
}

pub fn load_worktree_map(workspace_path: &str) -> Option<WorktreeMap> {
    let path = Path::new(workspace_path)
        .join(".embeddedcowork")
        .join("worktree-map.json");

    if !path.exists() {
        return None;
    }

    fs::read_to_string(&path)
        .ok()
        .and_then(|content| serde_json::from_str::<WorktreeMapFile>(&content).ok())
        .map(|f| WorktreeMap {
            version: f.version,
            default_worktree_slug: f.default_worktree_slug,
            parent_session_worktree_slug: f.parent_session_worktree_slug,
        })
}

pub fn save_worktree_map(workspace_path: &str, map: &WorktreeMap) -> Result<(), String> {
    let path = Path::new(workspace_path).join(".embeddedcowork");
    fs::create_dir_all(&path).map_err(|e| e.to_string())?;

    let file_path = path.join("worktree-map.json");
    let file = WorktreeMapFile {
        version: map.version,
        default_worktree_slug: map.default_worktree_slug.clone(),
        parent_session_worktree_slug: map.parent_session_worktree_slug.clone(),
    };

    let content = serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?;
    fs::write(&file_path, &content).map_err(|e| e.to_string())?;
    Ok(())
}
