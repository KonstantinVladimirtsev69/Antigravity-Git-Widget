use git_widget_core::{
    ops::merge_branch,
    safety::{
        abort_merge, build_conflict_prompt, preview_merge, run_with_dirty_guard, DirtyStrategy,
        GuardedOpKind,
    },
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

fn init_repo(dir: &Path) {
    fs::create_dir_all(dir).unwrap();
    git_sync(dir, &["init", "-b", "main"]);
    git_sync(dir, &["config", "user.email", "test@example.com"]);
    git_sync(dir, &["config", "user.name", "Test User"]);
    fs::write(dir.join("README.md"), "# Init\n").unwrap();
    git_sync(dir, &["add", "."]);
    git_sync(dir, &["commit", "-m", "initial commit"]);
}

#[tokio::test]
async fn dirty_guard_blocks_or_stashes_and_pops() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("guarded");
    init_repo(&repo);
    git_sync(&repo, &["checkout", "-b", "developer"]);
    git_sync(&repo, &["checkout", "main"]);

    fs::write(repo.join("draft.txt"), "uncommitted\n").unwrap();

    let blocked = run_with_dirty_guard(
        &repo,
        GuardedOpKind::Checkout,
        Some("developer"),
        DirtyStrategy::FailIfDirty,
    )
    .await
    .unwrap();
    assert!(blocked.blocked_dirty);
    assert_eq!(blocked.dirty_files, vec!["draft.txt".to_string()]);

    let stashed = run_with_dirty_guard(
        &repo,
        GuardedOpKind::Checkout,
        Some("developer"),
        DirtyStrategy::StashAndPop,
    )
    .await
    .unwrap();
    assert!(!stashed.blocked_dirty);
    assert!(stashed.stash_restored);
    assert_eq!(get_status(&repo).await.unwrap().branch, "developer");
    assert!(repo.join("draft.txt").exists());
}

#[tokio::test]
async fn protected_preview_and_conflict_abort_prompt() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("preview");
    init_repo(&repo);

    git_sync(&repo, &["checkout", "-b", "feature"]);
    fs::write(repo.join("README.md"), "# Feature\n").unwrap();
    git_sync(&repo, &["commit", "-am", "feature change"]);

    git_sync(&repo, &["checkout", "main"]);
    fs::write(repo.join("README.md"), "# Main\n").unwrap();
    git_sync(&repo, &["commit", "-am", "main change"]);

    let preview = preview_merge(&repo, "feature").await.unwrap();
    assert!(preview.is_protected);
    assert_eq!(preview.target, "main");
    assert_eq!(preview.commit_count, 1);

    let res = merge_branch(&repo, "feature").await.unwrap();
    assert!(res.has_conflicts);

    let prompt = build_conflict_prompt(&repo).await.unwrap();
    assert!(prompt.contains("README.md"));
    assert!(prompt.contains("<<<<<<<"));

    assert!(abort_merge(&repo).await.unwrap().success);
    assert!(!get_status(&repo).await.unwrap().is_merging);
}

#[tokio::test]
async fn dirty_guard_pulls_with_remote_and_rolls_back_stash_on_failure() {
    let tmp = TempDir::new().unwrap();
    let bare = tmp.path().join("remote.git");
    fs::create_dir_all(&bare).unwrap();
    git_sync(&bare, &["init", "--bare", "-b", "main"]);

    let dev1 = tmp.path().join("dev1");
    init_repo(&dev1);
    git_sync(&dev1, &["remote", "add", "origin", bare.to_str().unwrap()]);
    git_sync(&dev1, &["push", "-u", "origin", "main"]);

    let dev2 = tmp.path().join("dev2");
    Command::new("git")
        .args(["clone", bare.to_str().unwrap(), dev2.to_str().unwrap()])
        .output()
        .unwrap();
    git_sync(&dev2, &["config", "user.email", "test@example.com"]);
    git_sync(&dev2, &["config", "user.name", "Test User"]);

    // dev1 pushes an update
    fs::write(dev1.join("remote_new.txt"), "hello\n").unwrap();
    git_sync(&dev1, &["add", "."]);
    git_sync(&dev1, &["commit", "-m", "remote update"]);
    git_sync(&dev1, &["push"]);

    // dev2 has uncommitted changes and runs guarded pull with remote = Some("origin")
    fs::write(dev2.join("local_wip.txt"), "wip\n").unwrap();
    let outcome = run_with_dirty_guard(
        &dev2,
        GuardedOpKind::Pull,
        Some("origin"),
        DirtyStrategy::StashAndPop,
    )
    .await
    .unwrap();
    assert!(!outcome.blocked_dirty);
    assert!(outcome.stash_restored);
    assert!(dev2.join("remote_new.txt").exists());
    assert!(dev2.join("local_wip.txt").exists());

    // Error during guarded checkout must roll back auto-stash so user doesn't lose track of it
    let err = run_with_dirty_guard(
        &dev2,
        GuardedOpKind::Checkout,
        Some("non-existent-branch"),
        DirtyStrategy::StashAndPop,
    )
    .await;
    assert!(err.is_err());
    assert!(dev2.join("local_wip.txt").exists());

    // Flag injection in preview_merge must be rejected
    assert!(preview_merge(&dev2, "--all").await.is_err());
}
