//! Git workflow operations: checkout, fetch, pull, merge, push, stash, commit.

use crate::{
    error::{CoreError, Result},
    git_cli::{run_git, run_git_raw, LOCAL_TIMEOUT_SECS, NETWORK_TIMEOUT_SECS},
    status::get_status,
    validation::{validate_git_arg, validate_ref_name},
};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpResult {
    pub success: bool,
    pub message: String,
    pub has_conflicts: bool,
}

pub async fn checkout_branch(repo_path: &Path, branch: &str) -> Result<OpResult> {
    let b = validate_ref_name(branch, "branch")?;
    let out = run_git(repo_path, &["checkout", b], LOCAL_TIMEOUT_SECS).await?;
    Ok(OpResult {
        success: true,
        message: format!("Switched to {b}\n{}", out.stderr.trim()),
        has_conflicts: false,
    })
}

pub async fn fetch_remote(repo_path: &Path, remote: Option<&str>) -> Result<OpResult> {
    let args: Vec<&str> = match remote.map(str::trim).filter(|r| !r.is_empty()) {
        Some(r) => vec!["fetch", validate_ref_name(r, "remote")?, "--prune"],
        None => vec!["fetch", "--all", "--prune"],
    };
    let out = run_git(repo_path, &args, NETWORK_TIMEOUT_SECS).await?;
    Ok(OpResult {
        success: true,
        message: out.stdout.trim().to_string(),
        has_conflicts: false,
    })
}

pub async fn fetch_all(repo_path: &Path) -> Result<OpResult> {
    fetch_remote(repo_path, None).await
}

pub async fn pull_remote(repo_path: &Path, remote: Option<&str>) -> Result<OpResult> {
    let st_before = get_status(repo_path).await?;
    if st_before.remotes.is_empty() {
        return Err(CoreError::InvalidInput(
            "No git remote configured. Add a remote first.".to_string(),
        ));
    }
    let req_remote = match remote.map(str::trim).filter(|r| !r.is_empty()) {
        Some(r) => Some(validate_ref_name(r, "remote")?),
        None => None,
    };

    let out = match (req_remote, st_before.upstream.as_deref()) {
        (Some(r), Some(up)) if up.starts_with(&format!("{r}/")) => {
            run_git_raw(repo_path, &["pull"], NETWORK_TIMEOUT_SECS).await?
        }
        (Some(r), _) => {
            run_git_raw(repo_path, &["pull", r, &st_before.branch], NETWORK_TIMEOUT_SECS).await?
        }
        (None, None) => {
            return Err(CoreError::InvalidInput(
                "Branch has no upstream. Publish branch first.".to_string(),
            ));
        }
        (None, Some(_)) => run_git_raw(repo_path, &["pull"], NETWORK_TIMEOUT_SECS).await?,
    };

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
    Err(CoreError::GitFailed { code: out.code, stderr: out.stderr.trim().to_string() })
}

pub async fn pull_current(repo_path: &Path) -> Result<OpResult> {
    pull_remote(repo_path, None).await
}

pub async fn merge_branch(repo_path: &Path, source_branch: &str) -> Result<OpResult> {
    let src = validate_ref_name(source_branch, "source_branch")?;
    let out = run_git_raw(repo_path, &["merge", src], LOCAL_TIMEOUT_SECS).await?;
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
            message: format!("Merge conflict merging {src}"),
            has_conflicts: true,
        });
    }
    Err(CoreError::GitFailed { code: out.code, stderr: out.stderr.trim().to_string() })
}

pub async fn push_remote(repo_path: &Path, remote: Option<&str>) -> Result<OpResult> {
    let st = get_status(repo_path).await?;
    if st.remotes.is_empty() {
        return Err(CoreError::InvalidInput(
            "No git remote configured. Add a remote first.".to_string(),
        ));
    }
    if let Some(r) = remote.map(str::trim).filter(|r| !r.is_empty()) {
        let rem = validate_ref_name(r, "remote")?;
        let out = run_git(repo_path, &["push", "-u", rem, &st.branch], NETWORK_TIMEOUT_SECS).await?;
        return Ok(OpResult { success: true, message: out.stderr.trim().to_string(), has_conflicts: false });
    }
    if st.upstream.is_none() {
        return publish_branch(repo_path, None).await;
    }
    let out = run_git(repo_path, &["push"], NETWORK_TIMEOUT_SECS).await?;
    Ok(OpResult { success: true, message: out.stderr.trim().to_string(), has_conflicts: false })
}

pub async fn push_current(repo_path: &Path) -> Result<OpResult> {
    push_remote(repo_path, None).await
}

pub async fn publish_branch(repo_path: &Path, remote: Option<&str>) -> Result<OpResult> {
    let st = get_status(repo_path).await?;
    let remote_name = match remote.map(str::trim).filter(|r| !r.is_empty()) {
        Some(r) => validate_ref_name(r, "remote")?,
        None => st
            .remotes
            .first()
            .map(String::as_str)
            .ok_or_else(|| CoreError::InvalidInput("No git remote configured".to_string()))?,
    };
    let out = run_git(repo_path, &["push", "-u", remote_name, &st.branch], NETWORK_TIMEOUT_SECS).await?;
    Ok(OpResult {
        success: true,
        message: format!("Published {} to {remote_name}\n{}", st.branch, out.stderr.trim()),
        has_conflicts: false,
    })
}

pub async fn add_remote(repo_path: &Path, name: &str, url: &str) -> Result<OpResult> {
    let n = validate_ref_name(name, "remote name")?;
    let u = validate_git_arg(url, "remote url")?;
    run_git(repo_path, &["remote", "add", n, u], LOCAL_TIMEOUT_SECS).await?;
    Ok(OpResult { success: true, message: format!("Added remote {n}"), has_conflicts: false })
}

pub async fn init_repo(dir_path: &Path) -> Result<OpResult> {
    if !dir_path.exists() {
        std::fs::create_dir_all(dir_path)?;
    }
    run_git(dir_path, &["init", "-b", "main"], LOCAL_TIMEOUT_SECS).await?;
    Ok(OpResult {
        success: true,
        message: format!("Initialized git repository at {}", dir_path.display()),
        has_conflicts: false,
    })
}

pub async fn commit_all(repo_path: &Path, message: &str) -> Result<OpResult> {
    commit_smart(repo_path, message, false).await
}

/// VS Code-style smart commit: commits staged changes if any exist, otherwise stages all first.
pub async fn commit_smart(repo_path: &Path, message: &str, amend: bool) -> Result<OpResult> {
    let trimmed_msg = message.trim();
    if trimmed_msg.is_empty() {
        return Err(CoreError::InvalidInput("commit message cannot be empty".into()));
    }
    let staged_check = run_git_raw(repo_path, &["diff", "--cached", "--quiet"], LOCAL_TIMEOUT_SECS).await?;
    let has_staged = !staged_check.success;
    if !has_staged {
        let dirty_check = run_git_raw(repo_path, &["status", "--porcelain=v1"], LOCAL_TIMEOUT_SECS).await?;
        if !dirty_check.stdout.trim().is_empty() || !amend {
            run_git(repo_path, &["add", "-A"], LOCAL_TIMEOUT_SECS).await?;
        }
    }
    let mut args = vec!["commit"];
    if amend {
        args.push("--amend");
    }
    args.push("-m");
    args.push(message);
    let out = run_git(repo_path, &args, LOCAL_TIMEOUT_SECS).await?;
    Ok(OpResult { success: true, message: out.stdout.trim().to_string(), has_conflicts: false })
}

pub async fn stage_all(repo_path: &Path) -> Result<OpResult> {
    run_git(repo_path, &["add", "-A"], LOCAL_TIMEOUT_SECS).await?;
    Ok(OpResult { success: true, message: "Staged all changes".to_string(), has_conflicts: false })
}

pub async fn unstage_all(repo_path: &Path) -> Result<OpResult> {
    let out = run_git_raw(repo_path, &["reset", "HEAD"], LOCAL_TIMEOUT_SECS).await?;
    if !out.success {
        let rm_out = run_git_raw(repo_path, &["rm", "--cached", "-r", "--ignore-unmatch", "."], LOCAL_TIMEOUT_SECS).await?;
        if !rm_out.success {
            return Err(CoreError::GitFailed { code: rm_out.code, stderr: rm_out.stderr.trim().to_string() });
        }
    }
    Ok(OpResult { success: true, message: "Unstaged all changes".to_string(), has_conflicts: false })
}

pub async fn discard_all(repo_path: &Path) -> Result<OpResult> {
    let _ = run_git_raw(repo_path, &["checkout", "--", "."], LOCAL_TIMEOUT_SECS).await?;
    run_git(repo_path, &["clean", "-fd"], LOCAL_TIMEOUT_SECS).await?;
    Ok(OpResult { success: true, message: "Discarded all unstaged changes".to_string(), has_conflicts: false })
}

pub async fn stash_save(repo_path: &Path, message: Option<&str>) -> Result<OpResult> {
    let out = match message.map(str::trim).filter(|m| !m.is_empty()) {
        Some(msg) => run_git(repo_path, &["stash", "push", "-u", "-m", msg], LOCAL_TIMEOUT_SECS).await?,
        None => run_git(repo_path, &["stash", "push", "-u"], LOCAL_TIMEOUT_SECS).await?,
    };
    Ok(OpResult { success: true, message: out.stdout.trim().to_string(), has_conflicts: false })
}

pub async fn stash_pop(repo_path: &Path) -> Result<OpResult> {
    let out = run_git_raw(repo_path, &["stash", "pop"], LOCAL_TIMEOUT_SECS).await?;
    if out.success {
        return Ok(OpResult { success: true, message: out.stdout.trim().to_string(), has_conflicts: false });
    }
    let st = get_status(repo_path).await?;
    if !st.conflicts.is_empty() {
        return Ok(OpResult { success: false, message: "Stash pop resulted in conflicts".to_string(), has_conflicts: true });
    }
    Err(CoreError::GitFailed { code: out.code, stderr: out.stderr.trim().to_string() })
}
