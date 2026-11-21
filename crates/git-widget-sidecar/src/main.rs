//! Antigravity 2.0 Sidecar HTTP server with embedded React Web UI.

mod branch_routes;
mod config;
mod diff_routes;
mod git_routes;
mod github_routes;
mod graph_routes;
mod installer;
mod integrate_routes;
mod routes;
mod workspace;

use axum::{
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use branch_routes::{
    checkout_remote_handler, cherry_pick_handler, create_branch_handler, delete_branch_handler,
    discard_all_handler, list_stashes_handler, stage_all_handler, stash_apply_handler,
    stash_drop_handler, stash_pop_at_handler, undo_commit_handler, unstage_all_handler,
};
use diff_routes::{
    accept_hunk_handler, ask_ai_handler, commits_list_handler, diff_list_handler,
    discard_file_handler, reject_hunk_handler, stage_file_handler, unstage_file_handler,
};
use github_routes::{
    create_github_repo_handler, oauth_logout_handler, oauth_poll_handler, oauth_start_handler,
};
use graph_routes::{commit_details_handler, graph_handler, prs_handler, suggest_commit_handler};
use integrate_routes::{
    abort_rebase_handler, continue_rebase_handler, integrate_handler, preview_integrate_handler,
    pull_both_handler, save_integration_mode_handler, scan_sync_handler,
};
use git_routes::{
    abort_merge_handler, add_remote_handler, commit_handler, fetch_handler, guarded_handler,
    init_repo_handler, preview_merge_handler, preview_push_handler, publish_handler, pull_handler,
    push_handler, send_conflict_to_chat_handler, stash_pop_handler, stash_save_handler,
};
use routes::{get_repos, init_config, post_config, status_handler, AppState};
use workspace::resolve_workspace;
use rust_embed::Embed;
use std::{net::SocketAddr, sync::Arc};

#[derive(Embed)]
#[folder = "../../ui/dist"]
struct UiAssets;

async fn static_handler(uri: Uri) -> Response {
    let raw = uri.path().trim_start_matches('/');
    let path = if raw.is_empty() { "index.html" } else { raw };

    if let Some(file) = UiAssets::get(path) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        return (
            StatusCode::OK,
            [(header::CONTENT_TYPE, mime.as_ref())],
            file.data,
        )
            .into_response();
    }
    if !path.contains('.') {
        if let Some(index) = UiAssets::get("index.html") {
            return (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
                index.data,
            )
                .into_response();
        }
    }
    (StatusCode::NOT_FOUND, "Not found").into_response()
}

#[tokio::main]
async fn main() {
    if installer::should_run_installer() {
        installer::run_self_install();
        return;
    }

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
        .route(
            "/api/config/integration-mode",
            post(save_integration_mode_handler),
        )
        .route("/api/resolve-workspace", post(resolve_workspace))
        .route("/api/status", get(status_handler))
        .route("/api/git/guarded", post(guarded_handler))
        .route("/api/git/scan-sync", post(scan_sync_handler))
        .route("/api/git/pull-both", post(pull_both_handler))
        .route("/api/git/preview-integrate", post(preview_integrate_handler))
        .route("/api/git/integrate", post(integrate_handler))
        .route("/api/git/abort-rebase", post(abort_rebase_handler))
        .route("/api/git/continue-rebase", post(continue_rebase_handler))
        .route("/api/git/fetch", post(fetch_handler))
        .route("/api/git/pull", post(pull_handler))
        .route("/api/git/push", post(push_handler))
        .route("/api/git/publish", post(publish_handler))
        .route("/api/git/add-remote", post(add_remote_handler))
        .route("/api/git/init-repo", post(init_repo_handler))
        .route("/api/git/commit", post(commit_handler))
        .route("/api/git/stash-save", post(stash_save_handler))
        .route("/api/git/stash-pop", post(stash_pop_handler))
        .route("/api/git/stashes", get(list_stashes_handler))
        .route("/api/git/stash-apply", post(stash_apply_handler))
        .route("/api/git/stash-pop-at", post(stash_pop_at_handler))
        .route("/api/git/stash-drop", post(stash_drop_handler))
        .route("/api/git/branch-create", post(create_branch_handler))
        .route("/api/git/branch-delete", post(delete_branch_handler))
        .route("/api/git/branch-checkout-remote", post(checkout_remote_handler))
        .route("/api/git/undo-commit", post(undo_commit_handler))
        .route("/api/git/cherry-pick", post(cherry_pick_handler))
        .route("/api/git/stage-all", post(stage_all_handler))
        .route("/api/git/unstage-all", post(unstage_all_handler))
        .route("/api/git/discard-all", post(discard_all_handler))
        .route("/api/git/abort-merge", post(abort_merge_handler))
        .route("/api/git/preview-merge", post(preview_merge_handler))
        .route("/api/git/preview-push", post(preview_push_handler))
        .route(
            "/api/git/send-conflict-to-chat",
            post(send_conflict_to_chat_handler),
        )
        .route("/api/commits", get(commits_list_handler))
        .route("/api/graph", get(graph_handler))
        .route("/api/commit-details", get(commit_details_handler))
        .route("/api/prs", get(prs_handler))
        .route("/api/github/create-repo", post(create_github_repo_handler))
        .route("/api/github/oauth/start", post(oauth_start_handler))
        .route("/api/github/oauth/poll", post(oauth_poll_handler))
        .route("/api/github/oauth/logout", post(oauth_logout_handler))
        .route("/api/ai/suggest-commit", post(suggest_commit_handler))
        .route("/api/diff", get(diff_list_handler))
        .route("/api/diff/accept-hunk", post(accept_hunk_handler))
        .route("/api/diff/reject-hunk", post(reject_hunk_handler))
        .route("/api/diff/stage-file", post(stage_file_handler))
        .route("/api/diff/unstage-file", post(unstage_file_handler))
        .route("/api/diff/discard-file", post(discard_file_handler))
        .route("/api/diff/ask-ai", post(ask_ai_handler))
        .fallback(static_handler)
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("failed to bind loopback address");
    println!("[git-widget-sidecar] listening on http://{addr}");
    axum::serve(listener, app).await.expect("server error");
}
