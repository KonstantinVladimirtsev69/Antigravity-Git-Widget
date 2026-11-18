//! Commit graph, commit file details, author stats, and GitHub URL/PR helpers.

use crate::{
    error::{CoreError, Result},
    git_cli::{run_git_raw, LOCAL_TIMEOUT_SECS},
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::Path, process::Stdio};
use tokio::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RefDecoration {
    pub name: String,
    pub kind: String, // "head" | "local" | "remote" | "tag"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphCommit {
    pub hash: String,
    pub short_hash: String,
    pub parents: Vec<String>,
    pub subject: String,
    pub author_name: String,
    pub author_email: String,
    pub date_rel: String,
    pub date_exact: String,
    pub refs: Vec<RefDecoration>,
    pub is_head: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorStat {
    pub name: String,
    pub count: usize,
    pub last_date: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphResponse {
    pub commits: Vec<GraphCommit>,
    pub authors: Vec<AuthorStat>,
    pub github_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitFileStat {
    pub path: String,
    pub status: String,
    pub additions: usize,
    pub deletions: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitDetails {
    pub hash: String,
    pub files: Vec<CommitFileStat>,
    pub total_additions: usize,
    pub total_deletions: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullRequestInfo {
    pub number: u64,
    pub title: String,
    pub author: String,
    pub head_ref: String,
    pub base_ref: String,
    pub url: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrListResponse {
    pub gh_available: bool,
    pub github_url: Option<String>,
    pub prs: Vec<PullRequestInfo>,
}

/// Converts a GitHub SSH or HTTPS remote URL into a web base URL (`https://github.com/owner/repo`).
pub fn remote_to_github_url(remote_url: &str) -> Option<String> {
    let trimmed = remote_url.trim().trim_end_matches('/').trim_end_matches(".git");
    if let Some(rest) = trimmed.strip_prefix("git@github.com:") {
        return Some(format!("https://github.com/{rest}"));
    }
    if let Some(rest) = trimmed.strip_prefix("ssh://git@github.com/") {
        return Some(format!("https://github.com/{rest}"));
    }
    if trimmed.starts_with("https://github.com/") || trimmed.starts_with("http://github.com/") {
        return Some(trimmed.replace("http://", "https://"));
    }
    None
}

pub async fn get_github_url(repo_path: &Path) -> Option<String> {
    let out = run_git_raw(repo_path, &["remote", "get-url", "origin"], LOCAL_TIMEOUT_SECS)
        .await
        .ok()?;
    if out.success {
        if let Some(url) = remote_to_github_url(&out.stdout) {
            return Some(url);
        }
    }
    let list = run_git_raw(repo_path, &["remote"], LOCAL_TIMEOUT_SECS).await.ok()?;
    for name in list.stdout.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if let Ok(u) = run_git_raw(repo_path, &["remote", "get-url", name], LOCAL_TIMEOUT_SECS).await {
            if u.success {
                if let Some(url) = remote_to_github_url(&u.stdout) {
                    return Some(url);
                }
            }
        }
    }
    None
}

/// Loads commit graph with parent hashes, branch/tag decorations, and author statistics.
pub async fn get_commit_graph(
    repo_path: &Path,
    all_branches: bool,
    author_filter: Option<&str>,
    limit: usize,
) -> Result<GraphResponse> {
    let limit_arg = format!("-n{limit}");
    let mut args: Vec<String> = vec![
        "log".into(),
        "--topo-order".into(),
        limit_arg,
        "--date=format:%Y-%m-%d %H:%M".into(),
        "--format=%H%x1f%h%x1f%P%x1f%s%x1f%an%x1f%ae%x1f%cr%x1f%cd%x1f%D".into(),
    ];
    if all_branches {
        args.push("--all".into());
    }
    if let Some(author) = author_filter.filter(|a| !a.trim().is_empty()) {
        args.push(format!("--author={}", author.trim()));
    }

    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let out = run_git_raw(repo_path, &arg_refs, LOCAL_TIMEOUT_SECS).await?;
    let github_url = get_github_url(repo_path).await;

    if !out.success {
        return Ok(GraphResponse {
            commits: Vec::new(),
            authors: Vec::new(),
            github_url,
        });
    }

    let mut commits = Vec::new();
    let mut author_map: HashMap<String, (usize, String)> = HashMap::new();

    for line in out.stdout.lines() {
        let parts: Vec<&str> = line.split('\x1f').collect();
        if parts.len() < 9 {
            continue;
        }
        let parents: Vec<String> = parts[2]
            .split_whitespace()
            .filter(|s| !s.is_empty())
            .map(ToString::to_string)
            .collect();
        let (refs, is_head) = parse_decorations(parts[8]);
        let author_name = parts[4].to_string();
        let date_rel = parts[6].to_string();

        author_map
            .entry(author_name.clone())
            .and_modify(|e| e.0 += 1)
            .or_insert((1, date_rel.clone()));

        commits.push(GraphCommit {
            hash: parts[0].to_string(),
            short_hash: parts[1].to_string(),
            parents,
            subject: parts[3].to_string(),
            author_name,
            author_email: parts[5].to_string(),
            date_rel,
            date_exact: parts[7].to_string(),
            refs,
            is_head,
        });
    }

    let mut authors: Vec<AuthorStat> = author_map
        .into_iter()
        .map(|(name, (count, last_date))| AuthorStat {
            name,
            count,
            last_date,
        })
        .collect();
    authors.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.name.cmp(&b.name)));

    Ok(GraphResponse {
        commits,
        authors,
        github_url,
    })
}

fn parse_decorations(raw: &str) -> (Vec<RefDecoration>, bool) {
    let mut refs = Vec::new();
    let mut is_head = false;
    for item in raw.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        if let Some(branch) = item.strip_prefix("HEAD -> ") {
            is_head = true;
            refs.push(RefDecoration {
                name: branch.trim().to_string(),
                kind: "head".to_string(),
            });
        } else if item == "HEAD" {
            is_head = true;
        } else if let Some(tag) = item.strip_prefix("tag: ") {
            refs.push(RefDecoration {
                name: tag.trim().to_string(),
                kind: "tag".to_string(),
            });
        } else if item.ends_with("/HEAD") {
            continue;
        } else if item.contains('/') {
            refs.push(RefDecoration {
                name: item.to_string(),
                kind: "remote".to_string(),
            });
        } else {
            refs.push(RefDecoration {
                name: item.to_string(),
                kind: "local".to_string(),
            });
        }
    }
    (refs, is_head)
}

/// Returns per-file change statistics for a single commit.
pub async fn get_commit_details(repo_path: &Path, commit_hash: &str) -> Result<CommitDetails> {
    if commit_hash.trim().is_empty() {
        return Err(CoreError::InvalidInput("commit hash cannot be empty".into()));
    }
    let hash = commit_hash.trim();
    let status_out = run_git_raw(
        repo_path,
        &["show", "--name-status", "--format=", hash],
        LOCAL_TIMEOUT_SECS,
    )
    .await?;
    let numstat_out = run_git_raw(
        repo_path,
        &["show", "--numstat", "--format=", hash],
        LOCAL_TIMEOUT_SECS,
    )
    .await?;

    let mut status_map: HashMap<String, String> = HashMap::new();
    for line in status_out.stdout.lines().filter(|l| !l.trim().is_empty()) {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 2 {
            let code = parts[0].chars().next().unwrap_or('M').to_string();
            let path = parts.last().unwrap().to_string();
            status_map.insert(path, code);
        }
    }

    let mut files = Vec::new();
    let mut total_additions = 0usize;
    let mut total_deletions = 0usize;

    for line in numstat_out.stdout.lines().filter(|l| !l.trim().is_empty()) {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 3 {
            continue;
        }
        let additions = parts[0].parse::<usize>().unwrap_or(0);
        let deletions = parts[1].parse::<usize>().unwrap_or(0);
        let path = parts[2].to_string();
        let status = status_map.get(&path).cloned().unwrap_or_else(|| "M".into());
        total_additions += additions;
        total_deletions += deletions;
        files.push(CommitFileStat {
            path,
            status,
            additions,
            deletions,
        });
    }

    Ok(CommitDetails {
        hash: hash.to_string(),
        files,
        total_additions,
        total_deletions,
    })
}

/// Lists open GitHub PRs via `gh` CLI when available, gracefully degrading otherwise.
pub async fn list_github_prs(repo_path: &Path) -> PrListResponse {
    let github_url = get_github_url(repo_path).await;
    let mut cmd = Command::new("gh");
    cmd.current_dir(repo_path)
        .args([
            "pr",
            "list",
            "--limit",
            "15",
            "--json",
            "number,title,author,headRefName,baseRefName,url,updatedAt",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);

    let Ok(out) = cmd.output().await else {
        return PrListResponse {
            gh_available: false,
            github_url,
            prs: Vec::new(),
        };
    };
    if !out.status.success() {
        return PrListResponse {
            gh_available: false,
            github_url,
            prs: Vec::new(),
        };
    }

    let Ok(val) = serde_json::from_slice::<serde_json::Value>(&out.stdout) else {
        return PrListResponse {
            gh_available: true,
            github_url,
            prs: Vec::new(),
        };
    };

    let prs = val
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    Some(PullRequestInfo {
                        number: item.get("number")?.as_u64()?,
                        title: item.get("title")?.as_str()?.to_string(),
                        author: item
                            .get("author")
                            .and_then(|a| a.get("login"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown")
                            .to_string(),
                        head_ref: item.get("headRefName")?.as_str()?.to_string(),
                        base_ref: item.get("baseRefName")?.as_str()?.to_string(),
                        url: item.get("url")?.as_str()?.to_string(),
                        updated_at: item
                            .get("updatedAt")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    PrListResponse {
        gh_available: true,
        github_url,
        prs,
    }
}
