//! Unified diff parser for git diff / git show outputs.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffHunk {
    pub index: usize,
    pub header: String,
    pub lines: Vec<String>,
    pub raw_patch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffFile {
    pub path: String,
    pub status: String,
    pub staged: bool,
    pub additions: usize,
    pub deletions: usize,
    pub hunks: Vec<DiffHunk>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitInfo {
    pub hash: String,
    pub short_hash: String,
    pub subject: String,
    pub author: String,
    pub date_rel: String,
}

pub fn parse_unified_diff(raw: &str, staged: bool) -> Vec<DiffFile> {
    let normalized = raw.replace("\r\n", "\n");
    let mut result = Vec::new();
    let sections: Vec<&str> = normalized.split("\ndiff --git ").collect();

    for (idx, sec) in sections.into_iter().enumerate() {
        let block = if idx == 0 {
            sec.strip_prefix("diff --git ").unwrap_or("")
        } else {
            sec
        };
        if block.trim().is_empty() {
            continue;
        }
        if let Some(df) = parse_file_section(block, staged) {
            result.push(df);
        }
    }
    result
}

fn unquote_git_path(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2 {
        return trimmed[1..trimmed.len() - 1]
            .replace(r"\\", r"\")
            .replace(r#"\""#, "\"");
    }
    trimmed.to_string()
}

fn extract_path(first_line: &str, headers: &[String]) -> String {
    for h in headers {
        if let Some(rest) = h.strip_prefix("rename to ") {
            return unquote_git_path(rest);
        }
        if let Some(rest) = h.strip_prefix("+++ ") {
            let p = rest.trim();
            if p != "/dev/null" {
                let cleaned = p.strip_prefix("b/").unwrap_or(p);
                return unquote_git_path(cleaned);
            }
        }
    }
    for h in headers {
        if let Some(rest) = h.strip_prefix("--- ") {
            let p = rest.trim();
            if p != "/dev/null" {
                let cleaned = p.strip_prefix("a/").unwrap_or(p);
                return unquote_git_path(cleaned);
            }
        }
    }
    if let Some(idx) = first_line.rfind(" b/") {
        return unquote_git_path(&first_line[idx + 3..]);
    }
    unquote_git_path(first_line)
}

fn parse_file_section(block: &str, staged: bool) -> Option<DiffFile> {
    let mut lines_iter = block.lines();
    let first_line = lines_iter.next()?;

    let mut header_lines = vec![format!("diff --git {first_line}")];
    let mut hunks = Vec::new();
    let mut cur_header = String::new();
    let mut cur_lines: Vec<String> = Vec::new();
    let mut additions = 0usize;
    let mut deletions = 0usize;
    let mut in_hunk = false;

    for line in lines_iter {
        if line.starts_with("@@ ") {
            if in_hunk {
                let raw_patch = build_patch(&header_lines, &cur_header, &cur_lines);
                hunks.push(DiffHunk {
                    index: hunks.len(),
                    header: cur_header.clone(),
                    lines: cur_lines.clone(),
                    raw_patch,
                });
                cur_lines.clear();
            }
            in_hunk = true;
            cur_header = line.to_string();
        } else if in_hunk {
            if line.starts_with('+') && !line.starts_with("+++") {
                additions += 1;
            } else if line.starts_with('-') && !line.starts_with("---") {
                deletions += 1;
            }
            cur_lines.push(line.to_string());
        } else {
            header_lines.push(line.to_string());
        }
    }

    if in_hunk {
        let raw_patch = build_patch(&header_lines, &cur_header, &cur_lines);
        hunks.push(DiffHunk {
            index: hunks.len(),
            header: cur_header,
            lines: cur_lines,
            raw_patch,
        });
    }

    let path = extract_path(first_line, &header_lines);
    Some(DiffFile {
        path,
        status: if staged { "staged" } else { "modified" }.to_string(),
        staged,
        additions,
        deletions,
        hunks,
    })
}

fn build_patch(headers: &[String], hunk_header: &str, hunk_lines: &[String]) -> String {
    let mut out = headers.join("\n");
    out.push('\n');
    out.push_str(hunk_header);
    out.push('\n');
    for l in hunk_lines {
        out.push_str(l);
        out.push('\n');
    }
    out
}
