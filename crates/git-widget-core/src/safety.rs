//! Safety guards (dirty tree, protected main/master preview) and conflict workflow.

use crate::{
    error::Result,
    git_cli::{run_git, LOCAL_TIMEOUT_SECS},
    ops::{
        checkout_branch, commit_all, merge_branch, pull_current, stash_pop, stash_save, OpResult,
    },
    status::get_status,
};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum DirtyStrategy {
    FailIfDirty,
    StashAndPop,
    QuickCommit { message: String },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GuardedOpKind {
    Checkout,
    Pull,
    Merge,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardedOpOutcome {
    pub blocked_dirty: bool,
    pub dirty_files: Vec<String>,
    pub op_result: Option<OpResult>,
    pub stash_restored: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionPreview {
    pub source: String,
    pub target: String,
    pub is_protected: bool,
    pub commit_count: usize,
    pub commits: Vec<String>,
}

pub fn is_protected_branch(branch: &str) -> bool {
    matches!(branch.trim(), "main" | "master")
}

/// Executes checkout/pull/merge with a dirty-tree safety guard.
pub async fn run_with_dirty_guard(
    repo_path: &Path,
    kind: GuardedOpKind,
    target: Option<&str>,
    strategy: DirtyStrategy,
) -> Result<GuardedOpOutcome> {
    let status = get_status(repo_path).await?;
    let mut did_stash = false;

    if status.is_dirty {
        match strategy {
            DirtyStrategy::FailIfDirty => {
                let dirty_files = status.changes.into_iter().map(|c| c.path).collect();
                return Ok(GuardedOpOutcome {
                    blocked_dirty: true,
                    dirty_files,
                    op_result: None,
                    stash_restored: false,
                });
            }
            DirtyStrategy::StashAndPop => {
                stash_save(repo_path, Some("git-widget auto-stash")).await?;
                did_stash = true;
            }
            DirtyStrategy::QuickCommit { message } => {
                commit_all(repo_path, &message).await?;
            }
        }
    }

    let op_res = match kind {
        GuardedOpKind::Checkout => checkout_branch(repo_path, target.unwrap_or("")).await?,
        GuardedOpKind::Pull => pull_current(repo_path).await?,
        GuardedOpKind::Merge => merge_branch(repo_path, target.unwrap_or("")).await?,
    };

    let mut stash_restored = false;
    if did_stash && op_res.success && !op_res.has_conflicts {
        let pop_res = stash_pop(repo_path).await?;
        stash_restored = pop_res.success;
    }

    Ok(GuardedOpOutcome {
        blocked_dirty: false,
        dirty_files: Vec::new(),
        op_result: Some(op_res),
        stash_restored,
    })
}

/// Builds a commit preview before merging `source_branch` into the current branch.
pub async fn preview_merge(repo_path: &Path, source_branch: &str) -> Result<ActionPreview> {
    let status = get_status(repo_path).await?;
    let range = format!("HEAD..{source_branch}");
    let out = run_git(repo_path, &["log", "--oneline", &range], LOCAL_TIMEOUT_SECS).await?;
    let commits: Vec<String> = out
        .stdout
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(ToString::to_string)
        .collect();

    Ok(ActionPreview {
        source: source_branch.to_string(),
        target: status.branch.clone(),
        is_protected: is_protected_branch(&status.branch),
        commit_count: commits.len(),
        commits,
    })
}

/// Builds a commit preview before pushing current branch to upstream.
pub async fn preview_push(repo_path: &Path) -> Result<ActionPreview> {
    let status = get_status(repo_path).await?;
    let target = status
        .upstream
        .clone()
        .unwrap_or_else(|| format!("origin/{}", status.branch));
    let range = format!("{target}..HEAD");
    let commits = match run_git(repo_path, &["log", "--oneline", &range], LOCAL_TIMEOUT_SECS).await
    {
        Ok(out) => out
            .stdout
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(ToString::to_string)
            .collect(),
        Err(_) => Vec::new(),
    };

    Ok(ActionPreview {
        source: status.branch.clone(),
        target,
        is_protected: is_protected_branch(&status.branch),
        commit_count: commits.len(),
        commits,
    })
}

pub async fn abort_merge(repo_path: &Path) -> Result<OpResult> {
    let has_merge_head = run_git(
        repo_path,
        &["rev-parse", "--verify", "--quiet", "MERGE_HEAD"],
        LOCAL_TIMEOUT_SECS,
    )
    .await
    .is_ok();
    let args = if has_merge_head {
        &["merge", "--abort"][..]
    } else {
        &["reset", "--merge"][..]
    };
    run_git(repo_path, args, LOCAL_TIMEOUT_SECS).await?;
    Ok(OpResult {
        success: true,
        message: "Merge aborted".to_string(),
        has_conflicts: false,
    })
}

/// Generates a structured prompt with conflict markers for AI chat resolution.
pub async fn build_conflict_prompt(repo_path: &Path) -> Result<String> {
    let status = get_status(repo_path).await?;
    let mut prompt = format!(
        "Помоги разрешить merge-конфликты в репозитории `{}` (ветка `{}`).\n\nКонфликтные файлы:\n",
        repo_path.display(),
        status.branch
    );

    for file in &status.conflicts {
        prompt.push_str(&format!("\n### `{file}`\n```text\n"));
        let full = repo_path.join(file);
        if let Ok(content) = fs::read_to_string(&full) {
            let excerpt: Vec<&str> = content.lines().take(120).collect();
            prompt.push_str(&excerpt.join("\n"));
        } else {
            prompt.push_str("<binary or unreadable file>");
        }
        prompt.push_str("\n```\n");
    }
    Ok(prompt)
}
