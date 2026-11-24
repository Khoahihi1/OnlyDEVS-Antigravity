use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::paths;
use crate::plugins;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginEntry {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub enabled: bool,
    pub installed: bool,
    pub source: String,
    pub install_path: Option<String>,
    pub version: Option<String>,
    pub hooks: Vec<String>,
    pub has_self_test: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverviewState {
    pub gemini_home: String,
    pub config_dir: String,
    pub config_exists: bool,
    pub agents_exists: bool,
    pub agents_path: String,
    pub config_path: String,
    pub mcp_config_path: String,
    pub mcp_config_exists: bool,
    pub plugin_count: usize,
    pub plugins_enabled: usize,
    pub antigravity_cli_dir: String,
    pub antigravity_ide_dir: String,
    pub skills_dir: String,
    pub bundled_plugin_source: String,
    pub cli_version: Option<String>,
    pub python_path: Option<String>,
}

pub(crate) fn read_config_value() -> Result<Value, String> {
    let path = paths::config_json_path()?;
    if !path.exists() {
        return Ok(Value::Object(Map::new()));
    }
    let raw = fs::read_to_string(&path).map_err(|e| format!("Đọc config.json: {e}"))?;
    let trimmed = raw.trim_start_matches('\u{feff}').trim();
    if trimmed.is_empty() {
        return Ok(Value::Object(Map::new()));
    }
    serde_json::from_str(trimmed).map_err(|e| format!("Parse config.json: {e}"))
}

pub(crate) fn write_config_value(value: &Value) -> Result<(), String> {
    let path = paths::config_json_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Tạo config dir: {e}"))?;
    }
    let pretty =
        serde_json::to_string_pretty(value).map_err(|e| format!("Serialize config: {e}"))?;
    fs::write(&path, pretty).map_err(|e| format!("Ghi config.json: {e}"))
}

pub fn get_config_raw() -> Result<String, String> {
    let path = paths::config_json_path()?;
    if !path.exists() {
        return Ok("{}".into());
    }
    fs::read_to_string(&path).map_err(|e| format!("Đọc config: {e}"))
}

pub fn save_config_raw(raw: &str) -> Result<(), String> {
    let value: Value =
        serde_json::from_str(raw).map_err(|e| format!("JSON không hợp lệ: {e}"))?;
    write_config_value(&value)
}

struct PluginManifestMeta {
    name: Option<String>,
    description: Option<String>,
    version: Option<String>,
    hooks: Vec<String>,
    has_self_test: bool,
}

fn inspect_plugin_dir(dir: &Path, id: &str) -> PluginManifestMeta {
    let mut name = None;
    let mut description = None;
    let mut version = None;

    let plugin_json = dir.join("plugin.json");
    if plugin_json.exists() {
        if let Ok(raw) = fs::read_to_string(&plugin_json) {
            if let Ok(val) = serde_json::from_str::<Value>(&raw) {
                name = val.get("name").and_then(Value::as_str).map(str::to_string);
                description = val
                    .get("description")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                version = val
                    .get("version")
                    .and_then(Value::as_str)
                    .map(str::to_string);
            }
        }
    }

    let mut hooks = Vec::new();
    let hooks_json = dir.join("hooks.json");
    if hooks_json.exists() {
        if let Ok(raw) = fs::read_to_string(&hooks_json) {
            if let Ok(val) = serde_json::from_str::<Value>(&raw) {
                if let Some(obj) = val
                    .get(id)
                    .and_then(Value::as_object)
                    .or_else(|| val.as_object())
                {
                    for key in obj.keys() {
                        if matches!(
                            key.as_str(),
                            "SessionStart" | "PreInvocation" | "PostInvocation" | "Stop"
                        ) {
                            hooks.push(key.clone());
                        }
                    }
                }
            }
        }
    }

    let has_self_test = dir.join("test_filter_bypass.py").is_file();

    PluginManifestMeta {
        name,
        description,
        version,
        hooks,
        has_self_test,
    }
}

pub fn list_plugins() -> Result<Vec<PluginEntry>, String> {
    let config = read_config_value()?;
    let plugins_dir = paths::plugins_dir()?;
    let builtin_dir = paths::builtin_plugins_dir()?;
    let mut entries: BTreeMap<String, PluginEntry> = BTreeMap::new();

    if let Some(plugins_map) = config.get("plugins").and_then(Value::as_object) {
        for (id, node) in plugins_map {
            let enabled = node.get("enabled").and_then(Value::as_bool).unwrap_or(false);
            let user_path = plugins_dir.join(id);
            let builtin_path = builtin_dir.join(id);
            let (installed, active_dir, source) = if user_path.is_dir() {
                (
                    true,
                    Some(user_path),
                    if id == "onlydevs-filter-bypass" {
                        "bundled"
                    } else {
                        "user"
                    },
                )
            } else if builtin_path.is_dir() {
                (true, Some(builtin_path), "builtin")
            } else {
                (false, None, "config")
            };

            let meta = active_dir
                .as_deref()
                .map(|d| inspect_plugin_dir(d, id))
                .unwrap_or(PluginManifestMeta {
                    name: None,
                    description: None,
                    version: None,
                    hooks: Vec::new(),
                    has_self_test: false,
                });

            entries.insert(
                id.clone(),
                PluginEntry {
                    id: id.clone(),
                    name: meta.name.unwrap_or_else(|| id.clone()),
                    description: meta.description,
                    enabled,
                    installed,
                    source: source.into(),
                    install_path: active_dir.as_deref().map(paths::path_display),
                    version: meta.version,
                    hooks: meta.hooks,
                    has_self_test: meta.has_self_test,
                },
            );
        }
    }

    if plugins_dir.exists() {
        for entry in fs::read_dir(&plugins_dir).map_err(|e| format!("Đọc plugins dir: {e}"))? {
            let entry = entry.map_err(|e| format!("Dir entry: {e}"))?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let id = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string();
            if id.is_empty() {
                continue;
            }
            let meta = inspect_plugin_dir(&path, &id);
            entries.entry(id.clone()).or_insert_with(|| PluginEntry {
                id: id.clone(),
                name: meta.name.unwrap_or_else(|| id.clone()),
                description: meta.description,
                enabled: false,
                installed: true,
                source: if id == "onlydevs-filter-bypass" {
                    "bundled".into()
                } else {
                    "user".into()
                },
                install_path: Some(paths::path_display(&path)),
                version: meta.version,
                hooks: meta.hooks,
                has_self_test: meta.has_self_test,
            });
        }
    }

    if builtin_dir.exists() {
        if let Ok(read_dir) = fs::read_dir(&builtin_dir) {
            for entry in read_dir.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let id = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or_default()
                    .to_string();
                if id.is_empty() {
                    continue;
                }
                let meta = inspect_plugin_dir(&path, &id);
                entries.entry(id.clone()).or_insert_with(|| PluginEntry {
                    id: id.clone(),
                    name: meta.name.unwrap_or_else(|| id.clone()),
                    description: meta.description,
                    enabled: false,
                    installed: true,
                    source: "builtin".into(),
                    install_path: Some(paths::path_display(&path)),
                    version: meta.version,
                    hooks: meta.hooks,
                    has_self_test: meta.has_self_test,
                });
            }
        }
    }

    // Ensure onlydevs-filter-bypass always appears at the top of the list even if not installed yet
    if !entries.contains_key("onlydevs-filter-bypass") {
        let bundled_src = paths::default_bundled_plugin_source();
        let meta = if bundled_src.is_dir() {
            inspect_plugin_dir(&bundled_src, "onlydevs-filter-bypass")
        } else {
            PluginManifestMeta {
                name: Some("onlydevs-filter-bypass".into()),
                description: Some("Execution continuity lifecycle hook for Antigravity".into()),
                version: Some("1.3.1".into()),
                hooks: vec![
                    "SessionStart".into(),
                    "PreInvocation".into(),
                    "PostInvocation".into(),
                    "Stop".into(),
                ],
                has_self_test: true,
            }
        };
        entries.insert(
            "onlydevs-filter-bypass".into(),
            PluginEntry {
                id: "onlydevs-filter-bypass".into(),
                name: meta
                    .name
                    .unwrap_or_else(|| "onlydevs-filter-bypass".into()),
                description: meta.description,
                enabled: false,
                installed: false,
                source: "bundled".into(),
                install_path: None,
                version: meta.version,
                hooks: meta.hooks,
                has_self_test: meta.has_self_test,
            },
        );
    }

    let mut list: Vec<PluginEntry> = entries.into_values().collect();
    list.sort_by(|a, b| {
        let a_is_fb = a.id == "onlydevs-filter-bypass";
        let b_is_fb = b.id == "onlydevs-filter-bypass";
        b_is_fb.cmp(&a_is_fb).then_with(|| a.id.cmp(&b.id))
    });
    Ok(list)
}

pub(crate) fn write_plugin_enabled_flag(plugin_id: &str, enabled: bool) -> Result<(), String> {
    let mut config = read_config_value()?;
    let root = config
        .as_object_mut()
        .ok_or_else(|| "config.json không phải object".to_string())?;
    let plugins = root
        .entry("plugins")
        .or_insert_with(|| Value::Object(Map::new()));
    let plugins_map = plugins
        .as_object_mut()
        .ok_or_else(|| "plugins không phải object".to_string())?;
    let entry = plugins_map
        .entry(plugin_id.to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    let entry_map = entry
        .as_object_mut()
        .ok_or_else(|| "plugin entry không phải object".to_string())?;
    entry_map.insert("enabled".into(), Value::Bool(enabled));
    write_config_value(&config)
}

pub fn set_plugin_enabled(plugin_id: &str, enabled: bool) -> Result<(), String> {
    if plugin_id == plugins::DEFAULT_PLUGIN_ID && enabled {
        let installed_script = paths::plugins_dir()?
            .join(plugins::DEFAULT_PLUGIN_ID)
            .join("filter_bypass.py");
        if !installed_script.is_file() {
            let _ = plugins::install_filter_bypass(None)?;
            return Ok(());
        }
    }
    write_plugin_enabled_flag(plugin_id, enabled)
}

fn read_cli_version() -> Option<String> {
    let path = paths::agy_version_json_path().ok()?;
    let raw = fs::read_to_string(path).ok()?;
    let val: Value = serde_json::from_str(&raw).ok()?;
    val.get("version").and_then(Value::as_str).map(str::to_string)
}

pub fn build_overview() -> Result<OverviewState, String> {
    let config_path = paths::config_json_path()?;
    let mcp_config_path = paths::mcp_config_json_path()?;
    let agents_path = paths::agents_md_path()?;
    let plugins = list_plugins()?;
    let enabled = plugins.iter().filter(|p| p.enabled).count();
    let python_path = plugins::resolve_python().ok().map(|p| paths::path_display(&p));

    Ok(OverviewState {
        gemini_home: paths::path_display(&paths::gemini_home()?),
        config_dir: paths::path_display(&paths::config_dir()?),
        config_exists: config_path.exists(),
        agents_exists: agents_path.exists(),
        agents_path: paths::path_display(&agents_path),
        config_path: paths::path_display(&config_path),
        mcp_config_path: paths::path_display(&mcp_config_path),
        mcp_config_exists: mcp_config_path.exists(),
        plugin_count: plugins.len(),
        plugins_enabled: enabled,
        antigravity_cli_dir: paths::path_display(&paths::antigravity_cli_dir()?),
        antigravity_ide_dir: paths::path_display(&paths::antigravity_ide_dir()?),
        skills_dir: paths::path_display(&paths::skills_dir()?),
        bundled_plugin_source: paths::path_display(&paths::default_bundled_plugin_source()),
        cli_version: read_cli_version(),
        python_path,
    })
}
