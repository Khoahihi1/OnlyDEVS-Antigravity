use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};

use crate::paths;

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const GITHUB_REPO_SLUG: &str = "Khoahihi1/OnlyDEVS-Antigravity";
pub const GITHUB_RELEASES_API: &str =
    "https://api.github.com/repos/Khoahihi1/OnlyDEVS-Antigravity/releases/latest";
pub const GITHUB_LATEST_JSON_URL: &str =
    "https://github.com/Khoahihi1/OnlyDEVS-Antigravity/releases/latest/download/latest.json";
pub const GITHUB_RELEASES_PAGE: &str =
    "https://github.com/Khoahihi1/OnlyDEVS-Antigravity/releases";

const DEFAULT_CHECK_INTERVAL_HOURS: u64 = 1;
const SETTINGS_FILENAME: &str = "update_settings.json";

static SETTINGS_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn settings_lock() -> &'static Mutex<()> {
    SETTINGS_LOCK.get_or_init(|| Mutex::new(()))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSettings {
    #[serde(default = "default_true")]
    pub auto_check: bool,
    #[serde(default)]
    pub last_check_time: u64,
    #[serde(default = "default_interval")]
    pub check_interval_hours: u64,
    #[serde(default)]
    pub auto_install: bool,
    #[serde(default)]
    pub last_run_version: String,
    #[serde(default = "default_true")]
    pub remind_on_update: bool,
    #[serde(default)]
    pub skipped_version: String,
}

fn default_true() -> bool {
    true
}

fn default_interval() -> u64 {
    DEFAULT_CHECK_INTERVAL_HOURS
}

impl Default for UpdateSettings {
    fn default() -> Self {
        Self {
            auto_check: true,
            last_check_time: 0,
            check_interval_hours: DEFAULT_CHECK_INTERVAL_HOURS,
            auto_install: false,
            last_run_version: String::new(),
            remind_on_update: true,
            skipped_version: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionJumpInfo {
    pub previous_version: String,
    pub current_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheckResult {
    pub has_update: bool,
    pub current_version: String,
    pub latest_version: String,
    pub release_name: String,
    pub release_notes: String,
    pub published_at: Option<String>,
    pub release_url: String,
    pub download_url: Option<String>,
    pub asset_name: Option<String>,
    pub asset_size: Option<u64>,
    pub is_installer: bool,
    pub skipped: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProgressPayload {
    pub downloaded: u64,
    pub total: Option<u64>,
    pub percent: f64,
    pub status: String,
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn settings_path() -> Result<PathBuf, String> {
    let dir = paths::app_home()?;
    fs::create_dir_all(&dir).map_err(|e| format!("Tạo thư mục dữ liệu updater: {e}"))?;
    Ok(dir.join(SETTINGS_FILENAME))
}

fn load_settings_unlocked() -> Result<UpdateSettings, String> {
    let path = settings_path()?;
    if !path.is_file() {
        return Ok(UpdateSettings::default());
    }
    let raw = fs::read_to_string(&path).map_err(|e| format!("Đọc update_settings.json: {e}"))?;
    Ok(serde_json::from_str::<UpdateSettings>(&raw).unwrap_or_default())
}

fn save_settings_unlocked(settings: &UpdateSettings) -> Result<(), String> {
    let path = settings_path()?;
    let raw = serde_json::to_string_pretty(settings)
        .map_err(|e| format!("Serialize update_settings: {e}"))?;
    fs::write(&path, raw).map_err(|e| format!("Ghi update_settings.json: {e}"))
}

pub fn load_update_settings() -> Result<UpdateSettings, String> {
    let _g = settings_lock()
        .lock()
        .map_err(|_| "Updater lock poisoned".to_string())?;
    load_settings_unlocked()
}

pub fn patch_update_settings(
    auto_check: Option<bool>,
    check_interval_hours: Option<u64>,
    auto_install: Option<bool>,
    remind_on_update: Option<bool>,
    skipped_version: Option<String>,
) -> Result<UpdateSettings, String> {
    let _g = settings_lock()
        .lock()
        .map_err(|_| "Updater lock poisoned".to_string())?;
    let mut s = load_settings_unlocked()?;
    if let Some(v) = auto_check {
        s.auto_check = v;
    }
    if let Some(v) = check_interval_hours {
        s.check_interval_hours = v.max(1);
    }
    if let Some(v) = auto_install {
        s.auto_install = v;
    }
    if let Some(v) = remind_on_update {
        s.remind_on_update = v;
    }
    if let Some(v) = skipped_version {
        s.skipped_version = v.trim().trim_start_matches('v').to_string();
    }
    save_settings_unlocked(&s)?;
    Ok(s)
}

pub fn should_check_updates() -> Result<bool, String> {
    let s = load_update_settings()?;
    if !s.auto_check {
        return Ok(false);
    }
    let elapsed_hours = now_unix_secs().saturating_sub(s.last_check_time) / 3600;
    let interval = s.check_interval_hours.max(1);
    Ok(elapsed_hours >= interval)
}

pub fn check_version_jump() -> Result<Option<VersionJumpInfo>, String> {
    let _g = settings_lock()
        .lock()
        .map_err(|_| "Updater lock poisoned".to_string())?;
    let mut s = load_settings_unlocked()?;
    let prev = s.last_run_version.trim().to_string();
    let curr = CURRENT_VERSION.to_string();
    if prev != curr {
        s.last_run_version = curr.clone();
        let _ = save_settings_unlocked(&s);
        if !prev.is_empty() && is_newer_version(&curr, &prev) {
            return Ok(Some(VersionJumpInfo {
                previous_version: prev,
                current_version: curr,
            }));
        }
    }
    Ok(None)
}

fn normalize_version(raw: &str) -> String {
    raw.trim()
        .trim_start_matches(['v', 'V'])
        .split(['-', '+'])
        .next()
        .unwrap_or("")
        .trim()
        .to_string()
}

pub fn is_newer_version(latest: &str, current: &str) -> bool {
    let parse = |v: &str| -> Vec<u32> {
        normalize_version(v)
            .split('.')
            .filter_map(|p| p.parse::<u32>().ok())
            .collect()
    };
    let l = parse(latest);
    let c = parse(current);
    let max_len = l.len().max(c.len());
    for i in 0..max_len {
        let lp = *l.get(i).unwrap_or(&0);
        let cp = *c.get(i).unwrap_or(&0);
        if lp > cp {
            return true;
        }
        if lp < cp {
            return false;
        }
    }
    false
}

fn build_http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(format!("OnlyDEVS-Antigravity/{CURRENT_VERSION}"))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Khởi tạo HTTP client updater: {e}"))
}

fn pick_windows_asset(assets: &[Value]) -> (Option<String>, Option<String>, Option<u64>, bool) {
    let mut best_setup: Option<(String, String, Option<u64>)> = None;
    let mut best_msi: Option<(String, String, Option<u64>)> = None;
    let mut best_exe: Option<(String, String, Option<u64>)> = None;

    for a in assets {
        let Some(name) = a.get("name").and_then(Value::as_str) else {
            continue;
        };
        let Some(url) = a.get("browser_download_url").and_then(Value::as_str) else {
            continue;
        };
        let size = a.get("size").and_then(Value::as_u64);
        let lower = name.to_ascii_lowercase();
        if lower.ends_with("-setup.exe") || (lower.contains("setup") && lower.ends_with(".exe")) {
            best_setup = Some((url.to_string(), name.to_string(), size));
        } else if lower.ends_with(".msi") {
            best_msi = Some((url.to_string(), name.to_string(), size));
        } else if lower.ends_with(".exe") {
            best_exe = Some((url.to_string(), name.to_string(), size));
        }
    }

    if let Some((u, n, s)) = best_setup {
        return (Some(u), Some(n), s, true);
    }
    if let Some((u, n, s)) = best_msi {
        return (Some(u), Some(n), s, true);
    }
    if let Some((u, n, s)) = best_exe {
        return (Some(u), Some(n), s, false);
    }
    (None, None, None, false)
}

pub async fn check_for_updates(force: bool) -> Result<UpdateCheckResult, String> {
    let settings = load_update_settings()?;
    let client = build_http_client()?;

    // Update last_check_time
    if let Ok(_g) = settings_lock().lock() {
        if let Ok(mut s) = load_settings_unlocked() {
            s.last_check_time = now_unix_secs();
            let _ = save_settings_unlocked(&s);
        }
    }

    // 1. Primary: GitHub Releases Latest API (exact version + release notes + Windows setup asset)
    let api_res = client
        .get(GITHUB_RELEASES_API)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .send()
        .await;

    if let Ok(resp) = api_res {
        if resp.status().is_success() {
            if let Ok(val) = resp.json::<Value>().await {
                let tag = val
                    .get("tag_name")
                    .and_then(Value::as_str)
                    .unwrap_or(CURRENT_VERSION);
                let latest_version = normalize_version(tag);
                let release_name = val
                    .get("name")
                    .and_then(Value::as_str)
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or(tag)
                    .to_string();
                let release_notes = val
                    .get("body")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let published_at = val
                    .get("published_at")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                let release_url = val
                    .get("html_url")
                    .and_then(Value::as_str)
                    .unwrap_or(GITHUB_RELEASES_PAGE)
                    .to_string();
                let assets = val
                    .get("assets")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let (download_url, asset_name, asset_size, is_installer) =
                    pick_windows_asset(&assets);
                let has_update = is_newer_version(&latest_version, CURRENT_VERSION);
                let skipped = !force
                    && !settings.skipped_version.is_empty()
                    && normalize_version(&settings.skipped_version) == latest_version;

                return Ok(UpdateCheckResult {
                    has_update,
                    current_version: CURRENT_VERSION.to_string(),
                    latest_version,
                    release_name,
                    release_notes,
                    published_at,
                    release_url,
                    download_url,
                    asset_name,
                    asset_size,
                    is_installer,
                    skipped,
                });
            }
        }
    }

    // 2. Fallback: Tauri-style latest.json manifest on GitHub Releases
    let manifest_res = client
        .get(GITHUB_LATEST_JSON_URL)
        .send()
        .await
        .map_err(|e| format!("Không thể kết nối GitHub Releases ({GITHUB_REPO_SLUG}): {e}"))?;

    if !manifest_res.status().is_success() {
        return Err(format!(
            "Không thể kiểm tra bản cập nhật trên GitHub ({}) — HTTP {}",
            GITHUB_REPO_SLUG,
            manifest_res.status()
        ));
    }

    let manifest: Value = manifest_res
        .json()
        .await
        .map_err(|e| format!("Parse latest.json thất bại: {e}"))?;

    let latest_version = normalize_version(
        manifest
            .get("version")
            .and_then(Value::as_str)
            .unwrap_or(CURRENT_VERSION),
    );
    let release_notes = manifest
        .get("notes")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let published_at = manifest
        .get("pub_date")
        .and_then(Value::as_str)
        .map(str::to_string);
    let download_url = manifest
        .pointer("/platforms/windows-x86_64/url")
        .or_else(|| manifest.pointer("/platforms/windows-x86_64-nsis/url"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let asset_name = download_url
        .as_deref()
        .and_then(|u| u.rsplit('/').next())
        .map(str::to_string);
    let is_installer = asset_name
        .as_deref()
        .map(|n| n.to_ascii_lowercase().contains("setup") || n.to_ascii_lowercase().ends_with(".msi"))
        .unwrap_or(true);
    let has_update = is_newer_version(&latest_version, CURRENT_VERSION);
    let skipped = !force
        && !settings.skipped_version.is_empty()
        && normalize_version(&settings.skipped_version) == latest_version;

    Ok(UpdateCheckResult {
        has_update,
        current_version: CURRENT_VERSION.to_string(),
        latest_version: latest_version.clone(),
        release_name: format!("OnlyDEVS Antigravity v{latest_version}"),
        release_notes,
        published_at,
        release_url: GITHUB_RELEASES_PAGE.to_string(),
        download_url,
        asset_name,
        asset_size: None,
        is_installer,
        skipped,
    })
}

pub async fn download_and_install_update(
    app: AppHandle,
    download_url: String,
    asset_name: Option<String>,
    silent: bool,
) -> Result<String, String> {
    let url = download_url.trim();
    if url.is_empty() {
        return Err("Thiếu đường dẫn tải bản cập nhật.".into());
    }

    let file_name = asset_name
        .filter(|s| !s.trim().is_empty())
        .or_else(|| url.rsplit('/').next().map(str::to_string))
        .unwrap_or_else(|| "OnlyDEVS-Antigravity-setup.exe".to_string());

    let safe_name: String = file_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();

    let temp_dir = std::env::temp_dir().join("onlydevs-antigravity-update");
    fs::create_dir_all(&temp_dir).map_err(|e| format!("Tạo thư mục tải tạm: {e}"))?;
    let dest_path = temp_dir.join(&safe_name);

    let _ = app.emit(
        "updater://progress",
        UpdateProgressPayload {
            downloaded: 0,
            total: None,
            percent: 0.0,
            status: "downloading".into(),
        },
    );

    let client = reqwest::Client::builder()
        .user_agent(format!("OnlyDEVS-Antigravity/{CURRENT_VERSION}"))
        .timeout(std::time::Duration::from_secs(600))
        .build()
        .map_err(|e| format!("Khởi tạo client tải bản cập nhật: {e}"))?;

    let mut resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Tải bản cập nhật thất bại: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!(
            "Máy chủ trả về mã lỗi {} khi tải bản cập nhật.",
            resp.status()
        ));
    }

    let total = resp.content_length();
    let mut file =
        File::create(&dest_path).map_err(|e| format!("Tạo tệp cài đặt tạm thất bại: {e}"))?;
    let mut downloaded: u64 = 0;
    let mut last_emit = std::time::Instant::now();

    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(|e| format!("Lỗi đọc luồng tải xuống: {e}"))?
    {
        file.write_all(&chunk)
            .map_err(|e| format!("Ghi tệp cập nhật thất bại: {e}"))?;
        downloaded = downloaded.saturating_add(chunk.len() as u64);

        if last_emit.elapsed().as_millis() >= 100 {
            let percent = total
                .filter(|&t| t > 0)
                .map(|t| ((downloaded as f64 / t as f64) * 100.0).min(100.0))
                .unwrap_or(0.0);
            let _ = app.emit(
                "updater://progress",
                UpdateProgressPayload {
                    downloaded,
                    total,
                    percent,
                    status: "downloading".into(),
                },
            );
            last_emit = std::time::Instant::now();
        }
    }

    file.flush().ok();
    drop(file);

    let _ = app.emit(
        "updater://progress",
        UpdateProgressPayload {
            downloaded,
            total,
            percent: 100.0,
            status: "installing".into(),
        },
    );

    let lower = safe_name.to_ascii_lowercase();
    #[cfg(target_os = "windows")]
    {
        if lower.ends_with(".msi") {
            let mut cmd = Command::new("msiexec");
            cmd.arg("/i").arg(&dest_path);
            if silent {
                cmd.arg("/passive");
            }
            cmd.spawn()
                .map_err(|e| format!("Không thể chạy trình cài đặt MSI: {e}"))?;
        } else if lower.contains("setup") && lower.ends_with(".exe") {
            let mut cmd = Command::new(&dest_path);
            if silent {
                cmd.arg("/S");
            }
            cmd.spawn()
                .map_err(|e| format!("Không thể chạy trình cài đặt Setup: {e}"))?;
        } else if lower.ends_with(".exe") {
            // Portable binary swap script
            if let Ok(current_exe) = std::env::current_exe() {
                let bat_path = temp_dir.join("apply-update.cmd");
                let script = format!(
                    "@echo off\r\ntimeout /t 2 /nobreak >nul\r\ncopy /Y \"{}\" \"{}\"\r\nstart \"\" \"{}\"\r\ndel \"%~f0\"\r\n",
                    dest_path.display(),
                    current_exe.display(),
                    current_exe.display()
                );
                let _ = fs::write(&bat_path, script);
                Command::new("cmd")
                    .args(["/C", "start", "", "/MIN", &bat_path.to_string_lossy()])
                    .spawn()
                    .map_err(|e| format!("Không thể chạy script cập nhật: {e}"))?;
            }
        }
        // Gracefully exit app so NSIS / updater can overwrite the executable cleanly
        app.exit(0);
    }

    Ok(dest_path.to_string_lossy().to_string())
}
