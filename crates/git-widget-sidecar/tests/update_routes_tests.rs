use axum::{
    routing::{get, post},
    Router,
};
use git_widget_core::{integrate::IntegrationMode, scanner::ScannerConfig};
use git_widget_sidecar::{
    clone_routes::CloneManager,
    routes::{get_repos, merge_config_preserving_token, post_config, AppState, PublicScannerConfig},
    update_routes::{
        check_update_handler, install_update_handler, update_status_handler, UpdateCheckResponse,
        UpdateManager, UpdateStage, UpdateStatusResponse,
    },
};
use sha2::{Digest, Sha256};
use std::{
    fs,
    net::SocketAddr,
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
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

fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    format!("{:x}", h.finalize())
}

#[derive(Clone)]
struct MockReply {
    status: u16,
    body: Vec<u8>,
    content_type: &'static str,
    delay_ms: u64,
}

async fn spawn_mock_github(
    handler: impl Fn(&str) -> MockReply + Send + Sync + 'static,
) -> (String, Arc<AtomicUsize>, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let release_hits = Arc::new(AtomicUsize::new(0));
    let asset_hits = Arc::new(AtomicUsize::new(0));
    let rh = Arc::clone(&release_hits);
    let ah = Arc::clone(&asset_hits);
    let handler = Arc::new(handler);

    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let h = Arc::clone(&handler);
            let rh = Arc::clone(&rh);
            let ah = Arc::clone(&ah);
            tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                let path = req
                    .lines()
                    .next()
                    .and_then(|l| l.split_whitespace().nth(1))
                    .unwrap_or("/");
                if path.contains("/releases/latest") {
                    rh.fetch_add(1, Ordering::SeqCst);
                } else {
                    ah.fetch_add(1, Ordering::SeqCst);
                }
                let reply = h(path);
                if reply.delay_ms > 0 {
                    tokio::time::sleep(Duration::from_millis(reply.delay_ms)).await;
                }
                let reason = match reply.status {
                    200 => "OK",
                    403 => "Forbidden",
                    404 => "Not Found",
                    429 => "Too Many Requests",
                    _ => "Error",
                };
                let head = format!(
                    "HTTP/1.1 {} {reason}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    reply.status,
                    reply.content_type,
                    reply.body.len()
                );
                let _ = stream.write_all(head.as_bytes()).await;
                let _ = stream.write_all(&reply.body).await;
            });
        }
    });

    (format!("http://{addr}"), release_hits, asset_hits)
}

async fn spawn_sidecar_router(state: Arc<AppState>) -> SocketAddr {
    let app = Router::new()
        .route("/api/repos", get(get_repos))
        .route("/api/config", post(post_config))
        .route("/api/update/check", get(check_update_handler))
        .route("/api/update/status", get(update_status_handler))
        .route("/api/update/install", post(install_update_handler))
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

fn make_state(
    ui_token: Option<&str>,
    update_mgr: UpdateManager,
    update_repo: Option<&str>,
) -> Arc<AppState> {
    Arc::new(AppState {
        ui_token: ui_token.map(ToString::to_string),
        agentapi_exe: None,
        project_id: None,
        conversation_id: None,
        config: Arc::new(RwLock::new(ScannerConfig {
            roots: vec![],
            max_depth: 1,
            github_token: Some("ghp_secret_should_not_leak".into()),
            github_oauth_client_id: None,
            integration_mode: IntegrationMode::Merge,
            update_repo: update_repo.map(ToString::to_string),
            last_clone_dir: None,
        })),
        serve_port: Some(41234),
        port_file: None,
        update: Arc::new(update_mgr),
        clone: Arc::new(CloneManager::new_for_tests()),
    })
}

#[tokio::test]
async fn check_update_enforces_auth_caches_6h_and_force_bypasses() {
    let release_json = serde_json::json!({
        "tag_name": "v9.9.9",
        "draft": false,
        "prerelease": false,
        "body": "Release notes v9.9.9",
        "html_url": "https://github.com/o/r/releases/tag/v9.9.9",
        "published_at": "2026-11-25T00:00:00Z",
        "assets": []
    })
    .to_string();
    let (mock_base, hits, asset_hits) = spawn_mock_github(move |_| MockReply {
        status: 200,
        body: release_json.as_bytes().to_vec(),
        content_type: "application/json",
        delay_ms: 0,
    })
    .await;

    let mgr = UpdateManager::new_for_tests(
        mock_base.clone(),
        vec![mock_base],
        None,
        None,
        true,
    );
    let addr = spawn_sidecar_router(make_state(Some("tok-123"), mgr, Some("o/r"))).await;

    let (unauth_status, _) = http_req(addr, "GET", "/api/update/check", None, None).await;
    assert_eq!(unauth_status, 401);

    let (st1, b1) = http_req(addr, "GET", "/api/update/check", Some("tok-123"), None).await;
    assert_eq!(st1, 200);
    let r1: UpdateCheckResponse = serde_json::from_str(&b1).unwrap();
    assert!(r1.update_available);
    assert_eq!(r1.latest_version.as_deref(), Some("9.9.9"));
    assert!(!r1.cached);
    assert_eq!(hits.load(Ordering::SeqCst), 1);

    // Second call uses 6h TTL cache
    let (st2, b2) = http_req(addr, "GET", "/api/update/check", Some("tok-123"), None).await;
    assert_eq!(st2, 200);
    let r2: UpdateCheckResponse = serde_json::from_str(&b2).unwrap();
    assert!(r2.cached);
    assert!(r2.update_available);
    assert_eq!(hits.load(Ordering::SeqCst), 1);

    // force=true bypasses cache
    let (st3, b3) = http_req(addr, "GET", "/api/update/check?force=true", Some("tok-123"), None).await;
    assert_eq!(st3, 200);
    let r3: UpdateCheckResponse = serde_json::from_str(&b3).unwrap();
    assert!(!r3.cached);
    assert_eq!(hits.load(Ordering::SeqCst), 2);
    // AC-7: check never downloads assets
    assert_eq!(asset_hits.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn check_update_handles_rate_limit_and_offline_without_caching_errors() {
    let calls = Arc::new(AtomicUsize::new(0));
    let calls_cl = Arc::clone(&calls);
    let (mock_base, _, _) = spawn_mock_github(move |_| {
        let n = calls_cl.fetch_add(1, Ordering::SeqCst);
        if n == 0 {
            MockReply {
                status: 403,
                body: b"{}".to_vec(),
                content_type: "application/json",
                delay_ms: 0,
            }
        } else {
            MockReply {
                status: 200,
                body: serde_json::json!({
                    "tag_name": "v9.9.9",
                    "draft": false,
                    "prerelease": false,
                    "assets": []
                })
                .to_string()
                .into_bytes(),
                content_type: "application/json",
                delay_ms: 0,
            }
        }
    })
    .await;

    let mgr = UpdateManager::new_for_tests(mock_base.clone(), vec![mock_base], None, None, true);
    let addr = spawn_sidecar_router(make_state(None, mgr, Some("o/r"))).await;

    let (st1, b1) = http_req(addr, "GET", "/api/update/check", None, None).await;
    assert_eq!(st1, 200);
    let r1: UpdateCheckResponse = serde_json::from_str(&b1).unwrap();
    assert!(!r1.update_available);
    assert!(r1.error.as_deref().is_some_and(|e| e.contains("403")));

    // Error must NOT be cached, so next check retries and succeeds
    let (st2, b2) = http_req(addr, "GET", "/api/update/check", None, None).await;
    assert_eq!(st2, 200);
    let r2: UpdateCheckResponse = serde_json::from_str(&b2).unwrap();
    assert!(r2.update_available);
    assert_eq!(r2.error, None);

    // Closed/unreachable port resilience
    let dead_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let dead_addr = dead_listener.local_addr().unwrap();
    drop(dead_listener);
    let dead_base = format!("http://{dead_addr}");
    let mgr_off = UpdateManager::new_for_tests(dead_base.clone(), vec![dead_base], None, None, true);
    let addr_off = spawn_sidecar_router(make_state(None, mgr_off, Some("o/r"))).await;
    let (st_off, b_off) = http_req(addr_off, "GET", "/api/update/check", None, None).await;
    assert_eq!(st_off, 200);
    let r_off: UpdateCheckResponse = serde_json::from_str(&b_off).unwrap();
    assert!(!r_off.update_available);
    assert!(r_off.error.is_some());
}

#[tokio::test]
async fn install_update_preflight_no_host_and_single_flight_202_409() {
    let root = tmp_dir("gw_update_routes_install");
    let exe_bytes = b"MZ-TEST-EXE-PAYLOAD";
    let exe_hash = sha256_hex(exe_bytes);

    // Build mock server aware of its own base URL
    let base_holder = Arc::new(RwLock::new(String::new()));
    let bh_cl = Arc::clone(&base_holder);
    let sums_text2 = format!("{exe_hash}  git-widget-sidecar.exe\r\n");
    let (mock_base, _, asset_hits) = spawn_mock_github(move |path| {
        if path.contains("/releases/latest") {
            let base = futures_block_read(&bh_cl);
            let json = serde_json::json!({
                "tag_name": "v9.9.9",
                "draft": false,
                "prerelease": false,
                "assets": [
                    {
                        "name": "git-widget-sidecar.exe",
                        "browser_download_url": format!("{base}/dl/git-widget-sidecar.exe")
                    },
                    {
                        "name": "SHA256SUMS.txt",
                        "browser_download_url": format!("{base}/dl/SHA256SUMS.txt")
                    }
                ]
            })
            .to_string();
            MockReply {
                status: 200,
                body: json.into_bytes(),
                content_type: "application/json",
                delay_ms: 350,
            }
        } else if path.ends_with("/SHA256SUMS.txt") {
            MockReply {
                status: 200,
                body: sums_text2.as_bytes().to_vec(),
                content_type: "text/plain",
                delay_ms: 0,
            }
        } else {
            MockReply {
                status: 200,
                body: exe_bytes.to_vec(),
                content_type: "application/octet-stream",
                delay_ms: 0,
            }
        }
    })
    .await;
    *base_holder.write().await = mock_base.clone();

    // Case A: empty host overrides -> error without downloading assets
    let empty_ag = root.join("empty_ag");
    let empty_t3 = root.join("empty_t3");
    fs::create_dir_all(&empty_ag).unwrap();
    fs::create_dir_all(&empty_t3).unwrap();
    let mgr_empty = UpdateManager::new_for_tests(
        mock_base.clone(),
        vec![mock_base.clone()],
        Some(empty_ag),
        Some(empty_t3),
        true,
    );
    let addr_empty = spawn_sidecar_router(make_state(None, mgr_empty, Some("o/r"))).await;
    let (st_a, _) = http_req(addr_empty, "POST", "/api/update/install", None, Some("{}")).await;
    assert_eq!(st_a, 202);
    tokio::time::sleep(Duration::from_millis(120)).await;
    let (_, st_body) = http_req(addr_empty, "GET", "/api/update/status", None, None).await;
    let status_a: UpdateStatusResponse = serde_json::from_str(&st_body).unwrap();
    assert_eq!(status_a.stage, UpdateStage::Error);
    assert!(!status_a.in_progress);
    assert_eq!(asset_hits.load(Ordering::SeqCst), 0);

    // Case B: valid host -> 202 Accepted then concurrent 409 Conflict, reaches Restarting
    let t3_dir = root.join("t3_valid");
    fs::create_dir_all(&t3_dir).unwrap();
    fs::write(t3_dir.join("t3-loader.cjs"), "// loader").unwrap();
    let mgr = UpdateManager::new_for_tests(
        mock_base.clone(),
        vec![mock_base],
        None,
        Some(t3_dir),
        true,
    );
    let addr = spawn_sidecar_router(make_state(Some("tok"), mgr, Some("o/r"))).await;

    let (unauth, _) = http_req(addr, "POST", "/api/update/install", None, Some("{}")).await;
    assert_eq!(unauth, 401);

    let (c1, b1) = http_req(addr, "POST", "/api/update/install", Some("tok"), Some("{}")).await;
    assert_eq!(c1, 202);
    let s1: UpdateStatusResponse = serde_json::from_str(&b1).unwrap();
    assert!(s1.in_progress);

    let (c2, b2) = http_req(addr, "POST", "/api/update/install", Some("tok"), Some("{}")).await;
    assert_eq!(c2, 409);
    let s2: UpdateStatusResponse = serde_json::from_str(&b2).unwrap();
    assert!(s2.in_progress);

    let (cs, bs) = http_req(addr, "GET", "/api/update/status", Some("tok"), None).await;
    assert_eq!(cs, 200);
    let ss: UpdateStatusResponse = serde_json::from_str(&bs).unwrap();
    assert!(ss.in_progress);

    // Wait for background pipeline to finish verification -> restarting
    let mut final_status = ss;
    for _ in 0..30 {
        tokio::time::sleep(Duration::from_millis(80)).await;
        let (_, body) = http_req(addr, "GET", "/api/update/status", Some("tok"), None).await;
        final_status = serde_json::from_str(&body).unwrap();
        if final_status.stage == UpdateStage::Restarting || final_status.stage == UpdateStage::Error
        {
            break;
        }
    }
    assert_eq!(
        final_status.stage,
        UpdateStage::Restarting,
        "unexpected final status: {final_status:?}"
    );
    assert_eq!(final_status.target_version.as_deref(), Some("9.9.9"));

    // Case C: AC-10 downgrade / older tag rejected before asset download
    let hits_before_downgrade = asset_hits.load(Ordering::SeqCst);
    let (mock_old, _, old_asset_hits) = spawn_mock_github(|_| MockReply {
        status: 200,
        body: serde_json::json!({
            "tag_name": "v0.0.1",
            "draft": false,
            "prerelease": false,
            "assets": []
        })
        .to_string()
        .into_bytes(),
        content_type: "application/json",
        delay_ms: 0,
    })
    .await;
    let t3_dir2 = root.join("t3_valid2");
    fs::create_dir_all(&t3_dir2).unwrap();
    fs::write(t3_dir2.join("t3-loader.cjs"), "// loader").unwrap();
    let mgr_old = UpdateManager::new_for_tests(
        mock_old.clone(),
        vec![mock_old],
        None,
        Some(t3_dir2),
        true,
    );
    let addr_old = spawn_sidecar_router(make_state(None, mgr_old, Some("o/r"))).await;
    let (st_old, _) = http_req(addr_old, "POST", "/api/update/install", None, Some("{}")).await;
    assert_eq!(st_old, 202);
    tokio::time::sleep(Duration::from_millis(150)).await;
    let (_, b_old) = http_req(addr_old, "GET", "/api/update/status", None, None).await;
    let s_old: UpdateStatusResponse = serde_json::from_str(&b_old).unwrap();
    assert_eq!(s_old.stage, UpdateStage::Error);
    assert_eq!(old_asset_hits.load(Ordering::SeqCst), 0);
    let _ = hits_before_downgrade;

    let _ = fs::remove_dir_all(&root);
}

fn futures_block_read(lock: &Arc<RwLock<String>>) -> String {
    loop {
        if let Ok(g) = lock.try_read() {
            return g.clone();
        }
        std::thread::yield_now();
    }
}

#[tokio::test]
async fn config_validates_resets_update_repo_and_invalidates_update_cache() {
    let base_cfg = ScannerConfig {
        roots: vec![],
        max_depth: 2,
        github_token: Some("ghp_keep_me".into()),
        github_oauth_client_id: None,
        integration_mode: IntegrationMode::Merge,
        update_repo: Some("owner/custom".into()),
        last_clone_dir: None,
    };
    let pub_cfg = PublicScannerConfig::from(&base_cfg);
    assert_eq!(pub_cfg.update_repo.as_deref(), Some("owner/custom"));
    assert!(!serde_json::to_string(&pub_cfg)
        .unwrap()
        .contains("ghp_keep_me"));

    // merge_config_preserving_token allows clearing update_repo to None
    let cleared = merge_config_preserving_token(
        &base_cfg,
        ScannerConfig {
            roots: vec![],
            max_depth: 2,
            github_token: None,
            github_oauth_client_id: None,
            integration_mode: IntegrationMode::Merge,
            update_repo: Some("   ".into()),
            last_clone_dir: None,
        },
    );
    assert_eq!(cleared.github_token.as_deref(), Some("ghp_keep_me"));
    assert_eq!(cleared.update_repo, None);

    let (mock_base, hits, _) = spawn_mock_github(|_| MockReply {
        status: 200,
        body: serde_json::json!({
            "tag_name": "v9.9.9",
            "draft": false,
            "prerelease": false,
            "assets": []
        })
        .to_string()
        .into_bytes(),
        content_type: "application/json",
        delay_ms: 0,
    })
    .await;

    let mgr = UpdateManager::new_for_tests(mock_base.clone(), vec![mock_base], None, None, true);
    let addr = spawn_sidecar_router(make_state(None, mgr, Some("owner/one"))).await;

    // Warm up cache
    let (st1, _) = http_req(addr, "GET", "/api/update/check", None, None).await;
    assert_eq!(st1, 200);
    assert_eq!(hits.load(Ordering::SeqCst), 1);

    // Invalid update_repo rejected with 400
    let bad_body = serde_json::json!({
        "roots": [],
        "max_depth": 1,
        "integration_mode": "merge",
        "update_repo": "https://evil.example/a/b"
    })
    .to_string();
    let (bad_st, _) = http_req(addr, "POST", "/api/config", None, Some(&bad_body)).await;
    assert_eq!(bad_st, 400);

    // Valid update_repo accepted and invalidates update cache
    let good_body = serde_json::json!({
        "roots": [],
        "max_depth": 1,
        "integration_mode": "merge",
        "update_repo": "owner/two"
    })
    .to_string();
    let (good_st, good_resp) = http_req(addr, "POST", "/api/config", None, Some(&good_body)).await;
    assert_eq!(good_st, 200);
    assert!(good_resp.contains("owner/two"));

    let (st2, b2) = http_req(addr, "GET", "/api/update/check", None, None).await;
    assert_eq!(st2, 200);
    let r2: UpdateCheckResponse = serde_json::from_str(&b2).unwrap();
    assert!(!r2.cached);
    assert_eq!(r2.repo, "owner/two");
    assert_eq!(hits.load(Ordering::SeqCst), 2);
}
