//! GitHub repository creation and publishing (`Set Remote -> Create on GitHub`).

use crate::{
    error::{CoreError, Result},
    git_cli::{run_git, run_git_raw, LOCAL_TIMEOUT_SECS},
    ops::{publish_branch, OpResult},
};
use serde::{Deserialize, Serialize};
use std::{path::Path, process::Stdio};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateGithubRepoReq {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub private: bool,
    #[serde(default)]
    pub use_ssh: bool,
    pub token: Option<String>,
}

fn parse_credential_password(raw: &str) -> Option<String> {
    for line in raw.lines() {
        if let Some(pw) = line.strip_prefix("password=") {
            let trimmed = pw.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

fn gcm_credential_arg_sets() -> Vec<Vec<&'static str>> {
    vec![
        vec!["credential", "fill"],
        vec![
            "-c",
            "credential.helper=manager",
            "-c",
            "credential.interactive=never",
            "credential",
            "fill",
        ],
    ]
}

/// Attempts to retrieve a GitHub HTTPS token from env vars or Git Credential Manager.
async fn token_from_gcm() -> Option<String> {
    for var in ["GH_TOKEN", "GITHUB_TOKEN"] {
        if let Ok(val) = std::env::var(var) {
            let trimmed = val.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }

    for args in gcm_credential_arg_sets() {
        let mut cmd = Command::new("git");
        cmd.args(&args)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GCM_INTERACTIVE", "never")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());

        #[cfg(windows)]
        cmd.creation_flags(0x0800_0000);

        let Ok(mut child) = cmd.spawn() else { continue };
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(b"protocol=https\nhost=github.com\n\n").await;
        }
        let Ok(out) = child.wait_with_output().await else { continue };
        if out.status.success() {
            if let Some(tok) = parse_credential_password(&String::from_utf8_lossy(&out.stdout)) {
                return Some(tok);
            }
        }
    }
    None
}

/// Creates a GitHub repository via REST API (using system `curl.exe`) or `gh` CLI,
/// binds it as `origin`, and publishes the current branch.
pub async fn create_github_repo(repo_path: &Path, req: &CreateGithubRepoReq) -> Result<OpResult> {
    let name = req.name.trim();
    if name.is_empty() || name.contains(' ') {
        return Err(CoreError::InvalidInput(
            "Repository name cannot be empty or contain spaces".into(),
        ));
    }

    let token = req
        .token
        .as_ref()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty());
    let resolved_token = match token {
        Some(t) => Some(t),
        None => token_from_gcm().await,
    };

    let remote_url = if let Some(tok) = resolved_token {
        create_via_curl_api(name, &req.description, req.private, req.use_ssh, &tok).await?
    } else {
        create_via_gh_cli(repo_path, name, &req.description, req.private).await?
    };

    // Configure `origin` remote (add or set-url if it already exists)
    let has_origin = run_git_raw(repo_path, &["remote", "get-url", "origin"], LOCAL_TIMEOUT_SECS)
        .await
        .map(|o| o.success)
        .unwrap_or(false);
    if has_origin {
        run_git(
            repo_path,
            &["remote", "set-url", "origin", &remote_url],
            LOCAL_TIMEOUT_SECS,
        )
        .await?;
    } else {
        run_git(
            repo_path,
            &["remote", "add", "origin", &remote_url],
            LOCAL_TIMEOUT_SECS,
        )
        .await?;
    }

    let pub_res = publish_branch(repo_path, Some("origin")).await?;
    Ok(OpResult {
        success: true,
        message: format!("Created on GitHub ({remote_url}) & published!\n{}", pub_res.message),
        has_conflicts: false,
    })
}

async fn create_via_curl_api(
    name: &str,
    description: &str,
    private: bool,
    use_ssh: bool,
    token: &str,
) -> Result<String> {
    let body = serde_json::json!({
        "name": name,
        "description": description,
        "private": private,
        "auto_init": false
    })
    .to_string();

    let sanitized = token.replace(['\r', '\n', '"'], "");
    let cfg_stdin = format!("header = \"Authorization: Bearer {sanitized}\"\n");
    let mut cmd = Command::new("curl.exe");
    cmd.args([
        "-sS",
        "-K",
        "-",
        "-X",
        "POST",
        "https://api.github.com/user/repos",
        "-H",
        "Accept: application/vnd.github+json",
        "-H",
        "User-Agent: antigravity-git-widget",
        "-d",
        &body,
    ])
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());

    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);

    let mut child = cmd.spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(cfg_stdin.as_bytes()).await?;
    }
    let out = child.wait_with_output().await?;
    if !out.status.success() {
        return Err(CoreError::GitFailed {
            code: out.status.code().unwrap_or(-1),
            stderr: String::from_utf8_lossy(&out.stderr).trim().to_string(),
        });
    }

    let val: serde_json::Value = serde_json::from_slice(&out.stdout)?;
    let url_key = if use_ssh { "ssh_url" } else { "clone_url" };
    if let Some(url) = val.get(url_key).and_then(|v| v.as_str()) {
        return Ok(url.to_string());
    }
    let msg = val
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("GitHub API error");
    let details = val
        .get("errors")
        .map(|e| e.to_string())
        .unwrap_or_default();
    Err(CoreError::InvalidInput(format!("GitHub API: {msg} {details}")))
}

async fn create_via_gh_cli(
    repo_path: &Path,
    name: &str,
    description: &str,
    private: bool,
) -> Result<String> {
    let vis = if private { "--private" } else { "--public" };
    let mut cmd = Command::new("gh");
    cmd.current_dir(repo_path)
        .args(["repo", "create", name, vis, "--description", description])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);

    let Ok(out) = cmd.output().await else {
        return Err(CoreError::InvalidInput(
            "Provide a GitHub Personal Access Token (or install `gh` CLI) to create a repository."
                .into(),
        ));
    };
    if !out.status.success() {
        return Err(CoreError::InvalidInput(format!(
            "gh repo create failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    let url = String::from_utf8_lossy(&out.stdout).trim().to_string();
    Ok(if url.ends_with(".git") {
        url
    } else {
        format!("{url}.git")
    })
}

#[cfg(test)]
mod tests {
    use super::{gcm_credential_arg_sets, parse_credential_password};

    #[test]
    fn extracts_password_from_git_credential_fill_output() {
        let raw = "protocol=https\nhost=github.com\nusername=test\npassword=gho_secret_123\n";
        assert_eq!(parse_credential_password(raw).as_deref(), Some("gho_secret_123"));
        assert_eq!(parse_credential_password("protocol=https\npassword=\n"), None);
    }

    #[test]
    fn gcm_attempts_include_manager_fallback_for_empty_helper_override() {
        let attempts = gcm_credential_arg_sets();
        assert_eq!(attempts[0], vec!["credential", "fill"]);
        assert!(attempts.iter().any(|a| a.contains(&"credential.helper=manager")));
    }
}
