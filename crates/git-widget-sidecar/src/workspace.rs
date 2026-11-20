//! Workspace URI resolution helper for Antigravity `workspace-change` events.

use crate::routes::{check_auth, scan_with_cfg, AppState};
use axum::{
    extract::State,
    http::HeaderMap,
    response::{IntoResponse, Json, Response},
};
use serde::Deserialize;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Deserialize)]
pub struct ResolveWorkspaceBody {
    pub workspace_uris: Vec<String>,
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (
                (bytes[i + 1] as char).to_digit(16),
                (bytes[i + 2] as char).to_digit(16),
            ) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn normalize_path_str(p: &Path) -> String {
    let s = p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    let text = s.to_string_lossy().to_string();
    text.strip_prefix(r"\\?\")
        .unwrap_or(&text)
        .trim_end_matches('\\')
        .to_lowercase()
}

fn uri_to_path(uri: &str) -> PathBuf {
    let trimmed = uri
        .strip_prefix("file:///")
        .or_else(|| uri.strip_prefix("file://"))
        .unwrap_or(uri);
    let decoded = percent_decode(trimmed).replace('/', "\\");
    PathBuf::from(decoded)
}

pub async fn resolve_workspace(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<ResolveWorkspaceBody>,
) -> Response {
    if let Err(r) = check_auth(&st, &headers) {
        return r;
    }
    let cfg = st.config.read().await.clone();
    let repos = scan_with_cfg(&cfg).await.unwrap_or_default();

    for uri in &body.workspace_uris {
        let ws = uri_to_path(uri);
        let ws_norm = normalize_path_str(&ws);
        if let Some(found) = repos.iter().find(|r| {
            let r_norm = normalize_path_str(Path::new(&r.path));
            r_norm == ws_norm || r_norm.starts_with(&format!("{ws_norm}\\"))
        }) {
            return Json(serde_json::json!({ "matched_path": found.path })).into_response();
        }
        if ws.join(".git").exists() {
            let clean = ws.to_string_lossy().trim_end_matches('\\').to_string();
            return Json(serde_json::json!({ "matched_path": clean })).into_response();
        }
        let mut cur = ws.as_path();
        while let Some(parent) = cur.parent() {
            if parent.join(".git").exists() {
                let p_norm = normalize_path_str(parent);
                let matched = repos
                    .iter()
                    .find(|r| normalize_path_str(Path::new(&r.path)) == p_norm)
                    .map(|r| r.path.clone())
                    .unwrap_or_else(|| parent.to_string_lossy().to_string());
                return Json(serde_json::json!({ "matched_path": matched })).into_response();
            }
            cur = parent;
        }
    }
    Json(serde_json::json!({ "matched_path": null })).into_response()
}
