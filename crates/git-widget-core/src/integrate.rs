//! Multi-mode branch integration (merge, no-ff, squash, rebase, ff-only) and rebase conflict recovery.

pub use crate::sync::{
    pull_both_branches, pull_branch, scan_integration_sync, BranchSyncState, DualBranchSync,
};
use crate::{
    error::{CoreError, Result},
    git_cli::{run_git, run_git_raw, LOCAL_TIMEOUT_SECS},
    ops::{checkout_branch, commit_all, stash_pop, stash_save, OpResult},
    safety::{is_protected_branch, ActionPreview, DirtyStrategy, GuardedOpOutcome},
    status::get_status,
    sync::validate_branch,
};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum IntegrationMode {
    #[default]
    Merge,
    MergeNoFf,
    Squash,
    Rebase,
    FfOnly,
}

impl IntegrationMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Merge => "Merge",
            Self::MergeNoFf => "Merge (no-ff)",
            Self::Squash => "Squash",
            Self::Rebase => "Rebase",
            Self::FfOnly => "FF Only",
        }
    }
}

/// Builds a commit preview for integrating `source` into `target` with `mode`.
pub async fn preview_integration(
    repo_path: &Path,
    source: &str,
    target: &str,
    mode: IntegrationMode,
) -> Result<ActionPreview> {
    let src = validate_branch(source, "source branch")?;
    let dst = validate_branch(target, "target branch")?;
    let range = match mode {
        IntegrationMode::Rebase => format!("{src}..{dst}"),
        _ => format!("{dst}..{src}"),
    };
    let out = run_git(repo_path, &["log", "--oneline", &range], LOCAL_TIMEOUT_SECS).await?;
    let commits: Vec<String> = out
        .stdout
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(ToString::to_string)
        .collect();

    Ok(ActionPreview {
        source: src,
        target: dst.clone(),
        is_protected: is_protected_branch(&dst),
        commit_count: commits.len(),
        commits,
    })
}

async fn execute_mode(
    repo_path: &Path,
    source: &str,
    target: &str,
    mode: IntegrationMode,
) -> Result<OpResult> {
    let out = match mode {
        IntegrationMode::Merge => run_git_raw(repo_path, &["merge", source], LOCAL_TIMEOUT_SECS).await?,
        IntegrationMode::MergeNoFf => {
            run_git_raw(repo_path, &["merge", "--no-ff", source], LOCAL_TIMEOUT_SECS).await?
        }
        IntegrationMode::FfOnly => {
            run_git_raw(repo_path, &["merge", "--ff-only", source], LOCAL_TIMEOUT_SECS).await?
        }
        IntegrationMode::Squash => {
            let sq = run_git_raw(repo_path, &["merge", "--squash", source], LOCAL_TIMEOUT_SECS).await?;
            if !sq.success {
                sq
            } else {
                let st_after = get_status(repo_path).await?;
                if st_after.staged_changes.is_empty() {
                    return Ok(OpResult {
                        success: true,
                        message: format!("Already up to date (nothing to squash from {source})"),
                        has_conflicts: false,
                    });
                }
                let commit_out = run_git_raw(
                    repo_path,
                    &["commit", "--no-edit"],
                    LOCAL_TIMEOUT_SECS,
                )
                .await?;
                if commit_out.success {
                    return Ok(OpResult {
                        success: true,
                        message: format!("Squash-merged {source} → {target}"),
                        has_conflicts: false,
                    });
                }
                let fallback_msg = format!("Squash merge {source} into {target}");
                let fallback = run_git(
                    repo_path,
                    &["commit", "-m", &fallback_msg],
                    LOCAL_TIMEOUT_SECS,
                )
                .await?;
                return Ok(OpResult {
                    success: true,
                    message: fallback.stdout.trim().to_string(),
                    has_conflicts: false,
                });
            }
        }
        IntegrationMode::Rebase => {
            run_git_raw(repo_path, &["rebase", source], LOCAL_TIMEOUT_SECS).await?
        }
    };

    if out.success {
        let msg = out.stdout.trim();
        let fallback = match mode {
            IntegrationMode::Rebase => format!("Rebased {target} onto {source}"),
            _ => format!("{} {source} → {target}", mode.label()),
        };
        return Ok(OpResult {
            success: true,
            message: if msg.is_empty() { fallback } else { msg.to_string() },
            has_conflicts: false,
        });
    }

    let st = get_status(repo_path).await?;
    if !st.conflicts.is_empty() || st.is_merging || st.is_rebasing {
        let kind = if mode == IntegrationMode::Rebase { "Rebase" } else { "Merge" };
        return Ok(OpResult {
            success: false,
            message: format!("{kind} conflict integrating {source} → {target}"),
            has_conflicts: true,
        });
    }

    Err(CoreError::GitFailed {
        code: out.code,
        stderr: out.stderr.trim().to_string(),
    })
}

/// Atomically runs dirty-tree guard -> checkout `target` (if needed) -> integration -> stash pop.
pub async fn integrate_branches(
    repo_path: &Path,
    source: &str,
    target: &str,
    mode: IntegrationMode,
    strategy: DirtyStrategy,
) -> Result<GuardedOpOutcome> {
    let src = validate_branch(source, "source branch")?;
    let dst = validate_branch(target, "target branch")?;
    if src == dst {
        return Err(CoreError::InvalidInput(
            "Source and target branches must be different".to_string(),
        ));
    }

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

    if status.branch != dst {
        if let Err(err) = checkout_branch(repo_path, &dst).await {
            if did_stash {
                let _ = stash_pop(repo_path).await;
            }
            return Err(err);
        }
    }

    let op_res = match execute_mode(repo_path, &src, &dst, mode).await {
        Ok(res) => res,
        Err(err) => {
            if did_stash {
                let _ = stash_pop(repo_path).await;
            }
            return Err(err);
        }
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

pub async fn abort_rebase(repo_path: &Path) -> Result<OpResult> {
    run_git(repo_path, &["rebase", "--abort"], LOCAL_TIMEOUT_SECS).await?;
    Ok(OpResult {
        success: true,
        message: "Rebase aborted".to_string(),
        has_conflicts: false,
    })
}

pub async fn continue_rebase(repo_path: &Path) -> Result<OpResult> {
    let out = run_git_raw(
        repo_path,
        &["-c", "core.editor=true", "rebase", "--continue"],
        LOCAL_TIMEOUT_SECS,
    )
    .await?;
    if out.success {
        return Ok(OpResult {
            success: true,
            message: "Rebase continued".to_string(),
            has_conflicts: false,
        });
    }
    let st = get_status(repo_path).await?;
    if !st.conflicts.is_empty() || st.is_rebasing {
        return Ok(OpResult {
            success: false,
            message: "Remaining rebase conflicts".to_string(),
            has_conflicts: true,
        });
    }
    Err(CoreError::GitFailed {
        code: out.code,
        stderr: out.stderr.trim().to_string(),
    })
}
