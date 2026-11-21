//! GitHub OAuth Device Flow (RFC 8628) for zero-PAT browser login.

use crate::error::{CoreError, Result};
use serde::{Deserialize, Serialize};
use std::process::Stdio;
use tokio::process::Command;

pub const DEFAULT_GITHUB_OAUTH_CLIENT_ID: &str = match option_env!("GITHUB_OAUTH_CLIENT_ID") {
    Some(id) if !id.is_empty() => id,
    _ => "Ov23lig6JmbXKRt6zC2D",
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeviceFlowStart {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum DevicePollResult {
    Pending { interval: Option<u64> },
    Authorized { access_token: String },
    Expired,
    Denied,
    Error { message: String },
}

pub(crate) fn curl_binary() -> &'static str {
    if cfg!(windows) {
        "curl.exe"
    } else {
        "curl"
    }
}

/// Resolves the GitHub OAuth Client ID with priority: config override -> runtime env -> build default.
pub fn resolve_oauth_client_id(config_override: Option<&str>, runtime_env: Option<&str>) -> String {
    if let Some(id) = config_override.map(str::trim).filter(|s| !s.is_empty()) {
        return id.to_string();
    }
    if let Some(id) = runtime_env.map(str::trim).filter(|s| !s.is_empty()) {
        return id.to_string();
    }
    DEFAULT_GITHUB_OAUTH_CLIENT_ID.to_string()
}

pub fn parse_device_start_json(raw: &str) -> Result<DeviceFlowStart> {
    let val: serde_json::Value = serde_json::from_str(raw)?;
    if let Some(err) = val.get("error").and_then(|v| v.as_str()) {
        let desc = val
            .get("error_description")
            .and_then(|v| v.as_str())
            .unwrap_or(err);
        return Err(CoreError::InvalidInput(format!("GitHub OAuth: {desc}")));
    }
    let get_str = |k: &str| -> Result<String> {
        val.get(k)
            .and_then(|v| v.as_str())
            .map(ToString::to_string)
            .ok_or_else(|| CoreError::InvalidInput(format!("missing '{k}' in GitHub OAuth response")))
    };
    Ok(DeviceFlowStart {
        device_code: get_str("device_code")?,
        user_code: get_str("user_code")?,
        verification_uri: get_str("verification_uri")?,
        expires_in: val.get("expires_in").and_then(|v| v.as_u64()).unwrap_or(900),
        interval: val.get("interval").and_then(|v| v.as_u64()).unwrap_or(5).max(1),
    })
}

pub fn parse_device_poll_json(raw: &str) -> Result<DevicePollResult> {
    let val: serde_json::Value = serde_json::from_str(raw)?;
    if let Some(tok) = val.get("access_token").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
        return Ok(DevicePollResult::Authorized {
            access_token: tok.to_string(),
        });
    }
    let err = val.get("error").and_then(|v| v.as_str()).unwrap_or("unknown_error");
    match err {
        "authorization_pending" => Ok(DevicePollResult::Pending { interval: None }),
        "slow_down" => Ok(DevicePollResult::Pending {
            interval: val.get("interval").and_then(|v| v.as_u64()),
        }),
        "expired_token" => Ok(DevicePollResult::Expired),
        "access_denied" => Ok(DevicePollResult::Denied),
        _ => {
            let desc = val
                .get("error_description")
                .and_then(|v| v.as_str())
                .unwrap_or(err);
            Ok(DevicePollResult::Error {
                message: desc.to_string(),
            })
        }
    }
}

async fn post_github_oauth(url: &str, body_json: &str) -> Result<String> {
    let mut cmd = Command::new(curl_binary());
    cmd.args([
        "-sS",
        "-X",
        "POST",
        url,
        "-H",
        "Accept: application/json",
        "-H",
        "Content-Type: application/json",
        "-H",
        "User-Agent: antigravity-git-widget",
        "-d",
        body_json,
    ])
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());

    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);

    let out = cmd.output().await?;
    if !out.status.success() {
        return Err(CoreError::GitFailed {
            code: out.status.code().unwrap_or(-1),
            stderr: String::from_utf8_lossy(&out.stderr).trim().to_string(),
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

pub async fn start_device_flow(client_id: &str) -> Result<DeviceFlowStart> {
    let id = client_id.trim();
    if id.is_empty() {
        return Err(CoreError::InvalidInput("GitHub OAuth client_id is empty".into()));
    }
    let body = serde_json::json!({
        "client_id": id,
        "scope": "repo"
    })
    .to_string();
    let raw = post_github_oauth("https://github.com/login/device/code", &body).await?;
    parse_device_start_json(&raw)
}

pub async fn poll_device_flow(client_id: &str, device_code: &str) -> Result<DevicePollResult> {
    let id = client_id.trim();
    let code = device_code.trim();
    if id.is_empty() || code.is_empty() {
        return Err(CoreError::InvalidInput("client_id and device_code are required".into()));
    }
    let body = serde_json::json!({
        "client_id": id,
        "device_code": code,
        "grant_type": "urn:ietf:params:oauth:grant-type:device_code"
    })
    .to_string();
    let raw = post_github_oauth("https://github.com/login/oauth/access_token", &body).await?;
    parse_device_poll_json(&raw)
}
