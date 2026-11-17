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
