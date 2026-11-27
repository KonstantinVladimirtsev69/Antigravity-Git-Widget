use axum::{routing::{get, post}, Router};
use git_widget_core::{
    integrate::IntegrationMode,
    ops::OpResult,
    remote_focus::SwitchRemoteResult,
    safety::{ActionPreview, GuardedOpOutcome},
    scanner::ScannerConfig,
    status::RepoStatus,
    sync::DualBranchSync,
};
use git_widget_sidecar::{
    branch_routes::checkout_remote_handler,
    clone_routes::CloneManager,
    git_routes::{guarded_handler, preview_push_handler, switch_remote_handler},
    graph_routes::{graph_handler, prs_handler},
    integrate_routes::{pull_both_handler, scan_sync_handler},
    routes::{status_handler, AppState},
    update_routes::UpdateManager,
};
use serde_json::Value;
use std::{
    fs,
    net::SocketAddr,
    path::{Path, PathBuf},
    process::Command as StdCommand,
    sync::{Arc, atomic::{AtomicU64, Ordering}},
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::RwLock,
};

static COUNTER: AtomicU64 = AtomicU64::new(1);

fn tmp_dir(prefix: &str) -> PathBuf {
    let n = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("{prefix}_{}_{n}_{seq}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn git_sync(dir: &Path, args: &[&str]) -> String {
    let out = StdCommand::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {:?} failed in {}: {}",
        args,
        dir.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn git_try(dir: &Path, args: &[&str]) -> Option<String> {
    let out = StdCommand::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .unwrap();
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn file_url(p: &Path) -> String {
    format!("file:///{}", p.to_string_lossy().replace('\\', "/"))
}

fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn setup_two_remote_repo(prefix: &str) -> (PathBuf, PathBuf, PathBuf, PathBuf) {
    let root = tmp_dir(prefix);
    let seed = root.join("seed");
    fs::create_dir_all(&seed).unwrap();
    git_sync(&seed, &["init", "-b", "main"]);
    git_sync(&seed, &["config", "user.email", "test@example.com"]);
    git_sync(&seed, &["config", "user.name", "Test User"]);
    fs::write(seed.join("README.md"), "# base\n").unwrap();
    git_sync(&seed, &["add", "."]);
    git_sync(&seed, &["commit", "-m", "origin base commit"]);

    let origin_bare = root.join("origin.git");
    let second_bare = root.join("second.git");
    let out1 = StdCommand::new("git")
        .args(["clone", "--bare", "--"])
        .arg(&seed)
        .arg(&origin_bare)
        .output()
        .unwrap();
    assert!(out1.status.success());
    let out2 = StdCommand::new("git")
        .args(["clone", "--bare", "--"])
        .arg(&seed)
        .arg(&second_bare)
        .output()
        .unwrap();
    assert!(out2.status.success());

    let local = root.join("local");
    let out3 = StdCommand::new("git")
        .args(["clone", "--"])
        .arg(file_url(&origin_bare))
        .arg(&local)
        .output()
        .unwrap();
    assert!(out3.status.success());
    git_sync(&local, &["config", "user.email", "test@example.com"]);
    git_sync(&local, &["config", "user.name", "Test User"]);
    git_sync(&local, &["remote", "add", "second", &file_url(&second_bare)]);
    git_sync(&local, &["fetch", "second"]);

    (root, origin_bare, second_bare, local)
}

fn make_state(ui_token: Option<&str>) -> Arc<AppState> {
    Arc::new(AppState {
        ui_token: ui_token.map(ToString::to_string),
        agentapi_exe: None,
        project_id: None,
        conversation_id: None,
        config: Arc::new(RwLock::new(ScannerConfig {
            roots: vec![],
            max_depth: 1,
            github_token: None,
            github_oauth_client_id: None,
            integration_mode: IntegrationMode::Merge,
            update_repo: None,
            last_clone_dir: None,
        })),
        serve_port: Some(41234),
        port_file: None,
        update: Arc::new(UpdateManager::new_for_tests(
            "http://127.0.0.1:0",
            vec![],
            None,
            None,
            true,
        )),
        clone: Arc::new(CloneManager::new_for_tests()),
    })
}

async fn spawn_sidecar(state: Arc<AppState>) -> SocketAddr {
    let app = Router::new()
        .route("/api/status", get(status_handler))
        .route("/api/graph", get(graph_handler))
        .route("/api/prs", get(prs_handler))
        .route("/api/git/switch-remote", post(switch_remote_handler))
        .route("/api/git/preview-push", post(preview_push_handler))
        .route("/api/git/guarded", post(guarded_handler))
        .route("/api/git/branch-checkout-remote", post(checkout_remote_handler))
        .route("/api/git/scan-sync", post(scan_sync_handler))
        .route("/api/git/pull-both", post(pull_both_handler))
        .with_state(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    addr
}

async fn http_req(
    addr: SocketAddr,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: Option<&str>,
) -> (u16, String) {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let body_bytes = body.unwrap_or("");
    let mut req = format!(
        "{method} {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n",
        body_bytes.len()
    );
    if let Some(tok) = token {
        req.push_str(&format!("X-Sidecar-Token: {tok}\r\n"));
    }
    req.push_str("\r\n");
    req.push_str(body_bytes);
    stream.write_all(req.as_bytes()).await.unwrap();
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).await.unwrap();
    let text = String::from_utf8_lossy(&raw).to_string();
    let (head, resp_body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
    let status = head
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    (status, resp_body.to_string())
}

#[tokio::test]
async fn switch_remote_endpoint_returns_scoped_status_and_handles_invalid_remote() {
    let (root, _origin_bare, _second_bare, local) = setup_two_remote_repo("gw_sr_ok");
    let addr = spawn_sidecar(make_state(Some("tok-13"))).await;
    let repo_str = local.to_string_lossy().to_string();

    let payload_second = serde_json::json!({ "repo": repo_str, "remote": "second" }).to_string();
    assert_eq!(
        http_req(addr, "POST", "/api/git/switch-remote", None, Some(&payload_second))
            .await
            .0,
        401
    );

    for bad_remote in ["nonexistent", ""] {
        let bad = serde_json::json!({ "repo": repo_str, "remote": bad_remote }).to_string();
        let (st, _) = http_req(addr, "POST", "/api/git/switch-remote", Some("tok-13"), Some(&bad)).await;
        assert_eq!(st, 400, "expected 400 for remote={bad_remote:?}");
    }

    let (st_ok, body_ok) =
        http_req(addr, "POST", "/api/git/switch-remote", Some("tok-13"), Some(&payload_second)).await;
    assert_eq!(st_ok, 200, "body: {body_ok}");
    let res: SwitchRemoteResult = serde_json::from_str(&body_ok).unwrap();
    assert!(res.upstream_switched);
    assert_eq!(res.target_upstream.as_deref(), Some("second/main"));
    assert_eq!(res.fetch_warning, None);
    assert_eq!(res.status.upstream.as_deref(), Some("second/main"));
    assert_eq!(res.status.configured_upstream.as_deref(), Some("second/main"));

    // Branch only present on origin: switching to second must NOT clear/alter its .git/config upstream
    git_sync(&local, &["checkout", "-b", "origin-only"]);
    git_sync(&local, &["push", "-u", "origin", "origin-only"]);
    let (st_no, body_no) =
        http_req(addr, "POST", "/api/git/switch-remote", Some("tok-13"), Some(&payload_second)).await;
    assert_eq!(st_no, 200, "body: {body_no}");
    let res_no: SwitchRemoteResult = serde_json::from_str(&body_no).unwrap();
    assert!(!res_no.upstream_switched);
    assert_eq!(res_no.target_upstream, None);
    assert_eq!(res_no.status.upstream, None);
    assert_eq!(
        git_try(&local, &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"])
            .as_deref(),
        Some("origin/origin-only")
    );

    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
async fn switch_remote_endpoint_returns_fetch_warning_when_offline() {
    let (root, _origin_bare, _second_bare, local) = setup_two_remote_repo("gw_sr_offline");
    let missing_remote = root.join("missing-second.git");
    git_sync(
        &local,
        &["remote", "set-url", "second", &file_url(&missing_remote)],
    );

    let addr = spawn_sidecar(make_state(Some("tok-off"))).await;
    let payload = serde_json::json!({
        "repo": local.to_string_lossy(),
        "remote": "second"
    })
    .to_string();

    let (st, body) =
        http_req(addr, "POST", "/api/git/switch-remote", Some("tok-off"), Some(&payload)).await;
    assert_eq!(st, 200, "expected 200 even on offline fetch failure: {body}");
    let res: SwitchRemoteResult = serde_json::from_str(&body).unwrap();
    assert!(
        res.fetch_warning.as_deref().is_some_and(|w| !w.trim().is_empty()),
        "expected non-empty fetch_warning, got {res:?}"
    );
    assert!(res.upstream_switched);
    assert_eq!(res.target_upstream.as_deref(), Some("second/main"));
    assert_eq!(res.status.upstream.as_deref(), Some("second/main"));

    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
async fn status_graph_prs_preview_push_respect_remote_param() {
    let (root, origin_bare, second_bare, local) = setup_two_remote_repo("gw_sr_scoped");

    // Create a branch unique to origin and a commit unique to second
    let w_origin = root.join("w-origin");
    StdCommand::new("git")
        .args(["clone", "--"])
        .arg(file_url(&origin_bare))
        .arg(&w_origin)
        .output()
        .unwrap();
    git_sync(&w_origin, &["config", "user.email", "test@example.com"]);
    git_sync(&w_origin, &["config", "user.name", "Test User"]);
    git_sync(&w_origin, &["checkout", "-b", "origin-secret"]);
    fs::write(w_origin.join("o.txt"), "origin only\n").unwrap();
    git_sync(&w_origin, &["add", "."]);
    git_sync(&w_origin, &["commit", "-m", "origin secret commit"]);
    git_sync(&w_origin, &["push", "origin", "origin-secret"]);
    git_sync(&local, &["fetch", "origin"]);

    let w_second = root.join("w-second");
    StdCommand::new("git")
        .args(["clone", "--"])
        .arg(file_url(&second_bare))
        .arg(&w_second)
        .output()
        .unwrap();
    git_sync(&w_second, &["config", "user.email", "test@example.com"]);
    git_sync(&w_second, &["config", "user.name", "Test User"]);
    git_sync(&w_second, &["checkout", "-b", "dev"]);
    fs::write(w_second.join("dev.txt"), "dev on second\n").unwrap();
    git_sync(&w_second, &["add", "."]);
    git_sync(&w_second, &["commit", "-m", "second dev commit"]);
    git_sync(&w_second, &["push", "origin", "HEAD:dev"]);
    git_sync(&local, &["fetch", "second"]);

    // Local dev branch + unpushed commit on main so preview-push sees 1 commit ahead of second/main
    git_sync(&local, &["checkout", "-b", "dev", "second/dev"]);
    git_sync(&local, &["checkout", "main"]);
    fs::write(local.join("ahead.txt"), "local ahead\n").unwrap();
    git_sync(&local, &["add", "."]);
    git_sync(&local, &["commit", "-m", "local ahead commit"]);

    // Point origin to a GitHub URL while keeping second as a local non-GitHub path
    git_sync(
        &local,
        &[
            "remote",
            "set-url",
            "origin",
            "https://github.com/octocat/Hello-World.git",
        ],
    );

    let addr = spawn_sidecar(make_state(Some("tok-q"))).await;
    let repo_q = url_encode(&local.to_string_lossy());

    // 1) /api/status?repo=...&remote=second vs omitted remote
    let (st_s, body_s) = http_req(
        addr,
        "GET",
        &format!("/api/status?repo={repo_q}&remote=second"),
        Some("tok-q"),
        None,
    )
    .await;
    assert_eq!(st_s, 200, "status body: {body_s}");
    let status_second: RepoStatus = serde_json::from_str(&body_s).unwrap();
    assert_eq!(status_second.upstream.as_deref(), Some("second/main"));
    assert_eq!(
        status_second
            .branch_tracking
            .get("main")
            .and_then(|t| t.upstream.as_deref()),
        Some("second/main")
    );

    let (st_def, body_def) = http_req(
        addr,
        "GET",
        &format!("/api/status?repo={repo_q}"),
        Some("tok-q"),
        None,
    )
    .await;
    assert_eq!(st_def, 200);
    let status_def: RepoStatus = serde_json::from_str(&body_def).unwrap();
    assert_eq!(status_def.upstream.as_deref(), Some("origin/main"));

    // 2) /api/graph?repo=...&all=true&remote=second and all=false
    let (st_ga, body_ga) = http_req(
        addr,
        "GET",
        &format!("/api/graph?repo={repo_q}&all=true&remote=second"),
        Some("tok-q"),
        None,
    )
    .await;
    assert_eq!(st_ga, 200, "graph all=true: {body_ga}");
    let graph_all: Value = serde_json::from_str(&body_ga).unwrap();
    assert!(graph_all["github_url"].is_null());
    let commits_all = graph_all["commits"].as_array().unwrap();
    assert!(
        commits_all.iter().all(|c| c["refs"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| !r["name"].as_str().unwrap_or("").starts_with("origin/"))),
        "foreign origin refs leaked into remote=second graph: {graph_all}"
    );
    assert!(commits_all.iter().any(|c| c["refs"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["name"].as_str() == Some("second/main"))));

    let (st_gf, body_gf) = http_req(
        addr,
        "GET",
        &format!("/api/graph?repo={repo_q}&all=false&remote=second"),
        Some("tok-q"),
        None,
    )
    .await;
    assert_eq!(st_gf, 200, "graph all=false: {body_gf}");
    let graph_cur: Value = serde_json::from_str(&body_gf).unwrap();
    let commits_cur = graph_cur["commits"].as_array().unwrap();
    assert!(
        commits_cur
            .iter()
            .all(|c| c["subject"].as_str() != Some("origin secret commit")
                && c["subject"].as_str() != Some("second dev commit")),
        "all=false should exclude unrelated branches: {graph_cur}"
    );

    // 3) /api/prs?repo=...&remote=second (non-GitHub second must NOT fall back to GitHub origin)
    let (st_pr, body_pr) = http_req(
        addr,
        "GET",
        &format!("/api/prs?repo={repo_q}&remote=second"),
        Some("tok-q"),
        None,
    )
    .await;
    assert_eq!(st_pr, 200, "prs body: {body_pr}");
    let prs_val: Value = serde_json::from_str(&body_pr).unwrap();
    assert!(prs_val["github_url"].is_null());
    assert_eq!(prs_val["prs"].as_array().unwrap().len(), 0);

    // 4) POST /api/git/preview-push with remote=second
    let push_req = serde_json::json!({
        "repo": local.to_string_lossy(),
        "remote": "second"
    })
    .to_string();
    let (st_pp, body_pp) =
        http_req(addr, "POST", "/api/git/preview-push", Some("tok-q"), Some(&push_req)).await;
    assert_eq!(st_pp, 200, "preview-push body: {body_pp}");
    let preview: ActionPreview = serde_json::from_str(&body_pp).unwrap();
    assert_eq!(preview.target, "second/main");
    assert_eq!(preview.commit_count, 1);

    // 5) POST /api/git/scan-sync and POST /api/git/pull-both forward remote cleanly
    let sync_req = serde_json::json!({
        "repo": local.to_string_lossy(),
        "source": "dev",
        "target": "main",
        "remote": "second",
        "fetch": false
    })
    .to_string();
    let (st_sy, body_sy) =
        http_req(addr, "POST", "/api/git/scan-sync", Some("tok-q"), Some(&sync_req)).await;
    assert_eq!(st_sy, 200, "scan-sync body: {body_sy}");
    let sync_res: DualBranchSync = serde_json::from_str(&body_sy).unwrap();
    assert_eq!(sync_res.target.ahead, 1);

    let pull_req = serde_json::json!({
        "repo": local.to_string_lossy(),
        "source": "dev",
        "target": "main",
        "remote": "second"
    })
    .to_string();
    let (st_pb, body_pb) =
        http_req(addr, "POST", "/api/git/pull-both", Some("tok-q"), Some(&pull_req)).await;
    assert_eq!(st_pb, 200, "pull-both body: {body_pb}");
    let pb_res: OpResult = serde_json::from_str(&body_pb).unwrap();
    assert!(pb_res.success);

    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
async fn guarded_checkout_and_remote_checkout_accept_optional_remote() {
    let (root, origin_bare, second_bare, local) = setup_two_remote_repo("gw_sr_checkout");

    // Push `dev` to both origin and second, and `feature-x` to second
    let w_origin = root.join("w-origin");
    StdCommand::new("git")
        .args(["clone", "--"])
        .arg(file_url(&origin_bare))
        .arg(&w_origin)
        .output()
        .unwrap();
    git_sync(&w_origin, &["config", "user.email", "test@example.com"]);
    git_sync(&w_origin, &["config", "user.name", "Test User"]);
    git_sync(&w_origin, &["checkout", "-b", "dev"]);
    git_sync(&w_origin, &["push", "origin", "dev"]);
    git_sync(&w_origin, &["checkout", "-b", "feat-origin"]);
    git_sync(&w_origin, &["push", "origin", "feat-origin"]);
    git_sync(&local, &["fetch", "origin"]);

    let w_second = root.join("w-second");
    StdCommand::new("git")
        .args(["clone", "--"])
        .arg(file_url(&second_bare))
        .arg(&w_second)
        .output()
        .unwrap();
    git_sync(&w_second, &["config", "user.email", "test@example.com"]);
    git_sync(&w_second, &["config", "user.name", "Test User"]);
    git_sync(&w_second, &["checkout", "-b", "dev"]);
    git_sync(&w_second, &["push", "origin", "dev"]);
    git_sync(&w_second, &["checkout", "-b", "feature-x"]);
    git_sync(&w_second, &["push", "origin", "feature-x"]);
    git_sync(&local, &["fetch", "second"]);

    // Local `dev` initially tracks `origin/dev`, while we stay on `main`
    git_sync(&local, &["branch", "dev", "origin/dev"]);

    let addr = spawn_sidecar(make_state(Some("tok-co"))).await;

    // 1) POST /api/git/guarded checkout dev with remote="second"
    let guarded_req = serde_json::json!({
        "repo": local.to_string_lossy(),
        "kind": "checkout",
        "target": "dev",
        "strategy": { "mode": "fail_if_dirty" },
        "remote": "second"
    })
    .to_string();
    let (st_g, body_g) =
        http_req(addr, "POST", "/api/git/guarded", Some("tok-co"), Some(&guarded_req)).await;
    assert_eq!(st_g, 200, "guarded body: {body_g}");
    let out_g: GuardedOpOutcome = serde_json::from_str(&body_g).unwrap();
    assert!(!out_g.blocked_dirty);
    assert!(out_g.op_result.as_ref().is_some_and(|r| r.success));
    assert_eq!(
        git_try(&local, &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"])
            .as_deref(),
        Some("second/dev")
    );

    // 2) POST /api/git/branch-checkout-remote with optional remote="second"
    let co_req = serde_json::json!({
        "repo": local.to_string_lossy(),
        "remote_branch": "second/feature-x",
        "remote": "second"
    })
    .to_string();
    let (st_co, body_co) = http_req(
        addr,
        "POST",
        "/api/git/branch-checkout-remote",
        Some("tok-co"),
        Some(&co_req),
    )
    .await;
    assert_eq!(st_co, 200, "checkout-remote body: {body_co}");
    let res_co: OpResult = serde_json::from_str(&body_co).unwrap();
    assert!(res_co.success);
    assert_eq!(
        git_try(&local, &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"])
            .as_deref(),
        Some("second/feature-x")
    );

    // 3) Backward compatibility: omitting remote on branch-checkout-remote
    let co_compat = serde_json::json!({
        "repo": local.to_string_lossy(),
        "remote_branch": "origin/feat-origin"
    })
    .to_string();
    let (st_cc, body_cc) = http_req(
        addr,
        "POST",
        "/api/git/branch-checkout-remote",
        Some("tok-co"),
        Some(&co_compat),
    )
    .await;
    assert_eq!(st_cc, 200, "checkout-remote compat body: {body_cc}");
    let res_cc: OpResult = serde_json::from_str(&body_cc).unwrap();
    assert!(res_cc.success);

    let _ = fs::remove_dir_all(&root);
}
