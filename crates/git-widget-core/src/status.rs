//! Repository status, branch tracking, and conflict/rebase marker detection.

pub use crate::status_parser::{BranchTracking, FileChange};
use crate::{
    error::{CoreError, Result},
    git_cli::{run_git, LOCAL_TIMEOUT_SECS},
    remote_focus::apply_remote_scoped_tracking,
    status_parser::{parse_porcelain_entry, parse_track_counts},
    validation::validate_ref_name,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
use tokio::task::JoinSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoStatus {
    pub branch: String,
    pub upstream: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub configured_upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub is_dirty: bool,
    pub is_merging: bool,
    pub is_rebasing: bool,
    pub changes: Vec<FileChange>,
    pub staged_changes: Vec<FileChange>,
    pub unstaged_changes: Vec<FileChange>,
    pub conflicts: Vec<String>,
    pub local_branches: Vec<String>,
    pub remote_branches: Vec<String>,
    pub remotes: Vec<String>,
    pub branch_tracking: HashMap<String, BranchTracking>,
}

/// Reads full status (`git status --porcelain=v2 --branch`) and branch list concurrently.
pub async fn get_status(repo_path: &Path) -> Result<RepoStatus> {
    get_status_for_remote(repo_path, None).await
}

/// Reads repository status scoped to an optional `remote` without falling back to `origin`.
pub async fn get_status_for_remote(repo_path: &Path, remote: Option<&str>) -> Result<RepoStatus> {
    let scoped_remote = match remote.map(str::trim).filter(|r| !r.is_empty()) {
        Some(r) => Some(validate_ref_name(r, "remote")?),
        None => None,
    };

    let dot_git = repo_path.join(".git");
    if !dot_git.exists() {
        return Err(CoreError::NotAGitRepo(repo_path.to_string_lossy().to_string()));
    }

    let (status_res, remotes, branches_res, git_dir) = tokio::join!(
        run_git(repo_path, &["status", "--porcelain=v2", "--branch"], LOCAL_TIMEOUT_SECS),
        list_remotes(repo_path),
        list_branches(repo_path),
        resolve_git_dir(repo_path, &dot_git),
    );
    let status_out = status_res?;
    let (local_branches, remote_branches, mut branch_tracking) = branches_res?;

    let mut branch = String::from("HEAD");
    let mut upstream = None;
    let mut ahead = 0u32;
    let mut behind = 0u32;
    let mut changes = Vec::new();
    let mut staged_changes = Vec::new();
    let mut unstaged_changes = Vec::new();
    let mut conflicts = Vec::new();

    for line in status_out.stdout.lines() {
        if let Some(rest) = line.strip_prefix("# branch.head ") {
            branch = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix("# branch.upstream ") {
            upstream = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("# branch.ab ") {
            for part in rest.split_whitespace() {
                if let Some(a) = part.strip_prefix('+') {
                    ahead = a.parse().unwrap_or(0);
                } else if let Some(b) = part.strip_prefix('-') {
                    behind = b.parse().unwrap_or(0);
                }
            }
        } else if let Some(change) = parse_porcelain_entry(line) {
            if change.is_conflicted {
                conflicts.push(change.path.clone());
            } else if change.code == "??" {
                unstaged_changes.push(FileChange {
                    path: change.path.clone(),
                    code: "U".to_string(),
                    is_conflicted: false,
                });
            } else {
                let mut chars = change.code.chars();
                let x = chars.next().unwrap_or('.');
                let y = chars.next().unwrap_or('.');
                if x != '.' {
                    staged_changes.push(FileChange {
                        path: change.path.clone(),
                        code: x.to_string(),
                        is_conflicted: false,
                    });
                }
                if y != '.' {
                    unstaged_changes.push(FileChange {
                        path: change.path.clone(),
                        code: y.to_string(),
                        is_conflicted: false,
                    });
                }
            }
            changes.push(change);
        }
    }

    let configured_upstream = scoped_remote.as_ref().and(upstream.clone());
    if let Some(rem) = scoped_remote {
        let (scoped_up, scoped_a, scoped_b) = apply_remote_scoped_tracking(
            repo_path,
            rem,
            &branch,
            &local_branches,
            &remote_branches,
            &mut branch_tracking,
        )
        .await;
        upstream = scoped_up;
        ahead = scoped_a;
        behind = scoped_b;
    } else {
        fill_fallback_tracking(repo_path, &local_branches, &remote_branches, &remotes, &mut branch_tracking).await;
    }
    let is_merging = git_dir.join("MERGE_HEAD").exists();
    let is_rebasing = git_dir.join("rebase-merge").exists() || git_dir.join("rebase-apply").exists();
    let is_dirty = !changes.is_empty();

    Ok(RepoStatus {
        branch,
        upstream,
        configured_upstream,
        ahead,
        behind,
        is_dirty,
        is_merging,
        is_rebasing,
        changes,
        staged_changes,
        unstaged_changes,
        conflicts,
        local_branches,
        remote_branches,
        remotes,
        branch_tracking,
    })
}

async fn resolve_git_dir(repo_path: &Path, dot_git: &Path) -> PathBuf {
    if dot_git.is_dir() {
        return dot_git.to_path_buf();
    }
    if let Ok(out) = run_git(repo_path, &["rev-parse", "--git-dir"], LOCAL_TIMEOUT_SECS).await {
        let p = PathBuf::from(out.stdout.trim());
        if p.is_absolute() {
            return p;
        }
        return repo_path.join(p);
    }
    dot_git.to_path_buf()
}

async fn list_branches(
    repo_path: &Path,
) -> Result<(Vec<String>, Vec<String>, HashMap<String, BranchTracking>)> {
    let (local_res, remote_res) = tokio::join!(
        run_git(
            repo_path,
            &[
                "for-each-ref",
                "--sort=-committerdate",
                "--format=%(refname:short)\t%(upstream:short)\t%(upstream:track,nobracket)",
                "refs/heads",
            ],
            LOCAL_TIMEOUT_SECS,
        ),
        run_git(
            repo_path,
            &["for-each-ref", "--sort=-committerdate", "--format=%(refname:short)", "refs/remotes"],
            LOCAL_TIMEOUT_SECS,
        ),
    );
    let local_out = local_res?;
    let remote_out = remote_res?;

    let mut local = Vec::new();
    let mut tracking = HashMap::new();
    for line in local_out.stdout.lines().map(str::trim_end).filter(|l| !l.is_empty()) {
        let mut parts = line.splitn(3, '\t');
        let name = parts.next().unwrap_or("").trim().to_string();
        if name.is_empty() {
            continue;
        }
        let up_raw = parts.next().unwrap_or("").trim();
        let (ahead, behind, gone) = parse_track_counts(parts.next().unwrap_or("").trim());
        tracking.insert(
            name.clone(),
            BranchTracking {
                upstream: (!up_raw.is_empty()).then(|| up_raw.to_string()),
                ahead,
                behind,
                gone,
            },
        );
        local.push(name);
    }

    let remote = remote_out
        .stdout
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && l.contains('/') && !l.ends_with("/HEAD"))
        .map(ToString::to_string)
        .collect();
    Ok((local, remote, tracking))
}

async fn fill_fallback_tracking(
    repo_path: &Path,
    local_branches: &[String],
    remote_branches: &[String],
    remotes: &[String],
    tracking: &mut HashMap<String, BranchTracking>,
) {
    let Some(rem) = remotes.iter().find(|r| *r == "origin").or_else(|| remotes.first()) else {
        return;
    };
    let mut set = JoinSet::new();
    for branch in local_branches {
        let Some(entry) = tracking.get(branch) else { continue };
        if entry.upstream.is_some() {
            continue;
        }
        let candidate = format!("{rem}/{branch}");
        if !remote_branches.iter().any(|rb| rb == &candidate) {
            continue;
        }
        let repo = repo_path.to_path_buf();
        let b_name = branch.clone();
        set.spawn(async move {
            let spec = format!("{b_name}...{candidate}");
            let out = run_git(&repo, &["rev-list", "--left-right", "--count", &spec], LOCAL_TIMEOUT_SECS).await.ok()?;
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
}

pub(crate) async fn list_remotes(repo_path: &Path) -> Vec<String> {
    match run_git(repo_path, &["remote"], LOCAL_TIMEOUT_SECS).await {
        Ok(out) => out
            .stdout
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(ToString::to_string)
            .collect(),
        Err(_) => Vec::new(),
    }
}
