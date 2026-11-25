//! HTTP routes and single-flight coordinator for `/api/clone*`.

use crate::{
    config::save_config,
    routes::{check_auth, map_err, AppState},
};
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json, Response},
};
use git_widget_core::{
    clone::{run_clone, CloneOptions, CloneTimeouts},
    clone_progress::{CloneProgressSnapshot, CloneStage},
    clone_validation::{parse_clone_url, prepare_clone_target, validate_folder_name},
};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::sync::{watch, Mutex, RwLock};

#[derive(Debug, Deserialize)]
pub struct StartCloneRequest {
    pub url: String,
    pub parent_dir: String,
    #[serde(default)]
    pub folder_name: Option<String>,
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

impl From<CloneProgressSnapshot> for CloneStatusResponse {
    fn from(s: CloneProgressSnapshot) -> Self {
        Self {
            stage: s.stage,
            percent: s.percent,
            phase_label: s.phase_label,
            target_path: Some(s.target_path),
            sanitized_url: Some(s.sanitized_url),
            in_progress: s.in_progress,
            error: s.error,
            log_tail: s.log_tail,
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
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<StartCloneRequest>,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }

    let source = match parse_clone_url(&body.url) {
        Ok(s) => s,
        Err(e) => return map_err(e),
    };
    let parent_dir = body.parent_dir.trim().to_string();
    let folder_name = body
        .folder_name
        .unwrap_or_else(|| source.default_folder.clone());
    if let Err(e) = validate_folder_name(&folder_name) {
        return map_err(e);
    }

    let (tx, rx) = watch::channel(false);
    let rx_observer = rx.clone();
    let initial = {
        let mut status_guard = st.clone.status.write().await;
        if status_guard.in_progress {
            return (StatusCode::CONFLICT, Json(status_guard.clone())).into_response();
        }
        let guard = match prepare_clone_target(Path::new(&parent_dir), &folder_name) {
            Ok(g) => g,
            Err(e) => return map_err(e),
        };
        let snap = CloneStatusResponse {
            stage: CloneStage::Starting,
            percent: 1,
            phase_label: "Starting clone...".to_string(),
            target_path: Some(guard.target_path.to_string_lossy().to_string()),
            sanitized_url: Some(source.sanitized_url.clone()),
            in_progress: true,
            error: None,
            log_tail: Vec::new(),
        };
        *st.clone.cancel_tx.lock().await = Some(tx);
        *status_guard = snap.clone();
        snap
    };

    {
        let mut cfg_guard = st.config.write().await;
        cfg_guard.last_clone_dir = Some(parent_dir.clone());
        if !st.clone.is_test_mode() {
            let _ = save_config(&cfg_guard);
        }
    }

    let bg = Arc::clone(&st);
    let opts = CloneOptions {
        url: source.normalized_url,
        parent_dir: PathBuf::from(&parent_dir),
        folder_name: Some(folder_name),
        timeouts: st.clone.timeouts,
    };
    let target_str = initial.target_path.clone().unwrap_or_default();
    let sanitized = source.sanitized_url;
    tokio::spawn(async move {
        let mgr_status = Arc::clone(&bg);
        let result = run_clone(opts, rx, move |snap| {
            if let Ok(mut guard) = mgr_status.clone.status.try_write() {
                *guard = CloneStatusResponse::from(snap);
            }
        })
        .await;

        *bg.clone.cancel_tx.lock().await = None;
        let mut status_guard = bg.clone.status.write().await;
        match result {
            Ok(outcome) => {
                status_guard.stage = CloneStage::Done;
                status_guard.percent = 100;
                status_guard.phase_label = "Clone complete".to_string();
                status_guard.target_path = Some(outcome.target_path.to_string_lossy().to_string());
                status_guard.sanitized_url = Some(outcome.sanitized_url);
                status_guard.in_progress = false;
                status_guard.error = None;
                status_guard.log_tail = outcome.log_tail;
            }
            Err(err) => {
                if status_guard.in_progress
                    || !matches!(status_guard.stage, CloneStage::Cancelled | CloneStage::Error)
                {
                    let was_cancelled = *rx_observer.borrow();
                    status_guard.stage = if was_cancelled {
                        CloneStage::Cancelled
                    } else {
                        CloneStage::Error
                    };
                    status_guard.phase_label = if was_cancelled {
                        "Clone cancelled"
                    } else {
                        "Clone failed"
                    }
                    .to_string();
                    status_guard.target_path = Some(target_str);
                    status_guard.sanitized_url = Some(sanitized);
                    status_guard.in_progress = false;
                    status_guard.error = Some(err.to_string());
                }
            }
        }
    });

    (StatusCode::ACCEPTED, Json(initial)).into_response()
}

pub async fn clone_status_handler(State(st): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    Json(st.clone.status.read().await.clone()).into_response()
}

pub async fn cancel_clone_handler(State(st): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    if let Some(tx) = st.clone.cancel_tx.lock().await.as_ref() {
        let _ = tx.send(true);
    }
    Json(st.clone.status.read().await.clone()).into_response()
}
