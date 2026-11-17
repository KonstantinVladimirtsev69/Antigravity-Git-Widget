//! Antigravity 2.0 Sidecar HTTP server (Phase 1 Spike).

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Json},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, sync::Arc};
use tokio::process::Command;

#[derive(Clone)]
struct AppState {
    ui_token: Option<String>,
    agentapi_exe: Option<String>,
    project_id: Option<String>,
    conversation_id: Option<String>,
}

#[derive(Serialize)]
struct EnvReport {
    core_version: &'static str,
    sidecar_port: Option<String>,
    has_ui_token: bool,
    has_ls_address: bool,
    has_csrf_token: bool,
    project_id: Option<String>,
    conversation_id: Option<String>,
    agentapi_exe: Option<String>,
}

#[derive(Deserialize)]
struct TestChatRequest {
    conversation_id: Option<String>,
    message: String,
}

#[derive(Serialize)]
struct TestChatResponse {
    ok: bool,
    stdout: String,
    stderr: String,
}

fn is_authorized(state: &AppState, headers: &HeaderMap) -> bool {
    let Some(expected) = state.ui_token.as_deref() else {
        return true;
    };
    headers
        .get("x-sidecar-token")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|provided| provided == expected)
}

async fn index_handler() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn env_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if !is_authorized(&state, &headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "unauthorized" })),
        )
            .into_response();
    }

    let report = EnvReport {
        core_version: git_widget_core::core_version(),
        sidecar_port: std::env::var("ANTIGRAVITY_SIDECAR_WEB_PORT").ok(),
        has_ui_token: state.ui_token.is_some(),
        has_ls_address: std::env::var("ANTIGRAVITY_LS_ADDRESS").is_ok(),
        has_csrf_token: std::env::var("ANTIGRAVITY_CSRF_TOKEN").is_ok(),
        project_id: state.project_id.clone(),
        conversation_id: state.conversation_id.clone(),
        agentapi_exe: state.agentapi_exe.clone(),
    };

    (StatusCode::OK, Json(report)).into_response()
}

async fn test_chat_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<TestChatRequest>,
) -> impl IntoResponse {
    if !is_authorized(&state, &headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "unauthorized" })),
        )
            .into_response();
    }

    let Some(conv_id) = payload
        .conversation_id
        .or_else(|| state.conversation_id.clone())
    else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "missing conversation_id" })),
        )
            .into_response();
    };

    let (program, prefix_args): (&str, Vec<&str>) = match state.agentapi_exe.as_deref() {
        Some(exe) if !exe.is_empty() => (exe, vec!["agentapi"]),
        _ => ("agentapi", vec![]),
    };

    let mut cmd = Command::new(program);
    cmd.args(prefix_args)
        .arg("send-message")
        .arg(&conv_id)
        .arg(&payload.message);

    if let Some(pid) = &state.project_id {
        cmd.env("ANTIGRAVITY_PROJECT_ID", pid);
    }

    match cmd.output().await {
        Ok(out) => (
            StatusCode::OK,
            Json(TestChatResponse {
                ok: out.status.success(),
                stdout: String::from_utf8_lossy(&out.stdout).to_string(),
                stderr: String::from_utf8_lossy(&out.stderr).to_string(),
            }),
        )
            .into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": err.to_string() })),
        )
            .into_response(),
    }
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
    });

    let app = Router::new()
        .route("/", get(index_handler))
        .route("/api/env", get(env_handler))
        .route("/api/test-chat", post(test_chat_handler))
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("failed to bind loopback address");
    println!("[git-widget-sidecar] listening on http://{addr}");
    axum::serve(listener, app).await.expect("server error");
}

const INDEX_HTML: &str = r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>Git Widget Spike</title>
  <style>
    body {
      margin: 0; padding: 12px;
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
      background: var(--background, #18181b);
      color: var(--foreground, #f4f4f5);
      font-size: 13px;
    }
    pre {
      background: rgba(128,128,128,0.12);
      padding: 8px; border-radius: 6px;
      overflow-x: auto; font-size: 11px;
    }
    button {
      padding: 6px 10px; border-radius: 6px; border: 1px solid rgba(128,128,128,0.3);
      background: rgba(128,128,128,0.2); color: inherit; cursor: pointer;
    }
  </style>
</head>
<body>
  <h3 style="margin-top:0">Git Widget — Phase 1 Spike</h3>
  <div><strong>Active Workspaces (postMessage):</strong></div>
  <pre id="workspaces">Waiting for workspace-change...</pre>
  <div><strong>URL Query &amp; Backend Env:</strong></div>
  <pre id="env">Loading...</pre>
  <button id="pingBtn">Test agentapi send-message</button>
  <pre id="chatOut"></pre>
  <script>
    const params = new URLSearchParams(location.search);
    const token = params.get('token') || '';
    const conversationId = params.get('conversationId') || '';

    window.addEventListener('message', (event) => {
      if (event.data?.type === 'theme-change') {
        const vars = event.data.payload?.cssVariables || {};
        for (const [k, v] of Object.entries(vars)) {
          document.documentElement.style.setProperty(k, v);
        }
      } else if (event.data?.type === 'workspace-change') {
        const uris = event.data.payload?.workspaceUris || [];
        document.getElementById('workspaces').textContent = JSON.stringify(uris, null, 2);
      }
    });

    const headers = token ? { 'X-Sidecar-Token': token } : {};
    fetch('/api/env', { headers })
      .then(r => r.json())
      .then(data => {
        document.getElementById('env').textContent = JSON.stringify({
          queryConversationId: conversationId,
          backend: data
        }, null, 2);
      })
      .catch(err => {
        document.getElementById('env').textContent = String(err);
      });

    document.getElementById('pingBtn').onclick = async () => {
      const out = document.getElementById('chatOut');
      out.textContent = 'Sending...';
      try {
        const res = await fetch('/api/test-chat', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json', ...headers },
          body: JSON.stringify({
            conversation_id: conversationId || null,
            message: '[Git Widget Spike] Test message from AUX_PANE'
          })
        });
        out.textContent = JSON.stringify(await res.json(), null, 2);
      } catch (e) {
        out.textContent = String(e);
      }
    };
  </script>
</body>
</html>
"#;
