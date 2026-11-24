use git_widget_sidecar::{
    cli::{parse_cli_args, CliMode},
    self_update::{
        cleanup_stale_update_dirs, detect_installed_hosts_in, replace_exe_rename_before_copy,
        run_self_update, wait_for_parent_pid,
    },
    t3_scripts::{render_ui_bridge_js, T3_LOADER_CJS, T3_SERVER_HOOK_CJS},
};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::PathBuf,
    process::{Command, Stdio},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

fn tmp_dir(prefix: &str) -> PathBuf {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("{prefix}_{}_{n}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    format!("{:x}", h.finalize())
}

#[test]
fn parses_self_update_cli_flags_and_requires_parent_pid() {
    let args: Vec<String> = vec![
        "git-widget-sidecar.exe".into(),
        "--self-update".into(),
        "--parent-pid".into(),
        "1234".into(),
        "--serve-port".into(),
        "45123".into(),
        "--port-file".into(),
        r"C:\t\p.txt".into(),
    ];
    assert_eq!(
        parse_cli_args(&args, false).unwrap(),
        CliMode::SelfUpdate {
            parent_pid: 1234,
            serve_port: Some(45123),
            port_file: Some(PathBuf::from(r"C:\t\p.txt")),
            ag_dir: None,
            t3_local_dir: None,
        }
    );

    let eq_args: Vec<String> = vec![
        "bin".into(),
        "--self-update".into(),
        "--parent-pid=9876".into(),
        "--serve-port=8080".into(),
        r"--port-file=C:\t\q.txt".into(),
        r"--ag-dir=C:\ag".into(),
        r"--t3-local-dir=C:\t3".into(),
    ];
    assert_eq!(
        parse_cli_args(&eq_args, true).unwrap(),
        CliMode::SelfUpdate {
            parent_pid: 9876,
            serve_port: Some(8080),
            port_file: Some(PathBuf::from(r"C:\t\q.txt")),
            ag_dir: Some(PathBuf::from(r"C:\ag")),
            t3_local_dir: Some(PathBuf::from(r"C:\t3")),
        }
    );

    let missing_pid: Vec<String> = vec!["bin".into(), "--self-update".into()];
    assert!(parse_cli_args(&missing_pid, false).is_err());
}

#[test]
fn detects_hosts_strictly_and_ignores_settings_json_only() {
    let root = tmp_dir("gw_host_detect");
    let ag_settings_only = root.join("ag_settings_only");
    fs::create_dir_all(&ag_settings_only).unwrap();
    fs::write(ag_settings_only.join("settings.json"), "{}").unwrap();
    let t3_empty = root.join("t3_empty");
    fs::create_dir_all(&t3_empty).unwrap();

    let empty = detect_installed_hosts_in(Some(&ag_settings_only), Some(&t3_empty));
    assert!(empty.is_empty());
    assert_eq!(empty.antigravity_plugin_dir, None);
    assert_eq!(empty.t3_local_dir, None);

    let ag_real = root.join("ag_real");
    let sidecar_dir = ag_real.join("sidecars").join("git_widget");
    fs::create_dir_all(&sidecar_dir).unwrap();
    fs::write(sidecar_dir.join("sidecar.json"), "{}").unwrap();

    let t3_real = root.join("t3_real");
    fs::create_dir_all(&t3_real).unwrap();
    fs::write(t3_real.join("t3-loader.cjs"), "// loader").unwrap();

    let found = detect_installed_hosts_in(Some(&ag_real), Some(&t3_real));
    assert!(!found.is_empty());
    assert_eq!(found.antigravity_plugin_dir, Some(ag_real));
    assert_eq!(found.t3_local_dir, Some(t3_real));

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn run_self_update_updates_both_hosts_and_preserves_app_asar() {
    let root = tmp_dir("gw_self_update_hosts");
    let new_exe = root.join("new-git-widget-sidecar.exe");
    let new_bytes = b"MZ-UPDATED-SIDECAR-V020";
    fs::write(&new_exe, new_bytes).unwrap();

    // Error when neither host exists (must not create files in empty dirs)
    let empty_ag = root.join("empty_ag");
    let empty_t3 = root.join("empty_t3");
    fs::create_dir_all(&empty_ag).unwrap();
    fs::create_dir_all(&empty_t3).unwrap();
    fs::write(empty_ag.join("settings.json"), "{}").unwrap();
    let err = run_self_update(
        Some(&new_exe),
        u32::MAX - 1,
        None,
        None,
        Some(&empty_ag),
        Some(&empty_t3),
    );
    assert!(err.is_err());
    assert!(!empty_ag.join("sidecars").exists());
    assert!(!empty_t3.join("git-widget-sidecar.exe").exists());

    // Setup Antigravity + T3 Code hosts + untouched app.asar
    let ag_dir = root.join("ag_plugin");
    let ag_sidecar_dir = ag_dir.join("sidecars").join("git_widget");
    fs::create_dir_all(&ag_sidecar_dir).unwrap();
    fs::write(ag_sidecar_dir.join("sidecar.json"), "{}").unwrap();
    let ag_exe = ag_sidecar_dir.join("git-widget-sidecar.exe");
    fs::write(&ag_exe, b"MZ-OLD-AG").unwrap();

    let t3_local = root.join("t3_local");
    fs::create_dir_all(&t3_local).unwrap();
    let t3_exe = t3_local.join("git-widget-sidecar.exe");
    fs::write(&t3_exe, b"MZ-OLD-T3").unwrap();
    fs::write(t3_local.join("t3-loader.cjs"), "old-loader").unwrap();

    let t3_app_resources = root.join("t3_app").join("resources");
    fs::create_dir_all(&t3_app_resources).unwrap();
    let asar_path = t3_app_resources.join("app.asar");
    let asar_bytes = b"ASAR-ORIGINAL-UNTOUCHED-BYTES-0123456789";
    fs::write(&asar_path, asar_bytes).unwrap();
    let asar_hash_before = sha256_hex(asar_bytes);

    run_self_update(
        Some(&new_exe),
        u32::MAX - 1,
        None,
        None,
        Some(&ag_dir),
        Some(&t3_local),
    )
    .unwrap();

    assert_eq!(fs::read(&ag_exe).unwrap(), new_bytes);
    assert_eq!(fs::read(&t3_exe).unwrap(), new_bytes);
    assert_eq!(
        fs::read_to_string(t3_local.join("t3-loader.cjs")).unwrap(),
        T3_LOADER_CJS
    );
    assert_eq!(
        fs::read_to_string(t3_local.join("t3-server-hook.cjs")).unwrap(),
        T3_SERVER_HOOK_CJS
    );
    assert_eq!(
        fs::read_to_string(t3_local.join("t3-ui-bridge.js")).unwrap(),
        render_ui_bridge_js()
    );
    assert_eq!(sha256_hex(&fs::read(&asar_path).unwrap()), asar_hash_before);

    // Also check direct rename-before-copy rollback when source is missing
    let missing_src = root.join("does-not-exist.exe");
    assert!(replace_exe_rename_before_copy(&missing_src, &ag_exe).is_err());
    assert_eq!(fs::read(&ag_exe).unwrap(), new_bytes);

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn wait_for_parent_pid_waits_for_real_child_and_handles_exited_pid() {
    // Non-existent / already-exited PID returns Ok immediately
    wait_for_parent_pid(u32::MAX - 1, 500).unwrap();

    // Real short-lived child process (~300ms)
    #[cfg(windows)]
    let mut child = Command::new("powershell")
        .args(["-NoProfile", "-Command", "Start-Sleep -Milliseconds 300"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    #[cfg(not(windows))]
    let mut child = Command::new("sleep")
        .arg("0.3")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    let pid = child.id();
    let started = Instant::now();
    wait_for_parent_pid(pid, 10_000).unwrap();
    let elapsed = started.elapsed();
    let _ = child.wait();
    assert!(
        elapsed.as_millis() >= 150,
        "expected wait_for_parent_pid to wait for child exit, elapsed={elapsed:?}"
    );

    // Also exercise stale temp cleanup on a synthetic gw-update-* directory
    let stale = std::env::temp_dir().join(format!("gw-update-test-{}", std::process::id()));
    fs::create_dir_all(&stale).unwrap();
    fs::write(stale.join("dummy.txt"), "x").unwrap();
    cleanup_stale_update_dirs();
    assert!(!stale.exists());
}
