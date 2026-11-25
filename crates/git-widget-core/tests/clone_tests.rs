use git_widget_core::{
    clone::{build_clone_command, cleanup_clone_target, run_clone, CloneOptions, CloneTimeouts},
    clone_progress::{classify_clone_error, parse_git_progress_line, CloneProgressSnapshot, CloneStage, LogRingBuffer},
    clone_validation::{derive_folder_name, parse_clone_url, prepare_clone_target, redact_url_credentials, validate_folder_name},
    scanner::ScannerConfig,
};
use std::{ffi::OsStr, fs, path::Path, process::Command as StdCommand, sync::OnceLock, time::Duration};
use tempfile::TempDir;
use tokio::sync::{watch, Mutex as AsyncMutex};

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

fn git_sync(dir: &Path, args: &[&str]) {
    let out = StdCommand::new("git").arg("-C").arg(dir).args(args).env("GIT_TERMINAL_PROMPT", "0").output().unwrap();
    assert!(out.status.success(), "git {:?} failed: {}", args, String::from_utf8_lossy(&out.stderr));
}

fn create_bare_repo(root: &Path) -> std::path::PathBuf {
    let (work, bare) = (root.join("src-work"), root.join("bare-remote.git"));
    fs::create_dir_all(&work).unwrap();
    git_sync(&work, &["init", "-b", "main"]);
    git_sync(&work, &["config", "user.email", "test@example.com"]);
    git_sync(&work, &["config", "user.name", "Test User"]);
    fs::write(work.join("README.md"), "# Clone Test\n").unwrap();
    fs::write(work.join("data.txt"), "hello world\n".repeat(200)).unwrap();
    git_sync(&work, &["add", "."]);
    git_sync(&work, &["commit", "-m", "initial commit"]);
    let out = StdCommand::new("git").args(["clone", "--bare", "--"]).arg(&work).arg(&bare).env("GIT_TERMINAL_PROMPT", "0").output().unwrap();
    assert!(out.status.success(), "bare clone failed: {}", String::from_utf8_lossy(&out.stderr));
    bare
}

#[test]
fn url_and_folder_extraction() {
    let short = parse_clone_url("owner/repo").expect("owner/repo should parse");
    assert_eq!(short.normalized_url, "https://github.com/owner/repo.git");
    assert_eq!(short.default_folder, "repo");
    assert_eq!(parse_clone_url("https://host/org/my-app.git/").unwrap().default_folder, "my-app");
    assert_eq!(parse_clone_url("git@github.com:org/tool.git").unwrap().default_folder, "tool");
    assert_eq!(parse_clone_url("file:///C:/tmp/Мои%20проекты/app.git").unwrap().default_folder, "app");

    let encoded_space = parse_clone_url("https://host/org/my%20repo.git").unwrap();
    assert_eq!(encoded_space.default_folder, "my repo");
    assert!(validate_folder_name(&encoded_space.default_folder).is_ok());
    assert_eq!(derive_folder_name("https://host/org/demo.git/"), Some("demo".to_string()));

    let legacy: ScannerConfig = serde_json::from_str(r#"{"roots":["C:\\Projects"],"max_depth":3}"#).unwrap();
    assert_eq!(legacy.last_clone_dir, None);
    let mut updated = legacy;
    updated.last_clone_dir = Some(r"D:\Мои проекты\Clones".to_string());
    let roundtrip: ScannerConfig = serde_json::from_str(&serde_json::to_string(&updated).unwrap()).unwrap();
    assert_eq!(roundtrip.last_clone_dir.as_deref(), Some(r"D:\Мои проекты\Clones"));
}

#[test]
fn redacts_url_credentials_everywhere() {
    assert_eq!(redact_url_credentials("https://alice:ghp_secret123@github.com/org/repo.git"), "https://***@github.com/org/repo.git");
    assert_eq!(redact_url_credentials("https://tok@host/a.git"), "https://***@host/a.git");
    assert_eq!(redact_url_credentials("git@github.com:org/repo.git"), "git@github.com:org/repo.git");

    let multiline = "Cloning...\nfatal: 'https://bob:s3cr3t@example.com/a/b.git/': 403\nhttps://token2@git.local/x.git";
    let redacted = redact_url_credentials(multiline);
    assert!(!redacted.contains("s3cr3t") && !redacted.contains("token2"));
    assert!(redacted.contains("https://***@example.com/a/b.git/") && redacted.contains("https://***@git.local/x.git"));

    let mut ring = LogRingBuffer::new(2);
    ring.push("line 1 https://u:p1@host/r.git");
    ring.push("line 2");
    ring.push("line 3 https://t2@host/r.git");
    let entries = ring.entries();
    assert_eq!(entries.len(), 2);
    assert!(!entries[0].contains("p1") && !entries[1].contains("t2"));
    assert!(entries[1].contains("https://***@host/r.git"));
}

#[test]
fn command_is_non_interactive() {
    let target = Path::new(r"C:\tmp\target-repo");
    let cmd = build_clone_command("https://github.com/org/repo.git", target);
    let std_cmd = cmd.as_std();
    assert_eq!(std_cmd.get_program(), OsStr::new("git"));
    let args: Vec<&OsStr> = std_cmd.get_args().collect();
    assert_eq!(args, vec![OsStr::new("clone"), OsStr::new("--progress"), OsStr::new("--"), OsStr::new("https://github.com/org/repo.git"), target.as_os_str()]);

    let envs: Vec<(String, Option<String>)> = std_cmd.get_envs().map(|(k, v)| (k.to_string_lossy().into(), v.map(|x| x.to_string_lossy().into()))).collect();
    assert!(envs.contains(&("GIT_TERMINAL_PROMPT".into(), Some("0".into()))));
    assert!(envs.contains(&("LC_MESSAGES".into(), Some("C".into()))));
    assert!(envs.contains(&("GCM_INTERACTIVE".into(), Some("never".into()))));

    let (stage, pct, _) = parse_git_progress_line("Receiving objects:  50% (10/20)").expect("progress line parses");
    assert_eq!(stage, CloneStage::Receiving);
    assert!((25..=80).contains(&pct));
}

#[tokio::test]
async fn clone_local_bare_repo_reports_progress() {
    let tmp = TempDir::new().unwrap();
    let unicode_root = tmp.path().join("Мои проекты");
    fs::create_dir_all(&unicode_root).unwrap();
    let bare = create_bare_repo(&unicode_root);
    let bare_url = format!("file:///{}", bare.to_string_lossy().replace('\\', "/"));
    let dest_parent = unicode_root.join("папка назначения");
    fs::create_dir_all(&dest_parent).unwrap();

    let (_tx, rx) = watch::channel(false);
    let mut snapshots: Vec<CloneProgressSnapshot> = Vec::new();
    let outcome = run_clone(
        CloneOptions { url: bare_url, parent_dir: dest_parent, folder_name: Some("cloned app".into()), timeouts: CloneTimeouts::default() },
        rx,
        |snap| snapshots.push(snap),
    ).await.expect("local bare repo clone should succeed");

    assert!(outcome.target_path.join(".git").is_dir() && outcome.target_path.join("README.md").is_file());
    assert!(!outcome.sanitized_url.contains('@'));
    assert!(snapshots.iter().any(|s| s.stage == CloneStage::Starting));
    let last = snapshots.last().expect("final snapshot");
    assert_eq!((last.stage, last.percent, last.in_progress), (CloneStage::Done, 100, false));
}

#[test]
fn validation_blocks_injection_and_windows_reserved() {
    for bad_url in ["", "   ", "-c", "--upload-pack=evil", "ftp://bad/a/b", "https://github.com/a/b\0c", "https://github.com/a/\nb", "https://github.com/a/\tb", "./repo", "../repo"] {
        assert!(parse_clone_url(bad_url).is_err(), "expected URL rejected: {bad_url:?}");
    }
    for bad_folder in ["", "   ", ".", "..", "-repo", "a/b", r"a\b", "bad:name", "foo*", "foo.", " foo", "foo ", "foo\tbar", "foo<bar", "foo>bar", "foo\"bar", "foo|bar", "foo?bar", "CON", "con.git", "NUL", "AUX", "PRN", "COM1", "LPT9"] {
        assert!(validate_folder_name(bad_folder).is_err(), "expected folder rejected: {bad_folder:?}");
    }
    assert_eq!(validate_folder_name("my repo").unwrap(), "my repo");

    let tmp = TempDir::new().unwrap();
    assert!(prepare_clone_target(&tmp.path().join("does-not-exist"), "valid").is_err());

    let msg = classify_clone_error(128, &["fatal: Authentication failed for 'https://tok:sec@example.com/a/b.git/'".into()], false, None);
    assert!(!msg.contains("sec") && (msg.to_lowercase().contains("auth") || msg.contains("***@")));
}

#[test]
fn target_dir_empty_vs_nonempty() {
    let tmp = TempDir::new().unwrap();
    let parent = tmp.path();

    let nonempty = parent.join("nonempty");
    fs::create_dir_all(&nonempty).unwrap();
    fs::write(nonempty.join("keep.txt"), "important").unwrap();
    let err = prepare_clone_target(parent, "nonempty").expect_err("non-empty dir must fail");
    assert!(err.to_string().to_lowercase().contains("not empty"));
    assert_eq!(fs::read_to_string(nonempty.join("keep.txt")).unwrap(), "important");

    let hidden_only = parent.join("hidden-only");
    fs::create_dir_all(hidden_only.join(".git")).unwrap();
    assert!(prepare_clone_target(parent, "hidden-only").is_err());

    let empty_dir = parent.join("empty-dir");
    fs::create_dir_all(&empty_dir).unwrap();
    let guard_empty = prepare_clone_target(parent, "empty-dir").expect("empty dir allowed");
    assert!(guard_empty.existed_before && guard_empty.target_path == empty_dir);

    let guard_new = prepare_clone_target(parent, "brand-new").expect("new dir allowed");
    assert!(!guard_new.existed_before && guard_new.target_path == parent.join("brand-new"));
}

#[tokio::test]
async fn clone_timeouts_kill_process_and_cleanup() {
    let _guard = env_lock().lock().await;
    let tmp = TempDir::new().unwrap();
    let parent = tmp.path().join("parent");
    fs::create_dir_all(&parent).unwrap();
    fs::write(parent.join("sibling.txt"), "stay").unwrap();

    {
        let _ssh = GitSshEnvGuard::set("sh -c 'sleep 2'");
        let (_tx, rx) = watch::channel(false);
        let err = run_clone(
            CloneOptions { url: "ssh://alice:secret123@127.0.0.1/org/hung.git".into(), parent_dir: parent.clone(), folder_name: Some("hung-idle".into()), timeouts: CloneTimeouts { idle: Duration::from_millis(120), total: Duration::from_secs(5) } },
            rx, |_| {},
        ).await.expect_err("idle timeout should fail");
        let msg = err.to_string();
        assert!(!msg.contains("secret123"));
        assert!(msg.to_lowercase().contains("timed out") || msg.to_lowercase().contains("idle"));
        assert!(!parent.join("hung-idle").exists() && parent.join("sibling.txt").is_file());
    }

    {
        let _ssh = GitSshEnvGuard::set("sh -c 'i=0; while [ $i -lt 30 ]; do printf \"Receiving objects: 10%% (1/10)\\r\" >&2; sleep 0.04; i=$((i+1)); done'");
        let existing_empty = parent.join("pre-empty");
        fs::create_dir_all(&existing_empty).unwrap();
        let (_tx, rx) = watch::channel(false);
        let err = run_clone(
            CloneOptions { url: "ssh://alice:secret456@127.0.0.1/org/slow.git".into(), parent_dir: parent.clone(), folder_name: Some("pre-empty".into()), timeouts: CloneTimeouts { idle: Duration::from_millis(250), total: Duration::from_millis(300) } },
            rx, |_| {},
        ).await.expect_err("total timeout should fail");
        let msg = err.to_string();
        assert!(!msg.contains("secret456"));
        assert!(msg.to_lowercase().contains("timed out") || msg.to_lowercase().contains("total"));
        assert!(existing_empty.is_dir() && fs::read_dir(&existing_empty).unwrap().count() == 0);
    }
}

#[tokio::test]
async fn cancel_and_cleanup_respects_existed_before() {
    let _guard = env_lock().lock().await;
    let tmp = TempDir::new().unwrap();
    let parent = tmp.path().join("parent");
    fs::create_dir_all(&parent).unwrap();
    fs::write(parent.join("keep.txt"), "safe").unwrap();

    {
        let _ssh = GitSshEnvGuard::set("sh -c 'sleep 2'");
        let (tx, rx) = watch::channel(false);
        let p = parent.clone();
        let handle = tokio::spawn(async move {
            run_clone(CloneOptions { url: "ssh://git@127.0.0.1/org/cancel-new.git".into(), parent_dir: p, folder_name: Some("cancel-new".into()), timeouts: CloneTimeouts::default() }, rx, |_| {}).await
        });
        tokio::time::sleep(Duration::from_millis(120)).await;
        let _ = tx.send(true);
        assert!(handle.await.unwrap().is_err());
        assert!(!parent.join("cancel-new").exists() && parent.join("keep.txt").is_file());
    }

    {
        let _ssh = GitSshEnvGuard::set("sh -c 'sleep 2'");
        let pre = parent.join("pre-existing");
        fs::create_dir_all(&pre).unwrap();
        let (tx, rx) = watch::channel(false);
        let p = parent.clone();
        let handle = tokio::spawn(async move {
            run_clone(CloneOptions { url: "ssh://git@127.0.0.1/org/cancel-pre.git".into(), parent_dir: p, folder_name: Some("pre-existing".into()), timeouts: CloneTimeouts::default() }, rx, |_| {}).await
        });
        tokio::time::sleep(Duration::from_millis(120)).await;
        let _ = tx.send(true);
        let _ = tx.send(true);
        assert!(handle.await.unwrap().is_err());
        assert!(pre.is_dir() && fs::read_dir(&pre).unwrap().count() == 0);
    }

    let ro_target = parent.join("ro-cleanup");
    let pack_dir = ro_target.join(".git").join("objects").join("pack");
    fs::create_dir_all(&pack_dir).unwrap();
    let pack_file = pack_dir.join("test.pack");
    fs::write(&pack_file, b"pack").unwrap();
    let mut perms = fs::metadata(&pack_file).unwrap().permissions();
    perms.set_readonly(true);
    fs::set_permissions(&pack_file, perms).unwrap();
    cleanup_clone_target(&ro_target, false).await.expect("cleanup must clear readonly");
    assert!(!ro_target.exists() && parent.join("keep.txt").is_file());
}
