//! Multi-remote focus, conditional upstream switching, and remote-scoped tracking.

use crate::{
    error::{CoreError, Result},
    git_cli::{run_git, run_git_raw, LOCAL_TIMEOUT_SECS},
    ops::fetch_remote,
    status::{get_status_for_remote, list_remotes, BranchTracking, RepoStatus},
    validation::validate_ref_name,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::Path};
use tokio::task::JoinSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwitchRemoteResult {
    pub status: RepoStatus,
    pub upstream_switched: bool,
    pub target_upstream: Option<String>,
    pub fetch_warning: Option<String>,
}

/// Splits `<remote>/<branch>` matching the longest known remote prefix first
/// so remote names containing `/` (e.g. `team/fork`) resolve accurately.
pub fn split_remote_branch<'a>(
    remote_branch: &'a str,
    remotes: &[String],
) -> Option<(&'a str, &'a str)> {
    let mut candidates: Vec<&str> = remotes
        .iter()
        .map(String::as_str)
        .filter(|r| !r.is_empty())
        .collect();
    candidates.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
    for rem in candidates {
        if let Some(branch) = remote_branch.strip_prefix(rem).and_then(|s| s.strip_prefix('/')) {
            if !branch.is_empty() {
                return Some((&remote_branch[..rem.len()], branch));
            }
        }
    }
    None
}

/// Returns the currently checked-out local branch name, or `None` if detached/unborn.
pub async fn current_attached_branch(repo_path: &Path) -> Option<String> {
    let out = run_git_raw(
        repo_path,
        &["symbolic-ref", "--quiet", "--short", "HEAD"],
        LOCAL_TIMEOUT_SECS,
    )
    .await
    .ok()?;
    let br = out.stdout.trim();
    if out.success && is_attached_branch_name(br) {
        Some(br.to_string())
    } else {
        None
    }
}

/// Resolves `<remote>/<current_branch>` for single-branch graph scope when `remote` is active.
pub(crate) async fn scoped_graph_remote_ref(
    repo_path: &Path,
    remote: Option<&str>,
) -> Option<String> {
    let r = remote?;
    let br = current_attached_branch(repo_path).await?;
    remote_branch_exists(repo_path, r, &br)
        .await
        .then(|| format!("{r}/{br}"))
}

/// Returns true when `branch` represents an attached local branch rather than detached/unborn HEAD.
pub fn is_attached_branch_name(branch: &str) -> bool {
    let b = branch.trim();
    !b.is_empty() && b != "HEAD" && b != "(detached)" && b != "(initial)" && !b.starts_with('(')
}

/// Checks whether `refs/remotes/<remote>/<branch>` exists in the local repository.
pub async fn remote_branch_exists(repo_path: &Path, remote: &str, branch: &str) -> bool {
    let full_ref = format!("refs/remotes/{remote}/{branch}");
    run_git_raw(
        repo_path,
        &["show-ref", "--verify", "--quiet", &full_ref],
        LOCAL_TIMEOUT_SECS,
    )
    .await
    .map(|o| o.success)
    .unwrap_or(false)
}

/// Checks whether `refs/heads/<branch>` exists in the local repository (false for unborn HEAD).
pub async fn local_branch_exists(repo_path: &Path, branch: &str) -> bool {
    let full_ref = format!("refs/heads/{branch}");
    run_git_raw(
        repo_path,
        &["show-ref", "--verify", "--quiet", &full_ref],
        LOCAL_TIMEOUT_SECS,
    )
    .await
    .map(|o| o.success)
    .unwrap_or(false)
}

/// Updates `git branch --set-upstream-to=<remote>/<branch>` only when `branch` is attached,
/// exists locally, and `refs/remotes/<remote>/<branch>` exists. Never touches `.git/config` otherwise.
pub async fn try_set_upstream_to(repo_path: &Path, remote: &str, branch: &str) -> Result<bool> {
    let rem = validate_ref_name(remote, "remote")?;
    let trimmed_branch = branch.trim();
    if !is_attached_branch_name(trimmed_branch) {
        return Ok(false);
    }
    let br = validate_ref_name(trimmed_branch, "branch")?;
    if !local_branch_exists(repo_path, br).await || !remote_branch_exists(repo_path, rem, br).await
    {
        return Ok(false);
    }
    let arg = format!("--set-upstream-to={rem}/{br}");
    run_git(repo_path, &["branch", &arg, br], LOCAL_TIMEOUT_SECS).await?;
    Ok(true)
}

/// Recomputes `tracking` strictly against `remote/<branch>` without falling back to `origin`.
pub async fn apply_remote_scoped_tracking(
    repo_path: &Path,
    remote: &str,
    current_branch: &str,
    local_branches: &[String],
    remote_branches: &[String],
    tracking: &mut HashMap<String, BranchTracking>,
) -> (Option<String>, u32, u32) {
    let mut set = JoinSet::new();
    for branch in local_branches {
        let candidate = format!("{remote}/{branch}");
        let has_remote = remote_branches.iter().any(|rb| rb == &candidate);
        let entry = tracking.entry(branch.clone()).or_default();
        *entry = BranchTracking {
            upstream: has_remote.then(|| candidate.clone()),
            ahead: 0,
            behind: 0,
            gone: false,
        };
        if !has_remote {
            continue;
        }
        let repo = repo_path.to_path_buf();
        let b_name = branch.clone();
        set.spawn(async move {
            let spec = format!("{b_name}...{candidate}");
            let out = run_git(
                &repo,
                &["rev-list", "--left-right", "--count", &spec],
                LOCAL_TIMEOUT_SECS,
            )
            .await
            .ok()?;
            let mut nums = out.stdout.split_whitespace();
            let a = nums.next()?.parse().unwrap_or(0);
            let b = nums.next()?.parse().unwrap_or(0);
            Some((b_name, a, b))
        });
    }

    while let Some(res) = set.join_next().await {
        if let Ok(Some((b_name, a, b))) = res {
            if let Some(entry) = tracking.get_mut(&b_name) {
                entry.ahead = a;
                entry.behind = b;
            }
        }
    }

    if !is_attached_branch_name(current_branch) {
        return (None, 0, 0);
    }
    match tracking.get(current_branch) {
        Some(t) => (t.upstream.clone(), t.ahead, t.behind),
        None => (None, 0, 0),
    }
}

/// Validates `remote` exists, runs `fetch <remote>` (capturing network errors into `fetch_warning`),
/// conditionally switches upstream on attached HEAD, and returns remote-scoped `RepoStatus`.
pub async fn switch_remote(repo_path: &Path, remote: &str) -> Result<SwitchRemoteResult> {
    let rem = validate_ref_name(remote, "remote")?;
    let remotes = list_remotes(repo_path).await;
    if !remotes.iter().any(|r| r == rem) {
        return Err(CoreError::InvalidInput(format!(
            "Unknown git remote: '{rem}'"
        )));
    }

    let fetch_warning = match fetch_remote(repo_path, Some(rem)).await {
        Ok(_) => None,
        Err(err) => Some(err.to_string()),
    };

    let (upstream_switched, target_upstream) =
        if let Some(branch) = current_attached_branch(repo_path).await {
            let switched = try_set_upstream_to(repo_path, rem, &branch).await?;
            let target = switched.then(|| format!("{rem}/{branch}"));
            (switched, target)
        } else {
            (false, None)
        };

    let status = get_status_for_remote(repo_path, Some(rem)).await?;
    Ok(SwitchRemoteResult {
        status,
        upstream_switched,
        target_upstream,
        fetch_warning,
    })
}
