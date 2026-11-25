use git_widget_core::{
    clone::{build_clone_command, run_clone, CloneOptions, CloneTimeouts},
    clone_progress::{
        parse_git_progress_line, CloneProgressSnapshot, CloneStage, LogRingBuffer,
    },
    clone_validation::{
        derive_folder_name, parse_clone_url, redact_url_credentials, validate_folder_name,
    },
    scanner::ScannerConfig,
};
use std::{
    ffi::OsStr,
    fs,
    path::Path,
    process::Command as StdCommand,
};
use tempfile::TempDir;
use tokio::sync::watch;

fn git_sync(dir: &Path, args: &[&str]) {
    let out = StdCommand::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .expect("failed to execute git");
    assert!(
        out.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
}

fn create_bare_repo(root: &Path) -> std::path::PathBuf {
    let work = root.join("src-work");
    let bare = root.join("bare-remote.git");
    fs::create_dir_all(&work).unwrap();
    git_sync(&work, &["init", "-b", "main"]);
    git_sync(&work, &["config", "user.email", "test@example.com"]);
    git_sync(&work, &["config", "user.name", "Test User"]);
    fs::write(work.join("README.md"), "# Clone Test\n").unwrap();
    fs::write(work.join("data.txt"), "hello world\n".repeat(200)).unwrap();
    git_sync(&work, &["add", "."]);
    git_sync(&work, &["commit", "-m", "initial commit"]);

    let out = StdCommand::new("git")
        .args(["clone", "--bare", "--"])
        .arg(&work)
        .arg(&bare)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .expect("failed to create bare repo");
    assert!(
        out.status.success(),
        "bare clone failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    bare
}

#[test]
fn url_and_folder_extraction() {
    let short = parse_clone_url("owner/repo").expect("owner/repo should parse");
    assert_eq!(short.normalized_url, "https://github.com/owner/repo.git");
    assert_eq!(short.default_folder, "repo");

    let trailing = parse_clone_url("https://host/org/my-app.git/").expect("https url should parse");
    assert_eq!(trailing.default_folder, "my-app");

    let scp = parse_clone_url("git@github.com:org/tool.git").expect("scp url should parse");
    assert_eq!(scp.default_folder, "tool");

    let file_cyr =
        parse_clone_url("file:///C:/tmp/Мои%20проекты/app.git").expect("file url should parse");
    assert_eq!(file_cyr.default_folder, "app");

    let encoded_space =
        parse_clone_url("https://host/org/my%20repo.git").expect("encoded space url should parse");
    assert_eq!(encoded_space.default_folder, "my repo");
    assert!(validate_folder_name(&encoded_space.default_folder).is_ok());

    assert_eq!(derive_folder_name("https://host/org/demo.git/"), Some("demo".to_string()));

    let legacy_json = r#"{"roots":["C:\\Projects"],"max_depth":3}"#;
    let legacy: ScannerConfig = serde_json::from_str(legacy_json).unwrap();
    assert_eq!(legacy.last_clone_dir, None);

    let mut updated = legacy;
    updated.last_clone_dir = Some(r"D:\Мои проекты\Clones".to_string());
    let roundtrip: ScannerConfig =
        serde_json::from_str(&serde_json::to_string(&updated).unwrap()).unwrap();
    assert_eq!(
        roundtrip.last_clone_dir.as_deref(),
        Some(r"D:\Мои проекты\Clones")
    );
}

#[test]
fn redacts_url_credentials_everywhere() {
    assert_eq!(
        redact_url_credentials("https://alice:ghp_secret123@github.com/org/repo.git"),
        "https://***@github.com/org/repo.git"
    );
    assert_eq!(
        redact_url_credentials("https://tok@host/a.git"),
        "https://***@host/a.git"
    );
    assert_eq!(
        redact_url_credentials("git@github.com:org/repo.git"),
        "git@github.com:org/repo.git"
    );

    let multiline = "Cloning into 'repo'...\nfatal: unable to access 'https://bob:s3cr3t@example.com/a/b.git/': 403\nretry https://token2@git.local/x.git";
    let redacted = redact_url_credentials(multiline);
    assert!(!redacted.contains("s3cr3t"));
    assert!(!redacted.contains("token2"));
    assert!(redacted.contains("https://***@example.com/a/b.git/"));
    assert!(redacted.contains("https://***@git.local/x.git"));

    let mut ring = LogRingBuffer::new(2);
    ring.push("line 1 https://u:p1@host/r.git");
    ring.push("line 2");
    ring.push("line 3 https://t2@host/r.git");
    let entries = ring.entries();
    assert_eq!(entries.len(), 2);
    assert!(!entries[0].contains("p1"));
    assert!(!entries[1].contains("t2"));
    assert!(entries[1].contains("https://***@host/r.git"));
}

#[test]
fn command_is_non_interactive() {
    let target = Path::new(r"C:\tmp\target-repo");
    let cmd = build_clone_command("https://github.com/org/repo.git", target);
    let std_cmd = cmd.as_std();

    assert_eq!(std_cmd.get_program(), OsStr::new("git"));
    let args: Vec<&OsStr> = std_cmd.get_args().collect();
    assert_eq!(
        args,
        vec![
            OsStr::new("clone"),
            OsStr::new("--progress"),
            OsStr::new("--"),
            OsStr::new("https://github.com/org/repo.git"),
            target.as_os_str(),
        ]
    );

    let envs: Vec<(String, Option<String>)> = std_cmd
        .get_envs()
        .map(|(k, v)| {
            (
                k.to_string_lossy().to_string(),
                v.map(|val| val.to_string_lossy().to_string()),
            )
        })
        .collect();
    assert!(envs.contains(&("GIT_TERMINAL_PROMPT".into(), Some("0".into()))));
    assert!(envs.contains(&("LC_MESSAGES".into(), Some("C".into()))));
    assert!(envs.contains(&("GCM_INTERACTIVE".into(), Some("never".into()))));

    let (stage, pct, _) =
        parse_git_progress_line("Receiving objects:  50% (10/20)").expect("progress line parses");
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
        CloneOptions {
            url: bare_url,
            parent_dir: dest_parent.clone(),
            folder_name: Some("cloned app".to_string()),
            timeouts: CloneTimeouts::default(),
        },
        rx,
        |snap| snapshots.push(snap),
    )
    .await
    .expect("local bare repo clone should succeed");

    assert!(outcome.target_path.join(".git").is_dir());
    assert!(outcome.target_path.join("README.md").is_file());
    assert!(!outcome.sanitized_url.contains('@'));
    assert!(snapshots.iter().any(|s| s.stage == CloneStage::Starting));
    let last = snapshots.last().expect("at least one final snapshot");
    assert_eq!(last.stage, CloneStage::Done);
    assert_eq!(last.percent, 100);
    assert!(!last.in_progress);
}
