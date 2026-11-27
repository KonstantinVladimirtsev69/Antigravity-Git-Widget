//! Safety guards (dirty tree, protected main/master preview) and conflict workflow.

use crate::{
    error::Result,
    git_cli::{run_git, LOCAL_TIMEOUT_SECS},
    ops::{
        checkout_branch, commit_all, merge_branch, pull_remote, stash_pop, stash_save, OpResult,
    },
    remote_focus::try_set_upstream_to,
    status::{get_status, get_status_for_remote, RepoStatus},
    validation::validate_ref_name,
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

/// Shared dirty-tree pre-flight guard returning `Ok(Some(blocked_outcome))` or `Ok(None)` with `did_stash` flag.
pub(crate) async fn prepare_dirty_tree(
    repo_path: &Path,
    status: &RepoStatus,
    strategy: DirtyStrategy,
) -> Result<(Option<GuardedOpOutcome>, bool)> {
    if !status.is_dirty {
        return Ok((None, false));
    }
    match strategy {
        DirtyStrategy::FailIfDirty => {
            let dirty_files = status.changes.iter().map(|c| c.path.clone()).collect();
            Ok((
                Some(GuardedOpOutcome {
                    blocked_dirty: true,
                    dirty_files,
                    op_result: None,
                    stash_restored: false,
                }),
                false,
            ))
        }
        DirtyStrategy::StashAndPop => {
            stash_save(repo_path, Some("git-widget auto-stash")).await?;
            Ok((None, true))
        }
        DirtyStrategy::QuickCommit { message } => {
            commit_all(repo_path, &message).await?;
            Ok((None, false))
        }
    }
}

/// Restores auto-stash when clean, or annotates conflict message when preserved in `stash@{0}`.
pub(crate) async fn finalize_auto_stash(
    repo_path: &Path,
    did_stash: bool,
    op_res: &mut OpResult,
) -> Result<bool> {
    if !did_stash {
        return Ok(false);
    }
    if op_res.success && !op_res.has_conflicts {
        let pop_res = stash_pop(repo_path).await?;
        return Ok(pop_res.success);
    }
    if op_res.has_conflicts {
        op_res.message.push_str("\n(Local WIP preserved in stash@{0})");
    }
    Ok(false)
}

/// Executes checkout/pull/merge with a dirty-tree safety guard and optional remote upstream sync.
pub async fn run_with_dirty_guard_for_remote(
    repo_path: &Path,
    kind: GuardedOpKind,
    target: Option<&str>,
    strategy: DirtyStrategy,
    remote: Option<&str>,
) -> Result<GuardedOpOutcome> {
    let req_remote = match remote.map(str::trim).filter(|r| !r.is_empty()) {
        Some(r) => Some(validate_ref_name(r, "remote")?),
        None => None,
    };
    let status = get_status(repo_path).await?;
    let (blocked, did_stash) = prepare_dirty_tree(repo_path, &status, strategy).await?;
    if let Some(outcome) = blocked {
        return Ok(outcome);
    }

    let op_attempt = match kind {
        GuardedOpKind::Checkout => checkout_branch(repo_path, target.unwrap_or("")).await,
        GuardedOpKind::Pull => pull_remote(repo_path, target).await,
        GuardedOpKind::Merge => merge_branch(repo_path, target.unwrap_or("")).await,
    };

    let mut op_res = match op_attempt {
        Ok(res) => res,
        Err(err) => {
            if did_stash {
                let _ = stash_pop(repo_path).await;
            }
            return Err(err);
        }
    };

    let stash_restored = finalize_auto_stash(repo_path, did_stash, &mut op_res).await?;
    if matches!(kind, GuardedOpKind::Checkout) && op_res.success && !op_res.has_conflicts {
        if let Some(r) = req_remote {
            let checked_out = target.map(str::trim).filter(|b| !b.is_empty());
            if let Some(b) = checked_out {
                try_set_upstream_to(repo_path, r, b).await?;
            }
        }
    }
    Ok(GuardedOpOutcome {
        blocked_dirty: false,
        dirty_files: Vec::new(),
        op_result: Some(op_res),
        stash_restored,
    })
}

/// Executes checkout/pull/merge with a dirty-tree safety guard and automatic stash rollback on error.
pub async fn run_with_dirty_guard(
    repo_path: &Path,
    kind: GuardedOpKind,
    target: Option<&str>,
    strategy: DirtyStrategy,
) -> Result<GuardedOpOutcome> {
    run_with_dirty_guard_for_remote(repo_path, kind, target, strategy, None).await
}

/// Builds a commit preview before merging `source_branch` into the current branch.
pub async fn preview_merge(repo_path: &Path, source_branch: &str) -> Result<ActionPreview> {
    let src = validate_ref_name(source_branch, "source branch")?;
    let status = get_status(repo_path).await?;
    let range = format!("HEAD..{src}");
    let out = run_git(repo_path, &["log", "--oneline", &range], LOCAL_TIMEOUT_SECS).await?;
    let commits: Vec<String> = out
        .stdout
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(ToString::to_string)
        .collect();

    Ok(ActionPreview {
        source: src.to_string(),
        target: status.branch.clone(),
        is_protected: is_protected_branch(&status.branch),
        commit_count: commits.len(),
        commits,
    })
}

/// Builds a commit preview before pushing current branch to `remote` (or configured upstream).
pub async fn preview_push_for_remote(
    repo_path: &Path,
    remote: Option<&str>,
) -> Result<ActionPreview> {
    let req_remote = match remote.map(str::trim).filter(|r| !r.is_empty()) {
        Some(r) => Some(validate_ref_name(r, "remote")?),
        None => None,
    };
    let status = get_status_for_remote(repo_path, req_remote).await?;
    let effective_remote = req_remote.unwrap_or("origin");
    let target = status
        .upstream
        .clone()
        .unwrap_or_else(|| format!("{effective_remote}/{}", status.branch));
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

/// Builds a commit preview before pushing current branch to upstream.
pub async fn preview_push(repo_path: &Path) -> Result<ActionPreview> {
    preview_push_for_remote(repo_path, None).await
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
