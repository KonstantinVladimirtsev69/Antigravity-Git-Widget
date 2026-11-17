//! Antigravity 2.0 Sidecar HTTP server with embedded React Web UI.

mod config;
mod routes;

use axum::{
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use routes::{
    abort_merge_handler, commit_handler, fetch_handler, get_repos, guarded_handler, init_config,
    post_config, preview_merge_handler, preview_push_handler, push_handler, resolve_workspace,
    send_conflict_to_chat_handler, stash_pop_handler, stash_save_handler, status_handler, AppState,
};
use rust_embed::Embed;
use std::{net::SocketAddr, sync::Arc};

#[derive(Embed)]
#[folder = "../../ui/dist"]
struct UiAssets;

async fn static_handler(uri: Uri) -> Response {
    let raw = uri.path().trim_start_matches('/');
    let path = if raw.is_empty() { "index.html" } else { raw };

    if let Some(file) = UiAssets::get(path).or_else(|| UiAssets::get("index.html")) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        return (
            StatusCode::OK,
            [(header::CONTENT_TYPE, mime.as_ref())],
            file.data,
        )
            .into_response();
    }
    (StatusCode::NOT_FOUND, "UI assets not found").into_response()
}

#[tokio::main]
async fn main() {
    let port: u16 = std::env::var("ANTIGRAVITY_SIDECAR_WEB_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(39871);

    let state = Arc::new(AppState {
        ui_token: std::env::var("ANTIGRAVITY_SIDECAR_UI_TOKEN").ok(),
        agentapi_exe: std::env::var("ANTIGRAVITY_AGENTAPI_EXE").ok(),
        project_id: std::env::var("ANTIGRAVITY_PROJECT_ID").ok(),
        conversation_id: std::env::var("ANTIGRAVITY_CONVERSATION_ID").ok(),
        config: init_config(),
    });

    let app = Router::new()
        .route("/api/repos", get(get_repos))
        .route("/api/config", post(post_config))
        .route("/api/resolve-workspace", post(resolve_workspace))
        .route("/api/status", get(status_handler))
        .route("/api/git/guarded", post(guarded_handler))
        .route("/api/git/fetch", post(fetch_handler))
        .route("/api/git/push", post(push_handler))
        .route("/api/git/commit", post(commit_handler))
        .route("/api/git/stash-save", post(stash_save_handler))
        .route("/api/git/stash-pop", post(stash_pop_handler))
        .route("/api/git/abort-merge", post(abort_merge_handler))
        .route("/api/git/preview-merge", post(preview_merge_handler))
        .route("/api/git/preview-push", post(preview_push_handler))
        .route(
            "/api/git/send-conflict-to-chat",
            post(send_conflict_to_chat_handler),
        )
        .fallback(static_handler)
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("failed to bind loopback address");
    println!("[git-widget-sidecar] listening on http://{addr}");
    axum::serve(listener, app).await.expect("server error");
}
