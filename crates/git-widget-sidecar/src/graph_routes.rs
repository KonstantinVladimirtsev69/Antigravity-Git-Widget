//! Commit graph, commit file details, PR listing, and AI commit message routes.

use crate::{
    config::save_config,
    routes::{check_auth, map_err, send_agentapi_message, AppState, RepoQuery},
};
use axum::{
    extract::{Query, State},
    http::HeaderMap,
    response::{IntoResponse, Json, Response},
};
use git_widget_core::{
    diff::get_diff_files,
    github::{create_github_repo, CreateGithubRepoReq},
    graph::{get_commit_details, get_commit_graph, list_github_prs},
};
use serde::{Deserialize, Serialize};
use std::{path::Path, sync::Arc};

#[derive(Deserialize)]
pub struct GraphQuery {
    pub repo: String,
    #[serde(default = "default_true")]
    pub all: bool,
    pub author: Option<String>,
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
    match get_commit_graph(Path::new(&q.repo), q.all, q.author.as_deref(), 50).await {
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
    let prs = list_github_prs(Path::new(&q.repo)).await;
    Json(prs).into_response()
}

#[derive(Deserialize)]
pub struct CreateGithubBody {
    pub repo: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub private: bool,
    #[serde(default)]
    pub use_ssh: bool,
    pub token: Option<String>,
}

pub async fn create_github_repo_handler(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(b): Json<CreateGithubBody>,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    let provided_tok = b
        .token
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if let Some(ref tok) = provided_tok {
        let mut cfg = st.config.write().await;
        cfg.github_token = Some(tok.clone());
        let _ = save_config(&cfg);
    }
    let saved_tok = st.config.read().await.github_token.clone();
    let req = CreateGithubRepoReq {
        name: b.name,
        description: b.description,
        private: b.private,
        use_ssh: b.use_ssh,
        token: provided_tok.or(saved_tok),
    };
    match create_github_repo(Path::new(&b.repo), &req).await {
        Ok(res) => Json(res).into_response(),
        Err(e) => map_err(e),
    }
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
