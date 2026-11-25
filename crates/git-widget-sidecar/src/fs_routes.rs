//! Authenticated filesystem endpoints (`/api/fs/pick-folder`, `/api/fs/open-explorer`).

use crate::{
    folder_picker::pick_folder,
    routes::{check_auth, AppState},
};
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json, Response},
};
use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    process::{Command, Stdio},
    sync::Arc,
};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Deserialize)]
pub struct PickFolderRequest {
    #[serde(default)]
    pub initial_dir: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PickFolderResponse {
    pub path: Option<String>,
    pub cancelled: bool,
}

#[derive(Debug, Deserialize)]
pub struct OpenExplorerRequest {
    pub path: String,
}

pub async fn pick_folder_handler(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<PickFolderRequest>,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    match pick_folder(body.initial_dir).await {
        Ok(Some(path)) => Json(PickFolderResponse {
            path: Some(path),
            cancelled: false,
        })
        .into_response(),
        Ok(None) => Json(PickFolderResponse {
            path: None,
            cancelled: true,
        })
        .into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": err })),
        )
            .into_response(),
    }
}

pub async fn open_explorer_handler(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<OpenExplorerRequest>,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    let trimmed = body.path.trim();
    if trimmed.is_empty() || !Path::new(trimmed).is_dir() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Target path does not exist or is not a directory" })),
        )
            .into_response();
    }

    let mut cmd = Command::new("explorer.exe");
    cmd.arg(trimmed)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    match cmd.spawn() {
        Ok(_) => (StatusCode::OK, Json(serde_json::json!({ "ok": true }))).into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("Failed to open Explorer: {err}") })),
        )
            .into_response(),
    }
}
