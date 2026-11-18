//! Repository status and branch listing parser.

use crate::{
    error::{CoreError, Result},
    git_cli::{run_git, LOCAL_TIMEOUT_SECS},
};
use serde::{Deserialize, Serialize};
use std::path::Path;

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
    pub changes: Vec<FileChange>,
    pub staged_changes: Vec<FileChange>,
    pub unstaged_changes: Vec<FileChange>,
    pub conflicts: Vec<String>,
    pub local_branches: Vec<String>,
    pub remote_branches: Vec<String>,
    pub remotes: Vec<String>,
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

    let (local_branches, remote_branches) = list_branches(repo_path).await?;
    let remotes = list_remotes(repo_path).await;
    let is_merging = is_merge_in_progress(repo_path).await;
    let is_dirty = !changes.is_empty();

    Ok(RepoStatus {
        branch,
        upstream,
        ahead,
        behind,
        is_dirty,
        is_merging,
        changes,
        staged_changes,
        unstaged_changes,
        conflicts,
        local_branches,
        remote_branches,
        remotes,
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

async fn list_branches(repo_path: &Path) -> Result<(Vec<String>, Vec<String>)> {
    let local_out = run_git(
        repo_path,
        &[
            "for-each-ref",
            "--sort=-committerdate",
            "--format=%(refname:short)",
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

    let local: Vec<String> = local_out
        .stdout
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(ToString::to_string)
        .collect();

    let remote: Vec<String> = remote_out
        .stdout
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && l.contains('/') && !l.ends_with("/HEAD"))
        .map(ToString::to_string)
        .collect();

    Ok((local, remote))
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
