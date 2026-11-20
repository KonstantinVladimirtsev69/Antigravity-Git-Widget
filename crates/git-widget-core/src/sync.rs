//! Dual-branch sync scanning and safe pulling (fast-forward or clean checkout).

use crate::{
    error::{CoreError, Result},
    git_cli::{run_git_raw, NETWORK_TIMEOUT_SECS},
    ops::{checkout_branch, fetch_remote, pull_remote, OpResult},
    status::{get_status, BranchTracking},
};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BranchSyncState {
    pub branch: String,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub gone: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DualBranchSync {
    pub source: BranchSyncState,
    pub target: BranchSyncState,
    pub any_behind: bool,
    pub fetch_error: Option<String>,
}

pub(crate) fn validate_branch(val: &str, field: &str) -> Result<String> {
    let trimmed = val.trim();
    if trimmed.is_empty() || trimmed.contains(' ') || trimmed.starts_with('-') {
        return Err(CoreError::InvalidInput(format!("invalid {field}: '{val}'")));
    }
    Ok(trimmed.to_string())
}

fn state_from_tracking(branch: &str, tracking: Option<&BranchTracking>) -> BranchSyncState {
    match tracking {
        Some(t) => BranchSyncState {
            branch: branch.to_string(),
            upstream: t.upstream.clone(),
            ahead: t.ahead,
            behind: t.behind,
            gone: t.gone,
        },
        None => BranchSyncState {
            branch: branch.to_string(),
            upstream: None,
            ahead: 0,
            behind: 0,
            gone: false,
        },
    }
}

/// Scans both `source` and `target` branches for unpulled commits (optionally running `git fetch` first).
pub async fn scan_integration_sync(
    repo_path: &Path,
    source: &str,
    target: &str,
    remote: Option<&str>,
    do_fetch: bool,
) -> Result<DualBranchSync> {
    let src = validate_branch(source, "source branch")?;
    let dst = validate_branch(target, "target branch")?;

    let mut fetch_error = None;
    if do_fetch {
        let st_pre = get_status(repo_path).await?;
        if !st_pre.remotes.is_empty() {
            if let Err(e) = fetch_remote(repo_path, remote).await {
                fetch_error = Some(e.to_string());
            }
        }
    }

    let st = get_status(repo_path).await?;
    let source_state = state_from_tracking(&src, st.branch_tracking.get(&src));
    let target_state = state_from_tracking(&dst, st.branch_tracking.get(&dst));
    let any_behind = source_state.behind > 0 || target_state.behind > 0;

    Ok(DualBranchSync {
        source: source_state,
        target: target_state,
        any_behind,
        fetch_error,
    })
}

fn resolve_remote_and_branch<'a>(
    branch: &'a str,
    tracking: Option<&'a BranchTracking>,
    preferred_remote: Option<&'a str>,
    remotes: &'a [String],
    remote_branches: &'a [String],
) -> Option<(String, String)> {
    if let Some(up) = tracking.and_then(|t| t.upstream.as_deref()) {
        if let Some((r, rb)) = up.split_once('/') {
            return Some((r.to_string(), rb.to_string()));
        }
    }
    if let Some(r) = preferred_remote.filter(|r| !r.trim().is_empty()) {
        let candidate = format!("{r}/{branch}");
        if remote_branches.iter().any(|rb| rb == &candidate) {
            return Some((r.to_string(), branch.to_string()));
        }
    }
    for r in remotes {
        let candidate = format!("{r}/{branch}");
        if remote_branches.iter().any(|rb| rb == &candidate) {
            return Some((r.clone(), branch.to_string()));
        }
    }
    None
}

/// Pulls a single branch. If not currently checked out and strictly behind (`ahead == 0`),
/// fast-forwards via `git fetch <remote> <remote_branch>:<local_branch>` without touching the working tree.
pub async fn pull_branch(
    repo_path: &Path,
    branch: &str,
    remote: Option<&str>,
) -> Result<OpResult> {
    let branch = validate_branch(branch, "branch")?;
    let st = get_status(repo_path).await?;
    if st.remotes.is_empty() {
        return Err(CoreError::InvalidInput(
            "No git remote configured. Add a remote first.".to_string(),
        ));
    }

    if branch == st.branch {
        return pull_remote(repo_path, remote).await;
    }

    let tracking = st.branch_tracking.get(&branch);
    let behind = tracking.map(|t| t.behind).unwrap_or(0);
    let ahead = tracking.map(|t| t.ahead).unwrap_or(0);
    if behind == 0 {
        return Ok(OpResult {
            success: true,
            message: format!("{branch} is already up to date"),
            has_conflicts: false,
        });
    }

    let Some((rem, rem_branch)) =
        resolve_remote_and_branch(&branch, tracking, remote, &st.remotes, &st.remote_branches)
    else {
        return Err(CoreError::InvalidInput(format!(
            "Branch '{branch}' has no upstream or matching remote branch."
        )));
    };

    if ahead == 0 {
        let refspec = format!("{rem_branch}:{branch}");
        let out = run_git_raw(repo_path, &["fetch", &rem, &refspec], NETWORK_TIMEOUT_SECS).await?;
        if out.success {
            return Ok(OpResult {
                success: true,
                message: format!("Fast-forwarded {branch} from {rem}/{rem_branch}"),
                has_conflicts: false,
            });
        }
        return Err(CoreError::GitFailed {
            code: out.code,
            stderr: out.stderr.trim().to_string(),
        });
    }

    if st.is_dirty {
        return Err(CoreError::InvalidInput(format!(
            "Branch '{branch}' has diverged (↑{ahead} ↓{behind}). Commit or stash changes before pulling it."
        )));
    }
    let original = st.branch.clone();
    checkout_branch(repo_path, &branch).await?;
    match pull_remote(repo_path, Some(&rem)).await {
        Ok(res) if res.success && !res.has_conflicts => {
            checkout_branch(repo_path, &original).await?;
            Ok(res)
        }
        Ok(conflict_res) => Ok(conflict_res),
        Err(err) => {
            let _ = checkout_branch(repo_path, &original).await;
            Err(err)
        }
    }
}

/// Fetches latest remote refs, then pulls any unpulled (`behind > 0`) commits on `source` and `target`.
pub async fn pull_both_branches(
    repo_path: &Path,
    source: &str,
    target: &str,
    remote: Option<&str>,
) -> Result<OpResult> {
    let sync = scan_integration_sync(repo_path, source, target, remote, true).await?;
    let mut messages = Vec::new();
    let st = get_status(repo_path).await?;

    let mut order = Vec::new();
    if sync.source.branch != st.branch {
        order.push(sync.source.clone());
        if sync.target.branch != sync.source.branch {
            order.push(sync.target.clone());
        }
    } else {
        if sync.target.branch != sync.source.branch {
            order.push(sync.target.clone());
        }
        order.push(sync.source.clone());
    }

    for item in order {
        if item.behind > 0 {
            let res = pull_branch(repo_path, &item.branch, remote).await?;
            if res.has_conflicts || !res.success {
                return Ok(res);
            }
            messages.push(format!("{}: pulled ↓{}", item.branch, item.behind));
        }
    }

    if messages.is_empty() {
        return Ok(OpResult {
            success: true,
            message: "Both branches are already up to date".to_string(),
            has_conflicts: false,
        });
    }

    Ok(OpResult {
        success: true,
        message: messages.join(" · "),
        has_conflicts: false,
    })
}
