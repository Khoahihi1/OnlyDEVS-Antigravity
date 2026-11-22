use base64::{engine::general_purpose, Engine as _};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::time::{timeout, Duration};
use url::Url;
use uuid::Uuid;

use crate::paths;
use crate::prompts::open_db;

// ============================================================================
// Constants (from cockpit-tools Antigravity OAuth & Cloud Code)
// ============================================================================

const CLIENT_ID: &str = "1071006060591-tmhssin2h21lcre235vtolojh4g403ep.apps.googleusercontent.com";
const CLIENT_SECRET: &str = "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";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const USERINFO_URL: &str = "https://www.googleapis.com/oauth2/v2/userinfo";
const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const NATIVE_OAUTH_USER_AGENT: &str = "vscode/1.X.X (Antigravity/4.3.0)";

const CLOUD_CODE_DAILY_BASE_URL: &str = "https://daily-cloudcode-pa.googleapis.com";
const CLOUD_CODE_PROD_BASE_URL: &str = "https://cloudcode-pa.googleapis.com";
const LOAD_CODE_ASSIST_PATH: &str = "v1internal:loadCodeAssist";
const ONBOARD_USER_PATH: &str = "v1internal:onboardUser";
const FETCH_AVAILABLE_MODELS_PATH: &str = "v1internal:fetchAvailableModels";
const RETRIEVE_QUOTA_SUMMARY_PATH: &str = "v1internal:retrieveUserQuotaSummary";

const DEFAULT_CLOUD_CODE_USER_AGENT: &str =
    "antigravity/2.25.0 (aidev_client; os_type=windows; arch=amd64; cl=1003475615)";
const DEFAULT_LOAD_CODE_ASSIST_USER_AGENT: &str =
    "antigravity/2.25.0 windows/amd64 google-api-nodejs-client/10.3.0";
const DEFAULT_X_GOOG_API_CLIENT: &str = "gl-node/22.21.1";

const DESKTOP_FALLBACK_FILE: &str = "jetski-standalone-oauth-token";
const CLI_OAUTH_TOKEN_FILE: &str = "antigravity-oauth-token";

// ============================================================================
// Models
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenData {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i64,
    pub expiry_timestamp: i64,
    pub token_type: String,
    pub email: Option<String>,
    pub project_id: Option<String>,
    pub is_gcp_tos: Option<bool>,
    pub id_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreditInfo {
    pub credit_type: String,
    pub credit_amount: Option<String>,
    pub minimum_credit_amount_for_usage: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelQuota {
    pub name: String,
    pub display_name: Option<String>,
    pub percentage: i32,
    pub reset_time: String,
    pub summary_exhausted: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaData {
    pub models: Vec<ModelQuota>,
    pub last_updated: i64,
    pub is_forbidden: bool,
    pub subscription_tier: Option<String>,
    pub credits: Vec<CreditInfo>,
    pub is_gcp_tos: Option<bool>,
    pub project_id: Option<String>,
    #[serde(default)]
    pub quota_summary_stale: bool,
    #[serde(default)]
    pub quota_summary_updated_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountGroup {
    pub id: String,
    pub name: String,
    pub account_ids: Vec<String>,
    pub sort_order: i64,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaErrorInfo {
    pub code: Option<u16>,
    pub message: String,
    pub reason: Option<String>,
    pub validation_url: Option<String>,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AntigravityAccount {
    pub id: String,
    pub email: String,
    pub name: Option<String>,
    pub picture: Option<String>,
    pub tags: Vec<String>,
    pub notes: Option<String>,
    pub two_factor_secret: Option<String>,
    pub account_password: Option<String>,
    pub phone_number: Option<String>,
    pub mail_url: Option<String>,
    pub aux_email: Option<String>,
    pub pending_oauth: bool,
    pub disabled: bool,
    pub disabled_reason: Option<String>,
    pub token: TokenData,
    pub quota: Option<QuotaData>,
    pub quota_error: Option<QuotaErrorInfo>,
    pub created_at: i64,
    pub last_used: i64,
    pub last_switched_at: Option<i64>,
    #[serde(default)]
    pub is_current: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchHistoryItem {
    pub id: String,
    pub timestamp: i64,
    pub account_id: String,
    pub target_email: String,
    pub from_email: Option<String>,
    pub switch_target: String,
    pub success: bool,
    pub duration_ms: u64,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshStats {
    pub total: usize,
    pub success: usize,
    pub failed: usize,
    pub details: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelTokenUsage {
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub turns: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsageSummary {
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cache_read_tokens: u64,
    pub total_conversations: usize,
    pub total_turns: usize,
    pub models: Vec<ModelTokenUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthPrepareResult {
    pub auth_url: String,
    pub redirect_uri: String,
    pub state: String,
}

// ============================================================================
// SQLite Schema & Storage
// ============================================================================

fn ensure_accounts_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS antigravity_accounts (
            id TEXT PRIMARY KEY,
            email TEXT NOT NULL UNIQUE,
            data_json TEXT NOT NULL,
            updated_at INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS account_switch_history (
            id TEXT PRIMARY KEY,
            timestamp INTEGER NOT NULL,
            account_id TEXT NOT NULL,
            target_email TEXT NOT NULL,
            from_email TEXT,
            switch_target TEXT NOT NULL,
            success INTEGER NOT NULL,
            duration_ms INTEGER NOT NULL,
            message TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS account_groups (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            account_ids_json TEXT NOT NULL,
            sort_order INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL
        );
        ",
    )
    .map_err(|e| format!("Init accounts schema: {e}"))
}

fn get_current_account_id(conn: &Connection) -> Option<String> {
    conn.query_row(
        "SELECT value FROM app_settings WHERE key = 'current_antigravity_account_id'",
        [],
        |r| r.get(0),
    )
    .ok()
}

fn set_current_account_id(conn: &Connection, account_id: &str) -> Result<(), String> {
    conn.execute(
        "INSERT INTO app_settings (key, value) VALUES ('current_antigravity_account_id', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![account_id],
    )
    .map_err(|e| format!("Save current_account_id: {e}"))?;
    Ok(())
}

fn save_account_to_db(conn: &Connection, account: &AntigravityAccount) -> Result<(), String> {
    ensure_accounts_schema(conn)?;
    let raw = serde_json::to_string(account).map_err(|e| format!("Serialize account: {e}"))?;
    conn.execute(
        "INSERT INTO antigravity_accounts (id, email, data_json, updated_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(id) DO UPDATE SET
           email = excluded.email,
           data_json = excluded.data_json,
           updated_at = excluded.updated_at",
        params![
            account.id,
            account.email.to_lowercase(),
            raw,
            Utc::now().timestamp()
        ],
    )
    .map_err(|e| format!("Upsert account DB: {e}"))?;
    Ok(())
}

fn load_account_by_id(conn: &Connection, id: &str) -> Result<AntigravityAccount, String> {
    ensure_accounts_schema(conn)?;
    let current_id = get_current_account_id(conn);
    let raw: String = conn
        .query_row(
            "SELECT data_json FROM antigravity_accounts WHERE id = ?1",
            params![id],
            |r| r.get(0),
        )
        .map_err(|_| format!("Không tìm thấy tài khoản {id}"))?;
    let mut acc: AntigravityAccount =
        serde_json::from_str(&raw).map_err(|e| format!("Parse account: {e}"))?;
    acc.is_current = current_id.as_deref() == Some(acc.id.as_str());
    Ok(acc)
}

fn find_account_by_email_db(
    conn: &Connection,
    email: &str,
) -> Result<Option<AntigravityAccount>, String> {
    ensure_accounts_schema(conn)?;
    let current_id = get_current_account_id(conn);
    let raw: Option<String> = conn
        .query_row(
            "SELECT data_json FROM antigravity_accounts WHERE lower(email) = lower(?1)",
            params![email.trim()],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| format!("Query account by email: {e}"))?;
    if let Some(json_str) = raw {
        let mut acc: AntigravityAccount =
            serde_json::from_str(&json_str).map_err(|e| format!("Parse account: {e}"))?;
        acc.is_current = current_id.as_deref() == Some(acc.id.as_str());
        Ok(Some(acc))
    } else {
        Ok(None)
    }
}

pub fn list_accounts_sync() -> Result<Vec<AntigravityAccount>, String> {
    let conn = open_db()?;
    ensure_accounts_schema(&conn)?;
    let current_id = get_current_account_id(&conn);
    let mut stmt = conn
        .prepare("SELECT data_json FROM antigravity_accounts ORDER BY updated_at DESC")
        .map_err(|e| format!("Prepare list_accounts: {e}"))?;
    let rows = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| format!("Query list_accounts: {e}"))?;
    let mut out = Vec::new();
    for raw in rows.flatten() {
        if let Ok(mut acc) = serde_json::from_str::<AntigravityAccount>(&raw) {
            acc.is_current = current_id.as_deref() == Some(acc.id.as_str());
            out.push(acc);
        }
    }
    out.sort_by(|a, b| {
        b.is_current
            .cmp(&a.is_current)
            .then_with(|| b.last_used.cmp(&a.last_used))
    });
    Ok(out)
}

// ============================================================================
// Windows Credential Manager & Local Antigravity Storage (from cockpit-tools)
// ============================================================================

#[cfg(target_os = "windows")]
fn read_windows_antigravity_credential() -> Result<Option<String>, String> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::ptr;

    #[repr(C)]
    struct FileTime {
        dw_low_date_time: u32,
        dw_high_date_time: u32,
    }

    #[repr(C)]
    struct CredentialW {
        flags: u32,
        cred_type: u32,
        target_name: *const u16,
        comment: *const u16,
        last_written: FileTime,
        credential_blob_size: u32,
        credential_blob: *const u8,
        persist: u32,
        attribute_count: u32,
        attributes: *const std::ffi::c_void,
        target_alias: *const u16,
        user_name: *const u16,
    }

    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn CredReadW(
            target_name: *const u16,
            type_: u32,
            flags: u32,
            credential: *mut *mut CredentialW,
        ) -> i32;
        fn CredFree(buffer: *mut std::ffi::c_void);
    }

    const CRED_TYPE_GENERIC: u32 = 1;
    const ERROR_NOT_FOUND: i32 = 1168;

    let target_wide: Vec<u16> = OsStr::new("gemini:antigravity")
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut credential_ptr: *mut CredentialW = ptr::null_mut();

    unsafe {
        if CredReadW(
            target_wide.as_ptr(),
            CRED_TYPE_GENERIC,
            0,
            &mut credential_ptr,
        ) == 0
        {
            let err = std::io::Error::last_os_error();
            if err.raw_os_error() == Some(ERROR_NOT_FOUND) {
                return Ok(None);
            }
            return Err(format!("Đọc Windows Credential Manager thất bại: {err}"));
        }
        if credential_ptr.is_null() {
            return Ok(None);
        }
        let cred = &*credential_ptr;
        let secret = if cred.credential_blob.is_null() || cred.credential_blob_size == 0 {
            String::new()
        } else {
            let bytes =
                std::slice::from_raw_parts(cred.credential_blob, cred.credential_blob_size as usize);
            String::from_utf8_lossy(bytes).trim().to_string()
        };
        CredFree(credential_ptr.cast());
        if secret.is_empty() {
            Ok(None)
        } else {
            Ok(Some(secret))
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn read_windows_antigravity_credential() -> Result<Option<String>, String> {
    Ok(None)
}

#[cfg(target_os = "windows")]
fn write_windows_antigravity_credential(payload_json: &str) -> Result<(), String> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::ptr;

    #[repr(C)]
    struct FileTime {
        dw_low_date_time: u32,
        dw_high_date_time: u32,
    }

    #[repr(C)]
    struct CredentialW {
        flags: u32,
        cred_type: u32,
        target_name: *const u16,
        comment: *const u16,
        last_written: FileTime,
        credential_blob_size: u32,
        credential_blob: *const u8,
        persist: u32,
        attribute_count: u32,
        attributes: *const std::ffi::c_void,
        target_alias: *const u16,
        user_name: *const u16,
    }

    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn CredWriteW(credential: *const CredentialW, flags: u32) -> i32;
    }

    const CRED_TYPE_GENERIC: u32 = 1;
    const CRED_PERSIST_LOCAL_MACHINE: u32 = 2;

    let target_wide: Vec<u16> = OsStr::new("gemini:antigravity")
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let user_wide: Vec<u16> = OsStr::new("antigravity")
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let secret = payload_json.as_bytes();
    if secret.len() > 5 * 512 {
        return Err("Credential payload vượt quá giới hạn Windows Credential Manager".into());
    }

    let credential = CredentialW {
        flags: 0,
        cred_type: CRED_TYPE_GENERIC,
        target_name: target_wide.as_ptr(),
        comment: ptr::null(),
        last_written: FileTime {
            dw_low_date_time: 0,
            dw_high_date_time: 0,
        },
        credential_blob_size: secret.len() as u32,
        credential_blob: secret.as_ptr(),
        persist: CRED_PERSIST_LOCAL_MACHINE,
        attribute_count: 0,
        attributes: ptr::null(),
        target_alias: ptr::null(),
        user_name: user_wide.as_ptr(),
    };

    unsafe {
        if CredWriteW(&credential, 0) == 0 {
            let err = std::io::Error::last_os_error();
            return Err(format!("Ghi Windows Credential Manager thất bại: {err}"));
        }
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn write_windows_antigravity_credential(_payload_json: &str) -> Result<(), String> {
    Ok(())
}

// ============================================================================
// Protobuf Helpers for Antigravity state.vscdb (ported from cockpit-tools)
// ============================================================================

fn encode_varint(mut value: u64) -> Vec<u8> {
    let mut buf = Vec::new();
    while value >= 0x80 {
        buf.push(((value & 0x7F) | 0x80) as u8);
        value >>= 7;
    }
    buf.push(value as u8);
    buf
}

fn encode_len_delim_field(field_num: u32, data: &[u8]) -> Vec<u8> {
    let tag = (field_num << 3) | 2;
    let mut f = encode_varint(tag as u64);
    f.extend(encode_varint(data.len() as u64));
    f.extend_from_slice(data);
    f
}

fn encode_string_field(field_num: u32, value: &str) -> Vec<u8> {
    encode_len_delim_field(field_num, value.as_bytes())
}

fn encode_varint_field(field_num: u32, value: u64) -> Vec<u8> {
    let tag = (field_num << 3) | 0;
    let mut field = encode_varint(tag as u64);
    field.extend(encode_varint(value));
    field
}

fn read_varint(data: &[u8], offset: usize) -> Result<(u64, usize), String> {
    let mut result = 0u64;
    let mut shift = 0;
    let mut pos = offset;
    loop {
        if pos >= data.len() {
            return Err("Incomplete varint".into());
        }
        let byte = data[pos];
        if shift == 63 && byte > 1 {
            return Err("Varint overflow".into());
        }
        result |= ((byte & 0x7F) as u64) << shift;
        pos += 1;
        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
    }
    Ok((result, pos))
}

fn skip_field(data: &[u8], offset: usize, wire_type: u8) -> Result<usize, String> {
    match wire_type {
        0 => Ok(read_varint(data, offset)?.1),
        1 => Ok(offset + 8),
        2 => {
            let (len, content_offset) = read_varint(data, offset)?;
            Ok(content_offset + (len as usize))
        }
        5 => Ok(offset + 4),
        _ => Err(format!("Unknown wire_type {wire_type}")),
    }
}

fn extract_refresh_token_from_oauth_info(data: &[u8]) -> Option<String> {
    let mut offset = 0;
    while offset < data.len() {
        let (tag, new_offset) = read_varint(data, offset).ok()?;
        let wire_type = (tag & 7) as u8;
        let field_num = (tag >> 3) as u32;
        if field_num == 3 && wire_type == 2 {
            let (length, content_offset) = read_varint(data, new_offset).ok()?;
            let length = length as usize;
            if content_offset + length <= data.len() {
                let token =
                    String::from_utf8_lossy(&data[content_offset..content_offset + length])
                        .trim()
                        .to_string();
                if !token.is_empty() {
                    return Some(token);
                }
            }
        }
        offset = skip_field(data, new_offset, wire_type).ok()?;
    }
    None
}

fn extract_refresh_token_from_unified_entry(data: &[u8]) -> Option<String> {
    let mut offset = 0;
    let mut sentinel_matched = false;
    let mut row_data: Option<Vec<u8>> = None;
    while offset < data.len() {
        let (tag, new_offset) = read_varint(data, offset).ok()?;
        let wire_type = (tag & 7) as u8;
        let field_num = (tag >> 3) as u32;
        if wire_type == 2 {
            let (length, content_offset) = read_varint(data, new_offset).ok()?;
            let length = length as usize;
            if content_offset + length > data.len() {
                return None;
            }
            let slice = &data[content_offset..content_offset + length];
            if field_num == 1 && slice == b"oauthTokenInfoSentinelKey" {
                sentinel_matched = true;
            } else if field_num == 2 {
                row_data = Some(slice.to_vec());
            }
        }
        offset = skip_field(data, new_offset, wire_type).ok()?;
    }
    if sentinel_matched {
        if let Some(row) = row_data {
            let mut r_off = 0;
            while r_off < row.len() {
                let (tag, new_off) = read_varint(&row, r_off).ok()?;
                let wt = (tag & 7) as u8;
                let fn_num = (tag >> 3) as u32;
                if fn_num == 1 && wt == 2 {
                    let (len, c_off) = read_varint(&row, new_off).ok()?;
                    let len = len as usize;
                    if c_off + len <= row.len() {
                        let b64_str = std::str::from_utf8(&row[c_off..c_off + len]).ok()?;
                        let decoded = general_purpose::STANDARD.decode(b64_str).ok()?;
                        return extract_refresh_token_from_oauth_info(&decoded);
                    }
                }
                r_off = skip_field(&row, new_off, wt).ok()?;
            }
        }
    }
    None
}

fn extract_refresh_token_from_unified_oauth_token(data: &[u8]) -> Option<String> {
    let mut offset = 0;
    while offset < data.len() {
        let (tag, new_offset) = read_varint(data, offset).ok()?;
        let wire_type = (tag & 7) as u8;
        let field_num = (tag >> 3) as u32;
        if field_num == 1 && wire_type == 2 {
            let (length, content_offset) = read_varint(data, new_offset).ok()?;
            let length = length as usize;
            if content_offset + length <= data.len() {
                let entry = &data[content_offset..content_offset + length];
                if let Some(rt) = extract_refresh_token_from_unified_entry(entry) {
                    return Some(rt);
                }
            }
        }
        offset = skip_field(data, new_offset, wire_type).ok()?;
    }
    None
}

fn create_oauth_info_with_metadata(
    access_token: &str,
    refresh_token: &str,
    expiry: i64,
    is_gcp_tos: Option<bool>,
    id_token: Option<&str>,
) -> Vec<u8> {
    let mut timestamp_msg = encode_varint((1 << 3) | 0);
    timestamp_msg.extend(encode_varint(expiry.max(0) as u64));
    timestamp_msg.extend(encode_varint_field(2, 0));

    let mut oauth_info = Vec::new();
    oauth_info.extend(encode_string_field(1, access_token));
    oauth_info.extend(encode_string_field(2, "Bearer"));
    oauth_info.extend(encode_string_field(3, refresh_token));
    oauth_info.extend(encode_len_delim_field(4, &timestamp_msg));
    if let Some(id_tok) = id_token.map(str::trim).filter(|v| !v.is_empty()) {
        oauth_info.extend(encode_string_field(5, id_tok));
    }
    if is_gcp_tos.unwrap_or(false) {
        oauth_info.extend(encode_varint_field(6, 1));
    }
    oauth_info
}

fn create_unified_topic_entry(sentinel_key: &str, payload: &[u8]) -> Vec<u8> {
    let row = encode_string_field(1, &general_purpose::STANDARD.encode(payload));
    let entry = [
        encode_string_field(1, sentinel_key),
        encode_len_delim_field(2, &row),
    ]
    .concat();
    encode_len_delim_field(1, &entry)
}

fn antigravity_state_db_candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(appdata) = std::env::var("APPDATA") {
        let base = PathBuf::from(appdata);
        for folder in ["Antigravity IDE", "Antigravity"] {
            let db = base
                .join(folder)
                .join("User")
                .join("globalStorage")
                .join("state.vscdb");
            if db.is_file() {
                out.push(db);
            }
        }
    }
    out
}

fn inject_account_into_state_db(db_path: &Path, account: &AntigravityAccount) -> Result<(), String> {
    let conn = Connection::open(db_path).map_err(|e| format!("Mở state.vscdb: {e}"))?;
    let is_gcp_tos = account
        .token
        .is_gcp_tos
        .or_else(|| account.quota.as_ref().and_then(|q| q.is_gcp_tos));
    let oauth_info = create_oauth_info_with_metadata(
        &account.token.access_token,
        &account.token.refresh_token,
        account.token.expiry_timestamp,
        is_gcp_tos,
        account.token.id_token.as_deref(),
    );
    let topic = create_unified_topic_entry("oauthTokenInfoSentinelKey", &oauth_info);
    let topic_b64 = general_purpose::STANDARD.encode(&topic);
    let _ = conn.execute(
        "INSERT OR REPLACE INTO ItemTable (key, value) VALUES (?1, ?2)",
        params!["antigravityUnifiedStateSync.oauthToken", topic_b64],
    );

    let user_payload = [
        encode_string_field(3, &account.email),
        encode_string_field(7, &account.email),
    ]
    .concat();
    let user_topic = create_unified_topic_entry("userStatusSentinelKey", &user_payload);
    let user_b64 = general_purpose::STANDARD.encode(&user_topic);
    let _ = conn.execute(
        "INSERT OR REPLACE INTO ItemTable (key, value) VALUES (?1, ?2)",
        params!["antigravityUnifiedStateSync.userStatus", user_b64],
    );
    let _ = conn.execute(
        "INSERT OR REPLACE INTO ItemTable (key, value) VALUES ('antigravityOnboarding', 'true')",
        [],
    );
    Ok(())
}

fn parse_refresh_token_from_credential_json(raw: &str) -> Option<String> {
    let val: Value = serde_json::from_str(raw.trim()).ok()?;
    val.get("token")
        .and_then(|t| t.get("refresh_token"))
        .or_else(|| val.get("refresh_token"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn read_local_antigravity_refresh_token() -> Result<(String, &'static str), String> {
    // 1. Windows Credential Manager ("gemini:antigravity")
    if let Ok(Some(secret)) = read_windows_antigravity_credential() {
        if let Some(rt) = parse_refresh_token_from_credential_json(&secret) {
            return Ok((rt, "Windows Credential Manager (gemini:antigravity)"));
        }
    }
    // 2. ~/.gemini/jetski-standalone-oauth-token
    if let Ok(gemini) = paths::gemini_home() {
        let fallback = gemini.join(DESKTOP_FALLBACK_FILE);
        if fallback.is_file() {
            if let Ok(raw) = fs::read_to_string(&fallback) {
                if let Some(rt) = parse_refresh_token_from_credential_json(&raw) {
                    return Ok((rt, "~/.gemini/jetski-standalone-oauth-token"));
                }
            }
        }
        // 3. ~/.gemini/antigravity-cli/antigravity-oauth-token
        let cli_tok = gemini.join("antigravity-cli").join(CLI_OAUTH_TOKEN_FILE);
        if cli_tok.is_file() {
            if let Ok(raw) = fs::read_to_string(&cli_tok) {
                if let Some(rt) = parse_refresh_token_from_credential_json(&raw) {
                    return Ok((rt, "~/.gemini/antigravity-cli/antigravity-oauth-token"));
                }
            }
        }
    }
    // 4. %APPDATA%/Antigravity*/User/globalStorage/state.vscdb
    for db_path in antigravity_state_db_candidates() {
        if let Ok(conn) = Connection::open(&db_path) {
            if let Ok(Some(b64_val)) = conn
                .query_row(
                    "SELECT value FROM ItemTable WHERE key = 'antigravityUnifiedStateSync.oauthToken'",
                    [],
                    |r| r.get::<_, String>(0),
                )
                .optional()
            {
                if let Ok(blob) = general_purpose::STANDARD.decode(b64_val.trim()) {
                    if let Some(rt) = extract_refresh_token_from_unified_oauth_token(&blob) {
                        return Ok((rt, "Antigravity IDE state.vscdb"));
                    }
                }
            }
        }
    }
    Err("Không tìm thấy phiên đăng nhập Antigravity trên máy (Credential Manager / .gemini / state.vscdb)".into())
}

// ============================================================================
// Google OAuth & Cloud Code Quota HTTP Client
// ============================================================================

#[derive(Debug, Deserialize)]
struct GoogleTokenResponse {
    access_token: String,
    expires_in: i64,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    id_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GoogleUserInfo {
    email: String,
    name: Option<String>,
    given_name: Option<String>,
    family_name: Option<String>,
    picture: Option<String>,
}

impl GoogleUserInfo {
    fn display_name(&self) -> Option<String> {
        if let Some(n) = &self.name {
            if !n.trim().is_empty() {
                return Some(n.trim().to_string());
            }
        }
        match (&self.given_name, &self.family_name) {
            (Some(g), Some(f)) => Some(format!("{g} {f}")),
            (Some(g), None) => Some(g.clone()),
            (None, Some(f)) => Some(f.clone()),
            (None, None) => None,
        }
    }
}

fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(18))
        .build()
        .map_err(|e| format!("Tạo HTTP client: {e}"))
}

async fn refresh_google_access_token(refresh_token: &str) -> Result<GoogleTokenResponse, String> {
    let client = http_client()?;
    let body = format!(
        "client_id={}&client_secret={}&refresh_token={}&grant_type=refresh_token",
        url::form_urlencoded::byte_serialize(CLIENT_ID.as_bytes()).collect::<String>(),
        url::form_urlencoded::byte_serialize(CLIENT_SECRET.as_bytes()).collect::<String>(),
        url::form_urlencoded::byte_serialize(refresh_token.trim().as_bytes()).collect::<String>(),
    );
    let res = client
        .post(TOKEN_URL)
        .header(reqwest::header::USER_AGENT, NATIVE_OAUTH_USER_AGENT)
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .body(body)
        .send()
        .await
        .map_err(|e| format!("OAuth refresh request lỗi: {e}"))?;

    if !res.status().is_success() {
        let status = res.status();
        let text = res.text().await.unwrap_or_default();
        return Err(format!("OAuth refresh thất bại ({status}): {text}"));
    }
    res.json::<GoogleTokenResponse>()
        .await
        .map_err(|e| format!("Parse token response: {e}"))
}

async fn exchange_google_code(
    code: &str,
    redirect_uri: &str,
) -> Result<GoogleTokenResponse, String> {
    let client = http_client()?;
    let body = format!(
        "client_id={}&client_secret={}&code={}&redirect_uri={}&grant_type=authorization_code",
        url::form_urlencoded::byte_serialize(CLIENT_ID.as_bytes()).collect::<String>(),
        url::form_urlencoded::byte_serialize(CLIENT_SECRET.as_bytes()).collect::<String>(),
        url::form_urlencoded::byte_serialize(code.trim().as_bytes()).collect::<String>(),
        url::form_urlencoded::byte_serialize(redirect_uri.trim().as_bytes()).collect::<String>(),
    );
    let res = client
        .post(TOKEN_URL)
        .header(reqwest::header::USER_AGENT, NATIVE_OAUTH_USER_AGENT)
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .body(body)
        .send()
        .await
        .map_err(|e| format!("OAuth exchange code lỗi: {e}"))?;

    if !res.status().is_success() {
        let status = res.status();
        let text = res.text().await.unwrap_or_default();
        return Err(format!("OAuth code exchange thất bại ({status}): {text}"));
    }
    res.json::<GoogleTokenResponse>()
        .await
        .map_err(|e| format!("Parse exchange response: {e}"))
}

async fn fetch_google_user_info(access_token: &str) -> Result<GoogleUserInfo, String> {
    let client = http_client()?;
    let res = client
        .get(USERINFO_URL)
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| format!("Fetch userinfo lỗi: {e}"))?;
    if !res.status().is_success() {
        let status = res.status();
        let text = res.text().await.unwrap_or_default();
        return Err(format!("Userinfo thất bại ({status}): {text}"));
    }
    res.json::<GoogleUserInfo>()
        .await
        .map_err(|e| format!("Parse userinfo: {e}"))
}

pub fn format_google_validation_url(raw_url: &str, email: &str) -> String {
    let email = email.trim();
    if raw_url.trim().is_empty() || email.is_empty() {
        return raw_url.to_string();
    }
    let Ok(mut parsed) = Url::parse(raw_url) else {
        return raw_url.to_string();
    };
    let Some(host) = parsed.host_str().map(|h| h.to_ascii_lowercase()) else {
        return raw_url.to_string();
    };
    if host != "accounts.google.com" && host != "google.com" && !host.ends_with(".google.com") {
        return raw_url.to_string();
    }
    let mut pairs: Vec<(String, String)> = parsed.query_pairs().into_owned().collect();
    pairs.retain(|(k, _)| k != "authuser" && k != "login_hint" && k != "Email");
    pairs.push(("authuser".into(), email.to_string()));
    pairs.push(("login_hint".into(), email.to_string()));
    pairs.push(("Email".into(), email.to_string()));
    parsed.query_pairs_mut().clear().extend_pairs(pairs);
    parsed.to_string()
}

fn parse_cloud_code_error(status: u16, text: &str, email: &str) -> QuotaErrorInfo {
    let mut message = if text.trim().is_empty() {
        format!("HTTP {status}")
    } else {
        text.to_string()
    };
    let mut reason: Option<String> = None;
    let mut validation_url: Option<String> = None;

    if let Ok(val) = serde_json::from_str::<Value>(text) {
        if let Some(err_obj) = val.get("error") {
            if let Some(m) = err_obj.get("message").and_then(Value::as_str) {
                message = m.to_string();
            }
            if let Some(details) = err_obj.get("details").and_then(Value::as_array) {
                for d in details {
                    if reason.is_none() {
                        reason = d.get("reason").and_then(Value::as_str).map(str::to_string);
                    }
                    if validation_url.is_none() {
                        validation_url = d
                            .get("metadata")
                            .and_then(|m| m.get("validation_url"))
                            .and_then(Value::as_str)
                            .map(|u| format_google_validation_url(u, email));
                    }
                }
            }
        }
    }
    if reason.is_none() {
        if message.contains("Verify your account") {
            reason = Some("VALIDATION_REQUIRED".into());
        } else if message.contains("Subscription required") {
            reason = Some("SUBSCRIPTION_REQUIRED".into());
        }
    }
    QuotaErrorInfo {
        code: Some(status),
        message,
        reason,
        validation_url,
        timestamp: Utc::now().timestamp(),
    }
}

fn build_cloud_code_metadata(project_id: Option<&str>) -> Value {
    let mut map = serde_json::Map::new();
    map.insert("ideName".into(), Value::String("antigravity".into()));
    map.insert("ideType".into(), Value::String("ANTIGRAVITY".into()));
    map.insert("ideVersion".into(), Value::String("2.25.0".into()));
    map.insert("pluginVersion".into(), Value::String("1.4.2".into()));
    map.insert("platform".into(), Value::String("WINDOWS_AMD64".into()));
    map.insert("updateChannel".into(), Value::String("stable".into()));
    map.insert("pluginType".into(), Value::String("GEMINI".into()));
    if let Some(p) = project_id.map(str::trim).filter(|s| !s.is_empty()) {
        map.insert("duetProject".into(), Value::String(p.to_string()));
    }
    Value::Object(map)
}

async fn fetch_cloud_code_quota(
    access_token: &str,
    email: &str,
    preferred_project: Option<&str>,
) -> Result<(QuotaData, Option<QuotaErrorInfo>), String> {
    let client = http_client()?;
    let mut subscription_tier: Option<String> = None;
    let mut credits: Vec<CreditInfo> = Vec::new();
    let mut is_gcp_tos: Option<bool> = None;
    let mut project_id: Option<String> = preferred_project
        .map(str::trim)
        .filter(|s| !s.is_empty() && *s != "aicode-consumers")
        .map(str::to_string);

    // Step 1: loadCodeAssist
    let load_payload = json!({
        "metadata": build_cloud_code_metadata(project_id.as_deref()),
        "mode": "FULL_ELIGIBILITY_CHECK"
    });

    if let Ok(res) = client
        .post(format!("{CLOUD_CODE_DAILY_BASE_URL}/{LOAD_CODE_ASSIST_PATH}"))
        .bearer_auth(access_token)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header(reqwest::header::USER_AGENT, DEFAULT_LOAD_CODE_ASSIST_USER_AGENT)
        .header("x-goog-api-client", DEFAULT_X_GOOG_API_CLIENT)
        .json(&load_payload)
        .send()
        .await
    {
        if res.status().is_success() {
            if let Ok(val) = res.json::<Value>().await {
                let paid_id = val
                    .get("paidTier")
                    .and_then(|t| t.get("id"))
                    .and_then(Value::as_str)
                    .map(str::to_string);
                let current_id = val
                    .get("currentTier")
                    .and_then(|t| t.get("id"))
                    .and_then(Value::as_str)
                    .map(str::to_string);
                subscription_tier = paid_id.or(current_id);
                is_gcp_tos = val
                    .get("currentTier")
                    .and_then(|t| t.get("usesGcpTos"))
                    .and_then(Value::as_bool);

                if let Some(avail) = val
                    .get("paidTier")
                    .and_then(|t| t.get("availableCredits"))
                    .and_then(Value::as_array)
                {
                    for c in avail {
                        if let (Some(ct), Some(amt)) = (
                            c.get("creditType").and_then(Value::as_str),
                            c.get("creditAmount").and_then(Value::as_str),
                        ) {
                            credits.push(CreditInfo {
                                credit_type: ct.to_string(),
                                credit_amount: Some(amt.to_string()),
                                minimum_credit_amount_for_usage: c
                                    .get("minimumCreditAmountForUsage")
                                    .and_then(Value::as_str)
                                    .map(str::to_string),
                            });
                        }
                    }
                }

                if let Some(proj_val) = val.get("cloudaicompanionProject") {
                    let extracted = proj_val
                        .as_str()
                        .map(str::to_string)
                        .or_else(|| {
                            proj_val
                                .get("id")
                                .and_then(Value::as_str)
                                .map(str::to_string)
                        })
                        .filter(|p| !p.trim().is_empty() && p != "aicode-consumers");
                    if extracted.is_some() {
                        project_id = extracted;
                    }
                }

                // Auto-onboard if no project_id yet
                if project_id.is_none() {
                    if let Some(tier_id) = subscription_tier.clone().or_else(|| {
                        val.get("allowedTiers")
                            .and_then(Value::as_array)
                            .and_then(|arr| arr.first())
                            .and_then(|t| t.get("id"))
                            .and_then(Value::as_str)
                            .map(str::to_string)
                    }) {
                        let onboard_payload = json!({
                            "tierId": tier_id,
                            "metadata": build_cloud_code_metadata(None)
                        });
                        if let Ok(ob_res) = client
                            .post(format!("{CLOUD_CODE_DAILY_BASE_URL}/{ONBOARD_USER_PATH}"))
                            .bearer_auth(access_token)
                            .header(
                                reqwest::header::USER_AGENT,
                                DEFAULT_LOAD_CODE_ASSIST_USER_AGENT,
                            )
                            .json(&onboard_payload)
                            .send()
                            .await
                        {
                            if let Ok(ob_val) = ob_res.json::<Value>().await {
                                if let Some(p) = ob_val
                                    .get("response")
                                    .and_then(|r| r.get("cloudaicompanionProject"))
                                {
                                    project_id = p
                                        .as_str()
                                        .map(str::to_string)
                                        .or_else(|| {
                                            p.get("id").and_then(Value::as_str).map(str::to_string)
                                        })
                                        .filter(|s| !s.is_empty() && s != "aicode-consumers");
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let base_url = if is_gcp_tos.unwrap_or(false) && project_id.is_some() {
        CLOUD_CODE_PROD_BASE_URL
    } else {
        CLOUD_CODE_DAILY_BASE_URL
    };

    let model_payload = project_id
        .as_ref()
        .map(|id| json!({ "project": id }))
        .unwrap_or_else(|| json!({}));

    let mut models: Vec<ModelQuota> = Vec::new();
    let mut quota_error: Option<QuotaErrorInfo> = None;
    let mut is_forbidden = false;

    // Step 2: fetchAvailableModels
    if let Ok(res) = client
        .post(format!("{base_url}/{FETCH_AVAILABLE_MODELS_PATH}"))
        .bearer_auth(access_token)
        .header(reqwest::header::USER_AGENT, DEFAULT_CLOUD_CODE_USER_AGENT)
        .json(&model_payload)
        .send()
        .await
    {
        let status = res.status();
        let text = res.text().await.unwrap_or_default();
        if status.is_success() {
            if let Ok(val) = serde_json::from_str::<Value>(&text) {
                if let Some(m_map) = val.get("models").and_then(Value::as_object) {
                    for (name, info) in m_map {
                        if !name.contains("gemini") && !name.contains("claude") {
                            continue;
                        }
                        if let Some(q_info) = info.get("quotaInfo") {
                            let fraction = q_info
                                .get("remainingFraction")
                                .and_then(Value::as_f64)
                                .unwrap_or(0.0);
                            let reset_time = q_info
                                .get("resetTime")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string();
                            let display_name = info
                                .get("displayName")
                                .and_then(Value::as_str)
                                .map(str::to_string);
                            models.push(ModelQuota {
                                name: name.clone(),
                                display_name,
                                percentage: (fraction * 100.0).round() as i32,
                                reset_time,
                                summary_exhausted: None,
                            });
                        }
                    }
                }
            }
        } else {
            if status.as_u16() == 403 {
                is_forbidden = true;
            }
            quota_error = Some(parse_cloud_code_error(status.as_u16(), &text, email));
        }
    }

    // Step 3: retrieveUserQuotaSummary (5h and Weekly buckets!)
    let mut summary_ok = false;
    if let Ok(res) = client
        .post(format!("{base_url}/{RETRIEVE_QUOTA_SUMMARY_PATH}"))
        .bearer_auth(access_token)
        .header(reqwest::header::USER_AGENT, DEFAULT_CLOUD_CODE_USER_AGENT)
        .json(&model_payload)
        .send()
        .await
    {
        let status = res.status();
        let text = res.text().await.unwrap_or_default();
        if status.is_success() {
            if let Ok(val) = serde_json::from_str::<Value>(&text) {
                if let Some(groups) = val.get("groups").and_then(Value::as_array) {
                    let mut got_buckets = 0usize;
                    for g in groups {
                        if let Some(buckets) = g.get("buckets").and_then(Value::as_array) {
                            for b in buckets {
                                let Some(bucket_id) = b.get("bucketId").and_then(Value::as_str)
                                else {
                                    continue;
                                };
                                let Some(fraction) =
                                    b.get("remainingFraction").and_then(Value::as_f64)
                                else {
                                    continue;
                                };
                                let display_name = b
                                    .get("displayName")
                                    .and_then(Value::as_str)
                                    .map(str::to_string);
                                let reset_time = b
                                    .get("resetTime")
                                    .and_then(Value::as_str)
                                    .unwrap_or_default()
                                    .to_string();
                                models.push(ModelQuota {
                                    name: bucket_id.to_string(),
                                    display_name,
                                    percentage: (fraction * 100.0).round() as i32,
                                    reset_time,
                                    summary_exhausted: Some(fraction <= 0.0),
                                });
                                got_buckets += 1;
                            }
                        }
                    }
                    if got_buckets > 0 {
                        summary_ok = true;
                    }
                }
            }
        } else if quota_error.is_none() {
            quota_error = Some(parse_cloud_code_error(status.as_u16(), &text, email));
        }
    }

    models.sort_by(|a, b| a.name.cmp(&b.name));
    let now_ts = Utc::now().timestamp();

    let quota = QuotaData {
        models,
        last_updated: now_ts,
        is_forbidden,
        subscription_tier,
        credits,
        is_gcp_tos,
        project_id,
        quota_summary_stale: !summary_ok,
        quota_summary_updated_at: if summary_ok { Some(now_ts) } else { None },
    };
    Ok((quota, quota_error))
}

// ============================================================================
// Core Account Lifecycle Operations
// ============================================================================

async fn build_or_update_account_from_refresh_token(
    refresh_token: String,
    mark_current: bool,
) -> Result<AntigravityAccount, String> {
    let token_res = refresh_google_access_token(&refresh_token).await?;
    let user_info = fetch_google_user_info(&token_res.access_token).await?;
    let email = user_info.email.trim().to_lowercase();
    let effective_refresh = token_res
        .refresh_token
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(refresh_token);

    let now = Utc::now().timestamp();
    let (quota, quota_error) =
        fetch_cloud_code_quota(&token_res.access_token, &email, None).await?;

    let conn = open_db()?;
    ensure_accounts_schema(&conn)?;
    let existing = find_account_by_email_db(&conn, &email)?;

    let id = existing
        .as_ref()
        .map(|a| a.id.clone())
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    let account = AntigravityAccount {
        id: id.clone(),
        email: email.clone(),
        name: user_info
            .display_name()
            .or_else(|| existing.as_ref().and_then(|a| a.name.clone())),
        picture: user_info
            .picture
            .or_else(|| existing.as_ref().and_then(|a| a.picture.clone())),
        tags: existing.as_ref().map(|a| a.tags.clone()).unwrap_or_default(),
        notes: existing.as_ref().and_then(|a| a.notes.clone()),
        two_factor_secret: existing.as_ref().and_then(|a| a.two_factor_secret.clone()),
        account_password: existing.as_ref().and_then(|a| a.account_password.clone()),
        phone_number: existing.as_ref().and_then(|a| a.phone_number.clone()),
        mail_url: existing.as_ref().and_then(|a| a.mail_url.clone()),
        aux_email: existing.as_ref().and_then(|a| a.aux_email.clone()),
        pending_oauth: false,
        disabled: quota_error
            .as_ref()
            .and_then(|e| e.reason.as_deref())
            .is_some_and(|r| r == "VALIDATION_REQUIRED"),
        disabled_reason: quota_error
            .as_ref()
            .and_then(|e| e.reason.as_deref())
            .and_then(|r| {
                if r == "VALIDATION_REQUIRED" {
                    Some("verification_required".into())
                } else {
                    None
                }
            }),
        token: TokenData {
            access_token: token_res.access_token,
            refresh_token: effective_refresh,
            expires_in: token_res.expires_in,
            expiry_timestamp: now + token_res.expires_in,
            token_type: "Bearer".into(),
            email: Some(email),
            project_id: quota.project_id.clone(),
            is_gcp_tos: quota.is_gcp_tos,
            id_token: token_res.id_token,
        },
        quota: Some(quota),
        quota_error,
        created_at: existing.as_ref().map(|a| a.created_at).unwrap_or(now),
        last_used: now,
        last_switched_at: existing.as_ref().and_then(|a| a.last_switched_at),
        is_current: mark_current || existing.as_ref().is_some_and(|a| a.is_current),
    };

    save_account_to_db(&conn, &account)?;
    if mark_current || get_current_account_id(&conn).is_none() {
        let _ = set_current_account_id(&conn, &id);
    }
    load_account_by_id(&conn, &id)
}

pub async fn import_local_account() -> Result<AntigravityAccount, String> {
    let (refresh_token, _source) = read_local_antigravity_refresh_token()?;
    build_or_update_account_from_refresh_token(refresh_token, true).await
}

pub async fn add_account_by_refresh_token(
    refresh_token_input: String,
) -> Result<Vec<AntigravityAccount>, String> {
    let tokens: Vec<String> = refresh_token_input
        .lines()
        .flat_map(|l| l.split(','))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    if tokens.is_empty() {
        return Err("Refresh token không được để trống".into());
    }
    let mut added = Vec::new();
    let mut errors = Vec::new();
    for tok in tokens {
        match build_or_update_account_from_refresh_token(tok, false).await {
            Ok(acc) => added.push(acc),
            Err(e) => errors.push(e),
        }
    }
    if added.is_empty() {
        return Err(errors.join("\n"));
    }
    Ok(added)
}

pub async fn refresh_single_account_quota(account_id: String) -> Result<AntigravityAccount, String> {
    let mut acc = {
        let conn = open_db()?;
        load_account_by_id(&conn, &account_id)?
    };
    if acc.pending_oauth || acc.token.refresh_token.trim().is_empty() {
        return Err("Tài khoản này chưa có Refresh Token (Pending OAuth)".into());
    }

    let now = Utc::now().timestamp();
    if acc.token.access_token.trim().is_empty() || acc.token.expiry_timestamp - now < 600 {
        let token_res = refresh_google_access_token(&acc.token.refresh_token).await?;
        acc.token.access_token = token_res.access_token;
        acc.token.expires_in = token_res.expires_in;
        acc.token.expiry_timestamp = now + token_res.expires_in;
        if let Some(rt) = token_res.refresh_token.filter(|s| !s.trim().is_empty()) {
            acc.token.refresh_token = rt;
        }
        if token_res.id_token.is_some() {
            acc.token.id_token = token_res.id_token;
        }
    }

    let (quota, quota_error) = fetch_cloud_code_quota(
        &acc.token.access_token,
        &acc.email,
        acc.token.project_id.as_deref(),
    )
    .await?;

    if quota.project_id.is_some() {
        acc.token.project_id = quota.project_id.clone();
    }
    if quota.is_gcp_tos.is_some() {
        acc.token.is_gcp_tos = quota.is_gcp_tos;
    }
    acc.disabled = quota_error
        .as_ref()
        .and_then(|e| e.reason.as_deref())
        .is_some_and(|r| r == "VALIDATION_REQUIRED");
    acc.disabled_reason = if acc.disabled {
        Some("verification_required".into())
    } else {
        None
    };
    acc.quota = Some(quota);
    acc.quota_error = quota_error;
    acc.last_used = now;

    let conn = open_db()?;
    save_account_to_db(&conn, &acc)?;
    load_account_by_id(&conn, &account_id)
}

pub async fn refresh_all_account_quotas() -> Result<RefreshStats, String> {
    let accounts = list_accounts_sync()?;
    let mut success = 0usize;
    let mut failed = 0usize;
    let mut details = Vec::new();
    for acc in &accounts {
        if acc.pending_oauth || acc.token.refresh_token.trim().is_empty() {
            continue;
        }
        match refresh_single_account_quota(acc.id.clone()).await {
            Ok(_) => success += 1,
            Err(e) => {
                failed += 1;
                details.push(format!("{}: {e}", acc.email));
            }
        }
    }
    Ok(RefreshStats {
        total: success + failed,
        success,
        failed,
        details,
    })
}

// ============================================================================
// 1-Click Account Switcher (Windows Credential Manager + .gemini + state.vscdb)
// ============================================================================

pub async fn switch_antigravity_account(
    account_id: String,
    switch_target: Option<String>,
) -> Result<AntigravityAccount, String> {
    let started = std::time::Instant::now();
    let target = switch_target.unwrap_or_else(|| "all".into());

    let (mut acc, previous_email) = {
        let conn = open_db()?;
        let prev = get_current_account_id(&conn)
            .and_then(|cid| load_account_by_id(&conn, &cid).ok())
            .map(|a| a.email);
        (load_account_by_id(&conn, &account_id)?, prev)
    };

    if acc.pending_oauth || acc.token.refresh_token.trim().is_empty() {
        return Err("Tài khoản chưa ủy quyền OAuth, không thể chuyển đổi".into());
    }

    let now = Utc::now().timestamp();
    if acc.token.access_token.trim().is_empty() || acc.token.expiry_timestamp - now < 600 {
        let token_res = refresh_google_access_token(&acc.token.refresh_token).await?;
        acc.token.access_token = token_res.access_token;
        acc.token.expires_in = token_res.expires_in;
        acc.token.expiry_timestamp = now + token_res.expires_in;
        if let Some(rt) = token_res.refresh_token.filter(|s| !s.trim().is_empty()) {
            acc.token.refresh_token = rt;
        }
        if token_res.id_token.is_some() {
            acc.token.id_token = token_res.id_token;
        }
    }

    let expiry_rfc3339 = chrono::DateTime::from_timestamp(acc.token.expiry_timestamp, 0)
        .unwrap_or_else(Utc::now)
        .to_rfc3339_opts(chrono::SecondsFormat::Micros, true);

    let payload_val = json!({
        "token": {
            "access_token": acc.token.access_token,
            "token_type": "Bearer",
            "refresh_token": acc.token.refresh_token,
            "expiry": expiry_rfc3339,
        },
        "auth_method": "consumer",
        "id_token": acc.token.id_token,
    });
    let payload_json =
        serde_json::to_string(&payload_val).map_err(|e| format!("Serialize credential: {e}"))?;

    // 1. Write Windows Credential Manager ("gemini:antigravity")
    if target != "cli" {
        let _ = write_windows_antigravity_credential(&payload_json);
    }

    // 2. Write ~/.gemini/jetski-standalone-oauth-token and ~/.gemini/antigravity-cli/antigravity-oauth-token
    let gemini = paths::gemini_home()?;
    fs::create_dir_all(&gemini).map_err(|e| format!("Tạo .gemini: {e}"))?;
    if target != "cli" {
        let fallback_path = gemini.join(DESKTOP_FALLBACK_FILE);
        fs::write(&fallback_path, &payload_json)
            .map_err(|e| format!("Ghi {DESKTOP_FALLBACK_FILE}: {e}"))?;
    }
    if target == "all" || target == "cli" {
        let cli_dir = gemini.join("antigravity-cli");
        fs::create_dir_all(&cli_dir).map_err(|e| format!("Tạo antigravity-cli: {e}"))?;
        fs::write(cli_dir.join(CLI_OAUTH_TOKEN_FILE), &payload_json)
            .map_err(|e| format!("Ghi {CLI_OAUTH_TOKEN_FILE}: {e}"))?;
    }

    // 3. Inject into %APPDATA%/Antigravity*/User/globalStorage/state.vscdb if present
    if target != "cli" {
        for db_path in antigravity_state_db_candidates() {
            let _ = inject_account_into_state_db(&db_path, &acc);
        }
    }

    acc.last_used = now;
    acc.last_switched_at = Some(now);
    acc.is_current = true;

    let duration_ms = started.elapsed().as_millis() as u64;
    let conn = open_db()?;
    save_account_to_db(&conn, &acc)?;
    set_current_account_id(&conn, &acc.id)?;

    let _ = conn.execute(
        "INSERT INTO account_switch_history (id, timestamp, account_id, target_email, from_email, switch_target, success, duration_ms, message)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7, ?8)",
        params![
            Uuid::new_v4().to_string(),
            now,
            acc.id,
            acc.email,
            previous_email,
            target,
            duration_ms as i64,
            "Đã đồng bộ Credential Manager + .gemini + state.vscdb"
        ],
    );

    load_account_by_id(&conn, &acc.id)
}

pub fn get_switch_history() -> Result<Vec<SwitchHistoryItem>, String> {
    let conn = open_db()?;
    ensure_accounts_schema(&conn)?;
    let mut stmt = conn
        .prepare(
            "SELECT id, timestamp, account_id, target_email, from_email, switch_target, success, duration_ms, message
             FROM account_switch_history
             ORDER BY timestamp DESC
             LIMIT 100",
        )
        .map_err(|e| format!("Prepare switch history: {e}"))?;
    let rows = stmt
        .query_map([], |r| {
            Ok(SwitchHistoryItem {
                id: r.get(0)?,
                timestamp: r.get(1)?,
                account_id: r.get(2)?,
                target_email: r.get(3)?,
                from_email: r.get(4)?,
                switch_target: r.get(5)?,
                success: r.get::<_, i64>(6)? != 0,
                duration_ms: r.get::<_, i64>(7)?.max(0) as u64,
                message: r.get(8)?,
            })
        })
        .map_err(|e| format!("Query switch history: {e}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Row switch history: {e}"))
}

pub fn clear_switch_history() -> Result<(), String> {
    let conn = open_db()?;
    ensure_accounts_schema(&conn)?;
    conn.execute("DELETE FROM account_switch_history", [])
        .map_err(|e| format!("Clear switch history: {e}"))?;
    Ok(())
}

// ============================================================================
// Account Notes, Tags, Deletion, Import & Export
// ============================================================================

pub fn update_account_metadata(
    account_id: String,
    tags: Vec<String>,
    notes: Option<String>,
    two_factor_secret: Option<String>,
    account_password: Option<String>,
    phone_number: Option<String>,
    mail_url: Option<String>,
    aux_email: Option<String>,
) -> Result<AntigravityAccount, String> {
    let conn = open_db()?;
    let mut acc = load_account_by_id(&conn, &account_id)?;
    acc.tags = tags
        .into_iter()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect();
    acc.notes = notes.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    acc.two_factor_secret = two_factor_secret
        .map(|s| s.trim().replace(' ', ""))
        .filter(|s| !s.is_empty());
    acc.account_password = account_password
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    acc.phone_number = phone_number
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    acc.mail_url = mail_url
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    acc.aux_email = aux_email
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    save_account_to_db(&conn, &acc)?;
    load_account_by_id(&conn, &account_id)
}

pub fn delete_accounts_by_ids(account_ids: Vec<String>) -> Result<usize, String> {
    let conn = open_db()?;
    ensure_accounts_schema(&conn)?;
    let mut deleted = 0usize;
    for id in account_ids {
        deleted += conn
            .execute("DELETE FROM antigravity_accounts WHERE id = ?1", params![id])
            .unwrap_or(0);
    }
    Ok(deleted)
}

fn create_pending_account(
    email: String,
    password: Option<String>,
    two_factor: Option<String>,
    aux_email: Option<String>,
    mail_url: Option<String>,
    notes: Option<String>,
    tags: Vec<String>,
) -> Result<AntigravityAccount, String> {
    let conn = open_db()?;
    ensure_accounts_schema(&conn)?;
    let clean_email = email.trim().to_lowercase();
    let now = Utc::now().timestamp();
    let existing = find_account_by_email_db(&conn, &clean_email)?;
    let id = existing
        .as_ref()
        .map(|a| a.id.clone())
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    let acc = AntigravityAccount {
        id: id.clone(),
        email: clean_email.clone(),
        name: existing.as_ref().and_then(|a| a.name.clone()),
        picture: existing.as_ref().and_then(|a| a.picture.clone()),
        tags: if tags.is_empty() {
            existing.as_ref().map(|a| a.tags.clone()).unwrap_or_default()
        } else {
            tags
        },
        notes: notes.or_else(|| existing.as_ref().and_then(|a| a.notes.clone())),
        two_factor_secret: two_factor
            .or_else(|| existing.as_ref().and_then(|a| a.two_factor_secret.clone())),
        account_password: password
            .or_else(|| existing.as_ref().and_then(|a| a.account_password.clone())),
        phone_number: existing.as_ref().and_then(|a| a.phone_number.clone()),
        mail_url: mail_url.or_else(|| existing.as_ref().and_then(|a| a.mail_url.clone())),
        aux_email: aux_email.or_else(|| existing.as_ref().and_then(|a| a.aux_email.clone())),
        pending_oauth: existing.as_ref().map(|a| a.pending_oauth).unwrap_or(true),
        disabled: false,
        disabled_reason: None,
        token: existing
            .as_ref()
            .map(|a| a.token.clone())
            .unwrap_or_else(|| TokenData {
                access_token: String::new(),
                refresh_token: String::new(),
                expires_in: 0,
                expiry_timestamp: 0,
                token_type: "Bearer".into(),
                email: Some(clean_email),
                project_id: None,
                is_gcp_tos: None,
                id_token: None,
            }),
        quota: existing.as_ref().and_then(|a| a.quota.clone()),
        quota_error: None,
        created_at: existing.as_ref().map(|a| a.created_at).unwrap_or(now),
        last_used: now,
        last_switched_at: existing.as_ref().and_then(|a| a.last_switched_at),
        is_current: existing.as_ref().is_some_and(|a| a.is_current),
    };
    save_account_to_db(&conn, &acc)?;
    load_account_by_id(&conn, &id)
}

pub async fn import_accounts_from_content(content: String) -> Result<Vec<AntigravityAccount>, String> {
    let trimmed = content.trim().trim_start_matches('\u{feff}');
    if trimmed.is_empty() {
        return Err("Nội dung import trống".into());
    }

    // Case 1: JSON Array or Single Object
    if trimmed.starts_with('[') || trimmed.starts_with('{') {
        let items: Vec<Value> = if trimmed.starts_with('[') {
            serde_json::from_str(trimmed).map_err(|e| format!("Parse JSON array: {e}"))?
        } else {
            let single: Value =
                serde_json::from_str(trimmed).map_err(|e| format!("Parse JSON object: {e}"))?;
            if let Some(arr) = single.get("accounts").and_then(Value::as_array) {
                arr.clone()
            } else {
                vec![single]
            }
        };

        let mut imported = Vec::new();
        for item in items {
            let refresh_token = item
                .get("refresh_token")
                .or_else(|| item.get("refreshToken"))
                .or_else(|| item.get("token").and_then(|t| t.get("refresh_token")))
                .or_else(|| item.get("token").and_then(|t| t.get("refreshToken")))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string);

            let email = item
                .get("email")
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or_default()
                .to_string();
            let notes = item
                .get("notes")
                .or_else(|| item.get("note"))
                .or_else(|| item.get("remark"))
                .and_then(Value::as_str)
                .map(str::to_string);
            let two_factor = item
                .get("two_factor_secret")
                .or_else(|| item.get("twoFactorSecret"))
                .and_then(Value::as_str)
                .map(str::to_string);
            let password = item
                .get("account_password")
                .or_else(|| item.get("accountPassword"))
                .or_else(|| item.get("password"))
                .and_then(Value::as_str)
                .map(str::to_string);
            let aux_email = item
                .get("aux_email")
                .or_else(|| item.get("auxEmail"))
                .and_then(Value::as_str)
                .map(str::to_string);
            let mail_url = item
                .get("mail_url")
                .or_else(|| item.get("mailUrl"))
                .and_then(Value::as_str)
                .map(str::to_string);
            let tags: Vec<String> = item
                .get("tags")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();

            if let Some(rt) = refresh_token {
                if let Ok(acc) = build_or_update_account_from_refresh_token(rt, false).await {
                    let updated = update_account_metadata(
                        acc.id,
                        if tags.is_empty() { acc.tags } else { tags },
                        notes.or(acc.notes),
                        two_factor.or(acc.two_factor_secret),
                        password.or(acc.account_password),
                        acc.phone_number,
                        mail_url.or(acc.mail_url),
                        aux_email.or(acc.aux_email),
                    )?;
                    imported.push(updated);
                    continue;
                }
            }
            if email.contains('@') {
                imported.push(create_pending_account(
                    email, password, two_factor, aux_email, mail_url, notes, tags,
                )?);
            }
        }
        return Ok(imported);
    }

    // Case 2: Delimited lines (email----password----2fa----refresh_token) or raw refresh tokens
    let mut imported = Vec::new();
    for line in trimmed.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if line.contains("----") {
            let parts: Vec<&str> = line.split("----").map(str::trim).collect();
            if parts.len() >= 3 && parts[0].contains('@') {
                let email = parts[0].to_string();
                let pass = (!parts[1].is_empty()).then(|| parts[1].to_string());
                let (aux, two_fa, rt_or_url) = if parts.len() == 5 {
                    (
                        (!parts[2].is_empty()).then(|| parts[2].to_string()),
                        (!parts[3].is_empty()).then(|| parts[3].to_string()),
                        (!parts[4].is_empty()).then(|| parts[4].to_string()),
                    )
                } else {
                    (
                        None,
                        (!parts[2].is_empty()).then(|| parts[2].to_string()),
                        parts.get(3).copied().filter(|s| !s.is_empty()).map(str::to_string),
                    )
                };

                if let Some(ref tok) = rt_or_url {
                    if tok.starts_with("1//") {
                        if let Ok(acc) =
                            build_or_update_account_from_refresh_token(tok.clone(), false).await
                        {
                            let updated = update_account_metadata(
                                acc.id,
                                acc.tags,
                                acc.notes,
                                two_fa,
                                pass,
                                acc.phone_number,
                                acc.mail_url,
                                aux,
                            )?;
                            imported.push(updated);
                            continue;
                        }
                    }
                }
                imported.push(create_pending_account(
                    email, pass, two_fa, aux, rt_or_url, None, Vec::new(),
                )?);
            }
        } else if line.starts_with("1//") {
            if let Ok(acc) =
                build_or_update_account_from_refresh_token(line.to_string(), false).await
            {
                imported.push(acc);
            }
        }
    }

    if imported.is_empty() {
        return Err("Không nhận diện được tài khoản hợp lệ từ dữ liệu nhập vào".into());
    }
    Ok(imported)
}

pub fn export_accounts_json(
    account_ids: Vec<String>,
    include_sensitive: bool,
) -> Result<String, String> {
    let all = list_accounts_sync()?;
    let filtered: Vec<Value> = all
        .into_iter()
        .filter(|a| account_ids.is_empty() || account_ids.contains(&a.id))
        .map(|a| {
            let mut obj = json!({
                "email": a.email,
                "name": a.name,
                "refresh_token": a.token.refresh_token,
                "tags": a.tags,
                "pending_oauth": a.pending_oauth,
                "subscription_tier": a.quota.as_ref().and_then(|q| q.subscription_tier.clone()),
                "project_id": a.token.project_id,
            });
            if include_sensitive {
                if let Some(map) = obj.as_object_mut() {
                    map.insert("notes".into(), json!(a.notes));
                    map.insert("two_factor_secret".into(), json!(a.two_factor_secret));
                    map.insert("account_password".into(), json!(a.account_password));
                    map.insert("phone_number".into(), json!(a.phone_number));
                    map.insert("mail_url".into(), json!(a.mail_url));
                    map.insert("aux_email".into(), json!(a.aux_email));
                }
            }
            obj
        })
        .collect();
    serde_json::to_string_pretty(&filtered).map_err(|e| format!("Serialize export JSON: {e}"))
}

// ============================================================================
// Google OAuth 2.0 Loopback Server & Manual Callback Flow
// ============================================================================

struct ActiveOAuthSession {
    redirect_uri: String,
    state: String,
    code_tx: Option<oneshot::Sender<String>>,
}

static OAUTH_SESSION: OnceLock<Mutex<Option<ActiveOAuthSession>>> = OnceLock::new();

fn oauth_session_lock() -> &'static Mutex<Option<ActiveOAuthSession>> {
    OAUTH_SESSION.get_or_init(|| Mutex::new(None))
}

fn build_google_auth_url(redirect_uri: &str, state: &str) -> String {
    let scopes = [
        "openid",
        "https://www.googleapis.com/auth/cloud-platform",
        "https://www.googleapis.com/auth/userinfo.email",
        "https://www.googleapis.com/auth/userinfo.profile",
        "https://www.googleapis.com/auth/cclog",
        "https://www.googleapis.com/auth/experimentsandconfigs",
        "https://www.googleapis.com/auth/aicode",
    ]
    .join(" ");

    let params = [
        ("client_id", CLIENT_ID),
        ("redirect_uri", redirect_uri),
        ("response_type", "code"),
        ("scope", &scopes),
        ("access_type", "offline"),
        ("prompt", "consent"),
        ("include_granted_scopes", "true"),
        ("state", state),
    ];
    Url::parse_with_params(AUTH_URL, &params)
        .expect("valid auth url")
        .to_string()
}

pub async fn prepare_oauth_session() -> Result<OAuthPrepareResult, String> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("Bind OAuth loopback port: {e}"))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("Local addr: {e}"))?
        .port();
    let redirect_uri = format!("http://localhost:{port}/oauth-callback");
    let state = Uuid::new_v4().to_string();
    let auth_url = build_google_auth_url(&redirect_uri, &state);

    let (code_tx, code_rx) = oneshot::channel::<String>();
    {
        let mut lock = oauth_session_lock()
            .lock()
            .map_err(|_| "OAuth lock error".to_string())?;
        *lock = Some(ActiveOAuthSession {
            redirect_uri: redirect_uri.clone(),
            state: state.clone(),
            code_tx: Some(code_tx),
        });
    }

    let expected_state = state.clone();
    let expected_redirect = redirect_uri.clone();

    // Background listener for browser callback
    tokio::spawn(async move {
        let accept_fut = async {
            if let Ok((mut stream, _)) = listener.accept().await {
                let mut buf = [0u8; 4096];
                if let Ok(n) = stream.read(&mut buf).await {
                    let req = String::from_utf8_lossy(&buf[..n]);
                    if let Some(first_line) = req.lines().next() {
                        if let Some(target) = first_line.split_whitespace().nth(1) {
                            if let Ok(parsed) =
                                Url::parse(&format!("http://localhost:{port}{target}"))
                            {
                                let mut code_opt = None;
                                let mut state_opt = None;
                                for (k, v) in parsed.query_pairs() {
                                    if k == "code" {
                                        code_opt = Some(v.into_owned());
                                    } else if k == "state" {
                                        state_opt = Some(v.into_owned());
                                    }
                                }
                                if let (Some(c), Some(st)) = (code_opt, state_opt) {
                                    if st == expected_state {
                                        let html = "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\n\r\n<html><body style='font-family:sans-serif;text-align:center;padding:50px;background:#0d1117;color:#fff;'><h2 style='color:#4ade80;'>✅ OnlyDEVS — OAuth Ủy quyền thành công!</h2><p>Bạn có thể đóng tab này và quay lại ứng dụng OnlyDEVS.</p><script>setTimeout(()=>window.close(),1800)</script></body></html>";
                                        let _ = stream.write_all(html.as_bytes()).await;
                                        let _ = stream.flush().await;
                                        if let Ok(mut lock) = oauth_session_lock().lock() {
                                            if let Some(sess) = lock.as_mut() {
                                                if let Some(tx) = sess.code_tx.take() {
                                                    let _ = tx.send(c);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        };
        let _ = timeout(Duration::from_secs(600), accept_fut).await;
        let _ = expected_redirect;
    });

    // Store receiver in a waiter task is handled by await_oauth_session or submit_oauth_callback_url
    tokio::spawn(async move {
        if let Ok(code) = code_rx.await {
            let redirect = {
                oauth_session_lock()
                    .lock()
                    .ok()
                    .and_then(|l| l.as_ref().map(|s| s.redirect_uri.clone()))
            };
            if let Some(redir) = redirect {
                if let Ok(tok_res) = exchange_google_code(&code, &redir).await {
                    if let Some(rt) = tok_res.refresh_token {
                        let _ = build_or_update_account_from_refresh_token(rt, false).await;
                    }
                }
            }
        }
    });

    Ok(OAuthPrepareResult {
        auth_url,
        redirect_uri,
        state,
    })
}

pub async fn submit_oauth_callback_url(callback_url: String) -> Result<AntigravityAccount, String> {
    let trimmed = callback_url.trim();
    if trimmed.is_empty() {
        return Err("Callback URL không được để trống".into());
    }
    let (redirect_uri, expected_state) = {
        let lock = oauth_session_lock()
            .lock()
            .map_err(|_| "OAuth lock error".to_string())?;
        let sess = lock
            .as_ref()
            .ok_or_else(|| "Chưa khởi tạo phiên OAuth (hãy bấm Tạo link OAuth trước)".to_string())?;
        (sess.redirect_uri.clone(), sess.state.clone())
    };

    let parsed = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        Url::parse(trimmed).map_err(|e| format!("Callback URL không hợp lệ: {e}"))?
    } else {
        Url::parse(&format!(
            "{}?{}",
            redirect_uri,
            trimmed.trim_start_matches('?')
        ))
        .map_err(|e| format!("Callback query không hợp lệ: {e}"))?
    };

    let mut code = None;
    let mut state = None;
    for (k, v) in parsed.query_pairs() {
        if k == "code" {
            code = Some(v.into_owned());
        } else if k == "state" {
            state = Some(v.into_owned());
        }
    }
    let code = code.ok_or_else(|| "Không tìm thấy tham số 'code' trong Callback URL".to_string())?;
    if let Some(st) = state {
        if st != expected_state {
            return Err("OAuth state không khớp với phiên hiện tại".into());
        }
    }

    let tok_res = exchange_google_code(&code, &redirect_uri).await?;
    let rt = tok_res
        .refresh_token
        .ok_or_else(|| "Google không trả về refresh_token (hãy thu hồi quyền rồi thử lại)".to_string())?;
    build_or_update_account_from_refresh_token(rt, false).await
}

// ============================================================================
// TOTP 2FA Code Generator (RFC 6238 Base32)
// ============================================================================

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TotpCodeResult {
    pub code: String,
    pub remaining_seconds: u64,
}

fn decode_base32(input: &str) -> Result<Vec<u8>, String> {
    let clean: String = input
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-' && *c != '=')
        .map(|c| c.to_ascii_uppercase())
        .collect();
    if clean.is_empty() {
        return Err("2FA secret trống".into());
    }
    let mut buffer = 0u64;
    let mut bits_left = 0;
    let mut out = Vec::new();
    for ch in clean.chars() {
        let val = match ch {
            'A'..='Z' => (ch as u8) - b'A',
            '2'..='7' => (ch as u8) - b'2' + 26,
            _ => return Err(format!("Ký tự Base32 không hợp lệ: {ch}")),
        };
        buffer = (buffer << 5) | (val as u64);
        bits_left += 5;
        if bits_left >= 8 {
            bits_left -= 8;
            out.push(((buffer >> bits_left) & 0xFF) as u8);
        }
    }
    Ok(out)
}

fn sha1_digest(data: &[u8]) -> [u8; 20] {
    let mut h0: u32 = 0x67452301;
    let mut h1: u32 = 0xEFCDAB89;
    let mut h2: u32 = 0x98BADCFE;
    let mut h3: u32 = 0x10325476;
    let mut h4: u32 = 0xC3D2E1F0;

    let bit_len = (data.len() as u64) * 8;
    let mut msg = data.to_vec();
    msg.push(0x80);
    while (msg.len() % 64) != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.chunks(64) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let (mut a, mut b, mut c, mut d, mut e) = (h0, h1, h2, h3, h4);
        for i in 0..80 {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A827999),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                _ => (b ^ c ^ d, 0xCA62C1D6),
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(w[i]);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }
        h0 = h0.wrapping_add(a);
        h1 = h1.wrapping_add(b);
        h2 = h2.wrapping_add(c);
        h3 = h3.wrapping_add(d);
        h4 = h4.wrapping_add(e);
    }

    let mut out = [0u8; 20];
    out[0..4].copy_from_slice(&h0.to_be_bytes());
    out[4..8].copy_from_slice(&h1.to_be_bytes());
    out[8..12].copy_from_slice(&h2.to_be_bytes());
    out[12..16].copy_from_slice(&h3.to_be_bytes());
    out[16..20].copy_from_slice(&h4.to_be_bytes());
    out
}

fn hmac_sha1(key: &[u8], message: &[u8]) -> [u8; 20] {
    let mut k = [0u8; 64];
    if key.len() > 64 {
        let hashed = sha1_digest(key);
        k[..20].copy_from_slice(&hashed);
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let mut ipad = [0x36u8; 64];
    let mut opad = [0x5Cu8; 64];
    for i in 0..64 {
        ipad[i] ^= k[i];
        opad[i] ^= k[i];
    }
    let mut inner = ipad.to_vec();
    inner.extend_from_slice(message);
    let inner_hash = sha1_digest(&inner);

    let mut outer = opad.to_vec();
    outer.extend_from_slice(&inner_hash);
    sha1_digest(&outer)
}

pub fn generate_totp_code(secret_raw: &str) -> Result<TotpCodeResult, String> {
    let secret = if let Some(idx) = secret_raw.find("secret=") {
        let rest = &secret_raw[idx + 7..];
        rest.split('&').next().unwrap_or(rest)
    } else {
        secret_raw
    };
    let key = decode_base32(secret)?;
    let now = Utc::now().timestamp().max(0) as u64;
    let counter = now / 30;
    let remaining_seconds = 30 - (now % 30);
    let mac = hmac_sha1(&key, &counter.to_be_bytes());
    let offset = (mac[19] & 0x0F) as usize;
    let binary = (((mac[offset] & 0x7F) as u32) << 24)
        | ((mac[offset + 1] as u32) << 16)
        | ((mac[offset + 2] as u32) << 8)
        | (mac[offset + 3] as u32);
    let otp = binary % 1_000_000;
    Ok(TotpCodeResult {
        code: format!("{otp:06}"),
        remaining_seconds,
    })
}

// ============================================================================
// Local Antigravity Token Usage Analytics (from brain/*/transcript.jsonl)
// ============================================================================

pub fn get_antigravity_token_usage() -> Result<TokenUsageSummary, String> {
    let mut total_input = 0u64;
    let mut total_output = 0u64;
    let mut total_cache = 0u64;
    let mut total_conversations = 0usize;
    let mut total_turns = 0usize;
    let mut model_map: BTreeMap<String, ModelTokenUsage> = BTreeMap::new();

    for root_res in [paths::antigravity_dir(), paths::antigravity_cli_dir()] {
        let Ok(root) = root_res else { continue };
        let brain = root.join("brain");
        if !brain.is_dir() {
            continue;
        }
        let Ok(entries) = fs::read_dir(&brain) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            if !p.is_dir() {
                continue;
            }
            let transcript = p
                .join(".system_generated")
                .join("logs")
                .join("transcript.jsonl");
            if !transcript.is_file() {
                continue;
            }
            let Ok(raw) = fs::read_to_string(&transcript) else {
                continue;
            };
            total_conversations += 1;
            for line in raw.lines() {
                let Ok(val) = serde_json::from_str::<Value>(line.trim()) else {
                    continue;
                };
                let inp = val.get("input_tokens").and_then(Value::as_u64).unwrap_or(0);
                let out = val
                    .get("output_tokens")
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
                let cache = val
                    .get("cache_read_tokens")
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
                if inp == 0 && out == 0 && cache == 0 {
                    continue;
                }
                total_input += inp;
                total_output += out;
                total_cache += cache;
                total_turns += 1;
                let model = val
                    .get("model")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_string();
                let entry = model_map.entry(model.clone()).or_insert(ModelTokenUsage {
                    model,
                    input_tokens: 0,
                    output_tokens: 0,
                    cache_read_tokens: 0,
                    turns: 0,
                });
                entry.input_tokens += inp;
                entry.output_tokens += out;
                entry.cache_read_tokens += cache;
                entry.turns += 1;
            }
        }
    }

    let mut models: Vec<ModelTokenUsage> = model_map.into_values().collect();
    models.sort_by(|a, b| {
        (b.input_tokens + b.output_tokens).cmp(&(a.input_tokens + a.output_tokens))
    });

    Ok(TokenUsageSummary {
        total_input_tokens: total_input,
        total_output_tokens: total_output,
        total_cache_read_tokens: total_cache,
        total_conversations,
        total_turns,
        models,
    })
}

// ============================================================================
// Account Groups & Custom Sort Order (Cockpit Overview parity)
// ============================================================================

pub fn list_account_groups() -> Result<Vec<AccountGroup>, String> {
    let conn = open_db()?;
    ensure_accounts_schema(&conn)?;
    let mut stmt = conn
        .prepare(
            "SELECT id, name, account_ids_json, sort_order, created_at
             FROM account_groups
             ORDER BY sort_order ASC, created_at ASC",
        )
        .map_err(|e| format!("Prepare list_account_groups: {e}"))?;
    let rows = stmt
        .query_map([], |r| {
            let ids_raw: String = r.get(2)?;
            let account_ids: Vec<String> = serde_json::from_str(&ids_raw).unwrap_or_default();
            Ok(AccountGroup {
                id: r.get(0)?,
                name: r.get(1)?,
                account_ids,
                sort_order: r.get(3)?,
                created_at: r.get(4)?,
            })
        })
        .map_err(|e| format!("Query list_account_groups: {e}"))?;
    Ok(rows.flatten().collect())
}

pub fn save_account_group(
    id: Option<String>,
    name: String,
    account_ids: Vec<String>,
) -> Result<AccountGroup, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("Tên nhóm không được để trống.".to_string());
    }
    let conn = open_db()?;
    ensure_accounts_schema(&conn)?;
    let now = Utc::now().timestamp();

    let mut deduped = Vec::new();
    for aid in account_ids {
        let a = aid.trim().to_string();
        if !a.is_empty() && !deduped.contains(&a) {
            deduped.push(a);
        }
    }
    let ids_json = serde_json::to_string(&deduped).map_err(|e| format!("Serialize ids: {e}"))?;

    let group_id = id
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    let existing: Option<(i64, i64)> = conn
        .query_row(
            "SELECT sort_order, created_at FROM account_groups WHERE id = ?1",
            params![&group_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(|e| format!("Query existing group: {e}"))?;

    let (sort_order, created_at) = match existing {
        Some(pair) => pair,
        None => {
            let max_order: i64 = conn
                .query_row(
                    "SELECT COALESCE(MAX(sort_order), -1) FROM account_groups",
                    [],
                    |r| r.get(0),
                )
                .unwrap_or(-1);
            (max_order + 1, now)
        }
    };

    conn.execute(
        "INSERT INTO account_groups (id, name, account_ids_json, sort_order, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(id) DO UPDATE SET
           name = excluded.name,
           account_ids_json = excluded.account_ids_json",
        params![&group_id, trimmed, &ids_json, sort_order, created_at],
    )
    .map_err(|e| format!("Upsert account group: {e}"))?;

    Ok(AccountGroup {
        id: group_id,
        name: trimmed.to_string(),
        account_ids: deduped,
        sort_order,
        created_at,
    })
}

pub fn delete_account_group(group_id: &str) -> Result<(), String> {
    let conn = open_db()?;
    ensure_accounts_schema(&conn)?;
    conn.execute("DELETE FROM account_groups WHERE id = ?1", params![group_id])
        .map_err(|e| format!("Delete account group: {e}"))?;
    Ok(())
}

pub fn assign_accounts_to_group(
    group_id: &str,
    account_ids: Vec<String>,
    add_mode: bool,
) -> Result<AccountGroup, String> {
    let conn = open_db()?;
    ensure_accounts_schema(&conn)?;
    let (name, existing_raw, sort_order, created_at): (String, String, i64, i64) = conn
        .query_row(
            "SELECT name, account_ids_json, sort_order, created_at FROM account_groups WHERE id = ?1",
            params![group_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .map_err(|_| format!("Không tìm thấy nhóm {group_id}"))?;

    let mut next_ids: Vec<String> = if add_mode {
        serde_json::from_str(&existing_raw).unwrap_or_default()
    } else {
        Vec::new()
    };
    for aid in account_ids {
        let a = aid.trim().to_string();
        if !a.is_empty() && !next_ids.contains(&a) {
            next_ids.push(a);
        }
    }
    let ids_json = serde_json::to_string(&next_ids).map_err(|e| format!("Serialize ids: {e}"))?;
    conn.execute(
        "UPDATE account_groups SET account_ids_json = ?1 WHERE id = ?2",
        params![&ids_json, group_id],
    )
    .map_err(|e| format!("Update group members: {e}"))?;

    Ok(AccountGroup {
        id: group_id.to_string(),
        name,
        account_ids: next_ids,
        sort_order,
        created_at,
    })
}

pub fn get_account_custom_sort() -> Result<Vec<String>, String> {
    let conn = open_db()?;
    ensure_accounts_schema(&conn)?;
    let raw: Option<String> = conn
        .query_row(
            "SELECT value FROM app_settings WHERE key = 'accounts_custom_sort_order'",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| format!("Read custom sort: {e}"))?;
    Ok(raw
        .and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok())
        .unwrap_or_default())
}

pub fn save_account_custom_sort(account_ids: Vec<String>) -> Result<Vec<String>, String> {
    let conn = open_db()?;
    ensure_accounts_schema(&conn)?;
    let raw = serde_json::to_string(&account_ids).map_err(|e| format!("Serialize sort: {e}"))?;
    conn.execute(
        "INSERT INTO app_settings (key, value) VALUES ('accounts_custom_sort_order', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![raw],
    )
    .map_err(|e| format!("Save custom sort: {e}"))?;
    Ok(account_ids)
}

// ============================================================================
// Import from existing Cockpit Tools storage (~/.antigravity_cockpit / AppData)
// ============================================================================

pub async fn import_accounts_from_cockpit() -> Result<Vec<AntigravityAccount>, String> {
    let mut candidate_dirs: Vec<PathBuf> = Vec::new();
    if let Some(home) = dirs::home_dir() {
        candidate_dirs.push(home.join(".antigravity_cockpit").join("accounts"));
        candidate_dirs.push(home.join(".cockpit_tools").join("accounts"));
    }
    if let Some(roaming) = dirs::config_dir() {
        candidate_dirs.push(roaming.join("com.jlcodes.antigravity-cockpit").join("accounts"));
        candidate_dirs.push(roaming.join("antigravity-cockpit").join("accounts"));
        candidate_dirs.push(roaming.join("cockpit-tools").join("accounts"));
    }
    if let Some(local) = dirs::data_local_dir() {
        candidate_dirs.push(local.join("com.jlcodes.antigravity-cockpit").join("accounts"));
        candidate_dirs.push(local.join("cockpit-tools").join("accounts"));
    }

    let mut parsed_items: Vec<Value> = Vec::new();
    let mut scanned_dirs = 0usize;
    for dir in candidate_dirs {
        if !dir.is_dir() {
            continue;
        }
        scanned_dirs += 1;
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Ok(raw) = fs::read_to_string(&path) else {
                continue;
            };
            let clean = raw.strip_prefix('\u{feff}').unwrap_or(&raw);
            let Ok(val) = serde_json::from_str::<Value>(clean) else {
                continue;
            };
            // Skip encrypted envelopes (no plaintext refresh_token)
            let has_token = val
                .pointer("/token/refresh_token")
                .or_else(|| val.pointer("/token/refreshToken"))
                .or_else(|| val.get("refresh_token"))
                .or_else(|| val.get("refreshToken"))
                .and_then(Value::as_str)
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false);
            if has_token {
                parsed_items.push(val);
            }
        }
    }

    if parsed_items.is_empty() {
        if scanned_dirs == 0 {
            return Err(
                "Không tìm thấy thư mục tài khoản Cockpit Tools trên máy. Hãy xuất JSON từ Cockpit Tools rồi nhập vào đây."
                    .to_string(),
            );
        }
        return Err(
            "Tìm thấy dữ liệu Cockpit Tools nhưng các tệp tài khoản đang được mã hóa cục bộ. Hãy nhấn 'Xuất JSON' trong Cockpit Tools rồi nhập vào tab JSON ở đây."
                .to_string(),
        );
    }

    let json_array = serde_json::to_string(&parsed_items)
        .map_err(|e| format!("Serialize imported items: {e}"))?;
    import_accounts_from_content(json_array).await
}

// ============================================================================
// Native Folder Picker (Windows FolderBrowserDialog via PowerShell STA)
// ============================================================================

pub fn pick_folder_dialog(default_path: Option<String>) -> Result<Option<String>, String> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        use std::process::Command;

        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let escaped_default = default_path
            .unwrap_or_default()
            .replace('\'', "''");
        let script = format!(
            "Add-Type -AssemblyName System.Windows.Forms; \
             $dlg = New-Object System.Windows.Forms.FolderBrowserDialog; \
             $dlg.Description = 'Chon thu muc du lieu Antigravity (~/.gemini/antigravity)'; \
             $dlg.ShowNewFolderButton = $false; \
             if ('{escaped_default}' -ne '' -and (Test-Path '{escaped_default}')) {{ $dlg.SelectedPath = '{escaped_default}' }}; \
             if ($dlg.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {{ [Console]::OutputEncoding = [System.Text.Encoding]::UTF8; Write-Output $dlg.SelectedPath }}"
        );
        let output = Command::new("powershell")
            .args(["-NoProfile", "-STA", "-Command", &script])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map_err(|e| format!("Không thể mở hộp thoại chọn thư mục: {e}"))?;

        let selected = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if selected.is_empty() {
            Ok(None)
        } else {
            Ok(Some(selected))
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = default_path;
        Ok(None)
    }
}
