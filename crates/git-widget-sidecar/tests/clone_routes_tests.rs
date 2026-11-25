use axum::{routing::{get, post}, Router};
use git_widget_core::{clone_progress::CloneStage, integrate::IntegrationMode, scanner::ScannerConfig};
use git_widget_sidecar::{
    clone_routes::{cancel_clone_handler, clone_status_handler, start_clone_handler, CloneManager, CloneStatusResponse},
    fs_routes::{open_explorer_handler, pick_folder_handler},
    routes::{get_repos, merge_config_preserving_token, post_config, AppState, PublicScannerConfig},
    update_routes::UpdateManager,
};
use std::{
    fs, net::SocketAddr, path::{Path, PathBuf}, process::Command as StdCommand,
    sync::{Arc, OnceLock}, time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::{TcpListener, TcpStream}, sync::{Mutex as AsyncMutex, RwLock}};

static ENV_MUTEX: OnceLock<AsyncMutex<()>> = OnceLock::new();
fn env_lock() -> &'static AsyncMutex<()> { ENV_MUTEX.get_or_init(|| AsyncMutex::new(())) }

struct GitSshEnvGuard { prev: Option<std::ffi::OsString> }
impl GitSshEnvGuard {
    fn set(val: &str) -> Self {
        let prev = std::env::var_os("GIT_SSH_COMMAND");
        std::env::set_var("GIT_SSH_COMMAND", val);
        Self { prev }
    }
}
impl Drop for GitSshEnvGuard {
    fn drop(&mut self) {
        if let Some(ref v) = self.prev { std::env::set_var("GIT_SSH_COMMAND", v); } else { std::env::remove_var("GIT_SSH_COMMAND"); }
    }
}

fn tmp_dir(prefix: &str) -> PathBuf {
    let n = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let dir = std::env::temp_dir().join(format!("{prefix}_{}_{n}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn git_sync(dir: &Path, args: &[&str]) {
    let out = StdCommand::new("git").arg("-C").arg(dir).args(args).env("GIT_TERMINAL_PROMPT", "0").output().unwrap();
    assert!(out.status.success(), "git {:?} failed: {}", args, String::from_utf8_lossy(&out.stderr));
}

fn create_bare_repo(root: &Path) -> PathBuf {
    let (work, bare) = (root.join("src-work"), root.join("bare-remote.git"));
    fs::create_dir_all(&work).unwrap();
    git_sync(&work, &["init", "-b", "main"]);
    git_sync(&work, &["config", "user.email", "test@example.com"]);
    git_sync(&work, &["config", "user.name", "Test User"]);
    fs::write(work.join("README.md"), "# Clone Sidecar Test\n").unwrap();
    git_sync(&work, &["add", "."]);
    git_sync(&work, &["commit", "-m", "initial commit"]);
    let out = StdCommand::new("git").args(["clone", "--bare", "--"]).arg(&work).arg(&bare).env("GIT_TERMINAL_PROMPT", "0").output().unwrap();
    assert!(out.status.success(), "bare clone failed: {}", String::from_utf8_lossy(&out.stderr));
    bare
}

fn make_state(ui_token: Option<&str>) -> Arc<AppState> {
    Arc::new(AppState {
        ui_token: ui_token.map(ToString::to_string),
        agentapi_exe: None,
        project_id: None,
        conversation_id: None,
        config: Arc::new(RwLock::new(ScannerConfig {
            roots: vec![], max_depth: 1, github_token: None, github_oauth_client_id: None,
            integration_mode: IntegrationMode::Merge, update_repo: None, last_clone_dir: None,
        })),
        serve_port: Some(41234),
        port_file: None,
        update: Arc::new(UpdateManager::new_for_tests("http://127.0.0.1:0", vec![], None, None, true)),
        clone: Arc::new(CloneManager::new_for_tests()),
    })
}

async fn spawn_sidecar(state: Arc<AppState>) -> SocketAddr {
    let app = Router::new()
        .route("/api/repos", get(get_repos))
        .route("/api/config", post(post_config))
        .route("/api/clone", post(start_clone_handler))
        .route("/api/clone/status", get(clone_status_handler))
        .route("/api/clone/cancel", post(cancel_clone_handler))
        .route("/api/fs/pick-folder", post(pick_folder_handler))
        .route("/api/fs/open-explorer", post(open_explorer_handler))
        .with_state(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { let _ = axum::serve(listener, app).await; });
    addr
}

async fn http_req(addr: SocketAddr, method: &str, path: &str, token: Option<&str>, body: Option<&str>) -> (u16, String) {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let body_bytes = body.unwrap_or("");
    let mut req = format!("{method} {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n", body_bytes.len());
    if let Some(tok) = token { req.push_str(&format!("X-Sidecar-Token: {tok}\r\n")); }
    req.push_str("\r\n");
    req.push_str(body_bytes);
    stream.write_all(req.as_bytes()).await.unwrap();
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).await.unwrap();
    let text = String::from_utf8_lossy(&raw).to_string();
    let (head, resp_body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
    let status = head.lines().next().and_then(|l| l.split_whitespace().nth(1)).and_then(|s| s.parse().ok()).unwrap_or(0);
    (status, resp_body.to_string())
}

async fn poll_until_idle(addr: SocketAddr, token: Option<&str>) -> CloneStatusResponse {
    let mut snap = CloneStatusResponse::default();
    for _ in 0..60 {
        tokio::time::sleep(Duration::from_millis(60)).await;
        let (st, body) = http_req(addr, "GET", "/api/clone/status", token, None).await;
        assert_eq!(st, 200);
        snap = serde_json::from_str(&body).unwrap();
        if !snap.in_progress { return snap; }
    }
    snap
}

#[tokio::test]
async fn fs_routes_auth_and_validation() {
    let addr = spawn_sidecar(make_state(Some("tok-123"))).await;
    assert_eq!(http_req(addr, "POST", "/api/fs/pick-folder", None, Some("{}")).await.0, 401);
    assert_eq!(http_req(addr, "POST", "/api/fs/open-explorer", None, Some(r#"{"path":"C:\\Windows"}"#)).await.0, 401);

    let root = tmp_dir("gw_fs_missing");
    let body = serde_json::json!({ "path": root.join("does_not_exist").to_string_lossy() }).to_string();
    assert_eq!(http_req(addr, "POST", "/api/fs/open-explorer", Some("tok-123"), Some(&body)).await.0, 400);
    assert_eq!(http_req(addr, "POST", "/api/fs/open-explorer", Some("tok-123"), Some(r#"{"path":"   "}"#)).await.0, 400);
    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
async fn clone_happy_path_and_auth_401() {
    let root = tmp_dir("gw_clone_happy");
    let bare = create_bare_repo(&root);
    let bare_url = format!("file:///{}", bare.to_string_lossy().replace('\\', "/"));
    let parent = root.join("dest");
    fs::create_dir_all(&parent).unwrap();
    let nonempty = parent.join("existing");
    fs::create_dir_all(&nonempty).unwrap();
    fs::write(nonempty.join("keep.txt"), "stay").unwrap();

    let addr = spawn_sidecar(make_state(Some("tok-123"))).await;
    assert_eq!(http_req(addr, "POST", "/api/clone", None, Some(r#"{"url":"owner/repo","parent_dir":"C:\\","folder_name":"repo"}"#)).await.0, 401);
    assert_eq!(http_req(addr, "GET", "/api/clone/status", None, None).await.0, 401);
    assert_eq!(http_req(addr, "POST", "/api/clone/cancel", None, Some("{}")).await.0, 401);

    for bad in [
        serde_json::json!({ "url": "--upload-pack=x", "parent_dir": parent, "folder_name": "ok" }),
        serde_json::json!({ "url": &bare_url, "parent_dir": parent, "folder_name": "" }),
        serde_json::json!({ "url": &bare_url, "parent_dir": parent, "folder_name": "CON" }),
        serde_json::json!({ "url": &bare_url, "parent_dir": parent, "folder_name": "../evil" }),
        serde_json::json!({ "url": &bare_url, "parent_dir": parent, "folder_name": "existing" }),
    ] {
        assert_eq!(http_req(addr, "POST", "/api/clone", Some("tok-123"), Some(&bad.to_string())).await.0, 400, "for {bad}");
    }

    let ok_body = serde_json::json!({ "url": bare_url, "parent_dir": parent, "folder_name": "my cloned app" }).to_string();
    let (st_ok, body_ok) = http_req(addr, "POST", "/api/clone", Some("tok-123"), Some(&ok_body)).await;
    assert_eq!(st_ok, 202, "body: {body_ok}");
    let init_snap: CloneStatusResponse = serde_json::from_str(&body_ok).unwrap();
    assert!(init_snap.in_progress);

    let final_snap = poll_until_idle(addr, Some("tok-123")).await;
    assert_eq!((final_snap.stage, final_snap.percent), (CloneStage::Done, 100), "snap: {final_snap:?}");
    assert!(final_snap.sanitized_url.as_deref().is_some_and(|u| !u.contains('@')));
    assert!(parent.join("my cloned app").join(".git").is_dir() && parent.join("my cloned app").join("README.md").is_file());
    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
async fn clone_single_flight_409() {
    let _guard = env_lock().lock().await;
    let _ssh = GitSshEnvGuard::set("sh -c 'sleep 2'");
    let root = tmp_dir("gw_clone_409");
    let addr = spawn_sidecar(make_state(Some("tok"))).await;

    let p1 = serde_json::json!({ "url": "ssh://git@127.0.0.1/org/one.git", "parent_dir": root, "folder_name": "one" }).to_string();
    let (st1, b1) = http_req(addr, "POST", "/api/clone", Some("tok"), Some(&p1)).await;
    assert_eq!(st1, 202, "b1: {b1}");

    let nonempty = root.join("already_nonempty");
    fs::create_dir_all(&nonempty).unwrap();
    fs::write(nonempty.join("file.txt"), "busy").unwrap();
    let p2 = serde_json::json!({ "url": "ssh://git@127.0.0.1/org/two.git", "parent_dir": root, "folder_name": "already_nonempty" }).to_string();
    let (st2, b2) = http_req(addr, "POST", "/api/clone", Some("tok"), Some(&p2)).await;
    assert_eq!(st2, 409, "expected 409 Conflict, got {st2}: {b2}");
    let conflict: CloneStatusResponse = serde_json::from_str(&b2).unwrap();
    assert!(conflict.in_progress);

    let _ = http_req(addr, "POST", "/api/clone/cancel", Some("tok"), Some("{}")).await;
    let _ = poll_until_idle(addr, Some("tok")).await;
    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
async fn cancel_after_completion_is_idempotent() {
    let _guard = env_lock().lock().await;
    let root = tmp_dir("gw_clone_cancel");
    let pre_empty = root.join("pre_empty");
    fs::create_dir_all(&pre_empty).unwrap();
    let addr = spawn_sidecar(make_state(None)).await;

    {
        let _ssh = GitSshEnvGuard::set("sh -c 'sleep 2'");
        let slow = serde_json::json!({ "url": "ssh://git@127.0.0.1/org/slow.git", "parent_dir": root, "folder_name": "pre_empty" }).to_string();
        assert_eq!(http_req(addr, "POST", "/api/clone", None, Some(&slow)).await.0, 202);
        tokio::time::sleep(Duration::from_millis(120)).await;
        assert_eq!(http_req(addr, "POST", "/api/clone/cancel", None, Some("{}")).await.0, 200);
        let cancelled = poll_until_idle(addr, None).await;
        assert_eq!(cancelled.stage, CloneStage::Cancelled, "snap: {cancelled:?}");
        assert!(pre_empty.is_dir() && fs::read_dir(&pre_empty).unwrap().count() == 0);
    }

    let bare = create_bare_repo(&root);
    let bare_url = format!("file:///{}", bare.to_string_lossy().replace('\\', "/"));
    let done_req = serde_json::json!({ "url": bare_url, "parent_dir": root, "folder_name": "finished" }).to_string();
    assert_eq!(http_req(addr, "POST", "/api/clone", None, Some(&done_req)).await.0, 202);
    let done_snap = poll_until_idle(addr, None).await;
    assert_eq!(done_snap.stage, CloneStage::Done);

    let (st_c, b_c) = http_req(addr, "POST", "/api/clone/cancel", None, Some("{}")).await;
    assert_eq!(st_c, 200);
    let after_cancel: CloneStatusResponse = serde_json::from_str(&b_c).unwrap();
    assert_eq!(after_cancel.stage, CloneStage::Done);
    assert!(root.join("finished").join(".git").is_dir());
    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
async fn clone_preserves_last_clone_dir_and_config_save_keeps_it() {
    let base_cfg = ScannerConfig {
        roots: vec![], max_depth: 2, github_token: None, github_oauth_client_id: None,
        integration_mode: IntegrationMode::Merge, update_repo: None, last_clone_dir: Some(r"D:\Work".into()),
    };
    let pub_val = serde_json::to_value(PublicScannerConfig::from(&base_cfg)).unwrap();
    assert_eq!(pub_val["last_clone_dir"].as_str(), Some(r"D:\Work"));
    let merged = merge_config_preserving_token(&base_cfg, ScannerConfig { last_clone_dir: None, ..base_cfg.clone() });
    assert_eq!(merged.last_clone_dir.as_deref(), Some(r"D:\Work"));

    let root = tmp_dir("gw_last_dir");
    let bare = create_bare_repo(&root);
    let bare_url = format!("file:///{}", bare.to_string_lossy().replace('\\', "/"));
    let parent = root.join("clones");
    fs::create_dir_all(&parent).unwrap();
    let addr = spawn_sidecar(make_state(None)).await;

    let req = serde_json::json!({ "url": bare_url, "parent_dir": parent }).to_string();
    assert_eq!(http_req(addr, "POST", "/api/clone", None, Some(&req)).await.0, 202);
    let done = poll_until_idle(addr, None).await;
    assert_eq!(done.stage, CloneStage::Done);
    assert!(parent.join("bare-remote").join(".git").is_dir());

    let (st_r, body_r) = http_req(addr, "GET", "/api/repos", None, None).await;
    assert_eq!(st_r, 200);
    let repos_json: serde_json::Value = serde_json::from_str(&body_r).unwrap();
    assert_eq!(repos_json["config"]["last_clone_dir"].as_str(), Some(parent.to_string_lossy().as_ref()));

    let cfg_body = serde_json::json!({ "roots": [], "max_depth": 2, "integration_mode": "merge" }).to_string();
    let (st_c, body_c) = http_req(addr, "POST", "/api/config", None, Some(&cfg_body)).await;
    assert_eq!(st_c, 200);
    let cfg_json: serde_json::Value = serde_json::from_str(&body_c).unwrap();
    assert_eq!(cfg_json["config"]["last_clone_dir"].as_str(), Some(parent.to_string_lossy().as_ref()));
    let _ = fs::remove_dir_all(&root);
}
