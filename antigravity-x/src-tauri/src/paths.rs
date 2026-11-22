use std::path::{Path, PathBuf};

pub(crate) fn home_dir() -> Result<PathBuf, String> {
    dirs::home_dir().ok_or_else(|| "Không tìm thấy thư mục home".into())
}

pub(crate) fn gemini_home() -> Result<PathBuf, String> {
    if let Ok(value) = std::env::var("GEMINI_HOME") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Ok(PathBuf::from(trimmed));
        }
    }
    Ok(home_dir()?.join(".gemini"))
}

pub(crate) fn config_dir() -> Result<PathBuf, String> {
    Ok(gemini_home()?.join("config"))
}

pub(crate) fn config_json_path() -> Result<PathBuf, String> {
    Ok(config_dir()?.join("config.json"))
}

pub(crate) fn mcp_config_json_path() -> Result<PathBuf, String> {
    Ok(config_dir()?.join("mcp_config.json"))
}

pub(crate) fn agents_md_path() -> Result<PathBuf, String> {
    Ok(config_dir()?.join("AGENTS.md"))
}

pub(crate) fn plugins_dir() -> Result<PathBuf, String> {
    Ok(config_dir()?.join("plugins"))
}

pub(crate) fn builtin_plugins_dir() -> Result<PathBuf, String> {
    Ok(antigravity_dir()?.join("builtin").join("plugins"))
}

pub(crate) fn skills_dir() -> Result<PathBuf, String> {
    Ok(config_dir()?.join("skills"))
}

pub(crate) fn builtin_skills_dir() -> Result<PathBuf, String> {
    Ok(antigravity_dir()?.join("builtin").join("skills"))
}

pub(crate) fn antigravity_dir() -> Result<PathBuf, String> {
    Ok(gemini_home()?.join("antigravity"))
}

pub(crate) fn antigravity_cli_dir() -> Result<PathBuf, String> {
    Ok(gemini_home()?.join("antigravity-cli"))
}

pub(crate) fn antigravity_ide_dir() -> Result<PathBuf, String> {
    Ok(gemini_home()?.join("antigravity-ide"))
}

pub(crate) fn agy_version_json_path() -> Result<PathBuf, String> {
    Ok(gemini_home()?.join("bin").join("agy.version.json"))
}

pub(crate) fn app_home() -> Result<PathBuf, String> {
    if let Ok(value) = std::env::var("ANTIGRAVITY_X_HOME") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Ok(PathBuf::from(trimmed));
        }
    }
    Ok(home_dir()?.join(".antigravity-x"))
}

pub(crate) fn disabled_skills_dir() -> Result<PathBuf, String> {
    Ok(app_home()?.join("disabled_skills"))
}

pub(crate) fn exports_dir() -> Result<PathBuf, String> {
    Ok(app_home()?.join("exports"))
}

pub(crate) fn db_path() -> Result<PathBuf, String> {
    Ok(app_home()?.join("antigravity-x.db"))
}

pub(crate) fn default_bundled_plugin_source() -> PathBuf {
    if let Ok(value) = std::env::var("ANTIGRAVITY_X_PLUGIN_SOURCE") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    let candidates = [
        PathBuf::from("../antigravity-filter-bypass-v1.3.1-source"),
        PathBuf::from("antigravity-filter-bypass-v1.3.1-source"),
        PathBuf::from(
            r"C:\Users\KhoaHihi\Desktop\KhoaHihi\OnlyDEVS-Antigravity\antigravity-filter-bypass-v1.3.1-source",
        ),
    ];
    for c in &candidates {
        if c.is_dir() {
            return std::fs::canonicalize(c).unwrap_or_else(|_| c.clone());
        }
    }
    candidates[2].clone()
}

pub(crate) fn workspace_examples_dirs() -> Vec<PathBuf> {
    let candidates = [
        PathBuf::from(r"C:\Users\KhoaHihi\Desktop\KhoaHihi\OnlyDEVS-Antigravity\Prompt"),
        PathBuf::from("../Prompt"),
        PathBuf::from("examples"),
        PathBuf::from(r"C:\Users\KhoaHihi\Desktop\KhoaHihi\OnlyDEVS-Antigravity\antigravity-x\examples"),
    ];
    let mut out = Vec::new();
    for c in candidates {
        if c.is_dir() {
            let resolved = std::fs::canonicalize(&c).unwrap_or(c);
            if !out.contains(&resolved) {
                out.push(resolved);
            }
        }
    }
    out
}

pub(crate) fn path_display(path: &Path) -> String {
    let raw = path.to_string_lossy().replace('\\', "/");
    raw.strip_prefix("//?/").unwrap_or(&raw).to_string()
}
