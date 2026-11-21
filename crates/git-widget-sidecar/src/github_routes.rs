//! GitHub repository creation and OAuth Device Flow routes.

use crate::{
    config::save_config,
    routes::{check_auth, map_err, AppState},
};
use axum::{
    extract::State,
    http::HeaderMap,
    response::{IntoResponse, Json, Response},
};
use git_widget_core::{
    github::{create_github_repo, CreateGithubRepoReq},
    github_oauth::{
        poll_device_flow, resolve_oauth_client_id, start_device_flow, DevicePollResult,
    },
};
use serde::Deserialize;
use std::{path::Path, sync::Arc};

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

#[derive(Deserialize)]
pub struct OAuthPollBody {
    pub device_code: String,
}

async fn current_client_id(st: &AppState) -> String {
    let cfg = st.config.read().await;
    let env_id = std::env::var("GITHUB_OAUTH_CLIENT_ID").ok();
    resolve_oauth_client_id(cfg.github_oauth_client_id.as_deref(), env_id.as_deref())
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
    let saved_tok = st.config.read().await.github_token.clone();
    let req = CreateGithubRepoReq {
        name: b.name,
        description: b.description,
        private: b.private,
        use_ssh: b.use_ssh,
        token: provided_tok.clone().or(saved_tok),
    };
    match create_github_repo(Path::new(&b.repo), &req).await {
        Ok(res) => {
            if let Some(tok) = provided_tok {
                let mut cfg = st.config.write().await;
                cfg.github_token = Some(tok);
                let _ = save_config(&cfg);
            }
            Json(res).into_response()
        }
        Err(e) => map_err(e),
    }
}

pub async fn oauth_start_handler(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    let client_id = current_client_id(&st).await;
    match start_device_flow(&client_id).await {
        Ok(flow) => Json(flow).into_response(),
        Err(e) => map_err(e),
    }
}

pub async fn oauth_poll_handler(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(b): Json<OAuthPollBody>,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    let client_id = current_client_id(&st).await;
    match poll_device_flow(&client_id, &b.device_code).await {
        Ok(DevicePollResult::Authorized { access_token }) => {
            let mut cfg = st.config.write().await;
            cfg.github_token = Some(access_token);
            let _ = save_config(&cfg);
            Json(serde_json::json!({ "status": "authorized" })).into_response()
        }
        Ok(DevicePollResult::Pending { interval }) => {
            Json(serde_json::json!({ "status": "pending", "interval": interval })).into_response()
        }
        Ok(DevicePollResult::Expired) => {
            Json(serde_json::json!({ "status": "expired" })).into_response()
        }
        Ok(DevicePollResult::Denied) => {
            Json(serde_json::json!({ "status": "denied" })).into_response()
        }
        Ok(DevicePollResult::Error { message }) => {
            Json(serde_json::json!({ "status": "error", "message": message })).into_response()
        }
        Err(e) => map_err(e),
    }
}

pub async fn oauth_logout_handler(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    let mut cfg = st.config.write().await;
    cfg.github_token = None;
    let _ = save_config(&cfg);
    Json(serde_json::json!({ "ok": true })).into_response()
}
