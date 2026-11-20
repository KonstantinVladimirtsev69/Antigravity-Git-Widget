//! HTTP routes for dual-branch pull scan, multi-mode integration, and mode persistence.

use crate::{
    config::save_config,
    routes::{check_auth, map_err, AppState, RepoBody},
};
use axum::{
    extract::State,
    http::HeaderMap,
    response::{IntoResponse, Json, Response},
};
use git_widget_core::{
    integrate::{
        abort_rebase, continue_rebase, integrate_branches, preview_integration, pull_both_branches,
        scan_integration_sync, IntegrationMode,
    },
    safety::DirtyStrategy,
};
use serde::Deserialize;
use std::{path::Path, sync::Arc};

#[derive(Deserialize)]
pub struct ScanSyncBody {
    pub repo: String,
    pub source: String,
    pub target: String,
    pub remote: Option<String>,
    #[serde(default = "default_true")]
    pub fetch: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Deserialize)]
pub struct PullBothBody {
    pub repo: String,
    pub source: String,
    pub target: String,
    pub remote: Option<String>,
}

#[derive(Deserialize)]
pub struct PreviewIntegrateBody {
    pub repo: String,
    pub source: String,
    pub target: String,
    pub mode: IntegrationMode,
}

#[derive(Deserialize)]
pub struct IntegrateBody {
    pub repo: String,
    pub source: String,
    pub target: String,
    pub mode: IntegrationMode,
    pub strategy: DirtyStrategy,
}

#[derive(Deserialize)]
pub struct IntegrationModeBody {
    pub mode: IntegrationMode,
}

pub async fn scan_sync_handler(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(b): Json<ScanSyncBody>,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    match scan_integration_sync(
        Path::new(&b.repo),
        &b.source,
        &b.target,
        b.remote.as_deref(),
        b.fetch,
    )
    .await
    {
        Ok(res) => Json(res).into_response(),
        Err(e) => map_err(e),
    }
}

pub async fn pull_both_handler(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(b): Json<PullBothBody>,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    match pull_both_branches(
        Path::new(&b.repo),
        &b.source,
        &b.target,
        b.remote.as_deref(),
    )
    .await
    {
        Ok(res) => Json(res).into_response(),
        Err(e) => map_err(e),
    }
}

pub async fn preview_integrate_handler(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(b): Json<PreviewIntegrateBody>,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    match preview_integration(Path::new(&b.repo), &b.source, &b.target, b.mode).await {
        Ok(res) => Json(res).into_response(),
        Err(e) => map_err(e),
    }
}

pub async fn integrate_handler(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(b): Json<IntegrateBody>,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    match integrate_branches(
        Path::new(&b.repo),
        &b.source,
        &b.target,
        b.mode,
        b.strategy,
    )
    .await
    {
        Ok(res) => Json(res).into_response(),
        Err(e) => map_err(e),
    }
}

pub async fn abort_rebase_handler(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(b): Json<RepoBody>,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    match abort_rebase(Path::new(&b.repo)).await {
        Ok(res) => Json(res).into_response(),
        Err(e) => map_err(e),
    }
}

pub async fn continue_rebase_handler(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(b): Json<RepoBody>,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    match continue_rebase(Path::new(&b.repo)).await {
        Ok(res) => Json(res).into_response(),
        Err(e) => map_err(e),
    }
}

pub async fn save_integration_mode_handler(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(b): Json<IntegrationModeBody>,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    let mut cfg = st.config.write().await;
    cfg.integration_mode = b.mode;
    let _ = save_config(&cfg);
    Json(serde_json::json!({ "mode": cfg.integration_mode })).into_response()
}
