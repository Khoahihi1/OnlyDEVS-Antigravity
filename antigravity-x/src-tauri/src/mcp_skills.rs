use chrono::Utc;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::paths;
use crate::prompts::open_db;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedMcpServer {
    pub id: String,
    pub enabled: bool,
    pub transport: String, // "http" or "stdio"
    pub summary: String,
    pub url: Option<String>,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub tool_count: usize,
    pub tools_preview: Vec<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedSkill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub source: String, // "user" or "builtin"
    pub path: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpSkillsState {
    pub mcp_config_path: String,
    pub skills_dir: String,
    pub builtin_skills_dir: String,
    pub mcp_servers: Vec<ManagedMcpServer>,
    pub skills: Vec<ManagedSkill>,
}

fn read_mcp_config_value() -> Result<Value, String> {
    let path = paths::mcp_config_json_path()?;
    if !path.exists() {
        return Ok(serde_json::json!({ "mcpServers": {} }));
    }
    let raw = fs::read_to_string(&path).map_err(|e| format!("Đọc mcp_config.json: {e}"))?;
    let trimmed = raw.trim_start_matches('\u{feff}');
    serde_json::from_str(trimmed).map_err(|e| format!("Parse mcp_config.json: {e}"))
}

fn write_mcp_config_value(value: &Value) -> Result<(), String> {
    let path = paths::mcp_config_json_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Tạo config dir: {e}"))?;
    }
    let pretty =
        serde_json::to_string_pretty(value).map_err(|e| format!("Serialize mcp_config: {e}"))?;
    fs::write(&path, pretty).map_err(|e| format!("Ghi mcp_config.json: {e}"))
}

pub fn get_mcp_config_raw() -> Result<String, String> {
    let path = paths::mcp_config_json_path()?;
    if !path.exists() {
        return Ok("{\n  \"mcpServers\": {}\n}".into());
    }
    fs::read_to_string(&path).map_err(|e| format!("Đọc mcp_config.json: {e}"))
}

pub fn save_mcp_config_raw(raw: &str) -> Result<(), String> {
    let trimmed = raw.trim_start_matches('\u{feff}');
    let value: Value =
        serde_json::from_str(trimmed).map_err(|e| format!("JSON không hợp lệ: {e}"))?;
    write_mcp_config_value(&value)
}

fn read_all_notes() -> BTreeMap<(String, String), String> {
    let mut out = BTreeMap::new();
    let Ok(conn) = open_db() else {
        return out;
    };
    let Ok(mut stmt) = conn.prepare("SELECT kind, id, note FROM item_notes") else {
        return out;
    };
    let Ok(rows) = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    }) else {
        return out;
    };
    for (k, id, n) in rows.flatten() {
        if !n.trim().is_empty() {
            out.insert((k, id), n);
        }
    }
    out
}

pub fn save_item_note(kind: &str, id: &str, note: &str) -> Result<(), String> {
    let conn = open_db()?;
    let trimmed = note.trim();
    if trimmed.is_empty() {
        conn.execute(
            "DELETE FROM item_notes WHERE kind = ?1 AND id = ?2",
            params![kind, id],
        )
        .map_err(|e| format!("Delete note: {e}"))?;
    } else {
        conn.execute(
            "INSERT INTO item_notes (kind, id, note) VALUES (?1, ?2, ?3)
             ON CONFLICT(kind, id) DO UPDATE SET note = excluded.note",
            params![kind, id, trimmed],
        )
        .map_err(|e| format!("Save note: {e}"))?;
    }
    Ok(())
}

fn discover_mcp_tools(server_id: &str) -> (usize, Vec<String>) {
    let mut tools = Vec::new();
    for root in [paths::antigravity_dir(), paths::antigravity_cli_dir()] {
        let Ok(base) = root else {
            continue;
        };
        let dir = base.join("mcp").join(server_id);
        if !dir.is_dir() {
            continue;
        }
        if let Ok(entries) = fs::read_dir(&dir) {
            for e in entries.flatten() {
                let p = e.path();
                if p.extension().and_then(|s| s.to_str()) == Some("json") {
                    if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                        let name = stem.to_string();
                        if !tools.contains(&name) {
                            tools.push(name);
                        }
                    }
                }
            }
        }
        if !tools.is_empty() {
            break;
        }
    }
    tools.sort();
    let count = tools.len();
    let preview = tools.into_iter().take(24).collect();
    (count, preview)
}

fn parse_mcp_node(
    id: &str,
    node: &Value,
    enabled: bool,
    notes: &BTreeMap<(String, String), String>,
) -> ManagedMcpServer {
    let url = node
        .get("url")
        .and_then(Value::as_str)
        .map(str::to_string);
    let command = node
        .get("command")
        .and_then(Value::as_str)
        .map(str::to_string);
    let args: Vec<String> = node
        .get("args")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let mut env = BTreeMap::new();
    if let Some(env_obj) = node.get("env").and_then(Value::as_object) {
        for (k, v) in env_obj {
            if let Some(s) = v.as_str() {
                env.insert(k.clone(), s.to_string());
            }
        }
    }

    let transport = if url.is_some()
        || node.get("type").and_then(Value::as_str) == Some("http")
    {
        "http".to_string()
    } else {
        "stdio".to_string()
    };

    let summary = if let Some(u) = &url {
        u.clone()
    } else if let Some(cmd) = &command {
        if args.is_empty() {
            cmd.clone()
        } else {
            format!("{cmd} {}", args.join(" "))
        }
    } else {
        "Custom MCP configuration".into()
    };

    let (tool_count, tools_preview) = discover_mcp_tools(id);
    let note = notes.get(&("mcp".to_string(), id.to_string())).cloned();

    ManagedMcpServer {
        id: id.to_string(),
        enabled,
        transport,
        summary,
        url,
        command,
        args,
        env,
        tool_count,
        tools_preview,
        note,
    }
}

fn parse_skill_frontmatter(raw: &str, fallback_id: &str) -> (String, String) {
    let mut name = fallback_id.to_string();
    let mut description = String::new();
    let trimmed = raw.trim_start_matches('\u{feff}').trim_start();
    if let Some(rest) = trimmed.strip_prefix("---") {
        if let Some(end_idx) = rest.find("\n---") {
            let fm = &rest[..end_idx];
            for line in fm.lines() {
                let l = line.trim();
                if let Some(v) = l.strip_prefix("name:") {
                    let clean = v.trim().trim_matches(|c| c == '"' || c == '\'');
                    if !clean.is_empty() {
                        name = clean.to_string();
                    }
                } else if let Some(v) = l.strip_prefix("description:") {
                    let clean = v.trim().trim_matches(|c| c == '"' || c == '\'');
                    if !clean.is_empty() {
                        description = clean.to_string();
                    }
                }
            }
        }
    }
    if description.is_empty() {
        for line in trimmed.lines() {
            let l = line.trim().trim_start_matches('#').trim();
            if !l.is_empty() && !l.starts_with("---") {
                description = l.chars().take(140).collect();
                break;
            }
        }
    }
    (name, description)
}

fn scan_skills_dir(
    dir: &Path,
    enabled: bool,
    source: &str,
    notes: &BTreeMap<(String, String), String>,
    out: &mut BTreeMap<String, ManagedSkill>,
) {
    if !dir.is_dir() {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let path = e.path();
        if !path.is_dir() {
            continue;
        }
        let id = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        if id.is_empty() || out.contains_key(&id) {
            continue;
        }
        let skill_md = path.join("SKILL.md");
        let raw = fs::read_to_string(&skill_md).unwrap_or_default();
        let (name, description) = parse_skill_frontmatter(&raw, &id);
        let note = notes.get(&("skill".to_string(), id.clone())).cloned();
        out.insert(
            id.clone(),
            ManagedSkill {
                id,
                name,
                description,
                enabled,
                source: source.to_string(),
                path: paths::path_display(&skill_md),
                note,
            },
        );
    }
}

pub fn list_mcp_and_skills() -> Result<McpSkillsState, String> {
    let notes = read_all_notes();
    let mcp_val = read_mcp_config_value()?;
    let mut mcp_map: BTreeMap<String, ManagedMcpServer> = BTreeMap::new();

    if let Some(servers) = mcp_val.get("mcpServers").and_then(Value::as_object) {
        for (id, node) in servers {
            mcp_map.insert(id.clone(), parse_mcp_node(id, node, true, &notes));
        }
    }

    // Load stashed (disabled) MCP servers from SQLite
    if let Ok(conn) = open_db() {
        if let Ok(mut stmt) = conn.prepare("SELECT id, config_json FROM mcp_stash") {
            if let Ok(rows) = stmt.query_map([], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            }) {
                for (id, raw_json) in rows.flatten() {
                    if mcp_map.contains_key(&id) {
                        continue;
                    }
                    if let Ok(node) = serde_json::from_str::<Value>(&raw_json) {
                        mcp_map.insert(id.clone(), parse_mcp_node(&id, &node, false, &notes));
                    }
                }
            }
        }
    }

    let mut skills_map: BTreeMap<String, ManagedSkill> = BTreeMap::new();
    let user_skills = paths::skills_dir()?;
    let disabled_skills = paths::disabled_skills_dir()?;
    let builtin_skills = paths::builtin_skills_dir()?;

    scan_skills_dir(&user_skills, true, "user", &notes, &mut skills_map);
    scan_skills_dir(&disabled_skills, false, "user", &notes, &mut skills_map);
    scan_skills_dir(&builtin_skills, true, "builtin", &notes, &mut skills_map);

    let mut skills: Vec<ManagedSkill> = skills_map.into_values().collect();
    skills.sort_by(|a, b| {
        (a.source != "user")
            .cmp(&(b.source != "user"))
            .then_with(|| a.id.cmp(&b.id))
    });

    Ok(McpSkillsState {
        mcp_config_path: paths::path_display(&paths::mcp_config_json_path()?),
        skills_dir: paths::path_display(&user_skills),
        builtin_skills_dir: paths::path_display(&builtin_skills),
        mcp_servers: mcp_map.into_values().collect(),
        skills,
    })
}

pub fn set_mcp_enabled(id: &str, enabled: bool) -> Result<(), String> {
    let conn = open_db()?;
    let mut root = read_mcp_config_value()?;
    let obj = root
        .as_object_mut()
        .ok_or_else(|| "mcp_config.json không phải object".to_string())?;
    let servers = obj
        .entry("mcpServers")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| "mcpServers không phải object".to_string())?;

    if enabled {
        if !servers.contains_key(id) {
            let raw: String = conn
                .query_row(
                    "SELECT config_json FROM mcp_stash WHERE id = ?1",
                    params![id],
                    |r| r.get(0),
                )
                .map_err(|_| format!("Không tìm thấy cấu hình MCP {id} trong stash"))?;
            let node: Value =
                serde_json::from_str(&raw).map_err(|e| format!("Parse stash: {e}"))?;
            servers.insert(id.to_string(), node);
        }
        let _ = conn.execute("DELETE FROM mcp_stash WHERE id = ?1", params![id]);
    } else if let Some(node) = servers.remove(id) {
        let raw = serde_json::to_string(&node).map_err(|e| format!("Serialize stash: {e}"))?;
        conn.execute(
            "INSERT INTO mcp_stash (id, config_json, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET config_json = excluded.config_json, updated_at = excluded.updated_at",
            params![id, raw, Utc::now().to_rfc3339()],
        )
        .map_err(|e| format!("Save stash: {e}"))?;
    }

    write_mcp_config_value(&root)
}

pub fn upsert_mcp_server(
    id: String,
    transport: String,
    url: Option<String>,
    command: Option<String>,
    args: Vec<String>,
    env: BTreeMap<String, String>,
    enabled: bool,
) -> Result<(), String> {
    let clean_id = id.trim();
    if clean_id.is_empty() {
        return Err("ID MCP server không được để trống".into());
    }

    let node = if transport == "http" {
        serde_json::json!({
            "type": "http",
            "url": url.unwrap_or_default().trim()
        })
    } else {
        let mut map = Map::new();
        map.insert(
            "command".into(),
            Value::String(command.unwrap_or_default().trim().to_string()),
        );
        map.insert(
            "args".into(),
            Value::Array(args.into_iter().map(Value::String).collect()),
        );
        if !env.is_empty() {
            let mut env_map = Map::new();
            for (k, v) in env {
                env_map.insert(k, Value::String(v));
            }
            map.insert("env".into(), Value::Object(env_map));
        }
        Value::Object(map)
    };

    let conn = open_db()?;
    let mut root = read_mcp_config_value()?;
    let servers = root
        .as_object_mut()
        .ok_or_else(|| "mcp_config.json không phải object".to_string())?
        .entry("mcpServers")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| "mcpServers không phải object".to_string())?;

    if enabled {
        servers.insert(clean_id.to_string(), node);
        let _ = conn.execute("DELETE FROM mcp_stash WHERE id = ?1", params![clean_id]);
        write_mcp_config_value(&root)?;
    } else {
        servers.remove(clean_id);
        write_mcp_config_value(&root)?;
        let raw = serde_json::to_string(&node).map_err(|e| format!("Serialize: {e}"))?;
        conn.execute(
            "INSERT INTO mcp_stash (id, config_json, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET config_json = excluded.config_json, updated_at = excluded.updated_at",
            params![clean_id, raw, Utc::now().to_rfc3339()],
        )
        .map_err(|e| format!("Stash MCP: {e}"))?;
    }

    Ok(())
}

pub fn delete_mcp_server(id: &str) -> Result<(), String> {
    if let Ok(conn) = open_db() {
        let _ = conn.execute("DELETE FROM mcp_stash WHERE id = ?1", params![id]);
        let _ = conn.execute(
            "DELETE FROM item_notes WHERE kind = 'mcp' AND id = ?1",
            params![id],
        );
    }
    let mut root = read_mcp_config_value()?;
    if let Some(servers) = root.get_mut("mcpServers").and_then(Value::as_object_mut) {
        if servers.remove(id).is_some() {
            write_mcp_config_value(&root)?;
        }
    }
    Ok(())
}

pub fn set_skill_enabled(id: &str, enabled: bool) -> Result<(), String> {
    let active_dir = paths::skills_dir()?.join(id);
    let disabled_dir = paths::disabled_skills_dir()?.join(id);

    if enabled {
        if disabled_dir.is_dir() {
            if let Some(parent) = active_dir.parent() {
                fs::create_dir_all(parent).map_err(|e| format!("Tạo skills dir: {e}"))?;
            }
            fs::rename(&disabled_dir, &active_dir)
                .map_err(|e| format!("Kích hoạt skill {id}: {e}"))?;
        }
    } else if active_dir.is_dir() {
        if let Some(parent) = disabled_dir.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("Tạo disabled_skills dir: {e}"))?;
        }
        if disabled_dir.exists() {
            let _ = fs::remove_dir_all(&disabled_dir);
        }
        fs::rename(&active_dir, &disabled_dir)
            .map_err(|e| format!("Tắt skill {id}: {e}"))?;
    }
    Ok(())
}

pub fn get_skill_content(id: &str, source: &str) -> Result<String, String> {
    let candidates = if source == "builtin" {
        vec![paths::builtin_skills_dir()?.join(id).join("SKILL.md")]
    } else {
        vec![
            paths::skills_dir()?.join(id).join("SKILL.md"),
            paths::disabled_skills_dir()?.join(id).join("SKILL.md"),
            paths::builtin_skills_dir()?.join(id).join("SKILL.md"),
        ]
    };
    for c in candidates {
        if c.is_file() {
            return fs::read_to_string(&c).map_err(|e| format!("Đọc SKILL.md: {e}"));
        }
    }
    Err(format!("Không tìm thấy SKILL.md cho {id}"))
}

pub fn upsert_skill(
    id: String,
    name: String,
    description: String,
    body: String,
) -> Result<(), String> {
    let clean_id = id
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>();
    if clean_id.is_empty() {
        return Err("ID Skill không hợp lệ".into());
    }

    let skill_dir = paths::skills_dir()?.join(&clean_id);
    fs::create_dir_all(&skill_dir).map_err(|e| format!("Tạo thư mục skill: {e}"))?;

    let full_content = if body.trim_start().starts_with("---") {
        body
    } else {
        format!(
            "---\nname: {}\ndescription: {}\n---\n\n{}",
            if name.trim().is_empty() {
                &clean_id
            } else {
                name.trim()
            },
            description.trim(),
            body.trim()
        )
    };

    fs::write(skill_dir.join("SKILL.md"), full_content)
        .map_err(|e| format!("Ghi SKILL.md: {e}"))
}

pub fn delete_skill(id: &str) -> Result<(), String> {
    for dir in [
        paths::skills_dir()?.join(id),
        paths::disabled_skills_dir()?.join(id),
    ] {
        if dir.is_dir() {
            fs::remove_dir_all(&dir).map_err(|e| format!("Xóa skill {id}: {e}"))?;
        }
    }
    Ok(())
}
