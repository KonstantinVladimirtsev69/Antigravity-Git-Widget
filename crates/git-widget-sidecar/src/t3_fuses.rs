//! Electron Fuses v1 sentinel inspection for safe ASAR patching.

use std::{
    fs::File,
    io::{BufReader, Read},
    path::Path,
};

pub const FUSE_SENTINEL: &[u8] = b"dL7pKGdnNz796PbbjQWNKmHXBZaB9tsX";

// Electron FuseV1Options indices:
// 0: RunAsNode, 1: EnableCookieEncryption, 2: EnableNodeOptionsEnvironmentVariable,
// 3: EnableNodeCliInspectArguments, 4: EnableEmbeddedAsarIntegrityValidation,
// 5: OnlyLoadAppFromAsar
const IDX_ENABLE_EMBEDDED_ASAR_INTEGRITY: usize = 4;
const IDX_ONLY_LOAD_APP_FROM_ASAR: usize = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElectronFuses {
    pub version: u8,
    pub enable_embedded_asar_integrity_validation: bool,
    pub only_load_app_from_asar: bool,
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

pub fn inspect_fuses_bytes(bytes: &[u8]) -> Result<ElectronFuses, String> {
    let pos = find_subslice(bytes, FUSE_SENTINEL)
        .ok_or_else(|| "Electron Fuses sentinel not found in executable".to_string())?;
    let meta_start = pos + FUSE_SENTINEL.len();
    if bytes.len() < meta_start + 2 {
        return Err("Truncated Electron Fuses header".to_string());
    }
    let version = bytes[meta_start];
    let len = bytes[meta_start + 1] as usize;
    let flags_start = meta_start + 2;
    if bytes.len() < flags_start + len {
        return Err("Truncated Electron Fuses payload".to_string());
    }
    let flags = &bytes[flags_start..flags_start + len];
    let integrity_enabled = flags
        .get(IDX_ENABLE_EMBEDDED_ASAR_INTEGRITY)
        .map(|b| *b == b'1')
        .unwrap_or(false);
    let only_asar = flags
        .get(IDX_ONLY_LOAD_APP_FROM_ASAR)
        .map(|b| *b == b'1')
        .unwrap_or(false);

    Ok(ElectronFuses {
        version,
        enable_embedded_asar_integrity_validation: integrity_enabled,
        only_load_app_from_asar: only_asar,
    })
}

pub fn inspect_fuses_file(exe_path: &Path) -> Result<ElectronFuses, String> {
    let file = File::open(exe_path)
        .map_err(|e| format!("Failed to open {}: {e}", exe_path.display()))?;
    let mut reader = BufReader::with_capacity(4 * 1024 * 1024, file);
    let chunk_size = 4 * 1024 * 1024;
    let overlap = FUSE_SENTINEL.len() + 64;
    let mut buf = vec![0u8; chunk_size + overlap];
    let mut carry = 0usize;

    loop {
        let read_n = reader
            .read(&mut buf[carry..carry + chunk_size])
            .map_err(|e| format!("Failed reading {}: {e}", exe_path.display()))?;
        if read_n == 0 {
            break;
        }
        let total = carry + read_n;
        if let Ok(fuses) = inspect_fuses_bytes(&buf[..total]) {
            return Ok(fuses);
        }
        if total > overlap {
            buf.copy_within(total - overlap..total, 0);
            carry = overlap;
        } else {
            carry = total;
        }
    }
    Err(format!(
        "Electron Fuses sentinel not found in {}",
        exe_path.display()
    ))
}

pub fn verify_safe_to_patch(exe_path: &Path) -> Result<ElectronFuses, String> {
    let fuses = inspect_fuses_file(exe_path)?;
    if fuses.enable_embedded_asar_integrity_validation {
        return Err(
            "T3 Code has EnableEmbeddedAsarIntegrityValidation=1; skipping ASAR patch to prevent launch failure."
                .to_string(),
        );
    }
    Ok(fuses)
}
