//! HTTP routes and single-flight/TTL-cache state for `/api/update/*`.

use crate::routes::AppState;
use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Arc};
use tokio::sync::RwLock;

pub const DEFAULT_TTL_SECS: u64 = 6 * 3600;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateStage {
    Idle,
    Downloading,
    Verifying,
    Installing,
    Restarting,
    Error,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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

pub struct UpdateManager {
    pub status: RwLock<UpdateStatusResponse>,
}

impl Default for UpdateManager {
    fn default() -> Self {
        Self::new()
    }
}

impl UpdateManager {
    pub fn new() -> Self {
        Self {
            status: RwLock::new(UpdateStatusResponse {
                stage: UpdateStage::Idle,
                current_version: env!("CARGO_PKG_VERSION").to_string(),
                target_version: None,
                message: String::new(),
                error: None,
                in_progress: false,
            }),
        }
    }

    pub fn new_for_tests(
        _api_base: impl Into<String>,
        _allowed_bases: Vec<String>,
        _ag_dir: Option<PathBuf>,
        _t3_dir: Option<PathBuf>,
        _dry_run_no_exit: bool,
    ) -> Self {
        Self::new()
    }

    pub async fn invalidate_cache(&self) {}
}

pub async fn check_update_handler(
    State(_st): State<Arc<AppState>>,
    _headers: HeaderMap,
    Query(_q): Query<CheckUpdateQuery>,
) -> Response {
    (StatusCode::NOT_IMPLEMENTED, "not implemented").into_response()
}

pub async fn update_status_handler(
    State(_st): State<Arc<AppState>>,
    _headers: HeaderMap,
) -> Response {
    (StatusCode::NOT_IMPLEMENTED, "not implemented").into_response()
}

pub async fn install_update_handler(
    State(_st): State<Arc<AppState>>,
    _headers: HeaderMap,
) -> Response {
    (StatusCode::NOT_IMPLEMENTED, "not implemented").into_response()
}
