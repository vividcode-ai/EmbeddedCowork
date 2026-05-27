use std::path::Path;

use crate::logger::Logger;

pub async fn resolve_worktree_directory(params: &WorktreeDirectoryParams) -> Option<String> {
    if params.worktree_slug == "root" {
        return Some(params.workspace_path.clone());
    }

    // In production, look up the slug in the worktree map
    let worktree_path = Path::new(&params.workspace_path)
        .join(".embeddedcowork")
        .join("worktrees")
        .join(&params.worktree_slug);

    if worktree_path.exists() {
        Some(worktree_path.to_string_lossy().to_string())
    } else {
        None
    }
}

pub struct WorktreeDirectoryParams {
    pub workspace_id: String,
    pub workspace_path: String,
    pub worktree_slug: String,
    pub logger: Logger,
}
