//! Core HTTP state, auth, config, workspace resolution, and status route handlers.

use crate::{
    config::{load_config, save_config},
    update_routes::UpdateManager,
};
use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json, Response},
};
use git_widget_core::{
    integrate::IntegrationMode,
    scanner::{scan_repositories, RepoInfo, ScannerConfig},
    status::get_status,
    CoreError,
};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::{process::Command, sync::RwLock};

#[derive(Clone)]
pub struct AppState {
    pub ui_token: Option<String>,
    pub agentapi_exe: Option<String>,
    pub project_id: Option<String>,
    pub conversation_id: Option<String>,
    pub config: Arc<RwLock<ScannerConfig>>,
    pub serve_port: Option<u16>,
    pub port_file: Option<PathBuf>,
    pub update: Arc<UpdateManager>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicScannerConfig {
    pub roots: Vec<String>,
    pub max_depth: usize,
    pub integration_mode: IntegrationMode,
    pub has_github_token: bool,
    pub update_repo: Option<String>,
}

impl From<&ScannerConfig> for PublicScannerConfig {
    fn from(cfg: &ScannerConfig) -> Self {
        Self {
            roots: cfg.roots.clone(),
            max_depth: cfg.max_depth,
            integration_mode: cfg.integration_mode,
            has_github_token: cfg.github_token.as_ref().is_some_and(|t| !t.trim().is_empty()),
            update_repo: cfg.update_repo.clone(),
        }
    }
}

#[derive(Serialize)]
pub struct ReposResponse {
    pub repos: Vec<RepoInfo>,
    pub config: PublicScannerConfig,
}

#[derive(Deserialize)]
pub struct RepoQuery {
    pub repo: String,
}

pub fn check_auth(state: &AppState, headers: &HeaderMap) -> Result<(), Response> {
    let Some(expected) = state.ui_token.as_deref() else {
        return Ok(());
    };
    let ok = headers
        .get("x-sidecar-token")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|p| p == expected);
    if ok {
        Ok(())
    } else {
        Err((StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "unauthorized" }))).into_response())
    }
}

pub fn map_err(err: CoreError) -> Response {
    let status = match err {
        CoreError::InvalidInput(_) | CoreError::NotAGitRepo(_) => StatusCode::BAD_REQUEST,
        CoreError::Timeout(_, _) => StatusCode::GATEWAY_TIMEOUT,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (status, Json(serde_json::json!({ "error": err.to_string() }))).into_response()
}

pub async fn scan_with_cfg(cfg: &ScannerConfig) -> Result<Vec<RepoInfo>, Response> {
    let roots: Vec<PathBuf> = cfg.roots.iter().map(PathBuf::from).collect();
    let max_depth = cfg.max_depth;
    tokio::task::spawn_blocking(move || scan_repositories(&roots, max_depth))
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("scan task failed: {e}") })),
            )
                .into_response()
        })?
        .map_err(map_err)
}

pub fn merge_config_preserving_token(existing: &ScannerConfig, mut incoming: ScannerConfig) -> ScannerConfig {
    if incoming.github_token.as_ref().is_none_or(|t| t.trim().is_empty()) {
        incoming.github_token = existing.github_token.clone();
    }
    if incoming.github_oauth_client_id.as_ref().is_none_or(|t| t.trim().is_empty()) {
        incoming.github_oauth_client_id = existing.github_oauth_client_id.clone();
    }
    incoming
}

pub async fn get_repos(State(st): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    let cfg = st.config.read().await.clone();
    match scan_with_cfg(&cfg).await {
        Ok(repos) => Json(ReposResponse {
            repos,
            config: PublicScannerConfig::from(&cfg),
        })
        .into_response(),
        Err(r) => r,
    }
}

pub async fn post_config(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(incoming): Json<ScannerConfig>,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    let cfg = {
        let mut guard = st.config.write().await;
        let merged = merge_config_preserving_token(&guard, incoming);
        let _ = save_config(&merged);
        *guard = merged.clone();
        merged
    };
    match scan_with_cfg(&cfg).await {
        Ok(repos) => Json(ReposResponse {
            repos,
            config: PublicScannerConfig::from(&cfg),
        })
        .into_response(),
        Err(r) => r,
    }
}

pub async fn status_handler(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<RepoQuery>,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    match get_status(Path::new(&q.repo)).await {
        Ok(s) => Json(s).into_response(),
        Err(e) => map_err(e),
    }
}

pub async fn send_agentapi_message(
    st: &AppState,
    conv_override: Option<String>,
    message: &str,
) -> (bool, Option<String>) {
    let Some(conv_id) = conv_override.or_else(|| st.conversation_id.clone()) else {
        return (false, Some("missing conversation_id".to_string()));
    };
    let (program, prefix): (&str, Vec<&str>) = match st.agentapi_exe.as_deref() {
        Some(exe) if !exe.is_empty() => (exe, vec!["agentapi"]),
        _ => ("agentapi", vec![]),
    };
    let mut cmd = Command::new(program);
    cmd.args(prefix).arg("send-message").arg(&conv_id).arg(message);
    if let Some(pid) = &st.project_id {
        cmd.env("ANTIGRAVITY_PROJECT_ID", pid);
    }
    match cmd.output().await {
        Ok(out) if out.status.success() => (true, None),
        Ok(out) => (false, Some(String::from_utf8_lossy(&out.stderr).trim().to_string())),
        Err(e) => (false, Some(e.to_string())),
    }
}

pub fn init_config() -> Arc<RwLock<ScannerConfig>> {
    Arc::new(RwLock::new(load_config()))
}

#[cfg(test)]
mod tests {
    use super::{merge_config_preserving_token, PublicScannerConfig};
    use git_widget_core::{integrate::IntegrationMode, scanner::ScannerConfig};

    #[test]
    fn public_config_redacts_github_token_and_preserves_on_save() {
        let secret = ScannerConfig {
            roots: vec![r"C:\Projects".into()],
            max_depth: 3,
            github_token: Some("ghp_super_secret".into()),
            github_oauth_client_id: None,
            integration_mode: IntegrationMode::Rebase,
            update_repo: None,
        };
        let pub_cfg = PublicScannerConfig::from(&secret);
        let serialized = serde_json::to_string(&pub_cfg).unwrap();
        assert!(!serialized.contains("ghp_super_secret"));
        assert!(pub_cfg.has_github_token);

        let incoming = ScannerConfig {
            roots: vec![r"D:\Code".into()],
            max_depth: 2,
            github_token: None,
            github_oauth_client_id: None,
            integration_mode: IntegrationMode::Squash,
            update_repo: None,
        };
        let merged = merge_config_preserving_token(&secret, incoming);
        assert_eq!(merged.github_token.as_deref(), Some("ghp_super_secret"));
        assert_eq!(merged.roots, vec![r"D:\Code".to_string()]);
    }
}
