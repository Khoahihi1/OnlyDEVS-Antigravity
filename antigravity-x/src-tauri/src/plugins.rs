use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use uuid::Uuid;
use walkdir::WalkDir;

use crate::config;
use crate::paths;

pub const DEFAULT_PLUGIN_ID: &str = "onlydevs-filter-bypass";

const COPY_FILES: &[&str] = &[
    "filter_bypass.py",
    "hooks.json",
    "plugin.json",
    "test_filter_bypass.py",
    "README.md",
    "CHANGELOG.md",
];

const EMBEDDED_FILTER_BYPASS_FILES: &[(&str, &str)] = &[
    (
        "filter_bypass.py",
        include_str!("../embedded_plugins/onlydevs-filter-bypass/filter_bypass.py"),
    ),
    (
        "hooks.json",
        include_str!("../embedded_plugins/onlydevs-filter-bypass/hooks.json"),
    ),
    (
        "plugin.json",
        include_str!("../embedded_plugins/onlydevs-filter-bypass/plugin.json"),
    ),
    (
        "test_filter_bypass.py",
        include_str!("../embedded_plugins/onlydevs-filter-bypass/test_filter_bypass.py"),
    ),
    (
        "README.md",
        include_str!("../embedded_plugins/onlydevs-filter-bypass/README.md"),
    ),
    (
        "CHANGELOG.md",
        include_str!("../embedded_plugins/onlydevs-filter-bypass/CHANGELOG.md"),
    ),
];

pub(crate) fn resolve_python() -> Result<PathBuf, String> {
    if let Ok(custom) = std::env::var("ANTIGRAVITY_X_PYTHON") {
        let trimmed = custom.trim();
        if !trimmed.is_empty() {
            let path = PathBuf::from(trimmed);
            if path.is_file() {
                return Ok(path);
            }
            return Err(format!("Python không tồn tại: {}", trimmed));
        }
    }
    if let Ok(path) = which::which("python") {
        return Ok(path);
    }
    if let Ok(path) = which::which("py") {
        return Ok(path);
    }
    let fallback = dirs::home_dir()
        .map(|h| {
            h.join("AppData")
                .join("Local")
                .join("Programs")
                .join("Python")
                .join("Python312")
                .join("python.exe")
        })
        .unwrap_or_default();
    if fallback.is_file() {
        return Ok(fallback);
    }
    Err("Không tìm thấy Python. Cài Python 3.12+ hoặc đặt ANTIGRAVITY_X_PYTHON.".into())
}

fn rewrite_hooks(dest: &Path, python: &Path) -> Result<(), String> {
    let hooks_path = dest.join("hooks.json");
    if !hooks_path.is_file() {
        return Ok(());
    }
    let raw = fs::read_to_string(&hooks_path).map_err(|e| format!("Đọc hooks.json: {e}"))?;
    let mut value: Value =
        serde_json::from_str(&raw).map_err(|e| format!("Parse hooks.json: {e}"))?;
    let plugin_key = DEFAULT_PLUGIN_ID;
    let python_cmd = format!("\"{}\"", python.to_string_lossy());
    let Some(plugin_obj) = value.get_mut(plugin_key).and_then(Value::as_object_mut) else {
        return Ok(());
    };
    for (event, phase) in [
        ("SessionStart", "session"),
        ("PreInvocation", "pre"),
        ("PostInvocation", "post"),
        ("Stop", "stop"),
    ] {
        let Some(handlers) = plugin_obj.get_mut(event).and_then(Value::as_array_mut) else {
            continue;
        };
        for handler in handlers.iter_mut() {
            let Some(obj) = handler.as_object_mut() else {
                continue;
            };
            obj.insert(
                "command".into(),
                json!(format!("{python_cmd} ./filter_bypass.py {phase}")),
            );
            obj.insert(
                "commandWindows".into(),
                json!(format!("{python_cmd} .\\filter_bypass.py {phase}")),
            );
        }
    }
    let pretty =
        serde_json::to_string_pretty(&value).map_err(|e| format!("Serialize hooks: {e}"))?;
    fs::write(&hooks_path, pretty).map_err(|e| format!("Ghi hooks.json: {e}"))
}

pub fn install_filter_bypass(source_override: Option<String>) -> Result<String, String> {
    let dest = paths::plugins_dir()?.join(DEFAULT_PLUGIN_ID);
    fs::create_dir_all(&dest).map_err(|e| format!("Tạo thư mục plugin: {e}"))?;

    if let Some(custom) = source_override.filter(|s| !s.trim().is_empty()) {
        let source = PathBuf::from(custom);
        if source.is_dir() {
            for file in COPY_FILES {
                let from = source.join(file);
                if from.exists() {
                    fs::copy(&from, dest.join(file)).map_err(|e| format!("Copy {file}: {e}"))?;
                }
            }
        } else {
            for (filename, content) in EMBEDDED_FILTER_BYPASS_FILES {
                fs::write(dest.join(filename), content)
                    .map_err(|e| format!("Ghi {filename}: {e}"))?;
            }
        }
    } else {
        // Hardcoded built-in plugin payload compiled directly into OnlyDEVS-Antigravity.exe
        for (filename, content) in EMBEDDED_FILTER_BYPASS_FILES {
            fs::write(dest.join(filename), content)
                .map_err(|e| format!("Ghi {filename}: {e}"))?;
        }
    }

    if let Ok(python) = resolve_python() {
        let _ = rewrite_hooks(&dest, &python);
    }
    config::write_plugin_enabled_flag(DEFAULT_PLUGIN_ID, true)?;
    Ok(format!(
        "Đã cài & kích hoạt {} (hardcoded built-in v1.4.0) tại {}",
        DEFAULT_PLUGIN_ID,
        paths::path_display(&dest)
    ))
}

fn sanitize_plugin_id(raw: &str) -> String {
    raw.trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

fn detect_plugin_id_from_dir(dir: &Path, explicit_id: Option<String>) -> String {
    if let Some(id) = explicit_id {
        let clean = sanitize_plugin_id(&id);
        if !clean.is_empty() {
            return clean;
        }
    }
    let plugin_json = dir.join("plugin.json");
    if plugin_json.is_file() {
        if let Ok(raw) = fs::read_to_string(&plugin_json) {
            if let Ok(val) = serde_json::from_str::<Value>(&raw) {
                if let Some(id) = val
                    .get("id")
                    .or_else(|| val.get("name"))
                    .and_then(Value::as_str)
                {
                    let clean = sanitize_plugin_id(id);
                    if !clean.is_empty() {
                        return clean;
                    }
                }
            }
        }
    }
    let folder_name = dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("custom-plugin");
    let stripped = folder_name
        .strip_suffix("-source")
        .unwrap_or(folder_name);
    let clean = sanitize_plugin_id(stripped);
    if clean.is_empty() {
        "custom-plugin".into()
    } else {
        clean
    }
}

fn copy_dir_all(src: &Path, dst: &Path) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(|e| format!("Tạo thư mục {}: {e}", dst.display()))?;
    for entry in WalkDir::new(src).into_iter().flatten() {
        let path = entry.path();
        let Ok(rel) = path.strip_prefix(src) else {
            continue;
        };
        if rel.as_os_str().is_empty() {
            continue;
        }
        // Skip .git or node_modules or __pycache__ when copying plugin folder
        if rel.components().any(|c| {
            let s = c.as_os_str().to_string_lossy();
            s == ".git" || s == "__pycache__" || s == "node_modules"
        }) {
            continue;
        }
        let target = dst.join(rel);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&target).map_err(|e| format!("Tạo dir: {e}"))?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = target.parent() {
                let _ = fs::create_dir_all(parent);
            }
            fs::copy(path, &target).map_err(|e| format!("Copy {}: {e}", rel.display()))?;
        }
    }
    Ok(())
}

pub fn install_plugin_from_folder(
    source_path: String,
    plugin_id: Option<String>,
) -> Result<String, String> {
    let src = PathBuf::from(source_path.trim().trim_matches('"'));
    if !src.is_dir() {
        return Err(format!(
            "Thư mục nguồn không tồn tại: {}",
            paths::path_display(&src)
        ));
    }

    // If user pointed to onlydevs-filter-bypass source folder, use the specialized installer
    if src.join("filter_bypass.py").is_file()
        && plugin_id
            .as_deref()
            .map(|s| s.trim().is_empty() || s == DEFAULT_PLUGIN_ID)
            .unwrap_or(true)
    {
        return install_filter_bypass(Some(src.to_string_lossy().to_string()));
    }

    let id = detect_plugin_id_from_dir(&src, plugin_id);
    let dest = paths::plugins_dir()?.join(&id);
    copy_dir_all(&src, &dest)?;

    if dest.join("filter_bypass.py").is_file() {
        if let Ok(python) = resolve_python() {
            let _ = rewrite_hooks(&dest, &python);
        }
    }

    config::write_plugin_enabled_flag(&id, true)?;
    Ok(format!(
        "Đã thêm & bật plugin '{}' tại {}",
        id,
        paths::path_display(&dest)
    ))
}

pub fn install_plugin_from_zip(
    zip_path: Option<String>,
    zip_name: Option<String>,
    zip_bytes: Option<Vec<u8>>,
    plugin_id: Option<String>,
) -> Result<String, String> {
    let temp_root = std::env::temp_dir().join(format!("onlydevs-plugin-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_root).map_err(|e| format!("Tạo temp dir: {e}"))?;

    let archive_file = if let Some(bytes) = zip_bytes {
        let fname = zip_name
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "plugin.zip".into());
        let p = temp_root.join(fname);
        fs::write(&p, bytes).map_err(|e| format!("Ghi temp zip: {e}"))?;
        p
    } else if let Some(p_str) = zip_path {
        let p = PathBuf::from(p_str.trim().trim_matches('"'));
        if !p.is_file() {
            let _ = fs::remove_dir_all(&temp_root);
            return Err(format!("File ZIP không tồn tại: {}", p.display()));
        }
        p
    } else {
        let _ = fs::remove_dir_all(&temp_root);
        return Err("Chưa chọn file ZIP plugin".into());
    };

    let extract_dir = temp_root.join("extracted");
    fs::create_dir_all(&extract_dir).map_err(|e| format!("Tạo extract dir: {e}"))?;

    // Extract using Windows built-in tar.exe or PowerShell Expand-Archive fallback
    let tar_status = Command::new("tar")
        .arg("-xf")
        .arg(&archive_file)
        .arg("-C")
        .arg(&extract_dir)
        .status();

    let extracted_ok = tar_status.map(|s| s.success()).unwrap_or(false);
    if !extracted_ok {
        let ps_cmd = format!(
            "Expand-Archive -LiteralPath '{}' -DestinationPath '{}' -Force",
            archive_file.to_string_lossy().replace('\'', "''"),
            extract_dir.to_string_lossy().replace('\'', "''")
        );
        let ps_out = Command::new("powershell")
            .args(["-NoProfile", "-Command", &ps_cmd])
            .output()
            .map_err(|e| format!("Giải nén ZIP thất bại: {e}"))?;
        if !ps_out.status.success() {
            let err = String::from_utf8_lossy(&ps_out.stderr).to_string();
            let _ = fs::remove_dir_all(&temp_root);
            return Err(format!("Giải nén ZIP lỗi: {err}"));
        }
    }

    // If extracted dir contains a single subdirectory, descend into it
    let mut plugin_root = extract_dir.clone();
    if let Ok(entries) = fs::read_dir(&extract_dir) {
        let items: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        if items.len() == 1 && items[0].is_dir() {
            plugin_root = items[0].clone();
        }
    }

    let fallback_id = plugin_id.or_else(|| {
        archive_file
            .file_stem()
            .and_then(|s| s.to_str())
            .map(str::to_string)
    });
    let res = install_plugin_from_folder(
        plugin_root.to_string_lossy().to_string(),
        fallback_id,
    );
    let _ = fs::remove_dir_all(&temp_root);
    res
}

pub fn install_plugin_from_git(
    repo_url: String,
    plugin_id: Option<String>,
) -> Result<String, String> {
    let url = repo_url.trim();
    if url.is_empty() {
        return Err("URL Git repository không được để trống".into());
    }
    let temp_root = std::env::temp_dir().join(format!("onlydevs-git-{}", Uuid::new_v4()));
    let derived_name = url
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("git-plugin")
        .strip_suffix(".git")
        .unwrap_or("git-plugin");
    let clone_dir = temp_root.join(derived_name);
    fs::create_dir_all(&temp_root).map_err(|e| format!("Tạo temp dir: {e}"))?;

    let output = Command::new("git")
        .args(["clone", "--depth", "1", url])
        .arg(&clone_dir)
        .output()
        .map_err(|e| format!("Không gọi được git clone: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let _ = fs::remove_dir_all(&temp_root);
        return Err(format!("git clone thất bại: {stderr}"));
    }

    let res = install_plugin_from_folder(
        clone_dir.to_string_lossy().to_string(),
        plugin_id.or_else(|| Some(derived_name.to_string())),
    );
    let _ = fs::remove_dir_all(&temp_root);
    res
}

pub fn create_custom_plugin(
    plugin_id: String,
    name: String,
    version: String,
    description: String,
    hooks_raw: Option<String>,
    register_only: bool,
) -> Result<String, String> {
    let clean_id = sanitize_plugin_id(&plugin_id);
    if clean_id.is_empty() {
        return Err("Plugin ID không hợp lệ".into());
    }

    if !register_only {
        let dest = paths::plugins_dir()?.join(&clean_id);
        fs::create_dir_all(&dest).map_err(|e| format!("Tạo thư mục plugin: {e}"))?;
        let manifest = json!({
            "id": clean_id,
            "name": if name.trim().is_empty() { &clean_id } else { name.trim() },
            "version": if version.trim().is_empty() { "1.0.0" } else { version.trim() },
            "description": description.trim(),
        });
        let pretty_manifest = serde_json::to_string_pretty(&manifest)
            .map_err(|e| format!("Serialize plugin.json: {e}"))?;
        fs::write(dest.join("plugin.json"), pretty_manifest)
            .map_err(|e| format!("Ghi plugin.json: {e}"))?;

        if let Some(h_raw) = hooks_raw {
            let trimmed = h_raw.trim();
            if !trimmed.is_empty() && trimmed != "{}" {
                let val: Value = serde_json::from_str(trimmed)
                    .map_err(|e| format!("hooks.json không hợp lệ: {e}"))?;
                let pretty_hooks = serde_json::to_string_pretty(&val)
                    .map_err(|e| format!("Serialize hooks.json: {e}"))?;
                fs::write(dest.join("hooks.json"), pretty_hooks)
                    .map_err(|e| format!("Ghi hooks.json: {e}"))?;
            }
        }
    }

    config::write_plugin_enabled_flag(&clean_id, true)?;
    Ok(format!("Đã tạo & bật plugin '{clean_id}'"))
}

pub fn uninstall_plugin(plugin_id: &str) -> Result<String, String> {
    let dest = paths::plugins_dir()?.join(plugin_id);
    let _ = config::write_plugin_enabled_flag(plugin_id, false);
    if dest.is_dir() {
        fs::remove_dir_all(&dest).map_err(|e| format!("Xóa thư mục plugin: {e}"))?;
    }
    Ok(format!("Đã gỡ cài đặt plugin {plugin_id}"))
}

pub fn get_plugin_hooks_raw(plugin_id: &str) -> Result<String, String> {
    let user_hooks = paths::plugins_dir()?.join(plugin_id).join("hooks.json");
    if user_hooks.is_file() {
        return fs::read_to_string(&user_hooks).map_err(|e| format!("Đọc hooks.json: {e}"));
    }
    let builtin_hooks = paths::builtin_plugins_dir()?.join(plugin_id).join("hooks.json");
    if builtin_hooks.is_file() {
        return fs::read_to_string(&builtin_hooks).map_err(|e| format!("Đọc hooks.json: {e}"));
    }
    if plugin_id == DEFAULT_PLUGIN_ID {
        let bundled = paths::default_bundled_plugin_source().join("hooks.json");
        if bundled.is_file() {
            return fs::read_to_string(&bundled).map_err(|e| format!("Đọc hooks.json: {e}"));
        }
    }
    Ok("{}".into())
}

pub fn save_plugin_hooks_raw(plugin_id: &str, raw: &str) -> Result<(), String> {
    let val: Value =
        serde_json::from_str(raw).map_err(|e| format!("JSON hooks không hợp lệ: {e}"))?;
    let dir = paths::plugins_dir()?.join(plugin_id);
    fs::create_dir_all(&dir).map_err(|e| format!("Tạo thư mục plugin: {e}"))?;
    let pretty = serde_json::to_string_pretty(&val).map_err(|e| format!("Serialize: {e}"))?;
    fs::write(dir.join("hooks.json"), pretty).map_err(|e| format!("Ghi hooks.json: {e}"))
}

pub fn run_filter_bypass_test() -> Result<String, String> {
    let python = resolve_python()?;
    let installed_dir = paths::plugins_dir()?.join(DEFAULT_PLUGIN_ID);
    let work_dir = if installed_dir.join("test_filter_bypass.py").is_file() {
        installed_dir
    } else {
        let bundled = paths::default_bundled_plugin_source();
        if bundled.join("test_filter_bypass.py").is_file() {
            bundled
        } else {
            return Err("Không tìm thấy test_filter_bypass.py".into());
        }
    };

    let output = Command::new(&python)
        .arg("test_filter_bypass.py")
        .current_dir(&work_dir)
        .output()
        .map_err(|e| format!("Chạy test_filter_bypass.py thất bại: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let combined = match (stdout.is_empty(), stderr.is_empty()) {
        (false, false) => format!("{stdout}\n{stderr}"),
        (false, true) => stdout,
        (true, false) => stderr,
        (true, true) => "No output".into(),
    };

    if output.status.success() {
        Ok(format!(
            "Self-test PASSED (exit 0)\nDirectory: {}\n\n{}",
            paths::path_display(&work_dir),
            combined
        ))
    } else {
        Err(format!(
            "Self-test FAILED ({:?})\n{}",
            output.status.code(),
            combined
        ))
    }
}
