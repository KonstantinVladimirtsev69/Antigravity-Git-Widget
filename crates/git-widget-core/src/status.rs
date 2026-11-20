//! Repository status and branch listing parser.

use crate::{
    error::{CoreError, Result},
    git_cli::{run_git, LOCAL_TIMEOUT_SECS},
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::Path};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct BranchTracking {
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub gone: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileChange {
    pub path: String,
    pub code: String,
    pub is_conflicted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoStatus {
    pub branch: String,
    pub upstream: Option<String>,
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

/// Reads full status (`git status --porcelain=v2 --branch`) and branch list.
pub async fn get_status(repo_path: &Path) -> Result<RepoStatus> {
    if !repo_path.join(".git").exists() {
        return Err(CoreError::NotAGitRepo(repo_path.to_string_lossy().to_string()));
    }

    let status_out = run_git(
        repo_path,
        &["status", "--porcelain=v2", "--branch"],
        LOCAL_TIMEOUT_SECS,
    )
    .await?;

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

    let remotes = list_remotes(repo_path).await;
    let (local_branches, remote_branches, mut branch_tracking) = list_branches(repo_path).await?;
    fill_fallback_tracking(repo_path, &local_branches, &remote_branches, &remotes, &mut branch_tracking).await;
    let is_merging = is_merge_in_progress(repo_path).await;
    let is_rebasing = is_rebase_in_progress(repo_path).await;
    let is_dirty = !changes.is_empty();

    Ok(RepoStatus {
        branch,
        upstream,
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

fn parse_porcelain_entry(line: &str) -> Option<FileChange> {
    if let Some(rest) = line.strip_prefix("u ") {
        let mut parts = rest.splitn(10, ' ');
        let xy = parts.next()?.to_string();
        let path = parts.nth(8)?.to_string();
        return Some(FileChange {
            path,
            code: xy,
            is_conflicted: true,
        });
    }
    if let Some(rest) = line.strip_prefix("1 ") {
        let mut parts = rest.splitn(8, ' ');
        let xy = parts.next()?.to_string();
        let path = parts.nth(6)?.to_string();
        return Some(FileChange {
            path,
            code: xy,
            is_conflicted: false,
        });
    }
    if let Some(rest) = line.strip_prefix("2 ") {
        let mut parts = rest.splitn(9, ' ');
        let xy = parts.next()?.to_string();
        let path_tab = parts.nth(7)?;
        let path = path_tab.split('\t').next().unwrap_or(path_tab).to_string();
        return Some(FileChange {
            path,
            code: xy,
            is_conflicted: false,
        });
    }
    if let Some(path) = line.strip_prefix("? ") {
        return Some(FileChange {
            path: path.to_string(),
            code: "??".to_string(),
            is_conflicted: false,
        });
    }
    None
}

fn parse_track_counts(track: &str) -> (u32, u32, bool) {
    let mut ahead = 0u32;
    let mut behind = 0u32;
    let mut gone = false;
    for seg in track.split(',') {
        let s = seg.trim();
        if s == "gone" {
            gone = true;
        } else if let Some(n) = s.strip_prefix("ahead ") {
            ahead = n.trim().parse().unwrap_or(0);
        } else if let Some(n) = s.strip_prefix("behind ") {
            behind = n.trim().parse().unwrap_or(0);
        }
    }
    (ahead, behind, gone)
}

async fn list_branches(
    repo_path: &Path,
) -> Result<(Vec<String>, Vec<String>, HashMap<String, BranchTracking>)> {
    let local_out = run_git(
        repo_path,
        &[
            "for-each-ref",
            "--sort=-committerdate",
            "--format=%(refname:short)\t%(upstream:short)\t%(upstream:track,nobracket)",
            "refs/heads",
        ],
        LOCAL_TIMEOUT_SECS,
    )
    .await?;

    let remote_out = run_git(
        repo_path,
        &[
            "for-each-ref",
            "--sort=-committerdate",
            "--format=%(refname:short)",
            "refs/remotes",
        ],
        LOCAL_TIMEOUT_SECS,
    )
    .await?;

    let mut local = Vec::new();
    let mut tracking = HashMap::new();
    for line in local_out.stdout.lines() {
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            continue;
        }
        let mut parts = trimmed.splitn(3, '\t');
        let name = parts.next().unwrap_or("").trim().to_string();
        if name.is_empty() {
            continue;
        }
        let up_raw = parts.next().unwrap_or("").trim();
        let track_raw = parts.next().unwrap_or("").trim();
        let (ahead, behind, gone) = parse_track_counts(track_raw);
        tracking.insert(
            name.clone(),
            BranchTracking {
                upstream: if up_raw.is_empty() {
                    None
                } else {
                    Some(up_raw.to_string())
                },
                ahead,
                behind,
                gone,
            },
        );
        local.push(name);
    }

    let remote: Vec<String> = remote_out
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
    let primary_remote = if remotes.iter().any(|r| r == "origin") {
        Some("origin")
    } else {
        remotes.first().map(String::as_str)
    };
    let Some(rem) = primary_remote else {
        return;
    };
    for branch in local_branches {
        let Some(entry) = tracking.get_mut(branch) else {
            continue;
        };
        if entry.upstream.is_some() {
            continue;
        }
        let candidate = format!("{rem}/{branch}");
        if !remote_branches.iter().any(|rb| rb == &candidate) {
            continue;
        }
        let spec = format!("{branch}...{candidate}");
        if let Ok(out) = run_git(
            repo_path,
            &["rev-list", "--left-right", "--count", &spec],
            LOCAL_TIMEOUT_SECS,
        )
        .await
        {
            let mut nums = out.stdout.split_whitespace();
            if let (Some(a), Some(b)) = (nums.next(), nums.next()) {
                entry.ahead = a.parse().unwrap_or(0);
                entry.behind = b.parse().unwrap_or(0);
            }
        }
    }
}

async fn list_remotes(repo_path: &Path) -> Vec<String> {
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

async fn is_merge_in_progress(repo_path: &Path) -> bool {
    run_git(
        repo_path,
        &["rev-parse", "--verify", "--quiet", "MERGE_HEAD"],
        LOCAL_TIMEOUT_SECS,
    )
    .await
    .is_ok()
}

async fn is_rebase_in_progress(repo_path: &Path) -> bool {
    for marker in ["rebase-merge", "rebase-apply"] {
        if let Ok(out) = run_git(
            repo_path,
            &["rev-parse", "--git-path", marker],
            LOCAL_TIMEOUT_SECS,
        )
        .await
        {
            let p = out.stdout.trim();
            if !p.is_empty() {
                let candidate = Path::new(p);
                let resolved = if candidate.is_absolute() {
                    candidate.to_path_buf()
                } else {
                    repo_path.join(candidate)
                };
                if resolved.exists() {
                    return true;
                }
            }
        }
    }
    false
}
