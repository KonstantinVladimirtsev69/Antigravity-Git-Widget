//! Branch CRUD, Stash manager, bulk staging, and commit action routes.

use crate::routes::{check_auth, map_err, AppState, RepoBody, RepoQuery};
use axum::{
    extract::{Query, State},
    http::HeaderMap,
    response::{IntoResponse, Json, Response},
};
use git_widget_core::{
    branches::{
        checkout_remote_branch, cherry_pick, create_branch, delete_branch, undo_last_commit,
    },
    ops::{discard_all, stage_all, unstage_all},
    stash::{list_stashes, stash_apply, stash_drop, stash_pop_at},
};
use serde::Deserialize;
use std::{path::Path, sync::Arc};

#[derive(Deserialize)]
pub struct CreateBranchBody {
    pub repo: String,
    pub name: String,
}

#[derive(Deserialize)]
pub struct DeleteBranchBody {
    pub repo: String,
    pub name: String,
    #[serde(default)]
    pub force: bool,
}

#[derive(Deserialize)]
pub struct RemoteBranchBody {
    pub repo: String,
    pub remote_branch: String,
}

#[derive(Deserialize)]
pub struct CherryPickBody {
    pub repo: String,
    pub hash: String,
}

#[derive(Deserialize)]
pub struct StashIndexBody {
    pub repo: String,
    pub index: usize,
}

pub async fn create_branch_handler(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(b): Json<CreateBranchBody>,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    match create_branch(Path::new(&b.repo), &b.name).await {
        Ok(r) => Json(r).into_response(),
        Err(e) => map_err(e),
    }
}

pub async fn delete_branch_handler(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(b): Json<DeleteBranchBody>,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    match delete_branch(Path::new(&b.repo), &b.name, b.force).await {
        Ok(r) => Json(r).into_response(),
        Err(e) => map_err(e),
    }
}

pub async fn checkout_remote_handler(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(b): Json<RemoteBranchBody>,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    match checkout_remote_branch(Path::new(&b.repo), &b.remote_branch).await {
        Ok(r) => Json(r).into_response(),
        Err(e) => map_err(e),
    }
}

pub async fn undo_commit_handler(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(b): Json<RepoBody>,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    match undo_last_commit(Path::new(&b.repo)).await {
        Ok(r) => Json(r).into_response(),
        Err(e) => map_err(e),
    }
}

pub async fn cherry_pick_handler(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(b): Json<CherryPickBody>,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    match cherry_pick(Path::new(&b.repo), &b.hash).await {
        Ok(r) => Json(r).into_response(),
        Err(e) => map_err(e),
    }
}

pub async fn stage_all_handler(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(b): Json<RepoBody>,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    match stage_all(Path::new(&b.repo)).await {
        Ok(r) => Json(r).into_response(),
        Err(e) => map_err(e),
    }
}

pub async fn unstage_all_handler(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(b): Json<RepoBody>,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    match unstage_all(Path::new(&b.repo)).await {
        Ok(r) => Json(r).into_response(),
        Err(e) => map_err(e),
    }
}

pub async fn discard_all_handler(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(b): Json<RepoBody>,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    match discard_all(Path::new(&b.repo)).await {
        Ok(r) => Json(r).into_response(),
        Err(e) => map_err(e),
    }
}

pub async fn list_stashes_handler(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<RepoQuery>,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    match list_stashes(Path::new(&q.repo)).await {
        Ok(list) => Json(list).into_response(),
        Err(e) => map_err(e),
    }
}

pub async fn stash_apply_handler(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(b): Json<StashIndexBody>,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    match stash_apply(Path::new(&b.repo), b.index).await {
        Ok(r) => Json(r).into_response(),
        Err(e) => map_err(e),
    }
}

pub async fn stash_pop_at_handler(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(b): Json<StashIndexBody>,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    match stash_pop_at(Path::new(&b.repo), b.index).await {
        Ok(r) => Json(r).into_response(),
        Err(e) => map_err(e),
    }
}

pub async fn stash_drop_handler(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(b): Json<StashIndexBody>,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    match stash_drop(Path::new(&b.repo), b.index).await {
        Ok(r) => Json(r).into_response(),
        Err(e) => map_err(e),
    }
}
