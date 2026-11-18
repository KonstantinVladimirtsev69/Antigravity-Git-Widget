//! HTTP API route handlers for git-widget-sidecar.

use crate::config::{load_config, save_config};
use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json, Response},
};
use git_widget_core::{
    ops::{
        add_remote, commit_smart, fetch_remote, init_repo, publish_branch, pull_remote,
        push_remote, stash_pop, stash_save,
    },
    safety::{
        abort_merge, build_conflict_prompt, preview_merge, preview_push, run_with_dirty_guard,
        DirtyStrategy, GuardedOpKind,
    },
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
}

#[derive(Serialize)]
pub struct ReposResponse {
    pub repos: Vec<RepoInfo>,
    pub config: ScannerConfig,
}

#[derive(Deserialize)]
pub struct RepoQuery {
    pub repo: String,
}

#[derive(Deserialize)]
pub struct RepoBody {
    pub repo: String,
    pub remote: Option<String>,
}

#[derive(Deserialize)]
pub struct CommitBody {
    pub repo: String,
    pub message: String,
    #[serde(default)]
    pub amend: bool,
}

#[derive(Deserialize)]
pub struct StashBody {
    pub repo: String,
    pub message: Option<String>,
}

#[derive(Deserialize)]
pub struct PreviewMergeBody {
    pub repo: String,
    pub source: String,
}

#[derive(Deserialize)]
pub struct GuardedBody {
    pub repo: String,
    pub kind: GuardedOpKind,
    pub target: Option<String>,
    pub strategy: DirtyStrategy,
}

#[derive(Deserialize)]
pub struct ResolveWorkspaceBody {
    pub workspace_uris: Vec<String>,
}

#[derive(Deserialize)]
pub struct ConflictChatBody {
    pub repo: String,
    pub conversation_id: Option<String>,
}

#[derive(Serialize)]
pub struct ConflictChatResponse {
    pub sent: bool,
    pub prompt: String,
    pub error: Option<String>,
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
        Err((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "unauthorized" })),
        )
            .into_response())
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

fn scan_with_cfg(cfg: &ScannerConfig) -> Result<Vec<RepoInfo>, Response> {
    let roots: Vec<PathBuf> = cfg.roots.iter().map(PathBuf::from).collect();
    scan_repositories(&roots, cfg.max_depth).map_err(map_err)
}

pub async fn get_repos(State(st): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    let cfg = st.config.read().await.clone();
    match scan_with_cfg(&cfg) {
        Ok(repos) => Json(ReposResponse { repos, config: cfg }).into_response(),
        Err(r) => r,
    }
}

pub async fn post_config(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(cfg): Json<ScannerConfig>,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    let _ = save_config(&cfg);
    *st.config.write().await = cfg.clone();
    match scan_with_cfg(&cfg) {
        Ok(repos) => Json(ReposResponse { repos, config: cfg }).into_response(),
        Err(r) => r,
    }
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (
                (bytes[i + 1] as char).to_digit(16),
                (bytes[i + 2] as char).to_digit(16),
            ) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn normalize_path_str(p: &Path) -> String {
    let s = p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    let text = s.to_string_lossy().to_string();
    text.strip_prefix(r"\\?\")
        .unwrap_or(&text)
        .trim_end_matches('\\')
        .to_lowercase()
}

fn uri_to_path(uri: &str) -> PathBuf {
    let trimmed = uri
        .strip_prefix("file:///")
        .or_else(|| uri.strip_prefix("file://"))
        .unwrap_or(uri);
    let decoded = percent_decode(trimmed).replace('/', "\\");
    PathBuf::from(decoded)
}

pub async fn resolve_workspace(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<ResolveWorkspaceBody>,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    let cfg = st.config.read().await.clone();
    let repos = scan_with_cfg(&cfg).unwrap_or_default();

    for uri in &body.workspace_uris {
        let ws = uri_to_path(uri);
        let ws_norm = normalize_path_str(&ws);
        if let Some(found) = repos.iter().find(|r| {
            let r_norm = normalize_path_str(Path::new(&r.path));
            r_norm == ws_norm || r_norm.starts_with(&format!("{ws_norm}\\"))
        }) {
            return Json(serde_json::json!({ "matched_path": found.path })).into_response();
        }
        if ws.join(".git").exists() {
            let clean = ws.to_string_lossy().trim_end_matches('\\').to_string();
            return Json(serde_json::json!({ "matched_path": clean })).into_response();
        }
        // Walk up in case workspace points to a subdirectory inside a git repo
        let mut cur = ws.as_path();
        while let Some(parent) = cur.parent() {
            if parent.join(".git").exists() {
                let p_norm = normalize_path_str(parent);
                let matched = repos
                    .iter()
                    .find(|r| normalize_path_str(Path::new(&r.path)) == p_norm)
                    .map(|r| r.path.clone())
                    .unwrap_or_else(|| parent.to_string_lossy().to_string());
                return Json(serde_json::json!({ "matched_path": matched })).into_response();
            }
            cur = parent;
        }
    }
    Json(serde_json::json!({ "matched_path": null })).into_response()
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

pub async fn guarded_handler(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<GuardedBody>,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    match run_with_dirty_guard(
        Path::new(&body.repo),
        body.kind,
        body.target.as_deref(),
        body.strategy,
    )
    .await
    {
        Ok(out) => Json(out).into_response(),
        Err(e) => map_err(e),
    }
}

pub async fn fetch_handler(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(b): Json<RepoBody>,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    match fetch_remote(Path::new(&b.repo), b.remote.as_deref()).await {
        Ok(r) => Json(r).into_response(),
        Err(e) => map_err(e),
    }
}

pub async fn pull_handler(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(b): Json<RepoBody>,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    match pull_remote(Path::new(&b.repo), b.remote.as_deref()).await {
        Ok(r) => Json(r).into_response(),
        Err(e) => map_err(e),
    }
}

#[derive(Deserialize)]
pub struct AddRemoteBody {
    pub repo: String,
    pub name: String,
    pub url: String,
}

#[derive(Deserialize)]
pub struct PublishBody {
    pub repo: String,
    pub remote: Option<String>,
}

pub async fn publish_handler(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(b): Json<PublishBody>,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    match publish_branch(Path::new(&b.repo), b.remote.as_deref()).await {
        Ok(r) => Json(r).into_response(),
        Err(e) => map_err(e),
    }
}

pub async fn add_remote_handler(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(b): Json<AddRemoteBody>,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    match add_remote(Path::new(&b.repo), &b.name, &b.url).await {
        Ok(r) => Json(r).into_response(),
        Err(e) => map_err(e),
    }
}

pub async fn init_repo_handler(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(b): Json<RepoBody>,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    match init_repo(Path::new(&b.repo)).await {
        Ok(r) => Json(r).into_response(),
        Err(e) => map_err(e),
    }
}

pub async fn push_handler(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(b): Json<RepoBody>,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    match push_remote(Path::new(&b.repo), b.remote.as_deref()).await {
        Ok(r) => Json(r).into_response(),
        Err(e) => map_err(e),
    }
}

pub async fn commit_handler(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(b): Json<CommitBody>,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    match commit_smart(Path::new(&b.repo), &b.message, b.amend).await {
        Ok(r) => Json(r).into_response(),
        Err(e) => map_err(e),
    }
}

pub async fn stash_save_handler(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(b): Json<StashBody>,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    match stash_save(Path::new(&b.repo), b.message.as_deref()).await {
        Ok(r) => Json(r).into_response(),
        Err(e) => map_err(e),
    }
}

pub async fn stash_pop_handler(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(b): Json<RepoBody>,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    match stash_pop(Path::new(&b.repo)).await {
        Ok(r) => Json(r).into_response(),
        Err(e) => map_err(e),
    }
}

pub async fn abort_merge_handler(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(b): Json<RepoBody>,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    match abort_merge(Path::new(&b.repo)).await {
        Ok(r) => Json(r).into_response(),
        Err(e) => map_err(e),
    }
}

pub async fn preview_merge_handler(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(b): Json<PreviewMergeBody>,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    match preview_merge(Path::new(&b.repo), &b.source).await {
        Ok(r) => Json(r).into_response(),
        Err(e) => map_err(e),
    }
}

pub async fn preview_push_handler(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(b): Json<RepoBody>,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    match preview_push(Path::new(&b.repo)).await {
        Ok(r) => Json(r).into_response(),
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
        Ok(out) => (
            false,
            Some(String::from_utf8_lossy(&out.stderr).trim().to_string()),
        ),
        Err(e) => (false, Some(e.to_string())),
    }
}

pub async fn send_conflict_to_chat_handler(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(b): Json<ConflictChatBody>,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    let prompt = match build_conflict_prompt(Path::new(&b.repo)).await {
        Ok(p) => p,
        Err(e) => return map_err(e),
    };
    let (sent, error) = send_agentapi_message(&st, b.conversation_id, &prompt).await;
    Json(ConflictChatResponse {
        sent,
        prompt,
        error,
    })
    .into_response()
}

pub fn init_config() -> Arc<RwLock<ScannerConfig>> {
    Arc::new(RwLock::new(load_config()))
}
