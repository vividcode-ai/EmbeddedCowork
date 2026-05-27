use std::fmt;
use tokio::process::Command;

#[derive(Debug)]
pub struct GitMutationError {
    pub status_code: u16,
    pub message: String,
}

impl fmt::Display for GitMutationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Git mutation error {}: {}", self.status_code, self.message)
    }
}

impl std::error::Error for GitMutationError {}

pub struct GitPathsParams {
    pub workspace_folder: String,
    pub paths: Vec<String>,
}

pub struct GitCommitParams {
    pub workspace_folder: String,
    pub message: String,
}

pub struct CommitResult {
    pub commit_sha: Option<String>,
}

/// Stage the specified paths by running `git add <paths>` in the workspace
/// folder. Returns `Ok(())` on success or a `GitMutationError` on failure.
pub async fn stage_worktree_paths(params: &GitPathsParams) -> Result<(), GitMutationError> {
    if params.paths.is_empty() {
        return Ok(());
    }

    let mut cmd = Command::new("git");
    cmd.arg("add");
    for path in &params.paths {
        cmd.arg(path);
    }
    cmd.current_dir(&params.workspace_folder)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let output = cmd.output().await.map_err(|e| GitMutationError {
        status_code: 500,
        message: format!("Failed to execute git add: {e}"),
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(GitMutationError {
            status_code: 500,
            message: format!("git add failed: {stderr}"),
        });
    }

    Ok(())
}

/// Unstage the specified paths by running `git reset HEAD -- <paths>` in the
/// workspace folder. Returns `Ok(())` on success or a `GitMutationError` on
/// failure.
pub async fn unstage_worktree_paths(params: &GitPathsParams) -> Result<(), GitMutationError> {
    if params.paths.is_empty() {
        return Ok(());
    }

    let mut cmd = Command::new("git");
    cmd.args(["reset", "HEAD", "--"]);
    for path in &params.paths {
        cmd.arg(path);
    }
    cmd.current_dir(&params.workspace_folder)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let output = cmd.output().await.map_err(|e| GitMutationError {
        status_code: 500,
        message: format!("Failed to execute git reset: {e}"),
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(GitMutationError {
            status_code: 500,
            message: format!("git reset failed: {stderr}"),
        });
    }

    Ok(())
}

/// Commit staged changes by running `git commit -m <message>` in the workspace
/// folder. Parses the commit SHA from the standard git output format:
/// `[<branch> <sha>] <message>`
pub async fn commit_worktree_changes(
    params: &GitCommitParams,
) -> Result<CommitResult, GitMutationError> {
    let output = Command::new("git")
        .args(["commit", "-m", &params.message])
        .current_dir(&params.workspace_folder)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .await
        .map_err(|e| GitMutationError {
            status_code: 500,
            message: format!("Failed to execute git commit: {e}"),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(GitMutationError {
            status_code: 500,
            message: format!("git commit failed: {stderr}"),
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let commit_sha = parse_commit_sha(&stdout);

    Ok(CommitResult { commit_sha })
}

/// Extract the commit SHA from git commit stdout.
///
/// Typical output format:
/// ```text
/// [main abc123def456] My commit message
/// ```
fn parse_commit_sha(output: &str) -> Option<String> {
    for line in output.lines() {
        // Look for the bracket-enclosed section: [branch SHA]
        let line = line.trim();
        if let Some(start) = line.find('[') {
            if let Some(end) = line.find(']') {
                let bracket_content = &line[start + 1..end];
                // Split by whitespace; the second token is the commit SHA
                let parts: Vec<&str> = bracket_content.split_whitespace().collect();
                if parts.len() >= 2 {
                    let sha = parts[1].to_string();
                    if !sha.is_empty() {
                        return Some(sha);
                    }
                }
            }
        }
    }
    None
}
