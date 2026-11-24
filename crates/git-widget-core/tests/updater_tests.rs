use git_widget_core::{
    error::CoreError,
    scanner::ScannerConfig,
    updater::{
        download_verified_release_with_bases, fetch_latest_release_with_base,
        parse_sha256sums_for_asset, resolve_update_repo, validate_release_url,
        validate_update_repo, verify_file_sha256, SemVer, UpdateReleaseInfo,
        DEFAULT_UPDATE_REPO, RELEASE_EXE_ASSET, RELEASE_SUMS_ASSET,
    },
};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs,
    io::{Read, Write},
    net::{SocketAddr, TcpListener},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};
use tempfile::TempDir;

type RouteHandler = Arc<dyn Fn(&str, &str) -> (u16, String, Vec<u8>) + Send + Sync>;

struct MockServer {
    base_url: String,
    requests: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl Drop for MockServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = std::net::TcpStream::connect(self.base_url.trim_start_matches("http://"));
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

fn spawn_mock_server<F>(handler: F) -> MockServer
where
    F: Fn(&str, &str) -> (u16, String, Vec<u8>) + Send + Sync + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let req_clone = Arc::clone(&requests);
    let stop = Arc::new(AtomicBool::new(false));
    let stop_clone = Arc::clone(&stop);
    let handler: RouteHandler = Arc::new(handler);

    let handle = thread::spawn(move || {
        while !stop_clone.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let _ = stream.set_nonblocking(false);
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                    let mut buf = [0u8; 8192];
                    let mut raw = Vec::new();
                    loop {
                        match stream.read(&mut buf) {
                            Ok(0) => break,
                            Ok(n) => {
                                raw.extend_from_slice(&buf[..n]);
                                if raw.windows(4).any(|w| w == b"\r\n\r\n") {
                                    break;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                    let req_text = String::from_utf8_lossy(&raw).to_string();
                    let first_line = req_text.lines().next().unwrap_or("");
                    let path = first_line.split_whitespace().nth(1).unwrap_or("/");
                    req_clone.lock().unwrap().push(req_text.clone());
                    let (status, content_type, body) = handler(path, &req_text);
                    let reason = match status {
                        200 => "OK",
                        403 => "Forbidden",
                        404 => "Not Found",
                        429 => "Too Many Requests",
                        _ => "Error",
                    };
                    let head = format!(
                        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = stream.write_all(head.as_bytes());
                    let _ = stream.write_all(&body);
                    let _ = stream.flush();
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(_) => break,
            }
        }
    });

    MockServer {
        base_url: format!("http://{addr}"),
        requests,
        stop,
        handle: Some(handle),
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn snapshot_gw_update_dirs() -> HashSet<PathBuf> {
    let Ok(entries) = fs::read_dir(std::env::temp_dir()) else {
        return HashSet::new();
    };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("gw-update-"))
        })
        .collect()
}

fn release_json(
    tag: &str,
    prerelease: bool,
    draft: bool,
    exe_url: Option<&str>,
    exe_api_url: Option<&str>,
    sums_url: Option<&str>,
    sums_api_url: Option<&str>,
) -> String {
    let mut assets = Vec::new();
    if let Some(u) = exe_url {
        assets.push(serde_json::json!({
            "name": RELEASE_EXE_ASSET,
            "browser_download_url": u,
            "url": exe_api_url.unwrap_or(u),
        }));
    }
    if let Some(u) = sums_url {
        assets.push(serde_json::json!({
            "name": RELEASE_SUMS_ASSET,
            "browser_download_url": u,
            "url": sums_api_url.unwrap_or(u),
        }));
    }
    serde_json::json!({
        "tag_name": tag,
        "prerelease": prerelease,
        "draft": draft,
        "body": "## Changes\n- Added self-updater",
        "html_url": "https://github.com/owner/repo/releases/tag/v0.2.0",
        "published_at": "2026-11-25T00:00:00Z",
        "assets": assets,
    })
    .to_string()
}

#[test]
fn semver_parses_stable_compares_numerically_and_rejects_prerelease() {
    let v010 = SemVer::parse_stable("0.1.0").unwrap();
    let v020 = SemVer::parse_stable("v0.2.0").unwrap();
    let upper_v020 = SemVer::parse_stable("V0.2.0").unwrap();
    let v012 = SemVer::parse_stable("0.1.2").unwrap();
    let v0110 = SemVer::parse_stable("0.1.10").unwrap();
    let build = SemVer::parse_stable("1.2.3+build.45").unwrap();

    assert!(v020 > v010);
    assert_eq!(v020, upper_v020);
    assert!(v0110 > v012);
    assert_eq!(build.to_string(), "1.2.3");

    assert!(SemVer::parse_stable("v0.2.0-rc.1").is_none());
    assert!(SemVer::parse_stable("0.2.0-beta").is_none());
    assert!(SemVer::parse_stable("").is_none());
    assert!(SemVer::parse_stable("   ").is_none());
    assert!(SemVer::parse_stable("invalid").is_none());
    assert!(SemVer::parse_stable("1.2").is_none());
    assert!(SemVer::parse_stable("1.2.3.4").is_none());
    assert!(SemVer::parse_stable("v+1.0.0").is_none());
}

#[test]
fn parse_sha256sums_handles_bom_crlf_prefixes_and_rejects_invalid() {
    let expected = "a".repeat(64);
    let bom_crlf = format!("\u{FEFF}{}  git-widget-sidecar.exe\r\n", expected.to_uppercase());
    assert_eq!(
        parse_sha256sums_for_asset(&bom_crlf, RELEASE_EXE_ASSET).unwrap(),
        expected
    );

    let prefixed = format!("deadbeef  other.txt\n{expected} *./git-widget-sidecar.exe\n");
    assert_eq!(
        parse_sha256sums_for_asset(&prefixed, RELEASE_EXE_ASSET).unwrap(),
        expected
    );

    let short_hex = "abcd  git-widget-sidecar.exe\n";
    assert!(parse_sha256sums_for_asset(short_hex, RELEASE_EXE_ASSET).is_err());

    let non_hex = format!("{}  git-widget-sidecar.exe\n", "z".repeat(64));
    assert!(parse_sha256sums_for_asset(&non_hex, RELEASE_EXE_ASSET).is_err());

    let missing = format!("{expected}  other-file.exe\n");
    assert!(parse_sha256sums_for_asset(&missing, RELEASE_EXE_ASSET).is_err());
}

#[test]
fn verify_file_sha256_validates_digest() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("bin.exe");
    let data = b"MZ-fake-sidecar-binary-content";
    fs::write(&path, data).unwrap();

    let digest = sha256_hex(data);
    verify_file_sha256(&path, &digest.to_uppercase()).unwrap();

    let bad = "0".repeat(64);
    assert!(verify_file_sha256(&path, &bad).is_err());
}

#[test]
fn resolve_and_validate_update_repo_enforces_priority_and_strict_format() {
    assert_eq!(
        resolve_update_repo(Some(" custom/fork "), Some("env/fork")),
        "custom/fork"
    );
    assert_eq!(
        resolve_update_repo(Some("   "), Some("env.org/my_repo-1")),
        "env.org/my_repo-1"
    );
    assert_eq!(
        resolve_update_repo(Some("https://github.com/bad/url"), Some("   ")),
        DEFAULT_UPDATE_REPO
    );
    assert_eq!(resolve_update_repo(None, None), DEFAULT_UPDATE_REPO);

    assert_eq!(
        validate_update_repo("owner.1/repo_2-a").unwrap(),
        "owner.1/repo_2-a"
    );
    for bad in [
        "",
        "   ",
        "onlyowner",
        "a/b/c",
        "https://github.com/a/b",
        "../evil",
        "owner/../repo",
        "-flag/repo",
        "owner/repo name",
    ] {
        assert!(validate_update_repo(bad).is_err(), "expected err for {bad}");
    }
}

#[test]
fn validate_release_url_enforces_https_github_allowlist() {
    for ok in [
        "https://api.github.com/repos/a/b/releases/latest",
        "https://github.com/a/b/releases/download/v0.2.0/git-widget-sidecar.exe",
        "https://objects.githubusercontent.com/github-production-release-asset/123",
        "https://release-assets.githubusercontent.com/github-production-release-asset/456",
    ] {
        validate_release_url(ok, &[]).unwrap();
    }

    for bad in [
        "",
        "http://example.com/a.exe",
        "https://evil.com/git-widget-sidecar.exe",
        "https://api.github.com.evil.com/x",
        "-o/evil",
    ] {
        assert!(validate_release_url(bad, &[]).is_err(), "expected err for {bad}");
    }

    validate_release_url("http://127.0.0.1:4321/assets/a.exe", &["http://127.0.0.1:4321"]).unwrap();
    assert!(validate_release_url(
        "http://127.0.0.1:43210/assets/a.exe",
        &["http://127.0.0.1:4321"]
    )
    .is_err());
}

#[tokio::test]
async fn fetch_latest_release_handles_versions_prereleases_drafts_and_http_errors() {
    let srv = spawn_mock_server(|path, _req| {
        if path == "/repos/owner/newer/releases/latest" {
            let body = release_json(
                "v0.2.0",
                false,
                false,
                Some("https://github.com/owner/newer/releases/download/v0.2.0/git-widget-sidecar.exe"),
                None,
                Some("https://github.com/owner/newer/releases/download/v0.2.0/SHA256SUMS.txt"),
                None,
            );
            (200, "application/json".into(), body.into_bytes())
        } else if path == "/repos/owner/same/releases/latest" {
            (200, "application/json".into(), release_json("v0.1.0", false, false, None, None, None, None).into_bytes())
        } else if path == "/repos/owner/older/releases/latest" {
            (200, "application/json".into(), release_json("v0.0.9", false, false, None, None, None, None).into_bytes())
        } else if path == "/repos/owner/rc/releases/latest" {
            (200, "application/json".into(), release_json("v0.2.0-rc.1", false, false, None, None, None, None).into_bytes())
        } else if path == "/repos/owner/prerelease/releases/latest" {
            (200, "application/json".into(), release_json("v0.2.0", true, false, None, None, None, None).into_bytes())
        } else if path == "/repos/owner/draft/releases/latest" {
            (200, "application/json".into(), release_json("v0.2.0", false, true, None, None, None, None).into_bytes())
        } else if path == "/repos/owner/ratelimit403/releases/latest" {
            (403, "application/json".into(), br#"{"message":"API rate limit exceeded"}"#.to_vec())
        } else if path == "/repos/owner/ratelimit429/releases/latest" {
            (429, "application/json".into(), br#"{"message":"rate limit"}"#.to_vec())
        } else if path == "/repos/owner/notfound/releases/latest" {
            (404, "application/json".into(), br#"{"message":"Not Found"}"#.to_vec())
        } else if path == "/repos/owner/badjson/releases/latest" {
            (200, "application/json".into(), b"{not-json".to_vec())
        } else if path == "/repos/owner/emptyjson/releases/latest" {
            (200, "application/json".into(), b"{}".to_vec())
        } else {
            (404, "application/json".into(), b"{}".to_vec())
        }
    });

    let allowed = [srv.base_url.as_str()];

    let newer = fetch_latest_release_with_base(&srv.base_url, "owner/newer", "0.1.0", Some("ghp_tok"), &allowed)
        .await
        .unwrap();
    assert!(newer.update_available);
    assert_eq!(newer.latest_version.as_deref(), Some("0.2.0"));
    assert!(newer.has_required_assets());
    assert!(newer.release_notes.contains("Added self-updater"));
    assert!(srv
        .requests
        .lock()
        .unwrap()
        .iter()
        .any(|r| r.contains("Authorization: Bearer ghp_tok")));

    for repo in ["owner/same", "owner/older", "owner/rc", "owner/prerelease", "owner/draft"] {
        let info = fetch_latest_release_with_base(&srv.base_url, repo, "0.1.0", None, &allowed)
            .await
            .unwrap();
        assert!(!info.update_available, "expected update_available=false for {repo}");
    }

    let err_403 = fetch_latest_release_with_base(&srv.base_url, "owner/ratelimit403", "0.1.0", None, &allowed)
        .await
        .unwrap_err();
    assert!(err_403.to_string().to_lowercase().contains("rate limit"));

    let err_429 = fetch_latest_release_with_base(&srv.base_url, "owner/ratelimit429", "0.1.0", None, &allowed)
        .await
        .unwrap_err();
    assert!(err_429.to_string().to_lowercase().contains("rate limit"));

    let err_404 = fetch_latest_release_with_base(&srv.base_url, "owner/notfound", "0.1.0", None, &allowed)
        .await
        .unwrap_err();
    assert!(err_404.to_string().contains("No published release found"));

    for bad_repo in ["owner/badjson", "owner/emptyjson"] {
        let err = fetch_latest_release_with_base(&srv.base_url, bad_repo, "0.1.0", None, &allowed)
            .await
            .unwrap_err();
        assert!(matches!(err, CoreError::InvalidInput(_)));
    }

    let closed_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let closed_addr = closed_listener.local_addr().unwrap();
    drop(closed_listener);
    let closed_base = format!("http://{closed_addr}");
    assert!(fetch_latest_release_with_base(&closed_base, "owner/newer", "0.1.0", None, &[closed_base.as_str()])
        .await
        .is_err());
}

#[tokio::test]
async fn download_verified_release_succeeds_and_fails_closed_with_temp_cleanup() {
    let exe_bytes = b"MZ-verified-git-widget-sidecar-v0.2.0".to_vec();
    let valid_sha = sha256_hex(&exe_bytes);
    let valid_sums = format!("\u{FEFF}{valid_sha}  {RELEASE_EXE_ASSET}\r\n").into_bytes();
    let wrong_sums = format!("{}  {RELEASE_EXE_ASSET}\r\n", "f".repeat(64)).into_bytes();

    let exe_clone = exe_bytes.clone();
    let sums_clone = valid_sums.clone();
    let wrong_clone = wrong_sums.clone();
    let srv = spawn_mock_server(move |path, _req| match path {
        "/ok/git-widget-sidecar.exe" => (200, "application/octet-stream".into(), exe_clone.clone()),
        "/ok/SHA256SUMS.txt" => (200, "text/plain".into(), sums_clone.clone()),
        "/bad/SHA256SUMS.txt" => (200, "text/plain".into(), wrong_clone.clone()),
        _ => (404, "text/plain".into(), b"missing".to_vec()),
    });

    let allowed = [srv.base_url.as_str()];
    let base_info = |exe_path: &str, sums_path: Option<&str>, latest: Option<&str>, avail: bool| UpdateReleaseInfo {
        repo: "owner/repo".into(),
        current_version: "0.1.0".into(),
        latest_version: latest.map(ToString::to_string),
        update_available: avail,
        release_notes: String::new(),
        html_url: None,
        published_at: None,
        exe_download_url: Some(format!("{}{}", srv.base_url, exe_path)),
        exe_api_url: None,
        sums_download_url: sums_path.map(|p| format!("{}{}", srv.base_url, p)),
        sums_api_url: None,
    };

    // 1. Valid download & SHA-256 verification
    let ok_info = base_info("/ok/git-widget-sidecar.exe", Some("/ok/SHA256SUMS.txt"), Some("0.2.0"), true);
    let verified = download_verified_release_with_bases(&ok_info, "0.1.0", None, &allowed)
        .await
        .unwrap();
    assert!(verified.exe_path.exists());
    assert!(verified.sums_path.exists());
    assert_eq!(verified.version, "0.2.0");
    assert_eq!(fs::read(&verified.exe_path).unwrap(), exe_bytes);
    fs::remove_dir_all(&verified.temp_dir).unwrap();

    // 2. Fail-closed cases must error and leave zero gw-update-* temp dirs behind
    let before_dirs = snapshot_gw_update_dirs();

    let bad_hash = base_info("/ok/git-widget-sidecar.exe", Some("/bad/SHA256SUMS.txt"), Some("0.2.0"), true);
    assert!(download_verified_release_with_bases(&bad_hash, "0.1.0", None, &allowed)
        .await
        .is_err());

    let missing_sums_url = base_info("/ok/git-widget-sidecar.exe", None, Some("0.2.0"), true);
    assert!(download_verified_release_with_bases(&missing_sums_url, "0.1.0", None, &allowed)
        .await
        .is_err());

    let missing_sums_404 = base_info("/ok/git-widget-sidecar.exe", Some("/missing/SHA256SUMS.txt"), Some("0.2.0"), true);
    assert!(download_verified_release_with_bases(&missing_sums_404, "0.1.0", None, &allowed)
        .await
        .is_err());

    let downgrade_same = base_info("/ok/git-widget-sidecar.exe", Some("/ok/SHA256SUMS.txt"), Some("0.1.0"), false);
    assert!(download_verified_release_with_bases(&downgrade_same, "0.1.0", None, &allowed)
        .await
        .is_err());

    let downgrade_older = base_info("/ok/git-widget-sidecar.exe", Some("/ok/SHA256SUMS.txt"), Some("0.0.9"), true);
    assert!(download_verified_release_with_bases(&downgrade_older, "0.1.0", None, &allowed)
        .await
        .is_err());

    let prerelease_none = base_info("/ok/git-widget-sidecar.exe", Some("/ok/SHA256SUMS.txt"), None, false);
    assert!(download_verified_release_with_bases(&prerelease_none, "0.1.0", None, &allowed)
        .await
        .is_err());

    let after_dirs = snapshot_gw_update_dirs();
    let leaked: Vec<_> = after_dirs.difference(&before_dirs).collect();
    assert!(leaked.is_empty(), "temp dirs were not cleaned up: {leaked:?}");
}

#[test]
fn scanner_config_update_repo_defaults_and_env_example_documented() {
    let legacy: ScannerConfig =
        serde_json::from_str(r#"{"roots":["C:\\Projects\\"],"max_depth":3}"#).unwrap();
    assert_eq!(legacy.update_repo, None);

    let custom = ScannerConfig {
        update_repo: Some("fork/widget".into()),
        ..Default::default()
    };
    let decoded: ScannerConfig =
        serde_json::from_str(&serde_json::to_string(&custom).unwrap()).unwrap();
    assert_eq!(decoded.update_repo.as_deref(), Some("fork/widget"));

    let env_example = include_str!("../../../.env.example");
    assert!(
        env_example.contains("GIT_WIDGET_UPDATE_REPO"),
        ".env.example must document GIT_WIDGET_UPDATE_REPO"
    );
}
