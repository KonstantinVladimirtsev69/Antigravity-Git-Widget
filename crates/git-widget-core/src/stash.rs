//! Stash list, apply, pop, and drop manager.

use crate::{
    error::{CoreError, Result},
    git_cli::{run_git, run_git_raw, LOCAL_TIMEOUT_SECS},
    ops::OpResult,
    status::get_status,
};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StashEntry {
    pub index: usize,
    pub reference: String,
    pub message: String,
    pub date_rel: String,
}

/// Lists all stashes in the repository.
pub async fn list_stashes(repo_path: &Path) -> Result<Vec<StashEntry>> {
    let out = run_git_raw(
        repo_path,
        &["stash", "list", "--format=%gd%x09%gs%x09%cr"],
        LOCAL_TIMEOUT_SECS,
    )
    .await?;
    if !out.success {
        return Ok(Vec::new());
    }
    let entries = out
        .stdout
        .lines()
        .enumerate()
        .filter_map(|(idx, line)| {
            let parts: Vec<&str> = line.splitn(3, '\t').collect();
            if parts.len() < 3 {
                return None;
            }
            Some(StashEntry {
                index: idx,
                reference: parts[0].to_string(),
                message: parts[1].to_string(),
                date_rel: parts[2].to_string(),
            })
        })
        .collect();
    Ok(entries)
}

fn stash_ref(index: usize) -> String {
    format!("stash@{{{index}}}")
}

/// Applies a specific stash entry without removing it.
pub async fn stash_apply(repo_path: &Path, index: usize) -> Result<OpResult> {
    let r = stash_ref(index);
    let out = run_git_raw(repo_path, &["stash", "apply", &r], LOCAL_TIMEOUT_SECS).await?;
    if out.success {
        return Ok(OpResult {
            success: true,
            message: format!("Applied {r}"),
            has_conflicts: false,
        });
    }
    let st = get_status(repo_path).await?;
    if !st.conflicts.is_empty() {
        return Ok(OpResult {
            success: false,
            message: format!("Applying {r} caused conflicts"),
            has_conflicts: true,
        });
    }
    Err(CoreError::GitFailed {
        code: out.code,
        stderr: out.stderr.trim().to_string(),
    })
}

/// Pops a specific stash entry (applies and removes it).
pub async fn stash_pop_at(repo_path: &Path, index: usize) -> Result<OpResult> {
    let r = stash_ref(index);
    let out = run_git_raw(repo_path, &["stash", "pop", &r], LOCAL_TIMEOUT_SECS).await?;
    if out.success {
        return Ok(OpResult {
            success: true,
            message: format!("Popped {r}"),
            has_conflicts: false,
        });
    }
    let st = get_status(repo_path).await?;
    if !st.conflicts.is_empty() {
        return Ok(OpResult {
            success: false,
            message: format!("Popping {r} caused conflicts"),
            has_conflicts: true,
        });
    }
    Err(CoreError::GitFailed {
        code: out.code,
        stderr: out.stderr.trim().to_string(),
    })
}

/// Drops a specific stash entry.
pub async fn stash_drop(repo_path: &Path, index: usize) -> Result<OpResult> {
    let r = stash_ref(index);
    run_git(repo_path, &["stash", "drop", &r], LOCAL_TIMEOUT_SECS).await?;
    Ok(OpResult {
        success: true,
        message: format!("Dropped {r}"),
        has_conflicts: false,
    })
}
