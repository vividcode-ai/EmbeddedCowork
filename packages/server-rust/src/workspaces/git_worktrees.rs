use crate::logger::Logger;
use std::path::Path;
use tokio::process::Command;

pub struct RepoRootResult {
    pub root: Option<String>,
    pub is_git_repo: bool,
}

pub struct WorktreeEntry {
    pub slug: String,
    pub directory: String,
    pub branch: Option<String>,
}

/// Validate a worktree slug (alphanumeric + hyphens + underscores)
pub fn is_valid_worktree_slug(slug: &str) -> bool {
    if slug.is_empty() {
        return false;
    }
    slug.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_')
}

/// Check if git is available by running `git --version`.
/// The workspace_path parameter is accepted for interface consistency
/// but is not used (git is a global tool).
pub async fn is_git_available(_workspace_path: &str) -> bool {
    let output = Command::new("git")
        .arg("--version")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .await;

    match output {
        Ok(out) => out.status.success(),
        Err(_) => false,
    }
}

/// Resolve the repo root for a given workspace path.
/// Runs `git rev-parse --show-toplevel` inside workspace_path to determine
/// whether the directory is inside a git repository and what its root is.
pub async fn resolve_repo_root(
    workspace_path: &str,
    _logger: &Logger,
) -> Result<RepoRootResult, String> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(workspace_path)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .await
        .map_err(|e| format!("Failed to execute git rev-parse: {e}"))?;

    if output.status.success() {
        let root = String::from_utf8_lossy(&output.stdout)
            .trim()
            .to_string();

        if root.is_empty() {
            return Ok(RepoRootResult {
                root: Some(workspace_path.to_string()),
                is_git_repo: false,
            });
        }

        Ok(RepoRootResult {
            root: Some(root),
            is_git_repo: true,
        })
    } else {
        // git rev-parse failed – not inside a git repository (or git is unavailable)
        Ok(RepoRootResult {
            root: Some(workspace_path.to_string()),
            is_git_repo: false,
        })
    }
}

/// List all worktrees for a given repository root.
/// Runs `git worktree list --porcelain` and parses the output into
/// `WorktreeEntry` structs. The main worktree receives slug "root";
/// additional worktrees use their directory basename as the slug.
pub async fn list_worktrees(repo_root: &str) -> Vec<WorktreeEntry> {
    let output = Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(repo_root)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .await;

    let output = match output {
        Ok(out) if out.status.success() => out,
        _ => return Vec::new(),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_worktree_porcelain(&stdout)
}

/// Parse `git worktree list --porcelain` output.
///
/// Each worktree entry is defined by:
/// ```text
/// worktree /path/to/directory
/// branch refs/heads/branch-name
/// HEAD <sha>        (unused, skipped)
/// ```
/// Entries are separated by a blank line.
fn parse_worktree_porcelain(output: &str) -> Vec<WorktreeEntry> {
    let mut entries: Vec<WorktreeEntry> = Vec::new();
    let mut current_dir: Option<String> = None;
    let mut current_branch: Option<String> = None;

    for line in output.lines() {
        if line.starts_with("worktree ") {
            // Flush the previous entry before starting a new one
            if let Some(dir) = current_dir.take() {
                let slug = derive_slug(&dir, &entries);
                entries.push(WorktreeEntry {
                    slug,
                    directory: dir,
                    branch: current_branch.take(),
                });
            }
            current_dir = Some(line["worktree ".len()..].to_string());
        } else if line.starts_with("branch ") {
            current_branch = Some(line["branch ".len()..].to_string());
        } else if line.is_empty() {
            // Blank line = end of current entry
            if let Some(dir) = current_dir.take() {
                let slug = derive_slug(&dir, &entries);
                entries.push(WorktreeEntry {
                    slug,
                    directory: dir,
                    branch: current_branch.take(),
                });
            }
        }
        // HEAD and other lines are ignored
    }

    // Flush the last entry if the output did not end with a blank line
    if let Some(dir) = current_dir.take() {
        let slug = derive_slug(&dir, &entries);
        entries.push(WorktreeEntry {
            slug,
            directory: dir,
            branch: current_branch.take(),
        });
    }

    entries
}

/// Derive a human-readable slug from a worktree directory path.
///
/// The very first entry encountered is always the **main** worktree and
/// receives the slug `"root"`. All subsequent worktrees use the last
/// component of their directory path as the slug.
fn derive_slug(directory: &str, existing: &[WorktreeEntry]) -> String {
    if existing.is_empty() {
        return "root".to_string();
    }

    let path = Path::new(directory);
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}
