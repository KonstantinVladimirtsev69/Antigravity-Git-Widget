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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TokenSource {
    ExplicitOrSaved,
    Env,
    Gcm,
}

fn parse_credential_password(raw: &str) -> Option<String> {
    raw.lines()
        .filter_map(|line| line.strip_prefix("password="))
        .map(str::trim)
        .find(|pw| !pw.is_empty())
        .map(ToString::to_string)
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

fn format_auth_error(source: TokenSource, api_msg: &str) -> String {
    match source {
        TokenSource::Gcm => format!(
            "GitHub token in Windows Credential Manager is expired or invalid ({api_msg}). \
             Paste a GitHub Personal Access Token (repo scope) in the token field above (saved once), \
             or install GitHub CLI (`gh auth login`)."
        ),
        TokenSource::Env => format!(
            "GitHub token from GH_TOKEN/GITHUB_TOKEN is invalid or expired ({api_msg})."
        ),
        TokenSource::ExplicitOrSaved => format!(
            "Provided or saved GitHub Personal Access Token is invalid or expired ({api_msg}). \
             Enter a valid PAT with `repo` scope."
        ),
    }
}

/// Attempts to retrieve a GitHub HTTPS token from env vars or Git Credential Manager.
async fn resolve_fallback_token() -> Option<(String, TokenSource)> {
    for var in ["GH_TOKEN", "GITHUB_TOKEN"] {
        if let Ok(val) = std::env::var(var) {
            let trimmed = val.trim();
            if !trimmed.is_empty() {
                return Some((trimmed.to_string(), TokenSource::Env));
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
                return Some((tok, TokenSource::Gcm));
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

    let explicit = req.token.as_ref().map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
    let resolved = match explicit {
        Some(t) => Some((t, TokenSource::ExplicitOrSaved)),
        None => resolve_fallback_token().await,
    };

    let remote_url = match resolved {
        Some((tok, source)) => {
            match create_via_curl_api(name, &req.description, req.private, req.use_ssh, &tok, source).await {
                Ok(url) => url,
                Err(api_err) if source != TokenSource::ExplicitOrSaved => {
                    match create_via_gh_cli(repo_path, name, &req.description, req.private).await {
                        Ok(url) => url,
                        Err(_) => return Err(api_err),
                    }
                }
                Err(api_err) => return Err(api_err),
            }
        }
        None => create_via_gh_cli(repo_path, name, &req.description, req.private).await?,
    };

    let has_origin = run_git_raw(repo_path, &["remote", "get-url", "origin"], LOCAL_TIMEOUT_SECS)
        .await
        .map(|o| o.success)
        .unwrap_or(false);
    let remote_args = if has_origin {
        ["remote", "set-url", "origin", &remote_url]
    } else {
        ["remote", "add", "origin", &remote_url]
    };
    run_git(repo_path, &remote_args, LOCAL_TIMEOUT_SECS).await?;

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
    source: TokenSource,
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
    let msg = val.get("message").and_then(|v| v.as_str()).unwrap_or("GitHub API error");
    if msg.eq_ignore_ascii_case("Bad credentials") || msg.contains("Requires authentication") {
        return Err(CoreError::InvalidInput(format_auth_error(source, msg)));
    }
    let details = val.get("errors").map(|e| e.to_string()).unwrap_or_default();
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
            "Provide a GitHub Personal Access Token (or install `gh` CLI) to create a repository.".into(),
        ));
    };
    if !out.status.success() {
        return Err(CoreError::InvalidInput(format!(
            "gh repo create failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    let url = String::from_utf8_lossy(&out.stdout).trim().to_string();
    Ok(if url.ends_with(".git") { url } else { format!("{url}.git") })
}

#[cfg(test)]
mod tests {
    use super::{
        format_auth_error, gcm_credential_arg_sets, parse_credential_password, TokenSource,
    };

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

    #[test]
    fn bad_credentials_error_explains_expired_gcm_vs_pat() {
        let gcm_msg = format_auth_error(TokenSource::Gcm, "Bad credentials");
        assert!(gcm_msg.contains("Windows Credential Manager"));
        assert!(gcm_msg.contains("Personal Access Token"));

        let pat_msg = format_auth_error(TokenSource::ExplicitOrSaved, "Bad credentials");
        assert!(pat_msg.contains("invalid or expired"));
    }
}
