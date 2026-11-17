//! Diff Review and AI question routes for git-widget-sidecar.

use crate::routes::{check_auth, map_err, send_agentapi_message, AppState, RepoQuery};
use axum::{
    extract::{Query, State},
    http::HeaderMap,
    response::{IntoResponse, Json, Response},
};
use git_widget_core::diff::{
    accept_hunk, discard_file, get_diff_files, reject_hunk, stage_file, unstage_file,
};
use serde::{Deserialize, Serialize};
use std::{path::Path, sync::Arc};

#[derive(Deserialize)]
pub struct HunkActionBody {
    pub repo: String,
    pub file: String,
    pub hunk_index: usize,
}

#[derive(Deserialize)]
pub struct FileActionBody {
    pub repo: String,
    pub file: String,
}

#[derive(Deserialize)]
pub struct AskAiBody {
    pub repo: String,
    pub file: String,
    pub snippet: String,
    pub question: String,
    pub conversation_id: Option<String>,
}

#[derive(Serialize)]
pub struct AskAiResponse {
    pub sent: bool,
    pub prompt: String,
    pub error: Option<String>,
}

pub async fn diff_list_handler(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<RepoQuery>,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    match get_diff_files(Path::new(&q.repo)).await {
        Ok(files) => Json(files).into_response(),
        Err(e) => map_err(e),
    }
}

pub async fn accept_hunk_handler(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(b): Json<HunkActionBody>,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    match accept_hunk(Path::new(&b.repo), &b.file, b.hunk_index).await {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => map_err(e),
    }
}

pub async fn reject_hunk_handler(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(b): Json<HunkActionBody>,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    match reject_hunk(Path::new(&b.repo), &b.file, b.hunk_index).await {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => map_err(e),
    }
}

pub async fn stage_file_handler(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(b): Json<FileActionBody>,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    match stage_file(Path::new(&b.repo), &b.file).await {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => map_err(e),
    }
}

pub async fn unstage_file_handler(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(b): Json<FileActionBody>,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    match unstage_file(Path::new(&b.repo), &b.file).await {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => map_err(e),
    }
}

pub async fn discard_file_handler(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(b): Json<FileActionBody>,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    match discard_file(Path::new(&b.repo), &b.file).await {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => map_err(e),
    }
}

pub async fn ask_ai_handler(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(b): Json<AskAiBody>,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    let prompt = format!(
        "Вопрос по diff в `{}` (репозиторий `{}`):\n\n```diff\n{}\n```\n\nКомментарий/вопрос: {}",
        b.file, b.repo, b.snippet.trim(), b.question.trim()
    );
    let (sent, error) = send_agentapi_message(&st, b.conversation_id, &prompt).await;
    Json(AskAiResponse {
        sent,
        prompt,
        error,
    })
    .into_response()
}
