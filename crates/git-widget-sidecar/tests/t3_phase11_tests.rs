use git_widget_sidecar::{
    cli::{parse_cli_args, CliMode, InstallTarget},
    t3_asar::{is_boot_patched, patch_app_asar, unpatch_app_asar, HOOK_MARKER, PatchOutcome},
    t3_fuses::{inspect_fuses_bytes, FUSE_SENTINEL},
    t3_installer::is_file_locked,
};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

fn tmp_dir(prefix: &str) -> PathBuf {
    let n = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let dir = std::env::temp_dir().join(format!("{prefix}_{n}"));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    format!("{:x}", h.finalize())
}

/// Builds a minimal valid Chromium Pickle + ASAR archive with `boot.cjs` and `main.cjs`.
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

fn read_asar_file(asar_path: &Path, name: &str) -> (Vec<u8>, String) {
    let raw = fs::read(asar_path).unwrap();
    let header_size = u32::from_le_bytes(raw[4..8].try_into().unwrap()) as usize + 8;
    let json_len = u32::from_le_bytes(raw[12..16].try_into().unwrap()) as usize;
    let header: serde_json::Value = serde_json::from_slice(&raw[16..16 + json_len]).unwrap();
    let entry = &header["files"]["apps"]["files"]["desktop"]["files"]["dist-electron"]["files"][name];
    let offset: usize = entry["offset"].as_str().unwrap().parse().unwrap();
    let size = entry["size"].as_u64().unwrap() as usize;
    let hash = entry["integrity"]["hash"].as_str().unwrap().to_string();
    (raw[header_size + offset..header_size + offset + size].to_vec(), hash)
}

#[test]
fn parses_t3_install_uninstall_ensure_and_serve_cli_flags() {
    let args: Vec<String> = vec![
        "git-widget-sidecar.exe".into(),
        "--install".into(),
        "--target".into(),
        "t3code".into(),
        "--t3-dir".into(),
        r"C:\Custom\t3code".into(),
    ];
    assert_eq!(
        parse_cli_args(&args, false).unwrap(),
        CliMode::Install {
            target: Some(InstallTarget::T3Code),
            t3_dir: Some(PathBuf::from(r"C:\Custom\t3code")),
            explicit_flag: true,
            auto_close_t3: false,
        }
    );

    let ensure: Vec<String> = vec!["bin".into(), "--ensure-t3-patched".into()];
    assert_eq!(
        parse_cli_args(&ensure, false).unwrap(),
        CliMode::EnsureT3Patched {
            t3_dir: None,
            watch_update: false,
        }
    );

    let uninstall: Vec<String> = vec!["bin".into(), "--uninstall-t3".into()];
    assert_eq!(
        parse_cli_args(&uninstall, false).unwrap(),
        CliMode::UninstallT3 { t3_dir: None }
    );

    let serve: Vec<String> = vec![
        "bin".into(),
        "--serve".into(),
        "--port".into(),
        "0".into(),
        "--port-file".into(),
        r"C:\Temp\port.txt".into(),
    ];
    assert_eq!(
        parse_cli_args(&serve, false).unwrap(),
        CliMode::Serve {
            port: 0,
            port_file: Some(PathBuf::from(r"C:\Temp\port.txt")),
        }
    );
}

#[test]
fn inspects_electron_fuses_safely() {
    // Live T3 Code vector: version=1, len=9, fuses="101100011"
    // Index 4 = EnableEmbeddedAsarIntegrityValidation ('0'), Index 5 = OnlyLoadAppFromAsar ('0')
    let mut safe_bin = vec![0u8; 32];
    safe_bin.extend_from_slice(FUSE_SENTINEL);
    safe_bin.push(1); // version
    safe_bin.push(9); // len
    safe_bin.extend_from_slice(b"101100011");

    let fuses = inspect_fuses_bytes(&safe_bin).unwrap();
    assert_eq!(fuses.version, 1);
    assert!(!fuses.enable_embedded_asar_integrity_validation);
    assert!(!fuses.only_load_app_from_asar);

    // Integrity enabled -> flag must be detected as true
    let mut locked_bin = Vec::new();
    locked_bin.extend_from_slice(FUSE_SENTINEL);
    locked_bin.push(1);
    locked_bin.push(9);
    locked_bin.extend_from_slice(b"101110011");
    let locked = inspect_fuses_bytes(&locked_bin).unwrap();
    assert!(locked.enable_embedded_asar_integrity_validation);

    // Missing sentinel -> error
    assert!(inspect_fuses_bytes(b"plain binary without sentinel").is_err());
}

#[test]
fn patches_recomputes_sha256_is_idempotent_and_restores_cleanly() {
    let dir = tmp_dir("gw_asar_test");
    let asar = dir.join("app.asar");
    let server_asar = dir.join("server.asar");
    let orig_boot = b"// src/boot.ts\nrequire(\"./compileCache.cjs\");\nrequire(\"./main.cjs\");\n";
    let orig_main = b"console.log('main-ok');\n";
    let orig_bytes = build_synthetic_asar(orig_boot, orig_main);
    fs::write(&asar, &orig_bytes).unwrap();
    fs::write(&server_asar, b"UNTOUCHED-SERVER-ASAR").unwrap();

    assert!(!is_boot_patched(&asar).unwrap());

    let loader = Path::new(r"C:\Users\Константин\AppData\Local\git-widget\t3-loader.cjs");
    assert_eq!(patch_app_asar(&asar, loader).unwrap(), PatchOutcome::Patched);
    assert!(is_boot_patched(&asar).unwrap());

    // Backup must equal original archive
    let bak = dir.join("app.asar.bak");
    assert_eq!(fs::read(&bak).unwrap(), orig_bytes);

    // Patched boot.cjs has hook marker and valid recomputed SHA256; main.cjs is intact
    let (boot_after, boot_hash) = read_asar_file(&asar, "boot.cjs");
    let boot_str = String::from_utf8(boot_after.clone()).unwrap();
    assert!(boot_str.contains(HOOK_MARKER));
    assert!(boot_str.contains("try"));
    assert_eq!(sha256_hex(&boot_after), boot_hash);

    let (main_after, main_hash) = read_asar_file(&asar, "main.cjs");
    assert_eq!(main_after, orig_main);
    assert_eq!(sha256_hex(&main_after), main_hash);

    // Re-patching must be idempotent and MUST NOT overwrite app.asar.bak
    assert_eq!(patch_app_asar(&asar, loader).unwrap(), PatchOutcome::AlreadyPatched);
    assert_eq!(fs::read(&bak).unwrap(), orig_bytes);

    // Prohibition P1: server.asar untouched
    assert_eq!(fs::read(&server_asar).unwrap(), b"UNTOUCHED-SERVER-ASAR");

    // Unpatch restores original archive and removes .bak
    assert!(unpatch_app_asar(&asar).unwrap());
    assert!(!is_boot_patched(&asar).unwrap());
    assert_eq!(fs::read(&asar).unwrap(), orig_bytes);
    assert!(!unpatch_app_asar(&asar).unwrap());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn detects_lock_when_another_handle_reads_file() {
    let dir = tmp_dir("gw_lock_test");
    let fpath = dir.join("app.asar");
    fs::write(&fpath, b"asar-bytes").unwrap();
    assert!(!is_file_locked(&fpath));

    // Simulate Electron keeping app.asar open for reading (with shared read/write)
    let reader = fs::File::open(&fpath).unwrap();
    assert!(
        is_file_locked(&fpath),
        "is_file_locked must detect open read handles (like Electron app.asar)"
    );
    drop(reader);
    assert!(!is_file_locked(&fpath));

    let _ = fs::remove_dir_all(&dir);
}
