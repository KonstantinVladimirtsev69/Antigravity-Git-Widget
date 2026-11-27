//! Commit graph, commit file details, author stats, and GitHub URL/PR helpers.

pub use crate::github_prs::{list_github_prs, PrListResponse, PullRequestInfo};
use crate::{
    error::{CoreError, Result},
    git_cli::{run_git_raw, LOCAL_TIMEOUT_SECS},
    remote_focus::{scoped_graph_remote_ref, split_remote_branch},
    status::list_remotes,
    validation::{validate_git_arg, validate_ref_name},
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::Path};

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

/// Resolves the GitHub web base URL for a specific `remote` (or `origin` then any remote when `None`).
pub async fn get_github_url_for_remote(repo_path: &Path, remote: Option<&str>) -> Option<String> {
    if let Some(r) = remote.map(str::trim).filter(|s| !s.is_empty()) {
        let valid = validate_ref_name(r, "remote").ok()?;
        let out = run_git_raw(repo_path, &["remote", "get-url", valid], LOCAL_TIMEOUT_SECS)
            .await
            .ok()?;
        return if out.success { remote_to_github_url(&out.stdout) } else { None };
    }
    let out = run_git_raw(repo_path, &["remote", "get-url", "origin"], LOCAL_TIMEOUT_SECS).await.ok()?;
    if out.success {
        if let Some(url) = remote_to_github_url(&out.stdout) {
            return Some(url);
        }
    }
    for name in list_remotes(repo_path).await {
        if let Ok(u) = run_git_raw(repo_path, &["remote", "get-url", &name], LOCAL_TIMEOUT_SECS).await {
            if u.success {
                if let Some(url) = remote_to_github_url(&u.stdout) {
                    return Some(url);
                }
            }
        }
    }
    None
}

pub async fn get_github_url(repo_path: &Path) -> Option<String> {
    get_github_url_for_remote(repo_path, None).await
}

/// Loads commit graph with parent hashes, decorations, and full author statistics.
pub async fn get_commit_graph(
    repo_path: &Path,
    all_branches: bool,
    author_filter: Option<&str>,
    limit: usize,
) -> Result<GraphResponse> {
    get_commit_graph_for_remote(repo_path, all_branches, author_filter, None, limit).await
}

/// Loads commit graph optionally scoped to a single `remote` without foreign remote decorations.
pub async fn get_commit_graph_for_remote(
    repo_path: &Path,
    all_branches: bool,
    author_filter: Option<&str>,
    remote: Option<&str>,
    limit: usize,
) -> Result<GraphResponse> {
    let author_exact = match author_filter.map(str::trim).filter(|a| !a.is_empty()) {
        Some(a) => Some(validate_git_arg(a, "author filter")?.to_string()),
        None => None,
    };
    let scoped_remote = match remote.map(str::trim).filter(|r| !r.is_empty()) {
        Some(r) => Some(validate_ref_name(r, "remote")?),
        None => None,
    };

    let fetch_limit = if author_exact.is_some() { limit.max(200) } else { limit };
    let limit_arg = format!("-n{fetch_limit}");
    let remotes_flag = scoped_remote.map(|r| format!("--remotes={r}"));
    let single_remote_ref = if all_branches {
        None
    } else {
        scoped_graph_remote_ref(repo_path, scoped_remote).await
    };

    let mut args: Vec<&str> = vec![
        "log",
        "--topo-order",
        &limit_arg,
        "--date=format:%Y-%m-%d %H:%M",
        "--format=%H%x1f%h%x1f%P%x1f%s%x1f%an%x1f%ae%x1f%cr%x1f%cd%x1f%D",
    ];
    if all_branches {
        // Avoid `--all` which pulls in internal namespaces like `refs/t3/orchestration-v2/checkpoints/*`.
        args.extend(["--branches", "--tags", "HEAD"]);
        args.push(remotes_flag.as_deref().unwrap_or("--remotes"));
    } else if let Some(ref rem_ref) = single_remote_ref {
        args.extend(["HEAD", rem_ref.as_str()]);
    }

    let (log_res, github_url, remotes) = tokio::join!(
        run_git_raw(repo_path, &args, LOCAL_TIMEOUT_SECS),
        get_github_url_for_remote(repo_path, scoped_remote),
        list_remotes(repo_path),
    );
    let out = log_res?;

    if !out.success {
        return Ok(GraphResponse { commits: Vec::new(), authors: Vec::new(), github_url });
    }

    let mut all_commits = Vec::new();
    let mut author_map: HashMap<String, (usize, String)> = HashMap::new();

    for line in out.stdout.lines() {
        let parts: Vec<&str> = line.split('\x1f').collect();
        if parts.len() < 9 {
            continue;
        }
        let parents = parts[2].split_whitespace().filter(|s| !s.is_empty()).map(ToString::to_string).collect();
        let (refs, is_head) = parse_decorations(parts[8], &remotes, scoped_remote);
        let author_name = parts[4].to_string();
        let date_rel = parts[6].to_string();

        author_map
            .entry(author_name.clone())
            .and_modify(|e| e.0 += 1)
            .or_insert((1, date_rel.clone()));

        all_commits.push(GraphCommit {
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
        .map(|(name, (count, last_date))| AuthorStat { name, count, last_date })
        .collect();
    authors.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.name.cmp(&b.name)));

    let commits: Vec<GraphCommit> = match author_exact {
        Some(ref target) => all_commits.into_iter().filter(|c| &c.author_name == target).take(limit).collect(),
        None => all_commits.into_iter().take(limit).collect(),
    };

    Ok(GraphResponse { commits, authors, github_url })
}

fn parse_decorations(raw: &str, remotes: &[String], scoped_remote: Option<&str>) -> (Vec<RefDecoration>, bool) {
    let mut refs = Vec::new();
    let mut is_head = false;
    for item in raw.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        if let Some(branch) = item.strip_prefix("HEAD -> ") {
            is_head = true;
            refs.push(RefDecoration { name: branch.trim().to_string(), kind: "head".to_string() });
        } else if item == "HEAD" {
            is_head = true;
        } else if let Some(tag) = item.strip_prefix("tag: ") {
            refs.push(RefDecoration { name: tag.trim().to_string(), kind: "tag".to_string() });
        } else if item.ends_with("/HEAD") {
            continue;
        } else if let Some((rem, _)) = split_remote_branch(item, remotes) {
            if scoped_remote.is_some_and(|target| rem != target) {
                continue;
            }
            refs.push(RefDecoration { name: item.to_string(), kind: "remote".to_string() });
        } else {
            refs.push(RefDecoration { name: item.to_string(), kind: "local".to_string() });
        }
    }
    (refs, is_head)
}

/// Returns per-file change statistics for a single commit.
pub async fn get_commit_details(repo_path: &Path, commit_hash: &str) -> Result<CommitDetails> {
    let hash = validate_ref_name(commit_hash, "commit hash")?;
    let args_status = ["show", "--name-status", "--format=", hash];
    let args_numstat = ["show", "--numstat", "--format=", hash];
    let (status_res, numstat_res) = tokio::join!(
        run_git_raw(repo_path, &args_status, LOCAL_TIMEOUT_SECS),
        run_git_raw(repo_path, &args_numstat, LOCAL_TIMEOUT_SECS),
    );
    let status_out = status_res?;
    let numstat_out = numstat_res?;
    if !status_out.success && !numstat_out.success {
        return Err(CoreError::GitFailed {
            code: status_out.code,
            stderr: status_out.stderr.trim().to_string(),
        });
    }

    let mut status_map: HashMap<String, String> = HashMap::new();
    for line in status_out.stdout.lines().filter(|l| !l.trim().is_empty()) {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 2 {
            let code = parts[0].chars().next().unwrap_or('M').to_string();
            status_map.insert(parts.last().unwrap().to_string(), code);
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
        files.push(CommitFileStat { path, status, additions, deletions });
    }

    Ok(CommitDetails { hash: hash.to_string(), files, total_additions, total_deletions })
}
