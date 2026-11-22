use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use walkdir::WalkDir;

use crate::paths;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionItem {
    pub id: String,
    pub title: String,
    pub first_user_message: String,
    pub workspace: String,
    pub model: Option<String>,
    pub source: String, // "IDE", "CLI", or "IDE + CLI"
    pub step_count: usize,
    pub user_turn_count: usize,
    pub updated_at_ms: i64,
    pub size_bytes: u64,
    pub archived: bool,
    pub needs_sync: bool,
    pub transcript_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionScanResult {
    pub gemini_home: String,
    pub ide_brain_dir: String,
    pub cli_brain_dir: String,
    pub total_sessions: usize,
    pub ide_count: usize,
    pub cli_count: usize,
    pub synced_count: usize,
    pub unsynced_count: usize,
    pub total_size_bytes: u64,
    pub sessions: Vec<SessionItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStepItem {
    pub step_index: usize,
    pub source: String,
    pub step_type: String,
    pub created_at: Option<String>,
    pub model: Option<String>,
    pub content: String,
    pub tool_count: usize,
}

fn is_uuid_like(name: &str) -> bool {
    name.len() >= 32
        && name.contains('-')
        && name.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

fn extract_pbtxt_title_and_archived(pbtxt_path: &Path) -> (Option<String>, bool) {
    let Ok(raw) = fs::read_to_string(pbtxt_path) else {
        return (None, false);
    };
    let archived = raw.contains("archived:true") || raw.contains("archived: true");
    let title = if let Some(start) = raw.find("title:\"") {
        let rest = &raw[start + 7..];
        let mut out = String::new();
        let mut escaped = false;
        for ch in rest.chars() {
            if escaped {
                out.push(ch);
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                break;
            } else {
                out.push(ch);
            }
        }
        let trimmed = out.trim().to_string();
        (!trimmed.is_empty()).then_some(trimmed)
    } else {
        None
    };
    (title, archived)
}

fn clean_user_request_text(raw: &str) -> String {
    let extracted = if let (Some(s), Some(e)) = (raw.find("<USER_REQUEST>"), raw.find("</USER_REQUEST>")) {
        if e > s + 14 {
            &raw[s + 14..e]
        } else {
            raw
        }
    } else {
        raw
    };
    extracted.trim().to_string()
}

fn extract_workspace_from_text(raw: &str) -> Option<String> {
    if let Some(idx) = raw.find("Command Working Directory:") {
        let line = raw[idx + "Command Working Directory:".len()..]
            .lines()
            .next()
            .unwrap_or_default()
            .trim();
        if !line.is_empty() {
            return Some(line.replace('\\', "/"));
        }
    }
    if let Some(idx) = raw.find("Active Workspaces:") {
        for line in raw[idx + "Active Workspaces:".len()..].lines().take(3) {
            let trimmed = line.trim().trim_start_matches('-').trim();
            if !trimmed.is_empty() {
                return Some(trimmed.replace('\\', "/"));
            }
        }
    }
    for line in raw.lines() {
        for token in line.split_whitespace() {
            let clean = token.trim_matches(|c: char| matches!(c, '"' | '\'' | '`' | ',' | ')' | '('));
            if clean.len() > 6
                && clean.as_bytes().get(1) == Some(&b':')
                && (clean.contains('\\') || clean.contains('/'))
                && !clean.to_lowercase().contains("/.gemini/")
                && !clean.to_lowercase().contains("\\.gemini\\")
            {
                let norm = clean.replace('\\', "/");
                let parts: Vec<&str> = norm.split('/').collect();
                if parts.len() >= 4 {
                    return Some(parts[..parts.len().min(5)].join("/"));
                }
            }
        }
    }
    None
}

fn dir_size_and_mtime(dir: &Path) -> (u64, i64) {
    let mut size = 0u64;
    let mut latest_ms = 0i64;
    for entry in WalkDir::new(dir).max_depth(4).into_iter().flatten() {
        if let Ok(meta) = entry.metadata() {
            if meta.is_file() {
                size += meta.len();
            }
            if let Ok(modified) = meta.modified() {
                if let Ok(dur) = modified.duration_since(UNIX_EPOCH) {
                    let ms = dur.as_millis() as i64;
                    if ms > latest_ms {
                        latest_ms = ms;
                    }
                }
            }
        }
    }
    (size, latest_ms)
}

struct ParsedTranscriptMeta {
    first_user_message: String,
    workspace: Option<String>,
    model: Option<String>,
    step_count: usize,
    user_turn_count: usize,
    latest_ts_ms: Option<i64>,
}

fn parse_transcript_meta(transcript_path: &Path) -> ParsedTranscriptMeta {
    let mut first_user_message = String::new();
    let mut workspace = None;
    let mut model = None;
    let mut step_count = 0usize;
    let mut user_turn_count = 0usize;
    let mut latest_ts_ms = None;

    if let Ok(raw) = fs::read_to_string(transcript_path) {
        for line in raw.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let Ok(val) = serde_json::from_str::<Value>(trimmed) else {
                continue;
            };
            step_count += 1;

            if let Some(ts) = val.get("created_at").and_then(Value::as_str) {
                if let Ok(dt) = DateTime::parse_from_rfc3339(ts) {
                    latest_ts_ms = Some(dt.timestamp_millis());
                }
            }
            if let Some(m) = val.get("model").and_then(Value::as_str) {
                if !m.trim().is_empty() {
                    model = Some(m.to_string());
                }
            }

            let stype = val.get("type").and_then(Value::as_str).unwrap_or_default();
            let content = val.get("content").and_then(Value::as_str).unwrap_or_default();

            if workspace.is_none() && !content.is_empty() {
                workspace = extract_workspace_from_text(content);
            }

            if stype == "USER_INPUT" {
                user_turn_count += 1;
                if first_user_message.is_empty() && !content.is_empty() {
                    first_user_message = clean_user_request_text(content);
                }
            }
        }
    }

    ParsedTranscriptMeta {
        first_user_message,
        workspace,
        model,
        step_count,
        user_turn_count,
        latest_ts_ms,
    }
}

fn summarize_title(pbtxt_title: Option<String>, first_msg: &str, id: &str) -> String {
    if let Some(t) = pbtxt_title {
        let clean = t.trim();
        if !clean.is_empty() {
            return clean.to_string();
        }
    }
    for line in first_msg.lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            let chars: Vec<char> = trimmed.chars().collect();
            if chars.len() > 86 {
                return format!("{}…", chars[..86].iter().collect::<String>());
            }
            return trimmed.to_string();
        }
    }
    format!("Session {}", &id[..id.len().min(8)])
}

pub fn list_sessions() -> Result<SessionScanResult, String> {
    let gemini_home = paths::gemini_home()?;
    let ide_root = paths::antigravity_dir()?;
    let cli_root = paths::antigravity_cli_dir()?;
    let ide_brain = ide_root.join("brain");
    let cli_brain = cli_root.join("brain");

    let mut map: BTreeMap<String, (Option<PathBuf>, Option<PathBuf>)> = BTreeMap::new();

    if ide_brain.is_dir() {
        if let Ok(entries) = fs::read_dir(&ide_brain) {
            for e in entries.flatten() {
                let p = e.path();
                if !p.is_dir() {
                    continue;
                }
                let id = p.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
                if is_uuid_like(&id) {
                    map.entry(id).or_insert((None, None)).0 = Some(p);
                }
            }
        }
    }

    if cli_brain.is_dir() {
        if let Ok(entries) = fs::read_dir(&cli_brain) {
            for e in entries.flatten() {
                let p = e.path();
                if !p.is_dir() {
                    continue;
                }
                let id = p.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
                if is_uuid_like(&id) {
                    map.entry(id).or_insert((None, None)).1 = Some(p);
                }
            }
        }
    }

    let mut sessions = Vec::new();
    let mut ide_count = 0usize;
    let mut cli_count = 0usize;
    let mut synced_count = 0usize;
    let mut unsynced_count = 0usize;
    let mut total_size_bytes = 0u64;

    for (id, (ide_dir, cli_dir)) in map {
        let (source, primary_dir, needs_sync) = match (&ide_dir, &cli_dir) {
            (Some(i), Some(_)) => {
                ide_count += 1;
                cli_count += 1;
                synced_count += 1;
                ("IDE + CLI".to_string(), i.clone(), false)
            }
            (Some(i), None) => {
                ide_count += 1;
                unsynced_count += 1;
                ("IDE".to_string(), i.clone(), true)
            }
            (None, Some(c)) => {
                cli_count += 1;
                unsynced_count += 1;
                ("CLI".to_string(), c.clone(), true)
            }
            (None, None) => continue,
        };

        let pbtxt_ide = ide_root.join("annotations").join(format!("{id}.pbtxt"));
        let pbtxt_cli = cli_root.join("annotations").join(format!("{id}.pbtxt"));
        let (pb_title, archived) = if pbtxt_ide.is_file() {
            extract_pbtxt_title_and_archived(&pbtxt_ide)
        } else if pbtxt_cli.is_file() {
            extract_pbtxt_title_and_archived(&pbtxt_cli)
        } else {
            (None, false)
        };

        let transcript = primary_dir
            .join(".system_generated")
            .join("logs")
            .join("transcript.jsonl");
        let meta = parse_transcript_meta(&transcript);
        let (size_bytes, fs_mtime_ms) = dir_size_and_mtime(&primary_dir);
        total_size_bytes += size_bytes;

        let updated_at_ms = meta.latest_ts_ms.unwrap_or(fs_mtime_ms);
        let title = summarize_title(pb_title, &meta.first_user_message, &id);
        let workspace = meta
            .workspace
            .unwrap_or_else(|| paths::path_display(&primary_dir));

        let preview: String = meta.first_user_message.chars().take(400).collect();

        sessions.push(SessionItem {
            id,
            title,
            first_user_message: preview,
            workspace,
            model: meta.model,
            source,
            step_count: meta.step_count,
            user_turn_count: meta.user_turn_count,
            updated_at_ms,
            size_bytes,
            archived,
            needs_sync,
            transcript_path: paths::path_display(&transcript),
        });
    }

    sessions.sort_by(|a, b| b.updated_at_ms.cmp(&a.updated_at_ms));

    Ok(SessionScanResult {
        gemini_home: paths::path_display(&gemini_home),
        ide_brain_dir: paths::path_display(&ide_brain),
        cli_brain_dir: paths::path_display(&cli_brain),
        total_sessions: sessions.len(),
        ide_count,
        cli_count,
        synced_count,
        unsynced_count,
        total_size_bytes,
        sessions,
    })
}

pub fn get_session_transcript(session_id: &str) -> Result<Vec<SessionStepItem>, String> {
    let ide_transcript = paths::antigravity_dir()?
        .join("brain")
        .join(session_id)
        .join(".system_generated")
        .join("logs")
        .join("transcript.jsonl");
    let cli_transcript = paths::antigravity_cli_dir()?
        .join("brain")
        .join(session_id)
        .join(".system_generated")
        .join("logs")
        .join("transcript.jsonl");

    let target = if ide_transcript.is_file() {
        ide_transcript
    } else if cli_transcript.is_file() {
        cli_transcript
    } else {
        return Ok(Vec::new());
    };

    let raw = fs::read_to_string(&target).map_err(|e| format!("Đọc transcript: {e}"))?;
    let mut items = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(val) = serde_json::from_str::<Value>(trimmed) else {
            continue;
        };
        let step_index = val.get("step_index").and_then(Value::as_u64).unwrap_or(0) as usize;
        let source = val
            .get("source")
            .and_then(Value::as_str)
            .unwrap_or("SYSTEM")
            .to_string();
        let step_type = val
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("STEP")
            .to_string();
        let created_at = val
            .get("created_at")
            .and_then(Value::as_str)
            .map(str::to_string);
        let model = val.get("model").and_then(Value::as_str).map(str::to_string);
        let raw_content = val
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let content = if step_type == "USER_INPUT" {
            clean_user_request_text(raw_content)
        } else {
            raw_content.trim().to_string()
        };
        let tool_count = val
            .get("tool_calls")
            .and_then(Value::as_array)
            .map(|a| a.len())
            .unwrap_or(0);

        if content.is_empty() && tool_count == 0 {
            continue;
        }

        items.push(SessionStepItem {
            step_index,
            source,
            step_type,
            created_at,
            model,
            content,
            tool_count,
        });
    }
    Ok(items)
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(|e| format!("Tạo thư mục {}: {e}", dst.display()))?;
    for entry in WalkDir::new(src).into_iter().flatten() {
        let rel = entry
            .path()
            .strip_prefix(src)
            .map_err(|e| format!("Strip prefix: {e}"))?;
        let target = dst.join(rel);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&target).map_err(|e| format!("Tạo dir: {e}"))?;
        } else if entry.file_type().is_file() {
            if let Some(p) = target.parent() {
                let _ = fs::create_dir_all(p);
            }
            let _ = fs::copy(entry.path(), &target);
        }
    }
    Ok(())
}

pub fn sync_sessions() -> Result<String, String> {
    let ide_root = paths::antigravity_dir()?;
    let cli_root = paths::antigravity_cli_dir()?;
    let ide_brain = ide_root.join("brain");
    let cli_brain = cli_root.join("brain");
    fs::create_dir_all(&ide_brain).map_err(|e| format!("Tạo IDE brain: {e}"))?;
    fs::create_dir_all(&cli_brain).map_err(|e| format!("Tạo CLI brain: {e}"))?;

    let mut copied_to_cli = 0usize;
    let mut copied_to_ide = 0usize;

    if let Ok(entries) = fs::read_dir(&ide_brain) {
        for e in entries.flatten() {
            let p = e.path();
            let id = p.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
            if p.is_dir() && is_uuid_like(&id) {
                let dst = cli_brain.join(&id);
                if !dst.exists() {
                    copy_dir_recursive(&p, &dst)?;
                    copied_to_cli += 1;
                }
            }
        }
    }

    if let Ok(entries) = fs::read_dir(&cli_brain) {
        for e in entries.flatten() {
            let p = e.path();
            let id = p.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
            if p.is_dir() && is_uuid_like(&id) {
                let dst = ide_brain.join(&id);
                if !dst.exists() {
                    copy_dir_recursive(&p, &dst)?;
                    copied_to_ide += 1;
                }
            }
        }
    }

    Ok(format!(
        "Đồng bộ hoàn tất: +{} sang CLI, +{} sang IDE",
        copied_to_cli, copied_to_ide
    ))
}

pub fn delete_sessions(ids: Vec<String>) -> Result<String, String> {
    let ide_root = paths::antigravity_dir()?;
    let cli_root = paths::antigravity_cli_dir()?;
    let mut removed = 0usize;

    for id in ids {
        if !is_uuid_like(&id) {
            continue;
        }
        let mut hit = false;
        for dir in [
            ide_root.join("brain").join(&id),
            cli_root.join("brain").join(&id),
        ] {
            if dir.is_dir() {
                let _ = fs::remove_dir_all(&dir);
                hit = true;
            }
        }
        for ann in [
            ide_root.join("annotations").join(format!("{id}.pbtxt")),
            cli_root.join("annotations").join(format!("{id}.pbtxt")),
        ] {
            if ann.is_file() {
                let _ = fs::remove_file(&ann);
            }
        }
        if hit {
            removed += 1;
        }
    }

    Ok(format!("Đã xóa {removed} session"))
}

pub fn export_sessions_markdown(ids: Vec<String>) -> Result<String, String> {
    let export_dir = paths::exports_dir()?;
    fs::create_dir_all(&export_dir).map_err(|e| format!("Tạo thư mục export: {e}"))?;

    let mut out = String::from("# OnlyDEVS — Antigravity Session Export\n\n");
    out.push_str(&format!("Exported at: {}\n\n---\n\n", Utc::now().to_rfc3339()));

    for id in &ids {
        out.push_str(&format!("## Session `{id}`\n\n"));
        let steps = get_session_transcript(id).unwrap_or_default();
        for s in steps {
            out.push_str(&format!(
                "### Step #{} · {} ({}){}\n\n{}\n\n",
                s.step_index,
                s.step_type,
                s.source,
                s.model.map(|m| format!(" · `{m}`")).unwrap_or_default(),
                if s.content.is_empty() {
                    format!("*({} tool calls)*", s.tool_count)
                } else {
                    s.content
                }
            ));
        }
        out.push_str("---\n\n");
    }

    let filename = format!(
        "sessions-export-{}.md",
        Utc::now().format("%Y%m%d-%H%M%S")
    );
    let file_path = export_dir.join(filename);
    fs::write(&file_path, &out).map_err(|e| format!("Ghi file export: {e}"))?;
    Ok(paths::path_display(&file_path))
}
