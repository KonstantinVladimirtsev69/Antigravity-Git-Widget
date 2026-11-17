//! Git workflow operations: checkout, fetch, pull, merge, push, stash, commit.

use crate::{
    error::{CoreError, Result},
    git_cli::{run_git, run_git_raw, LOCAL_TIMEOUT_SECS, NETWORK_TIMEOUT_SECS},
    status::get_status,
};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpResult {
    pub success: bool,
    pub message: String,
    pub has_conflicts: bool,
}

fn validate_non_empty(val: &str, field: &str) -> Result<()> {
    if val.trim().is_empty() {
        return Err(CoreError::InvalidInput(format!("{field} cannot be empty")));
    }
    Ok(())
}

pub async fn checkout_branch(repo_path: &Path, branch: &str) -> Result<OpResult> {
    validate_non_empty(branch, "branch")?;
    let out = run_git(repo_path, &["checkout", branch], LOCAL_TIMEOUT_SECS).await?;
    Ok(OpResult {
        success: true,
        message: format!("Switched to {branch}\n{}", out.stderr.trim()),
        has_conflicts: false,
    })
}

pub async fn fetch_all(repo_path: &Path) -> Result<OpResult> {
    let out = run_git(
        repo_path,
        &["fetch", "--all", "--prune"],
        NETWORK_TIMEOUT_SECS,
    )
    .await?;
    Ok(OpResult {
        success: true,
        message: out.stdout.trim().to_string(),
        has_conflicts: false,
    })
}

pub async fn pull_current(repo_path: &Path) -> Result<OpResult> {
    let out = run_git_raw(repo_path, &["pull"], NETWORK_TIMEOUT_SECS).await?;
    if out.success {
        return Ok(OpResult {
            success: true,
            message: out.stdout.trim().to_string(),
            has_conflicts: false,
        });
    }
    let st = get_status(repo_path).await?;
    if !st.conflicts.is_empty() || st.is_merging {
        return Ok(OpResult {
            success: false,
            message: "Pull resulted in merge conflicts".to_string(),
            has_conflicts: true,
        });
    }
    Err(CoreError::GitFailed {
        code: out.code,
        stderr: out.stderr.trim().to_string(),
    })
}

pub async fn merge_branch(repo_path: &Path, source_branch: &str) -> Result<OpResult> {
    validate_non_empty(source_branch, "source_branch")?;
    let out = run_git_raw(repo_path, &["merge", source_branch], LOCAL_TIMEOUT_SECS).await?;
    if out.success {
        return Ok(OpResult {
            success: true,
            message: out.stdout.trim().to_string(),
            has_conflicts: false,
        });
    }
    let st = get_status(repo_path).await?;
    if !st.conflicts.is_empty() || st.is_merging {
        return Ok(OpResult {
            success: false,
            message: format!("Merge conflict merging {source_branch}"),
            has_conflicts: true,
        });
    }
    Err(CoreError::GitFailed {
        code: out.code,
        stderr: out.stderr.trim().to_string(),
    })
}

pub async fn push_current(repo_path: &Path) -> Result<OpResult> {
    let out = run_git(repo_path, &["push"], NETWORK_TIMEOUT_SECS).await?;
    Ok(OpResult {
        success: true,
        message: out.stderr.trim().to_string(),
        has_conflicts: false,
    })
}

pub async fn commit_all(repo_path: &Path, message: &str) -> Result<OpResult> {
    validate_non_empty(message, "commit message")?;
    run_git(repo_path, &["add", "-A"], LOCAL_TIMEOUT_SECS).await?;
    let out = run_git(repo_path, &["commit", "-m", message], LOCAL_TIMEOUT_SECS).await?;
    Ok(OpResult {
        success: true,
        message: out.stdout.trim().to_string(),
        has_conflicts: false,
    })
}

pub async fn stash_save(repo_path: &Path, message: Option<&str>) -> Result<OpResult> {
    let out = match message.filter(|m| !m.trim().is_empty()) {
        Some(msg) => {
            run_git(
                repo_path,
                &["stash", "push", "-u", "-m", msg],
                LOCAL_TIMEOUT_SECS,
            )
            .await?
        }
        None => run_git(repo_path, &["stash", "push", "-u"], LOCAL_TIMEOUT_SECS).await?,
    };
    Ok(OpResult {
        success: true,
        message: out.stdout.trim().to_string(),
        has_conflicts: false,
    })
}

pub async fn stash_pop(repo_path: &Path) -> Result<OpResult> {
    let out = run_git_raw(repo_path, &["stash", "pop"], LOCAL_TIMEOUT_SECS).await?;
    if out.success {
        return Ok(OpResult {
            success: true,
            message: out.stdout.trim().to_string(),
            has_conflicts: false,
        });
    }
    let st = get_status(repo_path).await?;
    if !st.conflicts.is_empty() {
        return Ok(OpResult {
            success: false,
            message: "Stash pop resulted in conflicts".to_string(),
            has_conflicts: true,
        });
    }
    Err(CoreError::GitFailed {
        code: out.code,
        stderr: out.stderr.trim().to_string(),
    })
}
