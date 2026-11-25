//! Authenticated filesystem endpoints (`/api/fs/pick-folder`, `/api/fs/open-explorer`).

use crate::routes::AppState;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json, Response},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

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
    State(_st): State<Arc<AppState>>,
    _headers: HeaderMap,
    Json(_body): Json<PickFolderRequest>,
) -> Response {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(serde_json::json!({ "error": "not implemented" })),
    )
        .into_response()
}

pub async fn open_explorer_handler(
    State(_st): State<Arc<AppState>>,
    _headers: HeaderMap,
    Json(_body): Json<OpenExplorerRequest>,
) -> Response {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(serde_json::json!({ "error": "not implemented" })),
    )
        .into_response()
}
