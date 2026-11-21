//! GitHub Pull Request listing via REST API (using OAuth/PAT token) with `gh` CLI fallback.

use crate::{github_oauth::curl_binary, graph::get_github_url};
use serde::{Deserialize, Serialize};
use std::{path::Path, process::Stdio};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

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

pub fn extract_github_owner_repo(github_url: &str) -> Option<(String, String)> {
    let rest = github_url
        .trim()
        .trim_end_matches('/')
        .strip_prefix("https://github.com/")?;
    let (owner, repo) = rest.split_once('/')?;
    if owner.is_empty() || repo.is_empty() || repo.contains('/') {
        return None;
    }
    Some((owner.to_string(), repo.to_string()))
}

pub fn parse_github_rest_prs_json(raw: &str) -> Option<Vec<PullRequestInfo>> {
    let val: serde_json::Value = serde_json::from_str(raw).ok()?;
    let arr = val.as_array()?;
    let prs = arr
        .iter()
        .filter_map(|item| {
            Some(PullRequestInfo {
                number: item.get("number")?.as_u64()?,
                title: item.get("title")?.as_str()?.to_string(),
                author: item
                    .get("user")
                    .and_then(|u| u.get("login"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string(),
                head_ref: item.get("head")?.get("ref")?.as_str()?.to_string(),
                base_ref: item.get("base")?.get("ref")?.as_str()?.to_string(),
                url: item.get("html_url")?.as_str()?.to_string(),
                updated_at: item.get("updated_at").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            })
        })
        .collect();
    Some(prs)
}

async fn fetch_prs_via_rest(owner: &str, repo: &str, token: Option<&str>) -> Option<Vec<PullRequestInfo>> {
    let url = format!("https://api.github.com/repos/{owner}/{repo}/pulls?state=open&per_page=15");
    let mut cmd = Command::new(curl_binary());
    cmd.args([
        "-sS",
        "-H",
        "Accept: application/vnd.github+json",
        "-H",
        "User-Agent: antigravity-git-widget",
    ]);
    let cfg_stdin = token.map(|t| {
        let sanitized = t.trim().replace(['\r', '\n', '"'], "");
        format!("header = \"Authorization: Bearer {sanitized}\"\n")
    });
    if cfg_stdin.is_some() {
        cmd.args(["-K", "-"]).stdin(Stdio::piped());
    } else {
        cmd.stdin(Stdio::null());
    }
    cmd.arg(&url).stdout(Stdio::piped()).stderr(Stdio::null());
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);

    let mut child = cmd.spawn().ok()?;
    if let (Some(cfg), Some(mut stdin)) = (cfg_stdin, child.stdin.take()) {
        let _ = stdin.write_all(cfg.as_bytes()).await;
    }
    let out = child.wait_with_output().await.ok()?;
    if !out.status.success() {
        return None;
    }
    parse_github_rest_prs_json(&String::from_utf8_lossy(&out.stdout))
}

async fn fetch_prs_via_gh_cli(repo_path: &Path) -> Option<Vec<PullRequestInfo>> {
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

    let out = cmd.output().await.ok()?;
    if !out.status.success() {
        return None;
    }
    let val = serde_json::from_slice::<serde_json::Value>(&out.stdout).ok()?;
    let prs = val
        .as_array()?
        .iter()
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
                updated_at: item.get("updatedAt").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            })
        })
        .collect();
    Some(prs)
}

/// Lists open GitHub PRs via GitHub REST API (OAuth/PAT or public) and falls back to `gh` CLI.
pub async fn list_github_prs_with_token(repo_path: &Path, token: Option<&str>) -> PrListResponse {
    let github_url = get_github_url(repo_path).await;
    let env_tok = std::env::var("GH_TOKEN")
        .ok()
        .or_else(|| std::env::var("GITHUB_TOKEN").ok());
    let effective_tok = token
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or_else(|| env_tok.as_deref().map(str::trim).filter(|s| !s.is_empty()));

    if let Some((owner, repo)) = github_url.as_deref().and_then(extract_github_owner_repo) {
        if let Some(prs) = fetch_prs_via_rest(&owner, &repo, effective_tok).await {
            return PrListResponse {
                gh_available: true,
                github_url,
                prs,
            };
        }
    }

    if let Some(prs) = fetch_prs_via_gh_cli(repo_path).await {
        return PrListResponse {
            gh_available: true,
            github_url,
            prs,
        };
    }

    PrListResponse {
        gh_available: false,
        github_url,
        prs: Vec::new(),
    }
}

pub async fn list_github_prs(repo_path: &Path) -> PrListResponse {
    list_github_prs_with_token(repo_path, None).await
}
