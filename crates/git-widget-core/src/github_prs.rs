//! GitHub CLI (`gh`) pull request listing helper.

use crate::graph::get_github_url;
use serde::{Deserialize, Serialize};
use std::{path::Path, process::Stdio};
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

/// Lists open GitHub PRs via `gh` CLI when available, gracefully degrading otherwise.
pub async fn list_github_prs(repo_path: &Path) -> PrListResponse {
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

    let (github_url, out_res) = tokio::join!(get_github_url(repo_path), cmd.output());
    let Ok(out) = out_res else {
        return PrListResponse { gh_available: false, github_url, prs: Vec::new() };
    };
    if !out.status.success() {
        return PrListResponse { gh_available: false, github_url, prs: Vec::new() };
    }
    let Ok(val) = serde_json::from_slice::<serde_json::Value>(&out.stdout) else {
        return PrListResponse { gh_available: true, github_url, prs: Vec::new() };
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
                        updated_at: item.get("updatedAt").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    PrListResponse { gh_available: true, github_url, prs }
}
