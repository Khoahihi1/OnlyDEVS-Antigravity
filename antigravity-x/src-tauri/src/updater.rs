use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
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

fn arch_match_score(lower_name: &str, arch: &str) -> u8 {
    let is_arm64 = lower_name.contains("aarch64") || lower_name.contains("arm64");
    let is_x64 =
        lower_name.contains("x86_64") || lower_name.contains("amd64") || lower_name.contains("_x64") || lower_name.contains("-x64");
    let is_universal = lower_name.contains("universal");

    match arch {
        "aarch64" => {
            if is_arm64 {
                3
            } else if is_universal {
                2
            } else if !is_x64 {
                1
            } else {
                0
            }
        }
        "x86_64" => {
            if is_x64 {
                3
            } else if is_universal {
                2
            } else if !is_arm64 {
                1
            } else {
                0
            }
        }
        _ => {
            if is_universal {
                2
            } else {
                1
            }
        }
    }
}

pub fn pick_asset_for_platform(
    assets: &[Value],
    os: &str,
    arch: &str,
    linux_prefer_appimage: bool,
    linux_prefer_deb: bool,
    linux_prefer_rpm: bool,
) -> (Option<String>, Option<String>, Option<u64>, bool) {
    let mut candidates: Vec<(u8, u8, String, String, Option<u64>, bool)> = Vec::new();

    for a in assets {
        let Some(name) = a.get("name").and_then(Value::as_str) else {
            continue;
        };
        let Some(url) = a.get("browser_download_url").and_then(Value::as_str) else {
            continue;
        };
        let size = a.get("size").and_then(Value::as_u64);
        let lower = name.to_ascii_lowercase();

        // Skip signature / checksum / manifest files
        if lower.ends_with(".sig")
            || lower.ends_with(".asc")
            || lower.ends_with(".sha256")
            || lower.ends_with(".json")
        {
            continue;
        }

        let arch_score = arch_match_score(&lower, arch);
        if arch_score == 0 {
            continue;
        }

        match os {
            "windows" => {
                if lower.ends_with("-setup.exe") || (lower.contains("setup") && lower.ends_with(".exe")) {
                    candidates.push((3, arch_score, url.to_string(), name.to_string(), size, true));
                } else if lower.ends_with(".msi") {
                    candidates.push((2, arch_score, url.to_string(), name.to_string(), size, true));
                } else if lower.ends_with(".exe") {
                    candidates.push((1, arch_score, url.to_string(), name.to_string(), size, false));
                }
            }
            "macos" => {
                if lower.ends_with(".app.tar.gz") || lower.ends_with(".app.tgz") {
                    candidates.push((3, arch_score, url.to_string(), name.to_string(), size, true));
                } else if lower.ends_with(".dmg") {
                    candidates.push((2, arch_score, url.to_string(), name.to_string(), size, true));
                }
            }
            "linux" => {
                let kind_rank = if lower.ends_with(".appimage") {
                    if linux_prefer_appimage {
                        4
                    } else if !linux_prefer_deb && !linux_prefer_rpm {
                        3
                    } else {
                        2
                    }
                } else if lower.ends_with(".deb") {
                    if linux_prefer_deb && !linux_prefer_appimage {
                        4
                    } else {
                        2
                    }
                } else if lower.ends_with(".rpm") {
                    if linux_prefer_rpm && !linux_prefer_appimage {
                        4
                    } else {
                        1
                    }
                } else {
                    0
                };
                if kind_rank > 0 {
                    candidates.push((kind_rank, arch_score, url.to_string(), name.to_string(), size, true));
                }
            }
            _ => {}
        }
    }

    candidates.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| b.0.cmp(&a.0)));
    if let Some((_, _, u, n, s, inst)) = candidates.into_iter().next() {
        return (Some(u), Some(n), s, inst);
    }
    (None, None, None, false)
}

fn pick_current_platform_asset(assets: &[Value]) -> (Option<String>, Option<String>, Option<u64>, bool) {
    let prefer_appimage = std::env::var_os("APPIMAGE").is_some();
    let prefer_deb = Path::new("/usr/bin/dpkg").exists() || Path::new("/etc/debian_version").exists();
    let prefer_rpm = Path::new("/usr/bin/rpm").exists() || Path::new("/etc/redhat-release").exists();
    pick_asset_for_platform(
        assets,
        std::env::consts::OS,
        std::env::consts::ARCH,
        prefer_appimage,
        prefer_deb,
        prefer_rpm,
    )
}

fn pick_manifest_platform_url_for(manifest: &Value, os: &str, arch: &str) -> Option<String> {
    let keys: &[&str] = match (os, arch) {
        ("windows", "aarch64") => &["windows-aarch64", "windows-x86_64"],
        ("windows", _) => &["windows-x86_64", "windows-x86_64-nsis"],
        ("macos", "aarch64") => &["darwin-aarch64", "darwin-universal"],
        ("macos", _) => &["darwin-x86_64", "darwin-universal"],
        ("linux", "aarch64") => &["linux-aarch64", "linux-aarch64-appimage"],
        ("linux", _) => &["linux-x86_64", "linux-x86_64-appimage"],
        _ => &[],
    };
    for k in keys {
        let ptr = format!("/platforms/{k}/url");
        if let Some(u) = manifest.pointer(&ptr).and_then(Value::as_str) {
            if !u.trim().is_empty() {
                return Some(u.to_string());
            }
        }
    }
    None
}

fn pick_manifest_platform_url(manifest: &Value) -> Option<String> {
    pick_manifest_platform_url_for(manifest, std::env::consts::OS, std::env::consts::ARCH)
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
                    pick_current_platform_asset(&assets);
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
    let download_url = pick_manifest_platform_url(&manifest);
    let asset_name = download_url
        .as_deref()
        .and_then(|u| u.rsplit('/').next())
        .map(str::to_string);
    let is_installer = asset_name
        .as_deref()
        .map(|n| {
            let l = n.to_ascii_lowercase();
            l.contains("setup")
                || l.ends_with(".msi")
                || l.ends_with(".app.tar.gz")
                || l.ends_with(".dmg")
                || l.ends_with(".appimage")
                || l.ends_with(".deb")
                || l.ends_with(".rpm")
        })
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

    let default_name = match std::env::consts::OS {
        "macos" => "OnlyDEVS-Antigravity.app.tar.gz",
        "linux" => "OnlyDEVS-Antigravity.AppImage",
        _ => "OnlyDEVS-Antigravity-setup.exe",
    };

    let file_name = asset_name
        .filter(|s| !s.trim().is_empty())
        .or_else(|| url.rsplit('/').next().map(str::to_string))
        .unwrap_or_else(|| default_name.to_string());

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
        return Ok("Đang khởi chạy trình cài đặt cập nhật...".into());
    }

    #[cfg(target_os = "macos")]
    {
        return install_update_macos(&app, &dest_path, &temp_dir, &lower);
    }

    #[cfg(target_os = "linux")]
    {
        return install_update_linux(&app, &dest_path, &temp_dir, &lower);
    }

    #[allow(unreachable_code)]
    Ok(dest_path.to_string_lossy().to_string())
}

#[cfg(any(unix, test))]
fn shell_single_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\"'\"'"))
}

#[cfg(unix)]
fn is_dir_writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".onlydevs_probe_{}", std::process::id()));
    match fs::OpenOptions::new().create(true).write(true).open(&probe) {
        Ok(_) => {
            let _ = fs::remove_file(probe);
            true
        }
        Err(_) => false,
    }
}

#[cfg(target_os = "macos")]
fn find_extracted_app_bundle(root: &Path) -> Option<PathBuf> {
    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() && p.extension().and_then(|e| e.to_str()) == Some("app") {
                return Some(p);
            }
        }
    }
    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            let sub = entry.path();
            if sub.is_dir() {
                if let Ok(sub_entries) = fs::read_dir(&sub) {
                    for se in sub_entries.flatten() {
                        let p = se.path();
                        if p.is_dir() && p.extension().and_then(|e| e.to_str()) == Some("app") {
                            return Some(p);
                        }
                    }
                }
            }
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn resolve_macos_target_app(fallback_name: &str) -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        for anc in exe.ancestors() {
            if anc.extension().and_then(|e| e.to_str()) == Some("app") {
                return anc.to_path_buf();
            }
        }
    }
    PathBuf::from("/Applications").join(fallback_name)
}

#[cfg(target_os = "macos")]
fn install_update_macos(
    app: &AppHandle,
    dest_path: &Path,
    temp_dir: &Path,
    lower: &str,
) -> Result<String, String> {
    use std::os::unix::fs::PermissionsExt;

    let _ = Command::new("xattr")
        .args(["-d", "com.apple.quarantine"])
        .arg(dest_path)
        .output();

    if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        let extract_dir = temp_dir.join("extracted");
        if extract_dir.exists() {
            let _ = fs::remove_dir_all(&extract_dir);
        }
        fs::create_dir_all(&extract_dir).map_err(|e| format!("Tạo thư mục giải nén: {e}"))?;

        let tar_out = Command::new("tar")
            .args(["-xzf"])
            .arg(dest_path)
            .arg("-C")
            .arg(&extract_dir)
            .output()
            .map_err(|e| format!("Chạy tar thất bại: {e}"))?;
        if !tar_out.status.success() {
            let err_txt = String::from_utf8_lossy(&tar_out.stderr);
            return Err(format!("Giải nén bản cập nhật macOS thất bại: {err_txt}"));
        }

        let extracted_app = find_extracted_app_bundle(&extract_dir)
            .ok_or_else(|| "Không tìm thấy gói .app trong bản cập nhật.".to_string())?;
        let _ = Command::new("xattr")
            .args(["-dr", "com.apple.quarantine"])
            .arg(&extracted_app)
            .output();

        let fallback_name = extracted_app
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("OnlyDEVS-Antigravity.app");
        let target_app = resolve_macos_target_app(fallback_name);
        let parent_dir = target_app.parent().unwrap_or_else(|| Path::new("/Applications"));
        let writable = is_dir_writable(parent_dir);

        let src_q = shell_single_quote(&extracted_app.to_string_lossy());
        let dst_q = shell_single_quote(&target_app.to_string_lossy());
        let script_path = temp_dir.join("apply-update.sh");

        let replace_cmd = format!(
            "rm -rf {dst_q} && ditto {src_q} {dst_q} && (xattr -dr com.apple.quarantine {dst_q} 2>/dev/null || true)"
        );
        let script_body = if writable {
            format!(
                "#!/bin/sh\nsleep 1.5\n{replace_cmd}\nopen -n {dst_q}\nrm -f -- \"$0\"\n"
            )
        } else {
            let osa_cmd = replace_cmd.replace('\\', "\\\\").replace('"', "\\\"");
            format!(
                "#!/bin/sh\nsleep 1.5\nosascript -e \"do shell script \\\"{osa_cmd}\\\" with administrator privileges\"\nopen -n {dst_q}\nrm -f -- \"$0\"\n"
            )
        };

        fs::write(&script_path, script_body)
            .map_err(|e| format!("Tạo script cập nhật macOS: {e}"))?;
        let _ = fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755));

        Command::new("sh")
            .arg(&script_path)
            .spawn()
            .map_err(|e| format!("Không thể chạy script cập nhật macOS: {e}"))?;
        app.exit(0);
        return Ok("Đang cài đặt bản cập nhật và khởi động lại ứng dụng...".into());
    }

    if lower.ends_with(".dmg") {
        Command::new("open")
            .arg(dest_path)
            .spawn()
            .map_err(|e| format!("Không thể mở tệp DMG: {e}"))?;
        return Ok(format!(
            "Đã tải và mở {}. Hãy kéo ứng dụng vào thư mục Applications để hoàn tất cập nhật.",
            dest_path.file_name().and_then(|n| n.to_str()).unwrap_or("installer.dmg")
        ));
    }

    Command::new("open")
        .args(["-R"])
        .arg(dest_path)
        .spawn()
        .map_err(|e| format!("Không thể mở Finder tới bản tải về: {e}"))?;
    Ok(format!("Đã tải bản cập nhật tại {}", dest_path.display()))
}

#[cfg(target_os = "linux")]
fn install_update_linux(
    app: &AppHandle,
    dest_path: &Path,
    temp_dir: &Path,
    lower: &str,
) -> Result<String, String> {
    use std::os::unix::fs::PermissionsExt;

    if lower.ends_with(".appimage") {
        fs::set_permissions(dest_path, fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("Cấp quyền thực thi AppImage thất bại: {e}"))?;

        if let Some(current_appimage) = std::env::var_os("APPIMAGE").map(PathBuf::from) {
            if let Some(parent) = current_appimage.parent() {
                if is_dir_writable(parent) {
                    let stage_target = parent.join(format!(
                        ".onlydevs-update-{}.AppImage",
                        std::process::id()
                    ));
                    let src_q = shell_single_quote(&dest_path.to_string_lossy());
                    let stage_q = shell_single_quote(&stage_target.to_string_lossy());
                    let dst_q = shell_single_quote(&current_appimage.to_string_lossy());
                    let script_path = temp_dir.join("apply-update.sh");
                    let script_body = format!(
                        "#!/bin/sh\nsleep 1.5\ncp -f {src_q} {stage_q} && chmod +x {stage_q} && mv -f {stage_q} {dst_q} && nohup {dst_q} >/dev/null 2>&1 &\nrm -f -- {src_q} \"$0\"\n"
                    );
                    fs::write(&script_path, script_body)
                        .map_err(|e| format!("Tạo script cập nhật AppImage: {e}"))?;
                    let _ = fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755));
                    Command::new("sh")
                        .arg(&script_path)
                        .spawn()
                        .map_err(|e| format!("Không thể chạy script cập nhật AppImage: {e}"))?;
                    app.exit(0);
                    return Ok("Đang cập nhật AppImage và khởi động lại...".into());
                }
            }
        }

        Command::new(dest_path)
            .spawn()
            .map_err(|e| format!("Không thể khởi chạy AppImage mới: {e}"))?;
        app.exit(0);
        return Ok("Đã tải và khởi chạy AppImage phiên bản mới.".into());
    }

    if lower.ends_with(".deb") || lower.ends_with(".rpm") {
        let has_gui = std::env::var_os("DISPLAY").is_some()
            || std::env::var_os("WAYLAND_DISPLAY").is_some();
        if has_gui && which::which("pkexec").is_ok() {
            let pkg_q = shell_single_quote(&dest_path.to_string_lossy());
            let install_cmd = if lower.ends_with(".deb") {
                format!("apt-get install -y {pkg_q} || (dpkg -i {pkg_q} && apt-get install -f -y)")
            } else {
                format!("dnf install -y {pkg_q} || rpm -Uvh --replacepkgs {pkg_q}")
            };
            let status = Command::new("pkexec")
                .args(["sh", "-c", &install_cmd])
                .status()
                .map_err(|e| format!("Không thể chạy pkexec: {e}"))?;
            if status.success() {
                for bin_name in ["onlydevs-antigravity", "OnlyDEVS-Antigravity"] {
                    if let Ok(bin_path) = which::which(bin_name) {
                        let _ = Command::new(bin_path).spawn();
                        break;
                    }
                }
                app.exit(0);
                return Ok("Đã cài đặt gói cập nhật và khởi động lại ứng dụng.".into());
            }
        }

        if which::which("xdg-open").is_ok() {
            Command::new("xdg-open")
                .arg(dest_path)
                .spawn()
                .map_err(|e| format!("Không thể mở trình cài đặt gói: {e}"))?;
            return Ok(format!(
                "Đã tải về {}. Hãy xác nhận cài đặt trong trình quản lý gói.",
                dest_path.file_name().and_then(|n| n.to_str()).unwrap_or("package")
            ));
        }

        return Err(format!(
            "Đã tải gói cài đặt tại {}, nhưng không tìm thấy pkexec/xdg-open. Vui lòng cài thủ công.",
            dest_path.display()
        ));
    }

    if which::which("xdg-open").is_ok() {
        let _ = Command::new("xdg-open").arg(dest_path).spawn();
    }
    Ok(format!("Đã tải bản cập nhật tại {}", dest_path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_assets() -> Vec<Value> {
        vec![
            json!({"name": "OnlyDEVS-Antigravity_1.2.16_x64-setup.exe", "browser_download_url": "https://example/win-setup.exe", "size": 100}),
            json!({"name": "OnlyDEVS-Antigravity_1.2.16_x64_en-US.msi", "browser_download_url": "https://example/win.msi", "size": 101}),
            json!({"name": "OnlyDEVS-Antigravity_aarch64.app.tar.gz", "browser_download_url": "https://example/mac-arm.app.tar.gz", "size": 200}),
            json!({"name": "OnlyDEVS-Antigravity_aarch64.app.tar.gz.sig", "browser_download_url": "https://example/mac-arm.app.tar.gz.sig", "size": 10}),
            json!({"name": "OnlyDEVS-Antigravity_1.2.16_aarch64.dmg", "browser_download_url": "https://example/mac-arm.dmg", "size": 201}),
            json!({"name": "OnlyDEVS-Antigravity_x64.app.tar.gz", "browser_download_url": "https://example/mac-x64.app.tar.gz", "size": 202}),
            json!({"name": "OnlyDEVS-Antigravity_1.2.16_x64.dmg", "browser_download_url": "https://example/mac-x64.dmg", "size": 203}),
            json!({"name": "OnlyDEVS-Antigravity_1.2.16_amd64.AppImage", "browser_download_url": "https://example/linux-amd64.AppImage", "size": 300}),
            json!({"name": "OnlyDEVS-Antigravity_1.2.16_amd64.deb", "browser_download_url": "https://example/linux-amd64.deb", "size": 301}),
            json!({"name": "OnlyDEVS-Antigravity-1.2.16-1.x86_64.rpm", "browser_download_url": "https://example/linux-x86_64.rpm", "size": 302}),
        ]
    }

    #[test]
    fn picks_correct_asset_for_windows_macos_and_linux() {
        let assets = sample_assets();

        let (u, n, _, inst) = pick_asset_for_platform(&assets, "windows", "x86_64", false, false, false);
        assert_eq!(u.as_deref(), Some("https://example/win-setup.exe"));
        assert_eq!(n.as_deref(), Some("OnlyDEVS-Antigravity_1.2.16_x64-setup.exe"));
        assert!(inst);

        let (u, _, _, inst) = pick_asset_for_platform(&assets, "macos", "aarch64", false, false, false);
        assert_eq!(u.as_deref(), Some("https://example/mac-arm.app.tar.gz"));
        assert!(inst);

        let (u, _, _, inst) = pick_asset_for_platform(&assets, "macos", "x86_64", false, false, false);
        assert_eq!(u.as_deref(), Some("https://example/mac-x64.app.tar.gz"));
        assert!(inst);

        let (u, _, _, inst) = pick_asset_for_platform(&assets, "linux", "x86_64", true, true, false);
        assert_eq!(u.as_deref(), Some("https://example/linux-amd64.AppImage"));
        assert!(inst);

        let (u, _, _, _) = pick_asset_for_platform(&assets, "linux", "x86_64", false, true, false);
        assert_eq!(u.as_deref(), Some("https://example/linux-amd64.deb"));

        let (u, _, _, _) = pick_asset_for_platform(&assets, "linux", "x86_64", false, false, true);
        assert_eq!(u.as_deref(), Some("https://example/linux-x86_64.rpm"));
    }

    #[test]
    fn manifest_fallback_and_shell_quoting() {
        let manifest = json!({
            "platforms": {
                "windows-x86_64": {"url": "https://example/win.exe"},
                "darwin-aarch64": {"url": "https://example/mac-arm.app.tar.gz"},
                "linux-x86_64": {"url": "https://example/linux.AppImage"}
            }
        });
        assert_eq!(
            pick_manifest_platform_url_for(&manifest, "macos", "aarch64").as_deref(),
            Some("https://example/mac-arm.app.tar.gz")
        );
        assert_eq!(
            pick_manifest_platform_url_for(&manifest, "linux", "x86_64").as_deref(),
            Some("https://example/linux.AppImage")
        );
        assert_eq!(
            shell_single_quote("/Applications/My 'App'.app"),
            "'/Applications/My '\"'\"'App'\"'\"'.app'"
        );
    }
}
