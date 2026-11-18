use git_widget_core::graph::{get_commit_details, get_commit_graph, remote_to_github_url};
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

#[test]
fn converts_ssh_and_https_github_urls() {
    assert_eq!(
        remote_to_github_url("git@github.com:acme/git-widget.git").as_deref(),
        Some("https://github.com/acme/git-widget")
    );
    assert_eq!(
        remote_to_github_url("https://github.com/acme/git-widget.git").as_deref(),
        Some("https://github.com/acme/git-widget")
    );
    assert_eq!(remote_to_github_url("https://gitlab.com/acme/repo.git"), None);
}

#[tokio::test]
async fn graph_returns_parents_authors_decorations_and_details() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("graph-repo");
    fs::create_dir_all(&repo).unwrap();
    git_sync(&repo, &["init", "-b", "main"]);
    git_sync(&repo, &["config", "user.email", "alice@example.com"]);
    git_sync(&repo, &["config", "user.name", "Alice"]);
    fs::write(repo.join("a.txt"), "hello\n").unwrap();
    git_sync(&repo, &["add", "."]);
    git_sync(&repo, &["commit", "-m", "alice init"]);

    git_sync(&repo, &["checkout", "-b", "feat/bob"]);
    git_sync(&repo, &["config", "user.email", "bob@example.com"]);
    git_sync(&repo, &["config", "user.name", "Bob"]);
    fs::write(repo.join("b.txt"), "line1\nline2\n").unwrap();
    git_sync(&repo, &["add", "."]);
    git_sync(&repo, &["commit", "-m", "bob feature"]);

    let graph = get_commit_graph(&repo, true, None, 20).await.unwrap();
    assert_eq!(graph.commits.len(), 2);
    assert_eq!(graph.authors.len(), 2);
    assert!(graph.commits[0].is_head);
    assert_eq!(graph.commits[0].author_name, "Bob");
    assert_eq!(graph.commits[0].parents.len(), 1);

    let filtered = get_commit_graph(&repo, true, Some("Alice"), 20).await.unwrap();
    assert_eq!(filtered.commits.len(), 1);
    assert_eq!(filtered.commits[0].author_name, "Alice");

    let details = get_commit_details(&repo, &graph.commits[0].hash).await.unwrap();
    assert_eq!(details.total_additions, 2);
    assert_eq!(details.files.len(), 1);
    assert_eq!(details.files[0].path, "b.txt");
    assert_eq!(details.files[0].status, "A");
}
