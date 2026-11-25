use axum::{
    routing::{get, post},
    Router,
};
use git_widget_core::{
    clone_progress::CloneStage, integrate::IntegrationMode, scanner::ScannerConfig,
};
use git_widget_sidecar::{
    clone_routes::{
        cancel_clone_handler, clone_status_handler, start_clone_handler, CloneManager,
        CloneStatusResponse,
    },
    fs_routes::{open_explorer_handler, pick_folder_handler},
    routes::{get_repos, post_config, AppState},
    update_routes::UpdateManager,
};
use std::{
    fs,
    net::SocketAddr,
    path::{Path, PathBuf},
    process::Command as StdCommand,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::RwLock,
};

fn tmp_dir(prefix: &str) -> PathBuf {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("{prefix}_{}_{n}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn git_sync(dir: &Path, args: &[&str]) {
    let out = StdCommand::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
}

fn create_bare_repo(root: &Path) -> PathBuf {
    let work = root.join("src-work");
    let bare = root.join("bare-remote.git");
    fs::create_dir_all(&work).unwrap();
    git_sync(&work, &["init", "-b", "main"]);
    git_sync(&work, &["config", "user.email", "test@example.com"]);
    git_sync(&work, &["config", "user.name", "Test User"]);
    fs::write(work.join("README.md"), "# Clone Sidecar Test\n").unwrap();
    git_sync(&work, &["add", "."]);
    git_sync(&work, &["commit", "-m", "initial commit"]);
    let out = StdCommand::new("git")
        .args(["clone", "--bare", "--"])
        .arg(&work)
        .arg(&bare)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "bare clone failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    bare
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
    let status: u16 = head
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    (status, resp_body.to_string())
}

#[tokio::test]
async fn fs_routes_auth_and_validation() {
    let addr = spawn_sidecar(make_state(Some("tok-123"))).await;

    let (st_pick_unauth, _) =
        http_req(addr, "POST", "/api/fs/pick-folder", None, Some("{}")).await;
    assert_eq!(st_pick_unauth, 401);

    let (st_exp_unauth, _) = http_req(
        addr,
        "POST",
        "/api/fs/open-explorer",
        None,
        Some(r#"{"path":"C:\\Windows"}"#),
    )
    .await;
    assert_eq!(st_exp_unauth, 401);

    let missing_path = tmp_dir("gw_fs_missing").join("does_not_exist");
    let body = serde_json::json!({ "path": missing_path.to_string_lossy() }).to_string();
    let (st_missing, _) =
        http_req(addr, "POST", "/api/fs/open-explorer", Some("tok-123"), Some(&body)).await;
    assert_eq!(st_missing, 400);

    let (st_empty, _) = http_req(
        addr,
        "POST",
        "/api/fs/open-explorer",
        Some("tok-123"),
        Some(r#"{"path":"   "}"#),
    )
    .await;
    assert_eq!(st_empty, 400);
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

    let (st_u1, _) = http_req(
        addr,
        "POST",
        "/api/clone",
        None,
        Some(r#"{"url":"owner/repo","parent_dir":"C:\\","folder_name":"repo"}"#),
    )
    .await;
    assert_eq!(st_u1, 401);
    let (st_u2, _) = http_req(addr, "GET", "/api/clone/status", None, None).await;
    assert_eq!(st_u2, 401);
    let (st_u3, _) = http_req(addr, "POST", "/api/clone/cancel", None, Some("{}")).await;
    assert_eq!(st_u3, 401);

    for bad_payload in [
        serde_json::json!({ "url": "--upload-pack=x", "parent_dir": parent, "folder_name": "ok" }),
        serde_json::json!({ "url": &bare_url, "parent_dir": parent, "folder_name": "CON" }),
        serde_json::json!({ "url": &bare_url, "parent_dir": parent, "folder_name": "../evil" }),
        serde_json::json!({ "url": &bare_url, "parent_dir": parent, "folder_name": "existing" }),
    ] {
        let (st_bad, _) = http_req(
            addr,
            "POST",
            "/api/clone",
            Some("tok-123"),
            Some(&bad_payload.to_string()),
        )
        .await;
        assert_eq!(st_bad, 400, "expected 400 for {bad_payload}");
    }

    let good_payload = serde_json::json!({
        "url": bare_url,
        "parent_dir": parent,
        "folder_name": "my cloned app"
    })
    .to_string();
    let (st_ok, body_ok) = http_req(
        addr,
        "POST",
        "/api/clone",
        Some("tok-123"),
        Some(&good_payload),
    )
    .await;
    assert_eq!(st_ok, 202, "body: {body_ok}");
    let init_snap: CloneStatusResponse = serde_json::from_str(&body_ok).unwrap();
    assert!(init_snap.in_progress);

    let mut final_snap = init_snap;
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(60)).await;
        let (st_s, body_s) =
            http_req(addr, "GET", "/api/clone/status", Some("tok-123"), None).await;
        assert_eq!(st_s, 200);
        final_snap = serde_json::from_str(&body_s).unwrap();
        if !final_snap.in_progress {
            break;
        }
    }
    assert_eq!(final_snap.stage, CloneStage::Done, "snap: {final_snap:?}");
    assert_eq!(final_snap.percent, 100);
    assert!(final_snap.sanitized_url.as_deref().is_some_and(|u| !u.contains('@')));
    let cloned = parent.join("my cloned app");
    assert!(cloned.join(".git").is_dir() && cloned.join("README.md").is_file());

    let _ = fs::remove_dir_all(&root);
}
