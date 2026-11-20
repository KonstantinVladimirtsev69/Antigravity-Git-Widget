//! Parsers for `git status --porcelain=v2` and `git for-each-ref` tracking info.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct BranchTracking {
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub gone: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileChange {
    pub path: String,
    pub code: String,
    pub is_conflicted: bool,
}

pub fn parse_porcelain_entry(line: &str) -> Option<FileChange> {
    if let Some(rest) = line.strip_prefix("u ") {
        let mut parts = rest.splitn(10, ' ');
        let xy = parts.next()?.to_string();
        let path = parts.nth(8)?.to_string();
        return Some(FileChange { path, code: xy, is_conflicted: true });
    }
    if let Some(rest) = line.strip_prefix("1 ") {
        let mut parts = rest.splitn(8, ' ');
        let xy = parts.next()?.to_string();
        let path = parts.nth(6)?.to_string();
        return Some(FileChange { path, code: xy, is_conflicted: false });
    }
    if let Some(rest) = line.strip_prefix("2 ") {
        let mut parts = rest.splitn(9, ' ');
        let xy = parts.next()?.to_string();
        let path_tab = parts.nth(7)?;
        let path = path_tab.split('\t').next().unwrap_or(path_tab).to_string();
        return Some(FileChange { path, code: xy, is_conflicted: false });
    }
    line.strip_prefix("? ").map(|path| FileChange {
        path: path.to_string(),
        code: "??".to_string(),
        is_conflicted: false,
    })
}

pub fn parse_track_counts(track: &str) -> (u32, u32, bool) {
    let mut ahead = 0u32;
    let mut behind = 0u32;
    let mut gone = false;
    for seg in track.split(',') {
        let s = seg.trim();
        if s == "gone" {
            gone = true;
        } else if let Some(n) = s.strip_prefix("ahead ") {
            ahead = n.trim().parse().unwrap_or(0);
        } else if let Some(n) = s.strip_prefix("behind ") {
            behind = n.trim().parse().unwrap_or(0);
        }
    }
    (ahead, behind, gone)
}
