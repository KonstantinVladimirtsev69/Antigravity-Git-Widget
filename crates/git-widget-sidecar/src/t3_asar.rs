//! Native Rust ASAR reader and atomic `boot.cjs` patcher with SHA256 integrity recalculation.

use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

pub const HOOK_MARKER: &str = "GIT_WIDGET_T3_HOOK";
pub const BOOT_REL_PATH: &[&str] = &["apps", "desktop", "dist-electron", "boot.cjs"];
const DEFAULT_BLOCK_SIZE: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatchOutcome {
    Patched,
    AlreadyPatched,
}

struct AsarHeader {
    header_size: u64,
    json: Value,
}

fn read_asar_header(file: &mut File) -> Result<AsarHeader, String> {
    let mut prefix = [0u8; 16];
    file.seek(SeekFrom::Start(0))
        .map_err(|e| format!("ASAR seek failed: {e}"))?;
    file.read_exact(&mut prefix)
        .map_err(|e| format!("Invalid ASAR prefix: {e}"))?;
    let header_size = u32::from_le_bytes(prefix[4..8].try_into().unwrap()) as u64 + 8;
    let json_len = u32::from_le_bytes(prefix[12..16].try_into().unwrap()) as usize;
    let mut json_buf = vec![0u8; json_len];
    file.read_exact(&mut json_buf)
        .map_err(|e| format!("Invalid ASAR JSON header: {e}"))?;
    let json: Value = serde_json::from_slice(&json_buf)
        .map_err(|e| format!("Corrupt ASAR JSON header: {e}"))?;
    Ok(AsarHeader { header_size, json })
}

fn get_entry<'a>(root: &'a Value, path: &[&str]) -> Result<&'a Value, String> {
    let mut cur = root;
    for seg in path {
        cur = cur
            .get("files")
            .and_then(|f| f.get(*seg))
            .ok_or_else(|| format!("Entry '{}' not found in ASAR", path.join("/")))?;
    }
    Ok(cur)
}

fn get_entry_mut<'a>(root: &'a mut Value, path: &[&str]) -> Result<&'a mut Value, String> {
    let mut cur = root;
    for seg in path {
        cur = cur
            .get_mut("files")
            .and_then(|f| f.get_mut(*seg))
            .ok_or_else(|| format!("Entry '{}' not found in ASAR", path.join("/")))?;
    }
    Ok(cur)
}

fn read_boot_bytes(file: &mut File, hdr: &AsarHeader) -> Result<Vec<u8>, String> {
    let entry = get_entry(&hdr.json, BOOT_REL_PATH)?;
    let offset = entry
        .get("offset")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<u64>().ok())
        .ok_or_else(|| "Missing boot.cjs offset in ASAR".to_string())?;
    let size = entry
        .get("size")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| "Missing boot.cjs size in ASAR".to_string())? as usize;
    let mut buf = vec![0u8; size];
    file.seek(SeekFrom::Start(hdr.header_size + offset))
        .map_err(|e| format!("ASAR boot.cjs seek failed: {e}"))?;
    file.read_exact(&mut buf)
        .map_err(|e| format!("ASAR boot.cjs read failed: {e}"))?;
    Ok(buf)
}

fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    format!("{:x}", h.finalize())
}

fn compute_integrity(data: &[u8], block_size: usize) -> Value {
    let bs = if block_size == 0 {
        DEFAULT_BLOCK_SIZE
    } else {
        block_size
    };
    let blocks: Vec<String> = if data.is_empty() {
        vec![sha256_hex(&[])]
    } else {
        data.chunks(bs).map(sha256_hex).collect()
    };
    serde_json::json!({
        "algorithm": "SHA256",
        "hash": sha256_hex(data),
        "blockSize": bs,
        "blocks": blocks,
    })
}

fn encode_pickle_header(json: &Value) -> Result<Vec<u8>, String> {
    let json_bytes = serde_json::to_vec(json).map_err(|e| format!("JSON encode error: {e}"))?;
    let json_len = json_bytes.len() as u32;
    let padded_json_len = (json_len + 3) & !3;
    let header_pickle_payload_size = 4 + padded_json_len;
    let size_pickle_payload_size: u32 = 4;

    let mut out = Vec::with_capacity(16 + padded_json_len as usize);
    out.extend_from_slice(&size_pickle_payload_size.to_le_bytes());
    out.extend_from_slice(&(header_pickle_payload_size + 4).to_le_bytes());
    out.extend_from_slice(&header_pickle_payload_size.to_le_bytes());
    out.extend_from_slice(&json_len.to_le_bytes());
    out.extend_from_slice(&json_bytes);
    out.resize(out.len() + (padded_json_len - json_len) as usize, 0);
    Ok(out)
}

fn atomic_replace(tmp_path: &Path, target: &Path) -> Result<(), String> {
    if fs::rename(tmp_path, target).is_ok() {
        return Ok(());
    }
    // Windows fallback when destination already exists
    let swap = target.with_extension(format!("asar.old.{}", std::process::id()));
    let _ = fs::remove_file(&swap);
    fs::rename(target, &swap).map_err(|e| format!("Failed staging old ASAR: {e}"))?;
    match fs::rename(tmp_path, target) {
        Ok(()) => {
            let _ = fs::remove_file(&swap);
            Ok(())
        }
        Err(e) => {
            let _ = fs::rename(&swap, target);
            Err(format!("Failed replacing ASAR atomically: {e}"))
        }
    }
}

pub fn backup_path_for(asar_path: &Path) -> PathBuf {
    let mut os = asar_path.as_os_str().to_os_string();
    os.push(".bak");
    PathBuf::from(os)
}

pub fn is_boot_patched(asar_path: &Path) -> Result<bool, String> {
    let mut file = File::open(asar_path)
        .map_err(|e| format!("Failed opening {}: {e}", asar_path.display()))?;
    let hdr = read_asar_header(&mut file)?;
    let boot = read_boot_bytes(&mut file, &hdr)?;
    let text = String::from_utf8_lossy(&boot);
    Ok(text.contains(HOOK_MARKER))
}

fn write_patched_asar(asar_path: &Path, new_boot: &[u8]) -> Result<(), String> {
    let mut src = File::open(asar_path)
        .map_err(|e| format!("Failed opening {}: {e}", asar_path.display()))?;
    let file_len = src
        .metadata()
        .map_err(|e| format!("ASAR metadata error: {e}"))?
        .len();
    let mut hdr = read_asar_header(&mut src)?;
    let payload_len = file_len
        .checked_sub(hdr.header_size)
        .ok_or_else(|| "Corrupt ASAR size".to_string())?;

    let entry = get_entry_mut(&mut hdr.json, BOOT_REL_PATH)?;
    let block_size = entry
        .get("integrity")
        .and_then(|i| i.get("blockSize"))
        .and_then(|v| v.as_u64())
        .unwrap_or(DEFAULT_BLOCK_SIZE as u64) as usize;
    entry["offset"] = Value::String(payload_len.to_string());
    entry["size"] = Value::Number((new_boot.len() as u64).into());
    entry["integrity"] = compute_integrity(new_boot, block_size);

    let new_header_bytes = encode_pickle_header(&hdr.json)?;
    let tmp_path = asar_path.with_extension(format!("asar.tmp.{}", std::process::id()));
    let mut dst = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&tmp_path)
        .map_err(|e| format!("Failed creating temp ASAR: {e}"))?;

    dst.write_all(&new_header_bytes)
        .map_err(|e| format!("Failed writing ASAR header: {e}"))?;
    src.seek(SeekFrom::Start(hdr.header_size))
        .map_err(|e| format!("Failed seeking ASAR payload: {e}"))?;
    std::io::copy(&mut src, &mut dst)
        .map_err(|e| format!("Failed copying ASAR payload: {e}"))?;
    dst.write_all(new_boot)
        .map_err(|e| format!("Failed appending boot.cjs: {e}"))?;
    dst.sync_all()
        .map_err(|e| format!("Failed syncing temp ASAR: {e}"))?;
    drop(dst);
    drop(src);

    atomic_replace(&tmp_path, asar_path)
}

pub fn patch_app_asar(asar_path: &Path, loader_path: &Path) -> Result<PatchOutcome, String> {
    if is_boot_patched(asar_path)? {
        return Ok(PatchOutcome::AlreadyPatched);
    }
    let bak = backup_path_for(asar_path);
    if !bak.exists() {
        fs::copy(asar_path, &bak)
            .map_err(|e| format!("Failed creating backup {}: {e}", bak.display()))?;
    }

    let mut file = File::open(asar_path)
        .map_err(|e| format!("Failed opening {}: {e}", asar_path.display()))?;
    let hdr = read_asar_header(&mut file)?;
    let orig_boot = read_boot_bytes(&mut file, &hdr)?;
    drop(file);

    let orig_str = String::from_utf8_lossy(&orig_boot);
    let loader_lit = serde_json::to_string(&loader_path.to_string_lossy().to_string())
        .map_err(|e| format!("Failed encoding loader path: {e}"))?;
    let hook_line = format!(
        "/* {HOOK_MARKER} */ try {{ require({loader_lit}); }} catch (_) {{}}\n"
    );
    let patched_str = format!("{hook_line}{orig_str}");
    write_patched_asar(asar_path, patched_str.as_bytes())?;
    Ok(PatchOutcome::Patched)
}

pub fn unpatch_app_asar(asar_path: &Path) -> Result<bool, String> {
    let bak = backup_path_for(asar_path);
    if bak.exists() {
        let tmp = asar_path.with_extension(format!("asar.restore.{}", std::process::id()));
        fs::copy(&bak, &tmp).map_err(|e| format!("Failed staging backup restore: {e}"))?;
        atomic_replace(&tmp, asar_path)?;
        let _ = fs::remove_file(&bak);
        return Ok(true);
    }
    if !asar_path.exists() || !is_boot_patched(asar_path)? {
        return Ok(false);
    }
    let mut file = File::open(asar_path)
        .map_err(|e| format!("Failed opening {}: {e}", asar_path.display()))?;
    let hdr = read_asar_header(&mut file)?;
    let boot = read_boot_bytes(&mut file, &hdr)?;
    drop(file);
    let cleaned: String = String::from_utf8_lossy(&boot)
        .lines()
        .filter(|line| !line.contains(HOOK_MARKER))
        .map(|line| format!("{line}\n"))
        .collect();
    write_patched_asar(asar_path, cleaned.as_bytes())?;
    Ok(true)
}
