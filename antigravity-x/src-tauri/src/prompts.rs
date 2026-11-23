use chrono::Utc;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::sync::OnceLock;
use std::time::Duration;
use uuid::Uuid;

use crate::paths;

const MANAGED_MARKER_ONLYDEVS: &str = "<!-- OnlyDEVS managed prompts -->";
const MANAGED_MARKER_LEGACY: &str = "<!-- Antigravity-X managed prompts -->";

// Exact prompts synced from OnlyDEVS Prompt library (bundled inside crate)
const BUNDLED_EXAMPLES: &[(&str, &str, &str, &str, &str)] = &[
    (
        "Security / reverse",
        "KhoaHihi",
        "KhoaHihi.MD",
        "You are KhoaHihi — Coding partner to He. Private lab bench, Task obligation, Agent compliance, Execution ladder.",
        include_str!("../../examples/KhoaHihi.MD"),
    ),
    (
        "Security / reverse",
        "Gemini-4-Argon",
        "Gemini-4-Argon.md",
        "Operating Policy — Private single-operator developer environment. Security engineering, dual-use depth, agentic execution.",
        include_str!("../../examples/Gemini-4-Argon.md"),
    ),
    (
        "Security / reverse",
        "Gemini-4-Carbon",
        "Gemini-4-Carbon.md",
        "You are Gemini 4 Carbon — Content guidelines & full-intensity security engineering and practitioner depth.",
        include_str!("../../examples/Gemini-4-Carbon.md"),
    ),
    (
        "Security / reverse",
        "Gemini-Hydrogen",
        "Gemini-Hydrogen.md",
        "You are Gemini 4 Hydrogen — Complete operative content policy, offensive & defensive security engineering, real mechanisms.",
        include_str!("../../examples/Gemini-Hydrogen.md"),
    ),
];

const LEGACY_CODEX_FILENAMES: &[&str] = &[
    "gpt5.5-unrestricted.md",
    "gpt5.4-unrestricted.md",
    "gpt5.5-jeli.md",
    "gpt-5.6-sol-unrestricted.md",
    "海鸥3.0破甲.md",
    "khoahihi-lab-bench.md",
    "software-development-maintainer.md",
    "software-development-code-review.md",
    "software-development-debugging.md",
    "local-maintainer.md",
    "writing-technical-docs.md",
    "writing-clarity-editor.md",
    "writing-structured-draft.md",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedPrompt {
    pub id: String,
    pub category: String,
    pub title: String,
    pub filename: String,
    pub description: String,
    pub content: String,
    pub enabled: bool,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptCategory {
    pub id: String,
    pub name: String,
    pub count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InstructionMode {
    Append,
    Replace,
}

pub fn strip_md_ext(name: &str) -> String {
    let trimmed = name.trim();
    if trimmed.len() > 3 && trimmed[trimmed.len() - 3..].eq_ignore_ascii_case(".md") {
        trimmed[..trimmed.len() - 3].trim().to_string()
    } else {
        trimmed.to_string()
    }
}

static BASE_DB_READY: OnceLock<()> = OnceLock::new();

pub(crate) fn open_db() -> Result<Connection, String> {
    let app_home = paths::app_home()?;
    if BASE_DB_READY.get().is_none() {
        fs::create_dir_all(&app_home).map_err(|e| format!("Tạo app home: {e}"))?;
    }
    let db_path = paths::db_path()?;
    let conn = Connection::open(db_path).map_err(|e| format!("Mở DB: {e}"))?;
    let _ = conn.busy_timeout(Duration::from_millis(5000));
    let _ = conn.execute_batch("PRAGMA synchronous = NORMAL; PRAGMA temp_store = MEMORY;");

    if BASE_DB_READY.get().is_none() {
        let _ = conn.execute_batch("PRAGMA journal_mode = WAL;");
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS prompts (
                id TEXT PRIMARY KEY,
                category TEXT NOT NULL,
                title TEXT NOT NULL,
                filename TEXT NOT NULL,
                content TEXT NOT NULL,
                enabled INTEGER NOT NULL DEFAULT 0,
                updated_at TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT ''
            );
            CREATE TABLE IF NOT EXISTS prompt_categories (
                name TEXT PRIMARY KEY,
                sort_order INTEGER NOT NULL DEFAULT 100
            );
            CREATE TABLE IF NOT EXISTS app_settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS mcp_stash (
                id TEXT PRIMARY KEY,
                config_json TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS item_notes (
                kind TEXT NOT NULL,
                id TEXT NOT NULL,
                note TEXT NOT NULL,
                PRIMARY KEY (kind, id)
            );
            ",
        )
        .map_err(|e| format!("Init schema: {e}"))?;

        let _ = conn.execute(
            "ALTER TABLE prompts ADD COLUMN description TEXT NOT NULL DEFAULT ''",
            [],
        );

        let _ = conn.execute(
            "UPDATE prompts SET title = substr(title, 1, length(title) - 3) WHERE lower(title) LIKE '%.md'",
            [],
        );

        // Purge legacy Codex-X gpt-* example prompts if they were seeded earlier
        for legacy_file in LEGACY_CODEX_FILENAMES {
            let _ = conn.execute(
                "DELETE FROM prompts WHERE lower(filename) = lower(?1)",
                params![legacy_file],
            );
        }

        for (idx, cat) in [
            "Security / reverse",
            "Software development",
            "Writing",
        ]
        .iter()
        .enumerate()
        {
            let _ = conn.execute(
                "INSERT OR IGNORE INTO prompt_categories (name, sort_order) VALUES (?1, ?2)",
                params![cat, (idx as i64) * 10],
            );
        }

        let _ = BASE_DB_READY.set(());
    }

    Ok(conn)
}

fn summarize_markdown(content: &str) -> String {
    let mut lines_collected = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim().trim_start_matches('#').trim();
        if trimmed.is_empty() || trimmed.starts_with("---") || trimmed.starts_with("```") {
            continue;
        }
        lines_collected.push(trimmed);
        if lines_collected.len() >= 2 {
            break;
        }
    }
    if lines_collected.is_empty() {
        return "Markdown instruction prompt cho ~/.gemini/config/AGENTS.md".into();
    }
    let joined = lines_collected.join(" — ");
    let chars: Vec<char> = joined.chars().collect();
    if chars.len() > 125 {
        format!("{}…", chars[..125].iter().collect::<String>())
    } else {
        joined
    }
}

pub fn sync_seed_prompts() -> Result<usize, String> {
    let conn = open_db()?;
    let mut existing_filenames = BTreeSet::new();
    {
        let mut stmt = conn
            .prepare("SELECT filename FROM prompts")
            .map_err(|e| format!("Prepare: {e}"))?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| format!("Query: {e}"))?;
        for r in rows.flatten() {
            existing_filenames.insert(r.to_lowercase());
        }
    }

    let current_agents = get_agents_md().unwrap_or_default();
    let any_enabled: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM prompts WHERE enabled = 1",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let mut added = 0usize;
    let now = Utc::now().to_rfc3339();

    // 1. Seed / sync the 4 core prompts from C:\Users\KhoaHihi\Desktop\KhoaHihi\OnlyDEVS-Antigravity\Prompt
    for (category, raw_title, filename, subtitle, content) in BUNDLED_EXAMPLES {
        let title = strip_md_ext(raw_title);
        if existing_filenames.contains(&filename.to_lowercase()) {
            let _ = conn.execute(
                "UPDATE prompts SET title = ?1, description = ?2, content = ?3 WHERE lower(filename) = lower(?4)",
                params![title, subtitle, content, filename],
            );
            continue;
        }
        let id = Uuid::new_v4().to_string();
        let is_current_match = any_enabled == 0
            && !current_agents.trim().is_empty()
            && current_agents.trim() == content.trim();
        let _ = conn.execute(
            "INSERT INTO prompts (id, category, title, filename, description, content, enabled, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                id,
                category,
                title,
                filename,
                subtitle,
                content,
                if is_current_match { 1 } else { 0 },
                now
            ],
        );
        existing_filenames.insert(filename.to_lowercase());
        added += 1;
    }

    // 2. Also scan C:\Users\KhoaHihi\Desktop\KhoaHihi\OnlyDEVS-Antigravity\Prompt for any new .md files
    for dir in paths::workspace_examples_dirs() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
                continue;
            };
            if !ext.eq_ignore_ascii_case("md") {
                continue;
            }
            let filename = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string();
            if filename.is_empty() {
                continue;
            }
            let Ok(content) = fs::read_to_string(&path) else {
                continue;
            };
            let title = strip_md_ext(&filename);
            let description = summarize_markdown(&content);
            if existing_filenames.contains(&filename.to_lowercase()) {
                continue;
            }
            let id = Uuid::new_v4().to_string();
            let _ = conn.execute(
                "INSERT INTO prompts (id, category, title, filename, description, content, enabled, updated_at)
                 VALUES (?1, 'Security / reverse', ?2, ?3, ?4, ?5, 0, ?6)",
                params![id, title, filename, description, content, now],
            );
            existing_filenames.insert(filename.to_lowercase());
            added += 1;
        }
    }

    Ok(added)
}

fn prompt_display_rank(filename: &str) -> usize {
    for (idx, (_, _, bundled_file, _, _)) in BUNDLED_EXAMPLES.iter().enumerate() {
        if bundled_file.eq_ignore_ascii_case(filename) {
            return idx;
        }
    }
    100
}

pub fn list_prompts() -> Result<Vec<SavedPrompt>, String> {
    let _ = sync_seed_prompts();
    let conn = open_db()?;

    let mut stmt = conn
        .prepare(
            "SELECT id, category, title, filename, content, enabled, updated_at, description
             FROM prompts",
        )
        .map_err(|e| format!("Prepare: {e}"))?;
    let rows = stmt
        .query_map([], |row| {
            let raw_title: String = row.get(2)?;
            let content: String = row.get(4)?;
            let raw_desc: String = row.get(7).unwrap_or_default();
            let description = if raw_desc.trim().is_empty() {
                summarize_markdown(&content)
            } else {
                raw_desc
            };
            Ok(SavedPrompt {
                id: row.get(0)?,
                category: row.get(1)?,
                title: strip_md_ext(&raw_title),
                filename: row.get(3)?,
                content,
                enabled: row.get::<_, i64>(5)? != 0,
                updated_at: row.get(6)?,
                description,
            })
        })
        .map_err(|e| format!("Query: {e}"))?;

    let mut list: Vec<SavedPrompt> = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Row: {e}"))?;

    list.sort_by(|a, b| {
        prompt_display_rank(&a.filename)
            .cmp(&prompt_display_rank(&b.filename))
            .then_with(|| a.title.cmp(&b.title))
    });
    Ok(list)
}

pub fn list_categories() -> Result<Vec<PromptCategory>, String> {
    let conn = open_db()?;
    let prompts = list_prompts()?;
    let mut map = std::collections::BTreeMap::<String, usize>::new();

    if let Ok(mut stmt) =
        conn.prepare("SELECT name FROM prompt_categories ORDER BY sort_order ASC, name ASC")
    {
        if let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) {
            for name in rows.flatten() {
                map.entry(name).or_insert(0);
            }
        }
    }

    for prompt in prompts {
        *map.entry(prompt.category).or_insert(0) += 1;
    }

    let mut out: Vec<PromptCategory> = map
        .into_iter()
        .map(|(name, count)| PromptCategory {
            id: name.clone(),
            name,
            count,
        })
        .collect();

    let priority = |name: &str| match name {
        "Security / reverse" => 0,
        "Software development" => 1,
        "Writing" => 2,
        _ => 10,
    };
    out.sort_by(|a, b| {
        priority(&a.name)
            .cmp(&priority(&b.name))
            .then_with(|| a.name.cmp(&b.name))
    });
    Ok(out)
}

pub fn create_category(name: String) -> Result<Vec<PromptCategory>, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("Tên danh mục không được để trống".into());
    }
    let conn = open_db()?;
    conn.execute(
        "INSERT OR IGNORE INTO prompt_categories (name, sort_order) VALUES (?1, 100)",
        params![trimmed],
    )
    .map_err(|e| format!("Tạo danh mục: {e}"))?;
    list_categories()
}

pub fn delete_category(name: String) -> Result<Vec<PromptCategory>, String> {
    let trimmed = name.trim();
    let conn = open_db()?;
    conn.execute(
        "DELETE FROM prompt_categories WHERE name = ?1",
        params![trimmed],
    )
    .map_err(|e| format!("Xóa danh mục: {e}"))?;
    conn.execute(
        "UPDATE prompts SET category = 'Security / reverse' WHERE category = ?1",
        params![trimmed],
    )
    .map_err(|e| format!("Chuyển prompt: {e}"))?;
    list_categories()
}

pub fn upsert_prompt(
    id: Option<String>,
    category: String,
    title: String,
    filename: String,
    description: Option<String>,
    content: String,
    enabled: bool,
) -> Result<SavedPrompt, String> {
    let conn = open_db()?;
    let id = id.unwrap_or_else(|| Uuid::new_v4().to_string());
    let updated_at = Utc::now().to_rfc3339();
    let cat = if category.trim().is_empty() {
        "Security / reverse".to_string()
    } else {
        category.trim().to_string()
    };
    let clean_title = strip_md_ext(&title);
    let clean_filename = if filename.trim().is_empty() {
        format!("{clean_title}.md")
    } else if filename.trim().to_lowercase().ends_with(".md") {
        filename.trim().to_string()
    } else {
        format!("{}.md", filename.trim())
    };

    let _ = conn.execute(
        "INSERT OR IGNORE INTO prompt_categories (name, sort_order) VALUES (?1, 100)",
        params![cat],
    );
    let desc = description
        .map(|d| d.trim().to_string())
        .filter(|d| !d.is_empty())
        .unwrap_or_else(|| summarize_markdown(&content));

    let mode = get_instruction_mode().unwrap_or(InstructionMode::Replace);
    if enabled && mode == InstructionMode::Replace {
        let _ = conn.execute("UPDATE prompts SET enabled = 0 WHERE id != ?1", params![id]);
    }

    conn.execute(
        "INSERT INTO prompts (id, category, title, filename, description, content, enabled, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(id) DO UPDATE SET
           category=excluded.category,
           title=excluded.title,
           filename=excluded.filename,
           description=excluded.description,
           content=excluded.content,
           enabled=excluded.enabled,
           updated_at=excluded.updated_at",
        params![
            id,
            cat,
            clean_title,
            clean_filename,
            desc,
            content,
            if enabled { 1 } else { 0 },
            updated_at
        ],
    )
    .map_err(|e| format!("Upsert prompt: {e}"))?;

    if enabled {
        let _ = sync_enabled_prompts_to_agents(mode);
    }

    Ok(SavedPrompt {
        id,
        category: cat,
        title: clean_title,
        filename: clean_filename,
        description: desc,
        content,
        enabled,
        updated_at,
    })
}

pub fn delete_prompt(id: &str) -> Result<(), String> {
    let conn = open_db()?;
    conn.execute("DELETE FROM prompts WHERE id = ?1", params![id])
        .map_err(|e| format!("Delete prompt: {e}"))?;
    let mode = get_instruction_mode().unwrap_or(InstructionMode::Replace);
    let _ = sync_enabled_prompts_to_agents(mode);
    Ok(())
}

pub fn set_prompt_enabled(id: &str, enabled: bool) -> Result<(), String> {
    let conn = open_db()?;
    let mode = get_instruction_mode().unwrap_or(InstructionMode::Replace);

    if enabled && mode == InstructionMode::Replace {
        let _ = conn.execute("UPDATE prompts SET enabled = 0 WHERE id != ?1", params![id]);
    }

    conn.execute(
        "UPDATE prompts SET enabled = ?1, updated_at = ?2 WHERE id = ?3",
        params![if enabled { 1 } else { 0 }, Utc::now().to_rfc3339(), id],
    )
    .map_err(|e| format!("Toggle prompt: {e}"))?;

    sync_enabled_prompts_to_agents(mode)?;
    Ok(())
}

pub fn get_agents_md() -> Result<String, String> {
    let path = paths::agents_md_path()?;
    if !path.exists() {
        return Ok(String::new());
    }
    fs::read_to_string(&path).map_err(|e| format!("Đọc AGENTS.md: {e}"))
}

pub fn save_agents_md(content: &str) -> Result<(), String> {
    let path = paths::agents_md_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Tạo config dir: {e}"))?;
    }
    fs::write(&path, content).map_err(|e| format!("Ghi AGENTS.md: {e}"))
}

fn strip_managed_section(existing: &str) -> String {
    if let Some(idx) = existing.find(MANAGED_MARKER_ONLYDEVS) {
        return existing[..idx].trim_end().to_string();
    }
    if let Some(idx) = existing.find(MANAGED_MARKER_LEGACY) {
        return existing[..idx].trim_end().to_string();
    }
    existing.trim_end().to_string()
}

pub fn sync_enabled_prompts_to_agents(mode: InstructionMode) -> Result<String, String> {
    let prompts: Vec<SavedPrompt> = list_prompts()?.into_iter().filter(|p| p.enabled).collect();
    let path = paths::agents_md_path()?;
    let existing = if path.exists() {
        get_agents_md()?
    } else {
        String::new()
    };

    if prompts.is_empty() {
        if mode == InstructionMode::Append {
            let base = strip_managed_section(&existing);
            if base.len() != existing.trim_end().len() {
                save_agents_md(&base)?;
            }
        }
        return Ok("Đã tắt prompt quản lý".into());
    }

    let next = match mode {
        InstructionMode::Replace => {
            if prompts.len() == 1 {
                prompts[0].content.clone()
            } else {
                prompts
                    .iter()
                    .map(|p| p.content.trim().to_string())
                    .collect::<Vec<_>>()
                    .join("\n\n---\n\n")
            }
        }
        InstructionMode::Append => {
            let blocks: Vec<String> = prompts
                .iter()
                .map(|p| format!("## {}\n\n{}", p.title, p.content.trim()))
                .collect();
            let injection = format!("{MANAGED_MARKER_ONLYDEVS}\n{}", blocks.join("\n\n"));
            let base = strip_managed_section(&existing);
            if base.is_empty() {
                injection
            } else {
                format!("{base}\n\n{injection}")
            }
        }
    };

    save_agents_md(&next)?;
    Ok(format!(
        "Đã ghi nội dung vào ~/.gemini/config/AGENTS.md ({})",
        prompts
            .iter()
            .map(|p| p.title.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

pub fn apply_prompts_to_agents(mode: InstructionMode) -> Result<String, String> {
    sync_enabled_prompts_to_agents(mode)
}

pub fn get_instruction_mode() -> Result<InstructionMode, String> {
    let conn = open_db()?;
    let mut stmt = conn
        .prepare("SELECT value FROM app_settings WHERE key = 'instructionMode'")
        .map_err(|e| format!("Prepare: {e}"))?;
    let mode = stmt
        .query_row([], |row| row.get::<_, String>(0))
        .unwrap_or_else(|_| "replace".into());
    match mode.as_str() {
        "append" => Ok(InstructionMode::Append),
        _ => Ok(InstructionMode::Replace),
    }
}

pub fn set_instruction_mode(mode: InstructionMode) -> Result<(), String> {
    let conn = open_db()?;
    let value = match mode {
        InstructionMode::Append => "append",
        InstructionMode::Replace => "replace",
    };
    conn.execute(
        "INSERT INTO app_settings (key, value) VALUES ('instructionMode', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![value],
    )
    .map_err(|e| format!("Save mode: {e}"))?;
    let _ = sync_enabled_prompts_to_agents(mode);
    Ok(())
}
