//! Git workflow HTTP route handlers for git-widget-sidecar.

use crate::routes::{check_auth, map_err, send_agentapi_message, AppState};
use axum::{
    extract::State,
    http::HeaderMap,
    response::{IntoResponse, Json, Response},
};
use git_widget_core::{
    ops::{
        add_remote, commit_smart, fetch_remote, init_repo, publish_branch, pull_remote,
        push_remote, stash_pop, stash_save,
    },
    remote_focus::switch_remote,
    safety::{
        abort_merge, build_conflict_prompt, preview_merge, preview_push_for_remote,
        run_with_dirty_guard_for_remote, DirtyStrategy, GuardedOpKind,
    },
};
use serde::{Deserialize, Serialize};
use std::{path::Path, sync::Arc};

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
    #[serde(default)]
    pub remote: Option<String>,
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

pub async fn guarded_handler(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<GuardedBody>,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    match run_with_dirty_guard_for_remote(
        Path::new(&body.repo),
        body.kind,
        body.target.as_deref(),
        body.strategy,
        body.remote.as_deref(),
    )
    .await
    {
        Ok(out) => Json(out).into_response(),
        Err(e) => map_err(e),
    }
}

pub async fn fetch_handler(State(st): State<Arc<AppState>>, headers: HeaderMap, Json(b): Json<RepoBody>) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    match fetch_remote(Path::new(&b.repo), b.remote.as_deref()).await {
        Ok(r) => Json(r).into_response(),
        Err(e) => map_err(e),
    }
}

pub async fn pull_handler(State(st): State<Arc<AppState>>, headers: HeaderMap, Json(b): Json<RepoBody>) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    match pull_remote(Path::new(&b.repo), b.remote.as_deref()).await {
        Ok(r) => Json(r).into_response(),
        Err(e) => map_err(e),
    }
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

pub async fn init_repo_handler(State(st): State<Arc<AppState>>, headers: HeaderMap, Json(b): Json<RepoBody>) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    match init_repo(Path::new(&b.repo)).await {
        Ok(r) => Json(r).into_response(),
        Err(e) => map_err(e),
    }
}

pub async fn push_handler(State(st): State<Arc<AppState>>, headers: HeaderMap, Json(b): Json<RepoBody>) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    match push_remote(Path::new(&b.repo), b.remote.as_deref()).await {
        Ok(r) => Json(r).into_response(),
        Err(e) => map_err(e),
    }
}

pub async fn commit_handler(State(st): State<Arc<AppState>>, headers: HeaderMap, Json(b): Json<CommitBody>) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    match commit_smart(Path::new(&b.repo), &b.message, b.amend).await {
        Ok(r) => Json(r).into_response(),
        Err(e) => map_err(e),
    }
}

pub async fn stash_save_handler(State(st): State<Arc<AppState>>, headers: HeaderMap, Json(b): Json<StashBody>) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    match stash_save(Path::new(&b.repo), b.message.as_deref()).await {
        Ok(r) => Json(r).into_response(),
        Err(e) => map_err(e),
    }
}

pub async fn stash_pop_handler(State(st): State<Arc<AppState>>, headers: HeaderMap, Json(b): Json<RepoBody>) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    match stash_pop(Path::new(&b.repo)).await {
        Ok(r) => Json(r).into_response(),
        Err(e) => map_err(e),
    }
}

pub async fn abort_merge_handler(State(st): State<Arc<AppState>>, headers: HeaderMap, Json(b): Json<RepoBody>) -> Response {
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

pub async fn preview_push_handler(State(st): State<Arc<AppState>>, headers: HeaderMap, Json(b): Json<RepoBody>) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    match preview_push_for_remote(Path::new(&b.repo), b.remote.as_deref()).await {
        Ok(r) => Json(r).into_response(),
        Err(e) => map_err(e),
    }
}

pub async fn switch_remote_handler(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(b): Json<RepoBody>,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    match switch_remote(Path::new(&b.repo), b.remote.as_deref().unwrap_or("")).await {
        Ok(r) => Json(r).into_response(),
        Err(e) => map_err(e),
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
    Json(ConflictChatResponse { sent, prompt, error }).into_response()
}
