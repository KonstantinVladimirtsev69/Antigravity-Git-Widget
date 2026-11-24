//! HTTP routes and single-flight / 6h TTL-cache state for `/api/update/*`.

use crate::{
    routes::{check_auth, AppState},
    self_update::{detect_installed_hosts, detect_installed_hosts_in},
};
use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json, Response},
};
use git_widget_core::updater::{
    download_verified_release, download_verified_release_with_bases, fetch_latest_release,
    fetch_latest_release_with_base, resolve_update_repo, SemVer, UpdateReleaseInfo,
};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf, process::{Command, Stdio}, sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::RwLock;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub const DEFAULT_TTL_SECS: u64 = 6 * 3600;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateStage {
    Idle, Downloading, Verifying, Installing, Restarting, Error,
}

impl UpdateStage {
    pub fn is_in_progress(self) -> bool {
        matches!(self, Self::Downloading | Self::Verifying | Self::Installing | Self::Restarting)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdateStatusResponse {
    pub stage: UpdateStage,
    pub current_version: String,
    pub target_version: Option<String>,
    pub message: String,
    pub error: Option<String>,
    pub in_progress: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdateCheckResponse {
    pub repo: String,
    pub current_version: String,
    pub latest_version: Option<String>,
    pub update_available: bool,
    pub release_notes: String,
    pub html_url: Option<String>,
    pub published_at: Option<String>,
    pub cached: bool,
    pub checked_at_epoch_s: u64,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CheckUpdateQuery {
    #[serde(default)]
    pub force: bool,
}

struct CachedCheck {
    response: UpdateCheckResponse,
    cached_at: Instant,
    repo: String,
}

pub struct UpdateManager {
    cache: RwLock<Option<CachedCheck>>,
    pub status: RwLock<UpdateStatusResponse>,
    api_base: Option<String>,
    allowed_bases: Vec<String>,
    ag_dir_override: Option<PathBuf>,
    t3_dir_override: Option<PathBuf>,
    dry_run_no_exit: bool,
    test_mode: bool,
}

impl Default for UpdateManager {
    fn default() -> Self { Self::new() }
}

impl UpdateManager {
    pub fn new() -> Self {
        Self {
            cache: RwLock::new(None),
            status: RwLock::new(UpdateStatusResponse {
                stage: UpdateStage::Idle,
                current_version: env!("CARGO_PKG_VERSION").to_string(),
                target_version: None,
                message: String::new(),
                error: None,
                in_progress: false,
            }),
            api_base: None, allowed_bases: Vec::new(),
            ag_dir_override: None, t3_dir_override: None,
            dry_run_no_exit: false, test_mode: false,
        }
    }

    pub fn new_for_tests(
        api_base: impl Into<String>, allowed_bases: Vec<String>,
        ag_dir: Option<PathBuf>, t3_dir: Option<PathBuf>, dry_run_no_exit: bool,
    ) -> Self {
        let mut mgr = Self::new();
        mgr.api_base = Some(api_base.into());
        mgr.allowed_bases = allowed_bases;
        mgr.ag_dir_override = ag_dir;
        mgr.t3_dir_override = t3_dir;
        mgr.dry_run_no_exit = dry_run_no_exit;
        mgr.test_mode = true;
        mgr
    }

    pub fn is_test_mode(&self) -> bool { self.test_mode }

    pub async fn invalidate_cache(&self) { *self.cache.write().await = None; }
}

fn now_epoch_s() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

async fn resolve_repo_and_token(st: &AppState) -> (String, Option<String>) {
    let (cfg_repo, token) = {
        let cfg = st.config.read().await;
        (cfg.update_repo.clone(), cfg.github_token.clone())
    };
    let env_repo = std::env::var("GIT_WIDGET_UPDATE_REPO").ok();
    (resolve_update_repo(cfg_repo.as_deref(), env_repo.as_deref()), token)
}

async fn fetch_release(
    mgr: &UpdateManager, repo: &str, current_version: &str, token: Option<&str>,
) -> git_widget_core::Result<UpdateReleaseInfo> {
    if let Some(base) = mgr.api_base.as_deref() {
        let bases: Vec<&str> = mgr.allowed_bases.iter().map(String::as_str).collect();
        fetch_latest_release_with_base(base, repo, current_version, token, &bases).await
    } else {
        fetch_latest_release(repo, current_version, token).await
    }
}

pub async fn check_update_handler(
    State(st): State<Arc<AppState>>, headers: HeaderMap, Query(q): Query<CheckUpdateQuery>,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) { return r; }
    let (repo, token) = resolve_repo_and_token(&st).await;

    if !q.force {
        let cache = st.update.cache.read().await;
        if let Some(hit) = cache.as_ref() {
            if hit.repo == repo && hit.cached_at.elapsed().as_secs() < DEFAULT_TTL_SECS {
                let mut resp = hit.response.clone();
                resp.cached = true;
                return Json(resp).into_response();
            }
        }
    }

    let current_version = env!("CARGO_PKG_VERSION");
    match fetch_release(&st.update, &repo, current_version, token.as_deref()).await {
        Ok(info) => {
            let resp = UpdateCheckResponse {
                repo: info.repo, current_version: info.current_version,
                latest_version: info.latest_version, update_available: info.update_available,
                release_notes: info.release_notes, html_url: info.html_url,
                published_at: info.published_at, cached: false,
                checked_at_epoch_s: now_epoch_s(), error: None,
            };
            *st.update.cache.write().await = Some(CachedCheck {
                response: resp.clone(), cached_at: Instant::now(), repo,
            });
            Json(resp).into_response()
        }
        Err(err) => Json(UpdateCheckResponse {
            repo,
            current_version: current_version.to_string(),
            checked_at_epoch_s: now_epoch_s(),
            error: Some(err.to_string()),
            ..Default::default()
        })
        .into_response(),
    }
}

pub async fn update_status_handler(State(st): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    if let Err(r) = check_auth(&st, &headers) { return r; }
    Json(st.update.status.read().await.clone()).into_response()
}

pub async fn install_update_handler(State(st): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    if let Err(r) = check_auth(&st, &headers) { return r; }
    let (accepted, snapshot) = {
        let mut guard = st.update.status.write().await;
        if guard.in_progress {
            (false, guard.clone())
        } else {
            guard.stage = UpdateStage::Downloading;
            guard.target_version = None;
            guard.message = "Checking release and host targets...".to_string();
            guard.error = None;
            guard.in_progress = true;
            (true, guard.clone())
        }
    };
    if !accepted {
        return (StatusCode::CONFLICT, Json(snapshot)).into_response();
    }
    let bg = Arc::clone(&st);
    tokio::spawn(async move { run_install_pipeline(bg).await });
    (StatusCode::ACCEPTED, Json(snapshot)).into_response()
}

async fn set_status(
    st: &AppState, stage: UpdateStage, target: Option<String>, msg: impl Into<String>, err: Option<String>,
) {
    let mut guard = st.update.status.write().await;
    guard.stage = stage;
    if target.is_some() { guard.target_version = target; }
    guard.message = msg.into();
    guard.error = err;
    guard.in_progress = stage.is_in_progress();
}

async fn fail(st: &AppState, err: impl Into<String>) {
    let msg = err.into();
    set_status(st, UpdateStage::Error, None, msg.clone(), Some(msg)).await;
}

async fn run_install_pipeline(st: Arc<AppState>) {
    let targets = if st.update.ag_dir_override.is_some() || st.update.t3_dir_override.is_some() {
        detect_installed_hosts_in(st.update.ag_dir_override.as_deref(), st.update.t3_dir_override.as_deref())
    } else {
        detect_installed_hosts()
    };
    if targets.is_empty() {
        return fail(&st, "No installed Antigravity or T3 Code host detected for self-update").await;
    }

    let (repo, token) = resolve_repo_and_token(&st).await;
    let current_version = env!("CARGO_PKG_VERSION");
    let info = match fetch_release(&st.update, &repo, current_version, token.as_deref()).await {
        Ok(i) => i,
        Err(e) => return fail(&st, e.to_string()).await,
    };

    let is_newer = info.latest_version.as_deref().is_some_and(|lv| {
        matches!((SemVer::parse_stable(lv), SemVer::parse_stable(current_version)), (Some(l), Some(c)) if l > c)
    });
    if !info.update_available || !is_newer {
        return fail(&st, "No newer stable release available (downgrade rejected)").await;
    }

    set_status(&st, UpdateStage::Verifying, info.latest_version.clone(), "Downloading and verifying SHA-256 checksum...", None).await;
    let verified = if st.update.api_base.is_some() {
        let bases: Vec<&str> = st.update.allowed_bases.iter().map(String::as_str).collect();
        download_verified_release_with_bases(&info, current_version, token.as_deref(), &bases).await
    } else {
        download_verified_release(&info, current_version, token.as_deref()).await
    };
    let verified = match verified {
        Ok(v) => v,
        Err(e) => return fail(&st, e.to_string()).await,
    };

    set_status(&st, UpdateStage::Installing, Some(verified.version.clone()), format!("Installing v{}...", verified.version), None).await;
    if st.update.dry_run_no_exit {
        let _ = std::fs::remove_dir_all(&verified.temp_dir);
    } else {
        let mut cmd = Command::new(&verified.exe_path);
        cmd.args(["--self-update", "--parent-pid", &std::process::id().to_string()]);
        if let Some(port) = st.serve_port { cmd.args(["--serve-port", &port.to_string()]); }
        if let Some(pf) = st.port_file.as_ref() { cmd.arg("--port-file").arg(pf); }
        cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);
        if let Err(e) = cmd.spawn() {
            let _ = std::fs::remove_dir_all(&verified.temp_dir);
            return fail(&st, format!("Failed launching self-updater: {e}")).await;
        }
    }

    set_status(&st, UpdateStage::Restarting, Some(verified.version.clone()), format!("Restarting v{}...", verified.version), None).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    if !st.update.dry_run_no_exit {
        std::process::exit(0);
    }
}
