use git_widget_core::{
    branches::{cherry_pick, create_branch, delete_branch, undo_last_commit},
    diff::stage_file,
    ops::{checkout_branch, commit_smart, stage_all, stash_save, unstage_all},
    stash::{list_stashes, stash_apply, stash_drop},
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
    assert!(
        out.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
}

fn init_repo(dir: &Path) {
    fs::create_dir_all(dir).unwrap();
    git_sync(dir, &["init", "-b", "main"]);
    git_sync(dir, &["config", "user.email", "test@example.com"]);
    git_sync(dir, &["config", "user.name", "Test User"]);
    fs::write(dir.join("README.md"), "# Init\n").unwrap();
    git_sync(dir, &["add", "."]);
    git_sync(dir, &["commit", "-m", "initial"]);
}

#[tokio::test]
async fn branch_crud_and_cherry_pick_work() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("branches");
    init_repo(&repo);

    create_branch(&repo, "feat/demo").await.unwrap();
    assert_eq!(get_status(&repo).await.unwrap().branch, "feat/demo");

    fs::write(repo.join("demo.txt"), "picked\n").unwrap();
    commit_smart(&repo, "add demo", false).await.unwrap();

    let hash_out = Command::new("git")
        .arg("-C")
        .arg(&repo)
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    let hash = String::from_utf8_lossy(&hash_out.stdout).trim().to_string();

    checkout_branch(&repo, "main").await.unwrap();
    assert!(!repo.join("demo.txt").exists());

    let cp = cherry_pick(&repo, &hash).await.unwrap();
    assert!(cp.success);
    assert!(repo.join("demo.txt").exists());

    delete_branch(&repo, "feat/demo", true).await.unwrap();
    assert!(!get_status(&repo)
        .await
        .unwrap()
        .local_branches
        .contains(&"feat/demo".to_string()));
}

#[tokio::test]
async fn staged_unstaged_smart_commit_amend_and_undo() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("staging");
    init_repo(&repo);

    fs::write(repo.join("a.txt"), "aaa\n").unwrap();
    fs::write(repo.join("b.txt"), "bbb\n").unwrap();

    stage_file(&repo, "a.txt").await.unwrap();
    let st = get_status(&repo).await.unwrap();
    assert_eq!(st.staged_changes.len(), 1);
    assert_eq!(st.staged_changes[0].path, "a.txt");
    assert_eq!(st.unstaged_changes.len(), 1);
    assert_eq!(st.unstaged_changes[0].path, "b.txt");

    // Smart commit only commits staged `a.txt`, leaving `b.txt` unstaged.
    commit_smart(&repo, "only a", false).await.unwrap();
    let st2 = get_status(&repo).await.unwrap();
    assert!(st2.staged_changes.is_empty());
    assert_eq!(st2.unstaged_changes.len(), 1);
    assert_eq!(st2.unstaged_changes[0].path, "b.txt");

    // Amend last commit message.
    commit_smart(&repo, "only a amended", true).await.unwrap();

    // Undo last commit brings `a.txt` back to staged.
    undo_last_commit(&repo).await.unwrap();
    let st3 = get_status(&repo).await.unwrap();
    assert!(st3.staged_changes.iter().any(|c| c.path == "a.txt"));

    stage_all(&repo).await.unwrap();
    assert!(get_status(&repo).await.unwrap().unstaged_changes.is_empty());
    unstage_all(&repo).await.unwrap();
    assert!(get_status(&repo).await.unwrap().staged_changes.is_empty());
}

#[tokio::test]
async fn stash_manager_list_apply_and_drop() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("stashes");
    init_repo(&repo);

    fs::write(repo.join("wip1.txt"), "1\n").unwrap();
    stash_save(&repo, Some("first wip")).await.unwrap();

    fs::write(repo.join("wip2.txt"), "2\n").unwrap();
    stash_save(&repo, Some("second wip")).await.unwrap();

    let list = list_stashes(&repo).await.unwrap();
    assert_eq!(list.len(), 2);
    assert!(list[0].message.contains("second wip"));
    assert!(list[1].message.contains("first wip"));

    stash_apply(&repo, 1).await.unwrap();
    assert!(repo.join("wip1.txt").exists());
    assert_eq!(list_stashes(&repo).await.unwrap().len(), 2);

    stash_drop(&repo, 0).await.unwrap();
    assert_eq!(list_stashes(&repo).await.unwrap().len(), 1);
}
