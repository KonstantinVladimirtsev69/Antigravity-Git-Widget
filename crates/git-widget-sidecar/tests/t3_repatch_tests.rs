use git_widget_sidecar::{
    cli::{parse_cli_args, CliMode},
    t3_asar::{is_boot_patched, patch_app_asar, unpatch_app_asar, PatchOutcome},
    t3_fuses::FUSE_SENTINEL,
    t3_scheduler::{build_register_task_script, TASK_NAME},
    t3_watch::{watch_and_repatch_t3_with, WatchConfig, WatchOutcome},
};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

fn tmp_dir(prefix: &str) -> PathBuf {
    let n = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let dir = std::env::temp_dir().join(format!("{prefix}_{}_{n}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    format!("{:x}", h.finalize())
}

fn build_synthetic_asar(boot_content: &[u8], main_content: &[u8]) -> Vec<u8> {
    let boot_hash = sha256_hex(boot_content);
    let main_hash = sha256_hex(main_content);
    let main_offset = boot_content.len().to_string();

    let header_json = serde_json::json!({
        "files": {
            "apps": {
                "files": {
                    "desktop": {
                        "files": {
                            "dist-electron": {
                                "files": {
                                    "boot.cjs": {
                                        "size": boot_content.len(),
                                        "offset": "0",
                                        "integrity": {
                                            "algorithm": "SHA256",
                                            "hash": boot_hash,
                                            "blockSize": 4194304,
                                            "blocks": [boot_hash]
                                        }
                                    },
                                    "main.cjs": {
                                        "size": main_content.len(),
                                        "offset": main_offset,
                                        "integrity": {
                                            "algorithm": "SHA256",
                                            "hash": main_hash,
                                            "blockSize": 4194304,
                                            "blocks": [main_hash]
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    })
    .to_string();

    let json_bytes = header_json.as_bytes();
    let json_len = json_bytes.len() as u32;
    let padded_json_len = (json_len + 3) & !3;
    let header_pickle_payload_size = 4 + padded_json_len;
    let size_pickle_payload_size: u32 = 4;

    let mut out = Vec::new();
    out.extend_from_slice(&size_pickle_payload_size.to_le_bytes());
    out.extend_from_slice(&(header_pickle_payload_size + 4).to_le_bytes());
    out.extend_from_slice(&header_pickle_payload_size.to_le_bytes());
    out.extend_from_slice(&json_len.to_le_bytes());
    out.extend_from_slice(json_bytes);
    out.resize(out.len() + (padded_json_len - json_len) as usize, 0);
    out.extend_from_slice(boot_content);
    out.extend_from_slice(main_content);
    out
}

fn write_fake_t3_exe(dir: &PathBuf, fuses_flags: &[u8; 9]) {
    let mut bin = vec![0u8; 64];
    bin.extend_from_slice(FUSE_SENTINEL);
    bin.push(1);
    bin.push(9);
    bin.extend_from_slice(fuses_flags);
    fs::write(dir.join("T3 Code (Beta).exe"), bin).unwrap();
}

#[test]
fn parses_watch_update_flag_for_ensure_t3_patched() {
    let plain: Vec<String> = vec!["bin".into(), "--ensure-t3-patched".into()];
    assert_eq!(
        parse_cli_args(&plain, false).unwrap(),
        CliMode::EnsureT3Patched {
            t3_dir: None,
            watch_update: false,
        }
    );

    let watch: Vec<String> = vec![
        "bin".into(),
        "--ensure-t3-patched".into(),
        "--watch-update".into(),
        "--t3-dir".into(),
        r"C:\T3".into(),
    ];
    assert_eq!(
        parse_cli_args(&watch, false).unwrap(),
        CliMode::EnsureT3Patched {
            t3_dir: Some(PathBuf::from(r"C:\T3")),
            watch_update: true,
        }
    );
}

#[test]
fn patch_app_asar_refreshes_stale_backup_after_t3_update() {
    let dir = tmp_dir("gw_stale_bak");
    let asar = dir.join("app.asar");
    let loader = dir.join("t3-loader.cjs");
    fs::write(&loader, "// loader").unwrap();

    let v1_bytes = build_synthetic_asar(b"// v1 boot\n", b"v1 main");
    let v2_bytes = build_synthetic_asar(b"// v2 boot updated\n", b"v2 main updated");

    fs::write(&asar, &v1_bytes).unwrap();
    assert_eq!(patch_app_asar(&asar, &loader).unwrap(), PatchOutcome::Patched);
    let bak = dir.join("app.asar.bak");
    assert_eq!(fs::read(&bak).unwrap(), v1_bytes);

    // Simulate T3 Code updater overwriting app.asar with a fresh unpatched v2 archive
    fs::write(&asar, &v2_bytes).unwrap();
    assert!(!is_boot_patched(&asar).unwrap());

    assert_eq!(patch_app_asar(&asar, &loader).unwrap(), PatchOutcome::Patched);
    assert!(is_boot_patched(&asar).unwrap());
    assert_eq!(
        fs::read(&bak).unwrap(),
        v2_bytes,
        "app.asar.bak must be refreshed to the new unpatched T3 version"
    );

    assert!(unpatch_app_asar(&asar).unwrap());
    assert_eq!(fs::read(&asar).unwrap(), v2_bytes);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn scheduler_script_scopes_at_logon_trigger_to_current_user() {
    let vbs = PathBuf::from(r"C:\Users\Константин\AppData\Local\git-widget\ensure-patched.vbs");
    let script = build_register_task_script(TASK_NAME, &vbs);
    assert!(
        script.contains("-AtLogOn -User $env:USERNAME"),
        "AtLogOn trigger must specify -User $env:USERNAME to avoid Access Denied without admin"
    );
    assert!(script.contains("-AllowStartIfOnBatteries"));
    assert!(script.contains("-DontStopIfGoingOnBatteries"));
    assert!(script.contains("ensure-patched.vbs"));
}

#[test]
fn watch_and_repatch_waits_for_unpatched_asar_and_patches_immediately() {
    let root = tmp_dir("gw_watch_repatch");
    let t3_dir = root.join("t3");
    let res_dir = t3_dir.join("resources");
    let install_dir = root.join("install");
    fs::create_dir_all(&res_dir).unwrap();
    fs::create_dir_all(&install_dir).unwrap();
    write_fake_t3_exe(&t3_dir, b"101100011");

    let loader = install_dir.join("t3-loader.cjs");
    fs::write(&loader, "// loader").unwrap();

    let asar = res_dir.join("app.asar");
    let v1_bytes = build_synthetic_asar(b"// v1 boot\n", b"v1");
    let v2_bytes = build_synthetic_asar(b"// v2 boot\n", b"v2");
    fs::write(&asar, &v1_bytes).unwrap();
    patch_app_asar(&asar, &loader).unwrap();
    assert!(is_boot_patched(&asar).unwrap());

    // Background thread simulates T3 Code updater replacing app.asar after 120ms
    let asar_clone = asar.clone();
    let v2_clone = v2_bytes.clone();
    let updater = thread::spawn(move || {
        thread::sleep(Duration::from_millis(120));
        fs::write(&asar_clone, &v2_clone).unwrap();
    });

    let cfg = WatchConfig {
        poll_interval: Duration::from_millis(20),
        update_wait_timeout: Duration::from_secs(3),
        unlock_wait_timeout: Duration::from_secs(2),
        unlock_poll_interval: Duration::from_millis(20),
    };
    let notify_count = Arc::new(AtomicUsize::new(0));
    let outcome = watch_and_repatch_t3_with(
        Some(&t3_dir),
        &install_dir,
        &cfg,
        || {
            notify_count.fetch_add(1, Ordering::SeqCst);
        },
    )
    .unwrap();

    updater.join().unwrap();
    assert_eq!(outcome, WatchOutcome::Repatched);
    assert_eq!(notify_count.load(Ordering::SeqCst), 0);
    assert!(is_boot_patched(&asar).unwrap());
    assert_eq!(fs::read(res_dir.join("app.asar.bak")).unwrap(), v2_bytes);

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn watch_and_repatch_notifies_once_and_patches_when_locked_asar_unlocks() {
    let root = tmp_dir("gw_watch_locked");
    let t3_dir = root.join("t3");
    let res_dir = t3_dir.join("resources");
    let install_dir = root.join("install");
    fs::create_dir_all(&res_dir).unwrap();
    fs::create_dir_all(&install_dir).unwrap();
    write_fake_t3_exe(&t3_dir, b"101100011");

    let asar = res_dir.join("app.asar");
    let v2_bytes = build_synthetic_asar(b"// v2 boot\n", b"v2");
    fs::write(&asar, &v2_bytes).unwrap();

    // Hold open read handle simulating freshly restarted T3 Code locking app.asar
    let handle = fs::File::open(&asar).unwrap();
    let releaser = thread::spawn(move || {
        thread::sleep(Duration::from_millis(200));
        drop(handle);
    });

    let cfg = WatchConfig {
        poll_interval: Duration::from_millis(20),
        update_wait_timeout: Duration::from_secs(1),
        unlock_wait_timeout: Duration::from_secs(3),
        unlock_poll_interval: Duration::from_millis(25),
    };
    let notify_count = Arc::new(AtomicUsize::new(0));
    let outcome = watch_and_repatch_t3_with(
        Some(&t3_dir),
        &install_dir,
        &cfg,
        || {
            notify_count.fetch_add(1, Ordering::SeqCst);
        },
    )
    .unwrap();

    releaser.join().unwrap();
    assert_eq!(outcome, WatchOutcome::RepatchedAfterUnlock);
    assert_eq!(
        notify_count.load(Ordering::SeqCst),
        1,
        "must notify the user at most once when locked by new T3 Code"
    );
    assert!(is_boot_patched(&asar).unwrap());
    assert!(install_dir.join("t3-loader.cjs").exists());

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn watch_and_repatch_stops_immediately_if_fuses_enable_integrity() {
    let root = tmp_dir("gw_watch_unsafe");
    let t3_dir = root.join("t3");
    let res_dir = t3_dir.join("resources");
    let install_dir = root.join("install");
    fs::create_dir_all(&res_dir).unwrap();
    fs::create_dir_all(&install_dir).unwrap();
    write_fake_t3_exe(&t3_dir, b"101110011");

    let asar = res_dir.join("app.asar");
    let v2_bytes = build_synthetic_asar(b"// v2 boot\n", b"v2");
    fs::write(&asar, &v2_bytes).unwrap();

    let cfg = WatchConfig {
        poll_interval: Duration::from_millis(20),
        update_wait_timeout: Duration::from_secs(1),
        unlock_wait_timeout: Duration::from_secs(1),
        unlock_poll_interval: Duration::from_millis(20),
    };
    let notify_count = Arc::new(AtomicUsize::new(0));
    let outcome = watch_and_repatch_t3_with(
        Some(&t3_dir),
        &install_dir,
        &cfg,
        || {
            notify_count.fetch_add(1, Ordering::SeqCst);
        },
    )
    .unwrap();

    assert_eq!(outcome, WatchOutcome::UnsafeFuses);
    assert_eq!(notify_count.load(Ordering::SeqCst), 0);
    assert!(!is_boot_patched(&asar).unwrap());

    let _ = fs::remove_dir_all(&root);
}
