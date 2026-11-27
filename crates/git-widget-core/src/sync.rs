//! Dual-branch sync scanning and safe pulling (fast-forward or clean checkout).

use crate::{
    error::{CoreError, Result},
    git_cli::{run_git_raw, NETWORK_TIMEOUT_SECS},
    ops::{checkout_branch, fetch_remote, pull_remote, OpResult},
    remote_focus::split_remote_branch,
    status::{get_status_for_remote, BranchTracking, RepoStatus},
    validation::validate_ref_name,
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
    let src = validate_ref_name(source, "source branch")?.to_string();
    let dst = validate_ref_name(target, "target branch")?.to_string();

    let mut fetch_error = None;
    if do_fetch {
        if let Err(e) = fetch_remote(repo_path, remote).await {
            let err_text = e.to_string();
            if !err_text.contains("No such remote") && !err_text.contains("does not appear to be a git repository") {
                fetch_error = Some(err_text);
            }
        }
    }

    let st = get_status_for_remote(repo_path, remote).await?;
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
    if let Some(r) = preferred_remote.map(str::trim).filter(|r| !r.is_empty()) {
        if let Some(up) = tracking.and_then(|t| t.upstream.as_deref()) {
            if let Some((up_r, rb)) = split_remote_branch(up, remotes) {
                if up_r == r {
                    return Some((up_r.to_string(), rb.to_string()));
                }
            }
        }
        let candidate = format!("{r}/{branch}");
        if remote_branches.iter().any(|rb| rb == &candidate) {
            return Some((r.to_string(), branch.to_string()));
        }
        return None;
    }
    if let Some(up) = tracking.and_then(|t| t.upstream.as_deref()) {
        if let Some((r, rb)) = split_remote_branch(up, remotes) {
            return Some((r.to_string(), rb.to_string()));
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

async fn pull_branch_with_status(
    repo_path: &Path,
    branch: &str,
    remote: Option<&str>,
    st: &RepoStatus,
) -> Result<OpResult> {
    if st.remotes.is_empty() {
        return Err(CoreError::InvalidInput("No git remote configured. Add a remote first.".to_string()));
    }
    if branch == st.branch {
        return pull_remote(repo_path, remote).await;
    }

    let tracking = st.branch_tracking.get(branch);
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
        resolve_remote_and_branch(branch, tracking, remote, &st.remotes, &st.remote_branches)
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
        return Err(CoreError::GitFailed { code: out.code, stderr: out.stderr.trim().to_string() });
    }

    if st.is_dirty {
        return Err(CoreError::InvalidInput(format!(
            "Branch '{branch}' has diverged (↑{ahead} ↓{behind}). Commit or stash changes before pulling it."
        )));
    }
    let original = st.branch.clone();
    checkout_branch(repo_path, branch).await?;
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

/// Pulls a single branch (fast-forwarding non-current branches via refspec when `ahead == 0`).
pub async fn pull_branch(repo_path: &Path, branch: &str, remote: Option<&str>) -> Result<OpResult> {
    let branch = validate_ref_name(branch, "branch")?;
    let st = get_status_for_remote(repo_path, remote).await?;
    pull_branch_with_status(repo_path, branch, remote, &st).await
}

/// Fetches latest remote refs, then pulls any unpulled (`behind > 0`) commits on `source` and `target`.
pub async fn pull_both_branches(
    repo_path: &Path,
    source: &str,
    target: &str,
    remote: Option<&str>,
) -> Result<OpResult> {
    let src = validate_ref_name(source, "source branch")?.to_string();
    let dst = validate_ref_name(target, "target branch")?.to_string();

    if let Err(e) = fetch_remote(repo_path, remote).await {
        let text = e.to_string();
        if text.contains("No git remote") {
            return Err(e);
        }
    }

    let st = get_status_for_remote(repo_path, remote).await?;
    let src_behind = st.branch_tracking.get(&src).map(|t| t.behind).unwrap_or(0);
    let dst_behind = st.branch_tracking.get(&dst).map(|t| t.behind).unwrap_or(0);

    let mut order = Vec::new();
    if src != st.branch {
        order.push((src.clone(), src_behind));
        if dst != src {
            order.push((dst.clone(), dst_behind));
        }
    } else {
        if dst != src {
            order.push((dst.clone(), dst_behind));
        }
        order.push((src.clone(), src_behind));
    }

    let mut messages = Vec::new();
    for (branch, behind) in order {
        if behind > 0 {
            let res = pull_branch_with_status(repo_path, &branch, remote, &st).await?;
            if res.has_conflicts || !res.success {
                return Ok(res);
            }
            messages.push(format!("{branch}: pulled ↓{behind}"));
        }
    }

    if messages.is_empty() {
        return Ok(OpResult {
            success: true,
            message: "Both branches are already up to date".to_string(),
            has_conflicts: false,
        });
    }

    Ok(OpResult { success: true, message: messages.join(" · "), has_conflicts: false })
}
