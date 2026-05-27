use crate::api_types::{GitChangeKind, WorktreeGitStatusEntry};
use tokio::process::Command;

pub struct GitStatusParams {
    pub workspace_folder: String,
}

pub struct GitDiffParams {
    pub workspace_folder: String,
    pub path: String,
    pub original_path: Option<String>,
    pub scope: GitDiffScope,
}

pub enum GitDiffScope {
    Staged,
    Unstaged,
}

pub struct GitDiffResult {
    pub path: String,
    pub original_path: Option<String>,
    pub scope: String,
    pub before: String,
    pub after: String,
    pub is_binary: Option<bool>,
}

/// Run `git status --porcelain -u` in the workspace folder and parse each
/// line into a `WorktreeGitStatusEntry`.
///
/// Porcelain format (V1):
/// ```text
/// XY PATH
/// XY ORIG_PATH -> NEW_PATH   (rename / copy)
/// ```
/// X = index (staged) status, Y = worktree (unstaged) status.
///
/// For every entry the functions also runs
/// - `git diff --numstat --cached -- <path>` to obtain staged line counts
/// - `git diff --numstat -- <path>`            to obtain unstaged line counts
pub async fn get_worktree_git_status(
    params: &GitStatusParams,
) -> Result<Vec<WorktreeGitStatusEntry>, String> {
    let output = Command::new("git")
        .args(["status", "--porcelain", "-u"])
        .current_dir(&params.workspace_folder)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .await
        .map_err(|e| format!("Failed to execute git status: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git status failed: {stderr}"));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut entries = Vec::new();

    for line in stdout.lines() {
        if line.len() < 4 {
            continue;
        }

        // First two bytes are the XY status codes (always ASCII).
        // Byte at index 2 is a space separator.
        let staged_char = line.as_bytes()[0] as char;
        let unstaged_char = line.as_bytes()[1] as char;
        let path_part = &line[3..];

        // Handle rename / copy entries (XY = R... or C...)
        let (path, original_path) = if staged_char == 'R' || unstaged_char == 'R'
            || staged_char == 'C' || unstaged_char == 'C'
        {
            if let Some(idx) = path_part.find(" -> ") {
                (
                    path_part[idx + 4..].trim().to_string(),
                    Some(path_part[..idx].trim().to_string()),
                )
            } else {
                (path_part.trim().to_string(), None)
            }
        } else {
            (path_part.trim().to_string(), None)
        };

        let staged_status = parse_status_char(staged_char);
        let unstaged_status = parse_status_char(unstaged_char);

        // Obtain diff line-counts per file by running git diff --numstat.
        // Untracked files are skipped – git diff would return empty output anyway.
        let (staged_additions, staged_deletions) = if staged_status
            == Some(GitChangeKind::Untracked)
        {
            (0, 0)
        } else {
            get_diff_numstat(&params.workspace_folder, &path, true).await
        };

        let (unstaged_additions, unstaged_deletions) = if unstaged_status
            == Some(GitChangeKind::Untracked)
        {
            (0, 0)
        } else {
            get_diff_numstat(&params.workspace_folder, &path, false).await
        };

        entries.push(WorktreeGitStatusEntry {
            path,
            original_path,
            staged_status,
            staged_additions,
            staged_deletions,
            unstaged_status,
            unstaged_additions,
            unstaged_deletions,
        });
    }

    Ok(entries)
}

/// Map a single git status character to the corresponding `GitChangeKind`.
fn parse_status_char(c: char) -> Option<GitChangeKind> {
    match c {
        'M' => Some(GitChangeKind::Modified),
        'A' => Some(GitChangeKind::Added),
        'D' => Some(GitChangeKind::Deleted),
        'R' => Some(GitChangeKind::Renamed),
        'C' => Some(GitChangeKind::Copied),
        'U' => Some(GitChangeKind::Unmerged),
        '?' => Some(GitChangeKind::Untracked),
        ' ' => None,
        '!' => None, // ignored (requires --ignored flag)
        _ => None,
    }
}

/// Run `git diff --numstat` (optionally `--cached`) for a single path and
/// return the number of added / deleted lines.
async fn get_diff_numstat(
    workspace_folder: &str,
    path: &str,
    cached: bool,
) -> (u64, u64) {
    let mut args: Vec<&str> = vec!["diff", "--numstat"];
    if cached {
        args.push("--cached");
    }
    args.push("--");
    args.push(path);

    let output = Command::new("git")
        .args(&args)
        .current_dir(workspace_folder)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .await;

    match output {
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            parse_numstat_line(&stdout)
        }
        _ => (0, 0),
    }
}

/// Parse a single `git diff --numstat` output line into (additions, deletions).
/// The format is: `<additions>\t<deletions>\t<path>`
fn parse_numstat_line(output: &str) -> (u64, u64) {
    for line in output.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 2 {
            let additions = parts[0].parse::<u64>().unwrap_or(0);
            let deletions = parts[1].parse::<u64>().unwrap_or(0);
            return (additions, deletions);
        }
    }
    (0, 0)
}

/// Run `git diff` (unstaged) or `git diff --cached` (staged) for a specific
/// path and return the parsed `GitDiffResult` containing before/after content
/// and a binary flag.
pub async fn get_worktree_git_diff(params: &GitDiffParams) -> Result<GitDiffResult, String> {
    let scope_str = match params.scope {
        GitDiffScope::Staged => "staged",
        GitDiffScope::Unstaged => "unstaged",
    };

    let mut args: Vec<&str> = vec!["diff"];
    match params.scope {
        GitDiffScope::Staged => {
            args.push("--cached");
        }
        GitDiffScope::Unstaged => {}
    }
    args.push("--");
    args.push(&params.path);

    let output = Command::new("git")
        .args(&args)
        .current_dir(&params.workspace_folder)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .await
        .map_err(|e| format!("Failed to execute git diff: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git diff failed: {stderr}"));
    }

    let diff_output = String::from_utf8_lossy(&output.stdout).to_string();

    // Detect binary files (git outputs "Binary files ... differ")
    let is_binary = diff_output.contains("Binary files") || diff_output.is_empty();

    let (before, after) = parse_unified_diff(&diff_output);

    Ok(GitDiffResult {
        path: params.path.clone(),
        original_path: params.original_path.clone(),
        scope: scope_str.to_string(),
        before,
        after,
        is_binary: Some(is_binary),
    })
}

/// Parse a unified diff string into removed (`before`) and added (`after`)
/// line content. Lines starting with `-` (but not `---`) are treated as
/// removed content; lines starting with `+` (but not `+++`) are treated as
/// added content.
fn parse_unified_diff(diff_output: &str) -> (String, String) {
    let mut before = String::new();
    let mut after = String::new();

    for line in diff_output.lines() {
        if line.starts_with('-') && !line.starts_with("---") {
            before.push_str(&line[1..]);
            before.push('\n');
        } else if line.starts_with('+') && !line.starts_with("+++") {
            after.push_str(&line[1..]);
            after.push('\n');
        }
    }

    let before = before.trim_end().to_string();
    let after = after.trim_end().to_string();

    (before, after)
}
