use git_widget_core::{
    diff::{accept_hunk, discard_file, get_commit_diff_files, get_diff_files, reject_hunk, stage_file},
    status::get_status,
};
use std::{fs, path::Path, process::Command};
use tempfile::TempDir;

fn git_sync(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .unwrap();
    assert!(out.status.success(), "{:?} failed", args);
}

#[tokio::test]
async fn diff_accept_and_reject_individual_hunks() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("diff-repo");
    fs::create_dir_all(&repo).unwrap();
    git_sync(&repo, &["init", "-b", "main"]);
    git_sync(&repo, &["config", "user.email", "test@example.com"]);
    git_sync(&repo, &["config", "user.name", "Test User"]);

    let initial: Vec<String> = (1..=20).map(|i| format!("line {i}")).collect();
    fs::write(repo.join("app.txt"), initial.join("\n") + "\n").unwrap();
    git_sync(&repo, &["add", "."]);
    git_sync(&repo, &["commit", "-m", "init"]);

    let mut modified = initial;
    modified[1] = "line 2 CHANGED_A".to_string();
    modified[18] = "line 19 CHANGED_B".to_string();
    fs::write(repo.join("app.txt"), modified.join("\n") + "\n").unwrap();

    let files = get_diff_files(&repo).await.unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].hunks.len(), 2);

    // Accept hunk 0 (line 2), reject hunk 0 of remaining unstaged (line 19).
    accept_hunk(&repo, "app.txt", 0).await.unwrap();
    let after_accept = get_diff_files(&repo).await.unwrap();
    let staged = after_accept.iter().find(|f| f.staged).unwrap();
    let unstaged = after_accept.iter().find(|f| !f.staged).unwrap();
    assert_eq!(staged.hunks.len(), 1);
    assert_eq!(unstaged.hunks.len(), 1);

    reject_hunk(&repo, "app.txt", 0).await.unwrap();
    let final_content = fs::read_to_string(repo.join("app.txt")).unwrap();
    assert!(final_content.contains("CHANGED_A"));
    assert!(!final_content.contains("CHANGED_B"));
}

#[tokio::test]
async fn diff_blocks_path_traversal_flag_injection_and_handles_cyrillic_paths() {
    let tmp = TempDir::new().unwrap();
    let outside_secret = tmp.path().join("outside.txt");
    fs::write(&outside_secret, "do not delete").unwrap();

    let repo = tmp.path().join("sec-repo");
    fs::create_dir_all(&repo).unwrap();
    git_sync(&repo, &["init", "-b", "main"]);
    git_sync(&repo, &["config", "user.email", "test@example.com"]);
    git_sync(&repo, &["config", "user.name", "Test User"]);
    fs::write(repo.join("init.txt"), "init\n").unwrap();
    git_sync(&repo, &["add", "."]);
    git_sync(&repo, &["commit", "-m", "init"]);

    // Path traversal must never touch outside files
    assert!(discard_file(&repo, "../outside.txt").await.is_err());
    assert!(outside_secret.exists());
    assert!(reject_hunk(&repo, "../outside.txt", 0).await.is_err());
    assert!(outside_secret.exists());

    // Flag injection must be rejected
    assert!(get_commit_diff_files(&repo, "--stat").await.is_err());

    // Cyrillic + space filenames must not be C-quoted by git
    let cyr_name = "тест файл.txt";
    fs::write(repo.join(cyr_name), "привет\n").unwrap();
    let st = get_status(&repo).await.unwrap();
    assert!(
        st.unstaged_changes.iter().any(|c| c.path == cyr_name),
        "expected unquoted path in status: {:?}",
        st.unstaged_changes
    );
    let diffs = get_diff_files(&repo).await.unwrap();
    assert!(
        diffs.iter().any(|d| d.path == cyr_name),
        "expected unquoted path in diff: {:?}",
        diffs
    );
    stage_file(&repo, cyr_name).await.unwrap();
    let st2 = get_status(&repo).await.unwrap();
    assert!(st2.staged_changes.iter().any(|c| c.path == cyr_name));
}
