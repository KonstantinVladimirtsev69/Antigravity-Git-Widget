use git_widget_core::{
    integrate::{
        abort_rebase, integrate_branches, preview_integration, pull_both_branches,
        scan_integration_sync, IntegrationMode,
    },
    safety::DirtyStrategy,
    scanner::ScannerConfig,
    status::get_status,
};
use std::{fs, path::Path, process::Command};
use tempfile::TempDir;

fn git_sync(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .expect("failed to run git");
    assert!(
        out.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).to_string()
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
async fn dual_branch_scan_detects_and_pulls_both_branches() {
    let tmp = TempDir::new().unwrap();
    let bare = tmp.path().join("remote.git");
    fs::create_dir_all(&bare).unwrap();
    git_sync(&bare, &["init", "--bare", "-b", "main"]);

    let dev1 = tmp.path().join("dev1");
    init_repo(&dev1);
    git_sync(&dev1, &["remote", "add", "origin", bare.to_str().unwrap()]);
    git_sync(&dev1, &["push", "-u", "origin", "main"]);
    git_sync(&dev1, &["checkout", "-b", "feature"]);
    fs::write(dev1.join("feat.txt"), "v1\n").unwrap();
    git_sync(&dev1, &["add", "."]);
    git_sync(&dev1, &["commit", "-m", "feat v1"]);
    git_sync(&dev1, &["push", "-u", "origin", "feature"]);

    let dev2 = tmp.path().join("dev2");
    Command::new("git")
        .args(["clone", bare.to_str().unwrap(), dev2.to_str().unwrap()])
        .output()
        .unwrap();
    git_sync(&dev2, &["config", "user.email", "test@example.com"]);
    git_sync(&dev2, &["config", "user.name", "Test User"]);
    git_sync(&dev2, &["checkout", "--track", "origin/feature"]);

    // dev1 pushes new commits to both main and feature
    git_sync(&dev1, &["checkout", "main"]);
    fs::write(dev1.join("main.txt"), "remote main\n").unwrap();
    git_sync(&dev1, &["add", "."]);
    git_sync(&dev1, &["commit", "-m", "remote main update"]);
    git_sync(&dev1, &["push"]);

    git_sync(&dev1, &["checkout", "feature"]);
    fs::write(dev1.join("feat2.txt"), "remote feat\n").unwrap();
    git_sync(&dev1, &["add", "."]);
    git_sync(&dev1, &["commit", "-m", "remote feature update"]);
    git_sync(&dev1, &["push"]);

    // Scan from dev2 (currently on feature)
    let sync = scan_integration_sync(&dev2, "feature", "main", None, true)
        .await
        .unwrap();
    assert_eq!(sync.source.branch, "feature");
    assert_eq!(sync.source.behind, 1);
    assert_eq!(sync.target.branch, "main");
    assert_eq!(sync.target.behind, 1);
    assert!(sync.any_behind);

    let st = get_status(&dev2).await.unwrap();
    assert_eq!(st.branch_tracking.get("main").unwrap().behind, 1);
    assert_eq!(st.branch_tracking.get("feature").unwrap().behind, 1);

    // Pull both branches without losing current branch
    let pulled = pull_both_branches(&dev2, "feature", "main", None)
        .await
        .unwrap();
    assert!(pulled.success);
    assert!(!pulled.has_conflicts);

    let sync_after = scan_integration_sync(&dev2, "feature", "main", None, false)
        .await
        .unwrap();
    assert_eq!(sync_after.source.behind, 0);
    assert_eq!(sync_after.target.behind, 0);
    assert!(!sync_after.any_behind);
    assert_eq!(get_status(&dev2).await.unwrap().branch, "feature");
    assert!(dev2.join("feat2.txt").exists());
}

#[tokio::test]
async fn integration_modes_work_and_rebase_conflict_aborts() {
    let tmp = TempDir::new().unwrap();

    // 1. MergeNoFf creates 2-parent merge commit even on fast-forwardable branch
    let r_noff = tmp.path().join("noff");
    init_repo(&r_noff);
    git_sync(&r_noff, &["checkout", "-b", "feat"]);
    fs::write(r_noff.join("a.txt"), "a\n").unwrap();
    git_sync(&r_noff, &["add", "."]);
    git_sync(&r_noff, &["commit", "-m", "feat a"]);
    let res = integrate_branches(
        &r_noff,
        "feat",
        "main",
        IntegrationMode::MergeNoFf,
        DirtyStrategy::FailIfDirty,
    )
    .await
    .unwrap();
    assert!(res.op_result.unwrap().success);
    let parents = git_sync(&r_noff, &["rev-list", "--parents", "-n", "1", "HEAD"]);
    assert_eq!(parents.split_whitespace().count(), 3); // commit + 2 parents

    // 2. Squash combines 2 commits into 1
    let r_sq = tmp.path().join("squash");
    init_repo(&r_sq);
    git_sync(&r_sq, &["checkout", "-b", "feat"]);
    fs::write(r_sq.join("s1.txt"), "1\n").unwrap();
    git_sync(&r_sq, &["add", "."]);
    git_sync(&r_sq, &["commit", "-m", "s1"]);
    fs::write(r_sq.join("s2.txt"), "2\n").unwrap();
    git_sync(&r_sq, &["add", "."]);
    git_sync(&r_sq, &["commit", "-m", "s2"]);
    let count_before = git_sync(&r_sq, &["rev-list", "--count", "main"]);
    assert_eq!(count_before.trim(), "1");
    let preview = preview_integration(&r_sq, "feat", "main", IntegrationMode::Squash)
        .await
        .unwrap();
    assert!(preview.is_protected);
    assert_eq!(preview.commit_count, 2);
    let res_sq = integrate_branches(
        &r_sq,
        "feat",
        "main",
        IntegrationMode::Squash,
        DirtyStrategy::FailIfDirty,
    )
    .await
    .unwrap();
    assert!(res_sq.op_result.unwrap().success);
    let count_after = git_sync(&r_sq, &["rev-list", "--count", "main"]);
    assert_eq!(count_after.trim(), "2");
    assert!(r_sq.join("s1.txt").exists() && r_sq.join("s2.txt").exists());

    // 3. FfOnly fails when diverged
    let r_ff = tmp.path().join("ff");
    init_repo(&r_ff);
    git_sync(&r_ff, &["checkout", "-b", "feat"]);
    fs::write(r_ff.join("f.txt"), "f\n").unwrap();
    git_sync(&r_ff, &["add", "."]);
    git_sync(&r_ff, &["commit", "-m", "feat"]);
    git_sync(&r_ff, &["checkout", "main"]);
    fs::write(r_ff.join("m.txt"), "m\n").unwrap();
    git_sync(&r_ff, &["add", "."]);
    git_sync(&r_ff, &["commit", "-m", "main"]);
    let ff_err = integrate_branches(
        &r_ff,
        "feat",
        "main",
        IntegrationMode::FfOnly,
        DirtyStrategy::FailIfDirty,
    )
    .await;
    assert!(ff_err.is_err());

    // 4. Rebase replays commits linearly and detects/aborts conflicts
    let r_rb = tmp.path().join("rebase");
    init_repo(&r_rb);
    git_sync(&r_rb, &["checkout", "-b", "feat"]);
    fs::write(r_rb.join("feat.txt"), "ok\n").unwrap();
    git_sync(&r_rb, &["add", "."]);
    git_sync(&r_rb, &["commit", "-m", "feat ok"]);
    git_sync(&r_rb, &["checkout", "main"]);
    fs::write(r_rb.join("main.txt"), "m\n").unwrap();
    git_sync(&r_rb, &["add", "."]);
    git_sync(&r_rb, &["commit", "-m", "main"]);
    let res_rb = integrate_branches(
        &r_rb,
        "main",
        "feat",
        IntegrationMode::Rebase,
        DirtyStrategy::FailIfDirty,
    )
    .await
    .unwrap();
    assert!(res_rb.op_result.unwrap().success);
    let rb_parents = git_sync(&r_rb, &["rev-list", "--parents", "-n", "1", "HEAD"]);
    assert_eq!(rb_parents.split_whitespace().count(), 2); // linear: commit + 1 parent

    // Rebase conflict + abort
    let r_rbc = tmp.path().join("rebase_conflict");
    init_repo(&r_rbc);
    git_sync(&r_rbc, &["checkout", "-b", "feat"]);
    fs::write(r_rbc.join("README.md"), "# Feat\n").unwrap();
    git_sync(&r_rbc, &["commit", "-am", "feat edit"]);
    git_sync(&r_rbc, &["checkout", "main"]);
    fs::write(r_rbc.join("README.md"), "# Main\n").unwrap();
    git_sync(&r_rbc, &["commit", "-am", "main edit"]);
    let res_rbc = integrate_branches(
        &r_rbc,
        "main",
        "feat",
        IntegrationMode::Rebase,
        DirtyStrategy::FailIfDirty,
    )
    .await
    .unwrap();
    let op = res_rbc.op_result.unwrap();
    assert!(!op.success);
    assert!(op.has_conflicts);
    let st_rbc = get_status(&r_rbc).await.unwrap();
    assert!(st_rbc.is_rebasing);
    assert!(!st_rbc.conflicts.is_empty());
    assert!(abort_rebase(&r_rbc).await.unwrap().success);
    assert!(!get_status(&r_rbc).await.unwrap().is_rebasing);
}

#[tokio::test]
async fn integrate_stash_and_pop_works_across_branch_switch() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("dirty_switch");
    init_repo(&repo);
    git_sync(&repo, &["checkout", "-b", "feat"]);
    fs::write(repo.join("feat.txt"), "committed\n").unwrap();
    git_sync(&repo, &["add", "."]);
    git_sync(&repo, &["commit", "-m", "feat"]);

    fs::write(repo.join("wip.txt"), "uncommitted\n").unwrap();

    let out = integrate_branches(
        &repo,
        "feat",
        "main",
        IntegrationMode::Merge,
        DirtyStrategy::StashAndPop,
    )
    .await
    .unwrap();
    assert!(!out.blocked_dirty);
    assert!(out.stash_restored);
    assert!(out.op_result.unwrap().success);
    assert_eq!(get_status(&repo).await.unwrap().branch, "main");
    assert!(repo.join("feat.txt").exists());
    assert!(repo.join("wip.txt").exists());
}

#[test]
fn scanner_config_integration_mode_defaults_and_persists() {
    let cfg: ScannerConfig =
        serde_json::from_str(r#"{"roots":["C:\\Projects\\"],"max_depth":3}"#).unwrap();
    assert_eq!(cfg.integration_mode, IntegrationMode::Merge);

    let cfg_rb = ScannerConfig {
        integration_mode: IntegrationMode::Rebase,
        ..Default::default()
    };
    let json = serde_json::to_string(&cfg_rb).unwrap();
    let decoded: ScannerConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.integration_mode, IntegrationMode::Rebase);
}
