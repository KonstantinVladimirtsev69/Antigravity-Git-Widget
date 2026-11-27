//! Commit graph, commit file details, PR listing, and AI commit message routes.

use crate::routes::{check_auth, map_err, send_agentapi_message, AppState, RepoQuery};
use axum::{
    extract::{Query, State},
    http::HeaderMap,
    response::{IntoResponse, Json, Response},
};
use git_widget_core::{
    diff::get_diff_files,
    github_prs::list_github_prs_for_remote_with_token,
    graph::{get_commit_details, get_commit_graph_for_remote},
};
use serde::{Deserialize, Serialize};
use std::{path::Path, sync::Arc};

#[derive(Deserialize)]
pub struct GraphQuery {
    pub repo: String,
    #[serde(default = "default_true")]
    pub all: bool,
    pub author: Option<String>,
    #[serde(default)]
    pub remote: Option<String>,
}

fn default_true() -> bool {
    true
}

#[derive(Deserialize)]
pub struct CommitDetailQuery {
    pub repo: String,
    pub hash: String,
}

#[derive(Deserialize)]
pub struct SuggestCommitBody {
    pub repo: String,
    pub conversation_id: Option<String>,
}

#[derive(Serialize)]
pub struct SuggestCommitResponse {
    pub sent: bool,
    pub prompt: String,
    pub error: Option<String>,
}

pub async fn graph_handler(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<GraphQuery>,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    match get_commit_graph_for_remote(
        Path::new(&q.repo),
        q.all,
        q.author.as_deref(),
        q.remote.as_deref(),
        50,
    )
    .await
    {
        Ok(data) => Json(data).into_response(),
        Err(e) => map_err(e),
    }
}

pub async fn commit_details_handler(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<CommitDetailQuery>,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    match get_commit_details(Path::new(&q.repo), &q.hash).await {
        Ok(details) => Json(details).into_response(),
        Err(e) => map_err(e),
    }
}

pub async fn prs_handler(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<RepoQuery>,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    let token = st.config.read().await.github_token.clone();
    let prs =
        list_github_prs_for_remote_with_token(Path::new(&q.repo), token.as_deref(), q.remote.as_deref())
            .await;
    Json(prs).into_response()
}

pub async fn suggest_commit_handler(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(b): Json<SuggestCommitBody>,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    let files = match get_diff_files(Path::new(&b.repo)).await {
        Ok(f) => f,
        Err(e) => return map_err(e),
    };
    let summary: Vec<String> = files
        .iter()
        .take(25)
        .map(|f| format!("- {} ({}, +{} -{})", f.path, f.status, f.additions, f.deletions))
        .collect();
    let prompt = format!(
        "Предложи краткое Conventional Commit сообщение для изменений в `{}`:\n\n{}",
        b.repo,
        summary.join("\n")
    );
    let (sent, error) = send_agentapi_message(&st, b.conversation_id, &prompt).await;
    Json(SuggestCommitResponse {
        sent,
        prompt,
        error,
    })
    .into_response()
}
