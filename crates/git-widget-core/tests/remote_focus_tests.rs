use git_widget_core::{
    branches::checkout_remote_branch_with_remote,
    error::CoreError,
    github_prs::list_github_prs_for_remote_with_token,
    graph::{get_commit_graph_for_remote, get_github_url_for_remote},
    ops::pull_remote,
    remote_focus::{switch_remote, SwitchRemoteResult},
    safety::{preview_push_for_remote, run_with_dirty_guard_for_remote, DirtyStrategy, GuardedOpKind},
    status::{get_status, get_status_for_remote},
    sync::{pull_both_branches, pull_branch, scan_integration_sync},
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use tempfile::TempDir;

fn git_sync(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("LC_ALL", "C")
        .output()
        .expect("failed to execute git");
    assert!(
        out.status.success(),
        "git {:?} in {} failed: {}",
        args,
        dir.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn git_config_get(dir: &Path, key: &str) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["config", "--get", key])
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn configure_user(dir: &Path) {
    git_sync(dir, &["config", "user.email", "test@example.com"]);
    git_sync(dir, &["config", "user.name", "Test User"]);
}

struct TwoRemoteFixture {
    _tmp: TempDir,
    bare_origin: PathBuf,
    bare_second: PathBuf,
    dev_origin: PathBuf,
    dev_second: PathBuf,
    work: PathBuf,
}

fn setup_two_bare_remotes() -> TwoRemoteFixture {
    let tmp = TempDir::new().unwrap();
    let bare_origin = tmp.path().join("bare_origin.git");
    let bare_second = tmp.path().join("bare_second.git");
    fs::create_dir_all(&bare_origin).unwrap();
    fs::create_dir_all(&bare_second).unwrap();
    git_sync(&bare_origin, &["init", "--bare", "-b", "main"]);
    git_sync(&bare_second, &["init", "--bare", "-b", "main"]);

    let dev_origin = tmp.path().join("dev_origin");
    fs::create_dir_all(&dev_origin).unwrap();
    git_sync(&dev_origin, &["init", "-b", "main"]);
    configure_user(&dev_origin);
    fs::write(dev_origin.join("README.md"), "# Init\n").unwrap();
    git_sync(&dev_origin, &["add", "."]);
    git_sync(&dev_origin, &["commit", "-m", "initial commit"]);
    git_sync(&dev_origin, &["remote", "add", "origin", bare_origin.to_str().unwrap()]);
    git_sync(&dev_origin, &["push", "-u", "origin", "main"]);
    git_sync(&dev_origin, &["remote", "add", "second", bare_second.to_str().unwrap()]);
    git_sync(&dev_origin, &["push", "second", "main"]);

    let dev_second = tmp.path().join("dev_second");
    let clone_second = Command::new("git")
        .args(["clone", bare_second.to_str().unwrap(), dev_second.to_str().unwrap()])
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .unwrap();
    assert!(clone_second.status.success());
    configure_user(&dev_second);

    let work = tmp.path().join("work");
    let clone_work = Command::new("git")
        .args(["clone", bare_origin.to_str().unwrap(), work.to_str().unwrap()])
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .unwrap();
    assert!(clone_work.status.success());
    configure_user(&work);
    git_sync(&work, &["remote", "add", "second", bare_second.to_str().unwrap()]);
    git_sync(&work, &["fetch", "second"]);

    TwoRemoteFixture {
        _tmp: tmp,
        bare_origin,
        bare_second,
        dev_origin,
        dev_second,
        work,
    }
}

#[tokio::test]
async fn switch_remote_fetches_and_updates_upstream() {
    let fx = setup_two_bare_remotes();
    let head_before = git_sync(&fx.work, &["rev-parse", "HEAD"]).trim().to_string();
    fs::write(fx.work.join("dirty.txt"), "keep me\n").unwrap();

    // Push a new commit on dev_second so switch_remote also proves fetch ran
    fs::write(fx.dev_second.join("second_only.txt"), "v2\n").unwrap();
    git_sync(&fx.dev_second, &["add", "."]);
    git_sync(&fx.dev_second, &["commit", "-m", "second commit"]);
    git_sync(&fx.dev_second, &["push", "origin", "main"]);

    let res: SwitchRemoteResult = switch_remote(&fx.work, "second").await.unwrap();
    assert!(res.upstream_switched);
    assert_eq!(res.target_upstream.as_deref(), Some("second/main"));
    assert_eq!(res.fetch_warning, None);
    assert_eq!(git_config_get(&fx.work, "branch.main.remote").as_deref(), Some("second"));
    assert_eq!(
        git_config_get(&fx.work, "branch.main.merge").as_deref(),
        Some("refs/heads/main")
    );
    assert_eq!(res.status.upstream.as_deref(), Some("second/main"));
    assert_eq!(res.status.behind, 1);

    // AC#16: HEAD and working tree untouched
    let head_after = git_sync(&fx.work, &["rev-parse", "HEAD"]).trim().to_string();
    assert_eq!(head_before, head_after);
    assert_eq!(fs::read_to_string(fx.work.join("dirty.txt")).unwrap(), "keep me\n");
}

#[tokio::test]
async fn switch_remote_preserves_existing_upstream_when_branch_missing() {
    let fx = setup_two_bare_remotes();
    git_sync(&fx.dev_origin, &["checkout", "-b", "feat"]);
    fs::write(fx.dev_origin.join("feat.txt"), "feat\n").unwrap();
    git_sync(&fx.dev_origin, &["add", "."]);
    git_sync(&fx.dev_origin, &["commit", "-m", "feat"]);
    git_sync(&fx.dev_origin, &["push", "-u", "origin", "feat"]);

    git_sync(&fx.work, &["fetch", "origin"]);
    git_sync(&fx.work, &["checkout", "--track", "origin/feat"]);
    fs::write(fx.work.join("wip.txt"), "untouched\n").unwrap();
    let head_before = git_sync(&fx.work, &["rev-parse", "HEAD"]).trim().to_string();

    let res = switch_remote(&fx.work, "second").await.unwrap();
    assert!(!res.upstream_switched);
    assert_eq!(res.target_upstream, None);
    assert_eq!(git_config_get(&fx.work, "branch.feat.remote").as_deref(), Some("origin"));
    assert_eq!(
        git_config_get(&fx.work, "branch.feat.merge").as_deref(),
        Some("refs/heads/feat")
    );
    assert_eq!(res.status.upstream, None);
    assert_eq!(res.status.configured_upstream.as_deref(), Some("origin/feat"));
    assert_eq!(res.status.ahead, 0);
    assert_eq!(res.status.behind, 0);
    assert_eq!(git_sync(&fx.work, &["rev-parse", "HEAD"]).trim(), head_before);
    assert_eq!(fs::read_to_string(fx.work.join("wip.txt")).unwrap(), "untouched\n");
}

#[tokio::test]
async fn switch_remote_validation_and_detached_head() {
    let fx = setup_two_bare_remotes();

    assert!(matches!(switch_remote(&fx.work, "").await, Err(CoreError::InvalidInput(_))));
    assert!(matches!(
        switch_remote(&fx.work, "--upload-pack=evil").await,
        Err(CoreError::InvalidInput(_))
    ));
    assert!(matches!(
        switch_remote(&fx.work, "missing-remote").await,
        Err(CoreError::InvalidInput(_))
    ));

    git_sync(&fx.work, &["checkout", "--detach", "HEAD"]);
    let detached = switch_remote(&fx.work, "second").await.unwrap();
    assert!(!detached.upstream_switched);
    assert_eq!(detached.target_upstream, None);
    assert_eq!(detached.status.upstream, None);
}

#[tokio::test]
async fn get_status_for_remote_isolates_tracking() {
    let fx = setup_two_bare_remotes();

    // Branch `feat` exists only on origin
    git_sync(&fx.dev_origin, &["checkout", "-b", "feat"]);
    fs::write(fx.dev_origin.join("feat.txt"), "f\n").unwrap();
    git_sync(&fx.dev_origin, &["add", "."]);
    git_sync(&fx.dev_origin, &["commit", "-m", "feat"]);
    git_sync(&fx.dev_origin, &["push", "-u", "origin", "feat"]);

    // Branch `dev` exists on both origin and second (second ahead by 1)
    git_sync(&fx.dev_origin, &["checkout", "-b", "dev", "main"]);
    git_sync(&fx.dev_origin, &["push", "-u", "origin", "dev"]);
    git_sync(&fx.dev_second, &["checkout", "-b", "dev", "main"]);
    fs::write(fx.dev_second.join("dev2.txt"), "d2\n").unwrap();
    git_sync(&fx.dev_second, &["add", "."]);
    git_sync(&fx.dev_second, &["commit", "-m", "second dev"]);
    git_sync(&fx.dev_second, &["push", "origin", "dev"]);

    // Push 1 commit to second/main and pull it into work/main (while work/main still tracks origin/main)
    git_sync(&fx.dev_second, &["checkout", "main"]);
    fs::write(fx.dev_second.join("m2.txt"), "m2\n").unwrap();
    git_sync(&fx.dev_second, &["add", "."]);
    git_sync(&fx.dev_second, &["commit", "-m", "second main"]);
    git_sync(&fx.dev_second, &["push", "origin", "main"]);

    git_sync(&fx.work, &["fetch", "--all"]);
    git_sync(&fx.work, &["checkout", "--track", "origin/feat"]);
    git_sync(&fx.work, &["checkout", "--track", "origin/dev"]);
    git_sync(&fx.work, &["checkout", "main"]);
    let pulled = pull_remote(&fx.work, Some("second")).await.unwrap();
    assert!(pulled.success);

    // Default get_status still sees origin/main with ahead == 1
    let raw_st = get_status(&fx.work).await.unwrap();
    assert_eq!(raw_st.upstream.as_deref(), Some("origin/main"));
    assert_eq!(raw_st.ahead, 1);

    // Scoped status for second isolates main, dev, and feat strictly against second
    let scoped = get_status_for_remote(&fx.work, Some("second")).await.unwrap();
    assert_eq!(scoped.upstream.as_deref(), Some("second/main"));
    assert_eq!(scoped.configured_upstream.as_deref(), Some("origin/main"));
    assert_eq!(scoped.ahead, 0);
    assert_eq!(scoped.behind, 0);

    let feat_track = scoped.branch_tracking.get("feat").unwrap();
    assert_eq!(feat_track.upstream, None);
    assert_eq!(feat_track.ahead, 0);
    assert_eq!(feat_track.behind, 0);

    let dev_track = scoped.branch_tracking.get("dev").unwrap();
    assert_eq!(dev_track.upstream.as_deref(), Some("second/dev"));
    assert_eq!(dev_track.behind, 1);
}

#[tokio::test]
async fn preview_push_and_sync_prefer_explicit_remote() {
    let fx = setup_two_bare_remotes();

    // Setup dev branch tracking origin/dev in .git/config
    git_sync(&fx.dev_origin, &["checkout", "-b", "dev", "main"]);
    git_sync(&fx.dev_origin, &["push", "-u", "origin", "dev"]);
    git_sync(&fx.dev_second, &["checkout", "-b", "dev", "main"]);
    git_sync(&fx.dev_second, &["push", "origin", "dev"]);

    git_sync(&fx.work, &["fetch", "--all"]);
    git_sync(&fx.work, &["checkout", "--track", "origin/dev"]);
    git_sync(&fx.work, &["checkout", "main"]);

    // Advance second/main and pull into work/main so work/main is ↑1 vs origin/main and 0/0 vs second/main
    git_sync(&fx.dev_second, &["checkout", "main"]);
    fs::write(fx.dev_second.join("from_second.txt"), "s\n").unwrap();
    git_sync(&fx.dev_second, &["add", "."]);
    git_sync(&fx.dev_second, &["commit", "-m", "second ahead"]);
    git_sync(&fx.dev_second, &["push", "origin", "main"]);
    pull_remote(&fx.work, Some("second")).await.unwrap();

    // AC#6 & AC#17: preview_push_for_remote on protected main must target second/main with 0 commits
    let preview = preview_push_for_remote(&fx.work, Some("second")).await.unwrap();
    assert!(preview.is_protected);
    assert_eq!(preview.target, "second/main");
    assert_eq!(preview.commit_count, 0);
    assert!(preview.commits.is_empty());

    // Advance second/dev while origin/dev stays unchanged
    git_sync(&fx.dev_second, &["checkout", "dev"]);
    fs::write(fx.dev_second.join("dev_second.txt"), "ds\n").unwrap();
    git_sync(&fx.dev_second, &["add", "."]);
    git_sync(&fx.dev_second, &["commit", "-m", "dev on second"]);
    git_sync(&fx.dev_second, &["push", "origin", "dev"]);

    // scan_integration_sync & pull_branch & pull_both_branches must prefer "second" over .git/config origin/dev
    let sync = scan_integration_sync(&fx.work, "main", "dev", Some("second"), true)
        .await
        .unwrap();
    assert_eq!(sync.target.upstream.as_deref(), Some("second/dev"));
    assert_eq!(sync.target.behind, 1);
    assert!(sync.any_behind);

    let pulled_dev = pull_branch(&fx.work, "dev", Some("second")).await.unwrap();
    assert!(pulled_dev.success);
    let after_dev = scan_integration_sync(&fx.work, "main", "dev", Some("second"), false)
        .await
        .unwrap();
    assert_eq!(after_dev.target.behind, 0);

    // Advance both second/main and second/dev and verify pull_both_branches
    fs::write(fx.dev_second.join("dev_second_2.txt"), "ds2\n").unwrap();
    git_sync(&fx.dev_second, &["add", "."]);
    git_sync(&fx.dev_second, &["commit", "-m", "dev on second 2"]);
    git_sync(&fx.dev_second, &["push", "origin", "dev"]);
    git_sync(&fx.dev_second, &["checkout", "main"]);
    fs::write(fx.dev_second.join("from_second_2.txt"), "s2\n").unwrap();
    git_sync(&fx.dev_second, &["add", "."]);
    git_sync(&fx.dev_second, &["commit", "-m", "main on second 2"]);
    git_sync(&fx.dev_second, &["push", "origin", "main"]);

    let both = pull_both_branches(&fx.work, "main", "dev", Some("second")).await.unwrap();
    assert!(both.success);
    assert!(fx.work.join("from_second_2.txt").exists());
}

#[tokio::test]
async fn graph_for_remote_excludes_other_remotes_and_preserves_topo_order() {
    let fx = setup_two_bare_remotes();

    // Add a remote whose name contains '/' (`team/fork`) pointing to bare_second
    git_sync(&fx.work, &["remote", "add", "team/fork", fx.bare_second.to_str().unwrap()]);
    git_sync(&fx.work, &["fetch", "team/fork"]);

    // origin gets an exclusive branch `only-origin`
    git_sync(&fx.dev_origin, &["checkout", "-b", "only-origin"]);
    fs::write(fx.dev_origin.join("origin_only.txt"), "oo\n").unwrap();
    git_sync(&fx.dev_origin, &["add", "."]);
    git_sync(&fx.dev_origin, &["commit", "-m", "origin exclusive"]);
    git_sync(&fx.dev_origin, &["push", "origin", "only-origin"]);
    git_sync(&fx.work, &["fetch", "origin"]);

    // Local branch with `/` in name (`feat/ui`)
    git_sync(&fx.work, &["checkout", "-b", "feat/ui"]);
    fs::write(fx.work.join("ui.txt"), "ui\n").unwrap();
    git_sync(&fx.work, &["add", "."]);
    git_sync(&fx.work, &["commit", "-m", "local feat/ui"]);
    git_sync(&fx.work, &["checkout", "main"]);

    // all_branches = true scoped to "second"
    let graph_all = get_commit_graph_for_remote(&fx.work, true, None, Some("second"), 50)
        .await
        .unwrap();
    assert!(graph_all.commits.iter().all(|c| c.subject != "origin exclusive"));
    assert!(graph_all
        .commits
        .iter()
        .flat_map(|c| c.refs.iter())
        .all(|r| !r.name.starts_with("origin/") && !r.name.starts_with("team/fork/")));
    assert!(graph_all
        .commits
        .iter()
        .flat_map(|c| c.refs.iter())
        .any(|r| r.name == "second/main" && r.kind == "remote"));
    assert!(graph_all
        .commits
        .iter()
        .flat_map(|c| c.refs.iter())
        .any(|r| r.name == "feat/ui" && r.kind == "local"));

    // Verify topo-order: every parent hash appears later in the list than its child
    for (idx, commit) in graph_all.commits.iter().enumerate() {
        for p in &commit.parents {
            if let Some(p_idx) = graph_all.commits.iter().position(|c| &c.hash == p) {
                assert!(p_idx > idx, "parent {p} appeared before child {}", commit.hash);
            }
        }
    }

    // all_branches = false scoped to "second"
    let graph_cur = get_commit_graph_for_remote(&fx.work, false, None, Some("second"), 50)
        .await
        .unwrap();
    assert!(graph_cur
        .commits
        .iter()
        .flat_map(|c| c.refs.iter())
        .all(|r| !r.name.starts_with("origin/") && !r.name.starts_with("team/fork/")));
    assert!(graph_cur
        .commits
        .iter()
        .flat_map(|c| c.refs.iter())
        .any(|r| r.name == "second/main" && r.kind == "remote"));

    // Remote name with `/` (`team/fork`) classifies `team/fork/main` as "remote" and `feat/ui` as "local"
    let graph_slash = get_commit_graph_for_remote(&fx.work, true, None, Some("team/fork"), 50)
        .await
        .unwrap();
    assert!(graph_slash
        .commits
        .iter()
        .flat_map(|c| c.refs.iter())
        .any(|r| r.name == "team/fork/main" && r.kind == "remote"));
    assert!(graph_slash
        .commits
        .iter()
        .flat_map(|c| c.refs.iter())
        .any(|r| r.name == "feat/ui" && r.kind == "local"));
}

#[tokio::test]
async fn github_url_and_prs_do_not_fallback_to_origin() {
    let fx = setup_two_bare_remotes();
    git_sync(
        &fx.work,
        &["remote", "set-url", "origin", "https://github.com/octocat/hello.git"],
    );
    git_sync(
        &fx.work,
        &["remote", "set-url", "second", "https://gitlab.com/acme/non-github.git"],
    );

    assert_eq!(
        get_github_url_for_remote(&fx.work, Some("origin")).await.as_deref(),
        Some("https://github.com/octocat/hello")
    );
    assert_eq!(get_github_url_for_remote(&fx.work, Some("second")).await, None);

    let prs = list_github_prs_for_remote_with_token(&fx.work, None, Some("second")).await;
    assert!(!prs.gh_available);
    assert_eq!(prs.github_url, None);
    assert!(prs.prs.is_empty());
}

#[tokio::test]
async fn guarded_checkout_and_remote_checkout_sync_upstream() {
    let fx = setup_two_bare_remotes();

    // Prepare `dev` on both origin and second, and `feature-x` on second
    git_sync(&fx.dev_origin, &["checkout", "-b", "dev", "main"]);
    git_sync(&fx.dev_origin, &["push", "-u", "origin", "dev"]);
    git_sync(&fx.dev_second, &["checkout", "-b", "dev", "main"]);
    git_sync(&fx.dev_second, &["push", "origin", "dev"]);
    git_sync(&fx.dev_second, &["checkout", "-b", "feature-x", "main"]);
    git_sync(&fx.dev_second, &["push", "origin", "feature-x"]);

    git_sync(&fx.work, &["fetch", "--all"]);
    git_sync(&fx.work, &["checkout", "--track", "origin/dev"]);
    git_sync(&fx.work, &["checkout", "main"]);

    // Point second to an unreachable URL to prove guarded checkout uses cached refs without network fetch
    let missing_path = fx._tmp.path().join("unreachable_second.git");
    git_sync(&fx.work, &["remote", "set-url", "second", missing_path.to_str().unwrap()]);

    let outcome = run_with_dirty_guard_for_remote(
        &fx.work,
        GuardedOpKind::Checkout,
        Some("dev"),
        DirtyStrategy::FailIfDirty,
        Some("second"),
    )
    .await
    .unwrap();
    assert!(!outcome.blocked_dirty);
    assert!(outcome.op_result.unwrap().success);
    assert_eq!(git_config_get(&fx.work, "branch.dev.remote").as_deref(), Some("second"));

    // Idempotent repeat on same branch & remote
    let repeat = run_with_dirty_guard_for_remote(
        &fx.work,
        GuardedOpKind::Checkout,
        Some("dev"),
        DirtyStrategy::FailIfDirty,
        Some("second"),
    )
    .await
    .unwrap();
    assert!(repeat.op_result.unwrap().success);
    assert_eq!(git_config_get(&fx.work, "branch.dev.remote").as_deref(), Some("second"));

    // checkout_remote_branch_with_remote creates local feature-x tracking second/feature-x
    let remote_co =
        checkout_remote_branch_with_remote(&fx.work, "second/feature-x", Some("second"))
            .await
            .unwrap();
    assert!(remote_co.success);
    assert_eq!(
        git_config_get(&fx.work, "branch.feature-x.remote").as_deref(),
        Some("second")
    );
}

#[tokio::test]
async fn switch_remote_offline_fetch_returns_warning_without_rollback() {
    let fx = setup_two_bare_remotes();
    let bad_path = fx._tmp.path().join("nonexistent_remote.git");
    git_sync(&fx.work, &["remote", "set-url", "second", bad_path.to_str().unwrap()]);

    let res = switch_remote(&fx.work, "second").await.unwrap();
    assert!(res.fetch_warning.is_some());
    assert!(res.upstream_switched);
    assert_eq!(res.target_upstream.as_deref(), Some("second/main"));
    assert_eq!(res.status.upstream.as_deref(), Some("second/main"));
    assert_eq!(git_config_get(&fx.work, "branch.main.remote").as_deref(), Some("second"));
}

#[tokio::test]
async fn diverged_pull_conflicts_and_unrelated_histories() {
    let fx = setup_two_bare_remotes();

    // Create conflicting commits on local work/main and dev_second/main
    fs::write(fx.work.join("README.md"), "# Local Diverged\n").unwrap();
    git_sync(&fx.work, &["commit", "-am", "local diverged"]);

    fs::write(fx.dev_second.join("README.md"), "# Second Diverged\n").unwrap();
    git_sync(&fx.dev_second, &["commit", "-am", "second diverged"]);
    git_sync(&fx.dev_second, &["push", "origin", "main"]);

    let sw = switch_remote(&fx.work, "second").await.unwrap();
    assert!(sw.status.ahead > 0 && sw.status.behind > 0);

    // Ensure non-fast-forward pull triggers merge conflict cleanly
    git_sync(&fx.work, &["config", "pull.rebase", "false"]);
    let conflict_res = pull_remote(&fx.work, Some("second")).await.unwrap();
    assert!(!conflict_res.success);
    assert!(conflict_res.has_conflicts);

    git_sync(&fx.work, &["merge", "--abort"]);

    // Unrelated histories on branch `orphan`
    git_sync(&fx.dev_second, &["checkout", "--orphan", "orphan"]);
    git_sync(&fx.dev_second, &["rm", "-rf", "."]);
    fs::write(fx.dev_second.join("orphan.txt"), "remote orphan\n").unwrap();
    git_sync(&fx.dev_second, &["add", "."]);
    git_sync(&fx.dev_second, &["commit", "-m", "remote orphan"]);
    git_sync(&fx.dev_second, &["push", "origin", "orphan"]);

    git_sync(&fx.work, &["checkout", "--orphan", "orphan"]);
    git_sync(&fx.work, &["rm", "-rf", "."]);
    fs::write(fx.work.join("local_orphan.txt"), "local orphan\n").unwrap();
    git_sync(&fx.work, &["add", "."]);
    git_sync(&fx.work, &["commit", "-m", "local orphan"]);

    let sw_orphan = switch_remote(&fx.work, "second").await.unwrap();
    assert!(sw_orphan.upstream_switched);

    let unrelated = pull_remote(&fx.work, Some("second")).await;
    assert!(matches!(unrelated, Err(CoreError::GitFailed { .. })));
}
