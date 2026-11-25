//! HTTP routes and single-flight coordinator for `/api/clone*`.

use crate::routes::AppState;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json, Response},
};
use git_widget_core::{clone::CloneTimeouts, clone_progress::CloneStage};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{watch, Mutex, RwLock};

#[derive(Debug, Deserialize)]
pub struct StartCloneRequest {
    pub url: String,
    pub parent_dir: String,
    #[serde(default)]
    pub folder_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CloneStatusResponse {
    pub stage: CloneStage,
    pub percent: u8,
    pub phase_label: String,
    pub target_path: Option<String>,
    pub sanitized_url: Option<String>,
    pub in_progress: bool,
    pub error: Option<String>,
    pub log_tail: Vec<String>,
}

impl Default for CloneStatusResponse {
    fn default() -> Self {
        Self {
            stage: CloneStage::Idle,
            percent: 0,
            phase_label: String::new(),
            target_path: None,
            sanitized_url: None,
            in_progress: false,
            error: None,
            log_tail: Vec::new(),
        }
    }
}

pub struct CloneManager {
    pub status: RwLock<CloneStatusResponse>,
    cancel_tx: Mutex<Option<watch::Sender<bool>>>,
    test_mode: bool,
    timeouts: CloneTimeouts,
}

impl Default for CloneManager {
    fn default() -> Self {
        Self::new()
    }
}

impl CloneManager {
    pub fn new() -> Self {
        Self {
            status: RwLock::new(CloneStatusResponse::default()),
            cancel_tx: Mutex::new(None),
            test_mode: false,
            timeouts: CloneTimeouts::default(),
        }
    }

    pub fn new_for_tests() -> Self {
        Self {
            test_mode: true,
            ..Self::new()
        }
    }

    pub fn is_test_mode(&self) -> bool {
        self.test_mode
    }
}

pub async fn start_clone_handler(
    State(_st): State<Arc<AppState>>,
    _headers: HeaderMap,
    Json(_body): Json<StartCloneRequest>,
) -> Response {
    let _ = (&_st.clone.cancel_tx, &_st.clone.timeouts);
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(serde_json::json!({ "error": "not implemented" })),
    )
        .into_response()
}

pub async fn clone_status_handler(
    State(_st): State<Arc<AppState>>,
    _headers: HeaderMap,
) -> Response {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(serde_json::json!({ "error": "not implemented" })),
    )
        .into_response()
}

pub async fn cancel_clone_handler(
    State(_st): State<Arc<AppState>>,
    _headers: HeaderMap,
) -> Response {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(serde_json::json!({ "error": "not implemented" })),
    )
        .into_response()
}
