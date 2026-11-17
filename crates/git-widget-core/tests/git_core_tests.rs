use git_widget_core::{
    ops::{
        checkout_branch, commit_all, fetch_all, merge_branch, pull_current, push_current,
        stash_pop, stash_save,
    },
    scanner::scan_repositories,
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
        .expect("failed to execute git");
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
    git_sync(dir, &["commit", "-m", "initial commit"]);
}

#[test]
fn scanner_finds_nested_repos_and_worktrees_ignoring_heavy_dirs() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    let repo_a = root.join("group").join("repo-a");
    init_repo(&repo_a);

    let wt_b = root.join("wt-b");
    fs::create_dir_all(&wt_b).unwrap();
    fs::write(wt_b.join(".git"), "gitdir: ../group/repo-a/.git/worktrees/wt-b\n").unwrap();

    let ignored = root.join("group").join("repo-a").join("node_modules").join("pkg");
    fs::create_dir_all(ignored.join(".git")).unwrap();

    let repos = scan_repositories(&[root.to_path_buf()], 3).unwrap();
    let names: Vec<&str> = repos.iter().map(|r| r.name.as_str()).collect();
    assert!(names.contains(&"repo-a"));
    assert!(names.contains(&"wt-b"));
    assert!(!names.contains(&"pkg"));

    let wt_entry = repos.iter().find(|r| r.name == "wt-b").unwrap();
    assert!(wt_entry.is_worktree);
}

#[tokio::test]
async fn status_and_basic_ops_work_on_temp_repo() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("app");
    init_repo(&repo);

    git_sync(&repo, &["checkout", "-b", "developer"]);
    fs::write(repo.join("file.txt"), "hello\n").unwrap();

    let st = get_status(&repo).await.unwrap();
    assert_eq!(st.branch, "developer");
    assert!(st.is_dirty);
    assert!(st.local_branches.contains(&"main".to_string()));
    assert!(st.local_branches.contains(&"developer".to_string()));

    let committed = commit_all(&repo, "add file").await.unwrap();
    assert!(committed.success);

    fs::write(repo.join("temp.txt"), "wip\n").unwrap();
    assert!(stash_save(&repo, Some("wip")).await.unwrap().success);
    assert!(!get_status(&repo).await.unwrap().is_dirty);
    assert!(stash_pop(&repo).await.unwrap().success);
    assert!(get_status(&repo).await.unwrap().is_dirty);
    fs::remove_file(repo.join("temp.txt")).unwrap();

    assert!(checkout_branch(&repo, "main").await.unwrap().success);
    let merged = merge_branch(&repo, "developer").await.unwrap();
    assert!(merged.success);
    assert!(!merged.has_conflicts);
    assert!(repo.join("file.txt").exists());
}

#[tokio::test]
async fn merge_detects_conflicts() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("conflict-repo");
    init_repo(&repo);

    git_sync(&repo, &["checkout", "-b", "feature"]);
    fs::write(repo.join("README.md"), "# Feature\n").unwrap();
    git_sync(&repo, &["commit", "-am", "feature edit"]);

    git_sync(&repo, &["checkout", "main"]);
    fs::write(repo.join("README.md"), "# Main\n").unwrap();
    git_sync(&repo, &["commit", "-am", "main edit"]);

    let res = merge_branch(&repo, "feature").await.unwrap();
    assert!(!res.success);
    assert!(res.has_conflicts);

    let st = get_status(&repo).await.unwrap();
    assert!(st.is_merging);
    assert_eq!(st.conflicts, vec!["README.md".to_string()]);
}

#[tokio::test]
async fn fetch_pull_push_work_with_bare_remote() {
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

    fs::write(dev1.join("sync.txt"), "from dev1\n").unwrap();
    assert!(commit_all(&dev1, "sync").await.unwrap().success);
    assert!(push_current(&dev1).await.unwrap().success);

    assert!(fetch_all(&dev2).await.unwrap().success);
    let st2 = get_status(&dev2).await.unwrap();
    assert_eq!(st2.behind, 1);

    assert!(pull_current(&dev2).await.unwrap().success);
    assert!(dev2.join("sync.txt").exists());
}
