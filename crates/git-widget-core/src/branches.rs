//! Branch management (create, delete, checkout remote) and commit actions (undo, cherry-pick).

use crate::{
    error::{CoreError, Result},
    git_cli::{run_git, run_git_raw, LOCAL_TIMEOUT_SECS},
    ops::OpResult,
    status::get_status,
};
use std::path::Path;

fn validate_ref(val: &str, field: &str) -> Result<()> {
    let trimmed = val.trim();
    if trimmed.is_empty() || trimmed.contains(' ') || trimmed.starts_with('-') {
        return Err(CoreError::InvalidInput(format!("invalid {field}: '{val}'")));
    }
    Ok(())
}

/// Creates a new local branch from current HEAD and checks it out.
pub async fn create_branch(repo_path: &Path, name: &str) -> Result<OpResult> {
    validate_ref(name, "branch name")?;
    let trimmed = name.trim();
    run_git(repo_path, &["checkout", "-b", trimmed], LOCAL_TIMEOUT_SECS).await?;
    Ok(OpResult {
        success: true,
        message: format!("Created and switched to {trimmed}"),
        has_conflicts: false,
    })
}

/// Deletes a local branch (`-d`, or `-D` when `force` is true). Refuses current/protected.
pub async fn delete_branch(repo_path: &Path, name: &str, force: bool) -> Result<OpResult> {
    validate_ref(name, "branch name")?;
    let trimmed = name.trim();
    let st = get_status(repo_path).await?;
    if trimmed == st.branch {
        return Err(CoreError::InvalidInput(
            "Cannot delete the currently checked out branch".to_string(),
        ));
    }
    let flag = if force { "-D" } else { "-d" };
    let out = run_git(repo_path, &["branch", flag, trimmed], LOCAL_TIMEOUT_SECS).await?;
    Ok(OpResult {
        success: true,
        message: out.stdout.trim().to_string(),
        has_conflicts: false,
    })
}

/// Checks out a remote tracking branch (e.g. `origin/feat-x`) as a local tracking branch.
pub async fn checkout_remote_branch(repo_path: &Path, remote_branch: &str) -> Result<OpResult> {
    validate_ref(remote_branch, "remote branch")?;
    let trimmed = remote_branch.trim();
    let out = run_git(repo_path, &["checkout", "--track", trimmed], LOCAL_TIMEOUT_SECS).await?;
    Ok(OpResult {
        success: true,
        message: format!("Tracking {trimmed}\n{}", out.stderr.trim()),
        has_conflicts: false,
    })
}

/// Soft-resets HEAD~1 so the last commit's changes return to Staged Changes (VS Code Undo Last Commit).
pub async fn undo_last_commit(repo_path: &Path) -> Result<OpResult> {
    run_git(repo_path, &["reset", "--soft", "HEAD~1"], LOCAL_TIMEOUT_SECS).await?;
    Ok(OpResult {
        success: true,
        message: "Last commit undone (changes kept staged)".to_string(),
        has_conflicts: false,
    })
}

/// Cherry-picks a commit hash onto the current branch.
pub async fn cherry_pick(repo_path: &Path, commit_hash: &str) -> Result<OpResult> {
    validate_ref(commit_hash, "commit hash")?;
    let trimmed = commit_hash.trim();
    let out = run_git_raw(repo_path, &["cherry-pick", trimmed], LOCAL_TIMEOUT_SECS).await?;
    if out.success {
        return Ok(OpResult {
            success: true,
            message: format!("Cherry-picked {trimmed}"),
            has_conflicts: false,
        });
    }
    let st = get_status(repo_path).await?;
    if !st.conflicts.is_empty() {
        return Ok(OpResult {
            success: false,
            message: format!("Cherry-pick {trimmed} resulted in conflicts"),
            has_conflicts: true,
        });
    }
    Err(CoreError::GitFailed {
        code: out.code,
        stderr: out.stderr.trim().to_string(),
    })
}
