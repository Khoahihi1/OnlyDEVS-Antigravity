//! OnlyDEVS Antigravity Local API Router
//!
//! Native OpenAI & Anthropic compatible proxy forwarding to Antigravity Cloud Code
//! with multi-account pooling, quota-aware routing, cooldown failover, API keys,
//! model alias rules, request logs, and token usage analytics.

use chrono::Utc;
use regex::Regex;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock, RwLock};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use uuid::Uuid;

use crate::accounts::{self, AntigravityAccount};
use crate::prompts::open_db;

// ============================================================================
// Antigravity Cloud Code Constants & Protocol Signatures
// ============================================================================

// Official Antigravity chat endpoint — NEVER fall back to prod cloudcode-pa.googleapis.com
// for chat traffic (prod returns fake 429 RESOURCE_EXHAUSTED for consumer accounts).
const ANTIGRAVITY_BASE_URLS: &[&str] = &["https://daily-cloudcode-pa.googleapis.com"];

// Official Antigravity 2.25.0 aidev_client fingerprint (verified HTTP 200 on Gemini 4 & Claude 4.6/5.5)
const ANTIGRAVITY_IDE_USER_AGENT: &str =
    "antigravity/2.25.0 (aidev_client; os_type=windows; arch=amd64; cl=1003475615)";
const MAX_ANTIGRAVITY_OUTPUT_TOKENS: u64 = 64_000;

const AG_TOOL_SUFFIX: &str = "_ide";

const AG_NATIVE_TOOL_NAMES: &[&str] = &[
    "browser_subagent",
    "command_status",
    "find_by_name",
    "generate_image",
    "grep_search",
    "list_dir",
    "list_resources",
    "mcp_sequential-thinking_sequentialthinking",
    "multi_replace_file_content",
    "notify_user",
    "read_resource",
    "read_terminal",
    "read_url_content",
    "replace_file_content",
    "run_command",
    "search_web",
    "send_command_input",
    "task_boundary",
    "view_content_chunk",
    "view_file",
    "write_to_file",
];

// Default thinking signature for Gemini 3+ functionCall backfill
const DEFAULT_THINKING_AG_SIGNATURE: &str = "EuwGCukGAXLI2nxwZIq54WWSoL/YN0P3TsDZ7zRnLi8g0S4aVr2HUGxvaHKySuY6HAVzcE0GPGjXrytLIldxthSvfxgUlJh6Qa9Z+Oj5QZBlYdg6HaJ6yuY5R7waE6rdwBsRf7Ft2j3DJ9rMi9qhWFqApewYtPhls3VHtuvND3l8Rm09+lbAXQs6KKWEWrxNLKTBkfpMgXhRERc/TQRMZu1twAablm6/Zk1tsYRvfWKLsNbeKF+CCojJdXJKvnR/8Ouuoa+Y2Ti20hcW7aZIIjZDFYPU//k6Ybmhg69J/imbFai2ckhfLaisqdDkdoIiBJScTOUvYqP6AE9d4MsydSC+UlhIMk4hoP76R8vUSCZRMkjOaDXstf/QoVZKbt94wyRZgAJ1G0BqI8L5ow86kLpA4wJEtxsRGymOE4bKUvApveBakYDNM9APkf+LbtbzWSseGjoZcSlycF9iN8Q2XNYKRrHbv3Lr5Y8JjdH/5y/6SHkNehTEZugaeGnSPSyCTWto1kQgHpxdWmhkLfJGNUGLmue7Mesj4TSms4J33mRpYVhNB/J333FCqIP0hr/E7BkkjEn7yZ4X7SQlh+xKPurapsnHRwiKmtsilmEFrnTE9iQr+pMr6M29qqFNv1tr5yumbaJw8JW9sB15tNsRv+dW6BjNanbsKz7HCgKUBc8tGy+7YuhXzAfViyRefcjK7eZW0Fbyt7AbybJTKz78W8NH7ye6LAwzOebXpeZ4D43fNIt8bKh26qgduSQv/7o+pAflkuqHZ99YWgHQ8h8OkZFi3eOiSYjsjhdZ/czWOdoPI/OnqIldzMPF5YlrKBLFX8VhRKVmqgsmWf5PHGulHhMkVlS+XG2UIseGy69ARa93D78Gsa+1n1kJr7EEB7Rh+27vUMxVYLdz1yMSvE5nalTAlg/ZeG8+XQ0cHuAI3KbQpHW2Q++RdXfm5JzD5WdJZUU+Zn8t8UUn85BH4RxZLeE0qJikgSsKoYVBc6YhiMjhPgkR95ReimY4Z0xCJdRo1gjexOFeODZMpQF6Yxnoic7IrdgsFA3iePTbFnPp3IAM1fAThWhXJUn3QInUOTd5o1qmTmn6REbL15g/JQNl+dqUoPkhleeb2V3kjqp1okmO3wMZbPknR3S1LZNmlS72/iBQUm+n2b/RCn4PjmM2";

const BLACKLISTED_FIELDS: &[&str] = &[
    "output_config",
    "thinking",
    "reasoning_effort",
    "reasoning",
    "enable_thinking",
    "thinking_budget",
    "thinkingConfig",
];

// Built-in Antigravity model catalog
const BUILTIN_MODELS: &[(&str, &str, &str, bool)] = &[
    ("gemini-4-neon-medium", "Gemini 4 Neon (Medium)", "gemini", true),
    ("gemini-4-neon-high", "Gemini 4 Neon (High)", "gemini", true),
    ("gemini-4-neon-low", "Gemini 4 Neon (Low)", "gemini", true),
    ("gemini-4-hydrogen-medium", "Gemini 4 Hydrogen (Medium)", "gemini", true),
    ("gemini-4-hydrogen-high", "Gemini 4 Hydrogen (High)", "gemini", true),
    ("gemini-4-argon-high", "Gemini 4 Argon (High)", "gemini", true),
    ("gemini-4-argon-medium", "Gemini 4 Argon (Medium)", "gemini", true),
    ("gemini-3.8-flash-high", "Gemini 3.8 Flash (High)", "gemini", true),
    ("gemini-3.8-flash-medium", "Gemini 3.8 Flash (Medium)", "gemini", true),
    ("gemini-3-flash-agent", "Gemini 3.5 Flash (High)", "gemini", true),
    ("gemini-3-flash", "Gemini 3 Flash", "gemini", false),
    ("claude-sonnet-5-5-medium", "Claude Sonnet 5.5 (Medium)", "claude", true),
    ("claude-sonnet-5-5-high", "Claude Sonnet 5.5 (High)", "claude", true),
    ("claude-opus-5-5-high", "Claude Opus 5.5 (High)", "claude", true),
    ("claude-sonnet-4-6", "Claude Sonnet 4.6 (Thinking)", "claude", true),
    ("claude-opus-4-6-thinking", "Claude Opus 4.6 (Thinking)", "claude", true),
    ("gpt-oss-120b-medium", "GPT-OSS 120B (Medium)", "openai", true),
    ("gemini-3.1-flash-image", "Gemini 3.1 Flash (Image)", "image", false),
];

// ============================================================================
// Data Models & State
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelAliasRule {
    pub id: String,
    pub pattern: String,
    pub target_model: String,
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

fn default_auto_refresh_minutes() -> u64 {
    10
}

fn default_allowed_profile() -> String {
    "all".into()
}

fn normalize_allowed_profile(raw: Option<&str>) -> String {
    match raw.map(|s| s.trim().to_ascii_lowercase()).as_deref() {
        Some("classic") => "classic".into(),
        Some("full_nextgen") | Some("full") => "full_nextgen".into(),
        _ => "all".into(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiServiceConfig {
    pub port: u16,
    /// "local" -> 127.0.0.1, "lan" -> 0.0.0.0
    pub access_scope: String,
    /// "smart" | "highest_quota" | "round_robin" | "sticky_session" | "failover"
    pub routing_strategy: String,
    /// Account IDs in the API service pool; empty = use all active accounts
    pub member_account_ids: Vec<String>,
    /// If true and member_account_ids is empty, automatically include all non-disabled accounts
    pub use_all_accounts_when_empty: bool,
    /// If true, prefer Classic (3.8/Claude 4.6) accounts for shared models (3.8/3-flash)
    /// to preserve Gemini quota on Full/Next-Gen (Argon/G4/Claude 5.5) accounts
    #[serde(default = "default_true")]
    pub preserve_exclusive_accounts: bool,
    pub auto_start: bool,
    /// Hide window to system tray when clicking Close (keeps API Router running in background)
    #[serde(default = "default_true")]
    pub minimize_to_tray: bool,
    /// Periodically refresh pool account quotas in background while API Router is running (0 = off)
    #[serde(default = "default_auto_refresh_minutes")]
    pub auto_refresh_quota_minutes: u64,
    pub timeout_seconds: u64,
    pub max_retries: u32,
    pub cooldown_seconds: u64,
    pub proxy_url: Option<String>,
    pub model_rules: Vec<ModelAliasRule>,
}

impl Default for ApiServiceConfig {
    fn default() -> Self {
        Self {
            port: 57120,
            access_scope: "local".into(),
            routing_strategy: "smart".into(),
            member_account_ids: Vec::new(),
            use_all_accounts_when_empty: true,
            preserve_exclusive_accounts: true,
            auto_start: false,
            minimize_to_tray: true,
            auto_refresh_quota_minutes: 10,
            timeout_seconds: 180,
            max_retries: 2,
            cooldown_seconds: 60,
            proxy_url: None,
            model_rules: vec![
                ModelAliasRule {
                    id: "rule-gpt4o".into(),
                    pattern: "gpt-4o".into(),
                    target_model: "gemini-pro-agent".into(),
                    enabled: true,
                },
                ModelAliasRule {
                    id: "rule-gpt4o-mini".into(),
                    pattern: "gpt-4o-mini".into(),
                    target_model: "gemini-3-flash-agent".into(),
                    enabled: true,
                },
                ModelAliasRule {
                    id: "rule-claude-sonnet".into(),
                    pattern: "claude-3-5-sonnet*".into(),
                    target_model: "claude-sonnet-4-6".into(),
                    enabled: true,
                },
                ModelAliasRule {
                    id: "rule-claude-opus".into(),
                    pattern: "claude-3-opus*".into(),
                    target_model: "claude-opus-4-6-thinking".into(),
                    enabled: true,
                },
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiKeyRecord {
    pub id: String,
    pub label: String,
    pub key: String,
    pub enabled: bool,
    pub is_master: bool,
    pub allowed_models: Vec<String>,
    pub pinned_account_ids: Vec<String>,
    /// "all" | "full_nextgen" | "classic" — restrict key to a specific Pro pool group
    #[serde(default = "default_allowed_profile")]
    pub allowed_profile: String,
    /// Max requests per minute (0 = unlimited)
    #[serde(default)]
    pub rate_limit_rpm: u32,
    /// Max tokens per UTC day (0 = unlimited)
    #[serde(default)]
    pub daily_token_limit: u64,
    /// Optional unix timestamp expiry (None = never expires)
    #[serde(default)]
    pub expires_at: Option<i64>,
    pub created_at: i64,
    pub last_used_at: Option<i64>,
    pub total_requests: u64,
    pub total_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountCooldownInfo {
    pub account_id: String,
    pub email: String,
    pub scope: String,
    pub until_ts: i64,
    pub remaining_seconds: i64,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiServiceStatus {
    pub running: bool,
    pub host: String,
    pub port: u16,
    pub base_url: String,
    pub lan_url: Option<String>,
    pub started_at: Option<i64>,
    pub active_requests: usize,
    pub pool_total: usize,
    pub pool_healthy: usize,
    pub pool_full_nextgen: usize,
    pub pool_classic: usize,
    pub pool_unknown: usize,
    pub cooldowns: Vec<AccountCooldownInfo>,
    pub config: ApiServiceConfig,
    pub keys: Vec<ApiKeyRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiAvailableModel {
    pub id: String,
    pub display_name: String,
    pub family: String,
    pub thinking: bool,
    pub source: String,
    pub supported_account_count: usize,
    pub pool_account_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiRequestLogItem {
    pub id: String,
    pub timestamp: i64,
    pub api_key_id: String,
    pub api_key_label: String,
    pub account_id: String,
    pub account_email: String,
    pub protocol: String,
    pub request_model: String,
    pub resolved_model: String,
    pub stream: bool,
    pub status_code: u16,
    pub latency_ms: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub thinking_tokens: u64,
    pub cache_read_tokens: u64,
    pub total_tokens: u64,
    pub error_message: Option<String>,
    pub request_preview: String,
    pub response_preview: String,
    #[serde(default)]
    pub client_ip: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiUsageBreakdownRow {
    pub key: String,
    pub label: String,
    pub requests: u64,
    pub errors: u64,
    pub input_tokens: u64,
    pub thinking_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub total_tokens: u64,
    pub avg_latency_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiUsageTimePoint {
    pub bucket: String,
    pub requests: u64,
    pub errors: u64,
    pub input_tokens: u64,
    pub thinking_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub total_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiServiceUsageSummary {
    pub range: String,
    pub total_requests: u64,
    pub success_requests: u64,
    pub error_requests: u64,
    pub success_rate: f64,
    pub avg_latency_ms: u64,
    pub input_tokens: u64,
    pub thinking_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub total_tokens: u64,
    pub by_model: Vec<ApiUsageBreakdownRow>,
    pub by_account: Vec<ApiUsageBreakdownRow>,
    pub by_key: Vec<ApiUsageBreakdownRow>,
    pub time_series: Vec<ApiUsageTimePoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LanDiagnostics {
    pub port: u16,
    pub lan_ip: Option<String>,
    pub firewall_rule_exists: bool,
    pub network_profile: Option<String>,
    pub manual_command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiTestChatResult {
    pub ok: bool,
    pub status_code: u16,
    pub account_email: String,
    pub resolved_model: String,
    pub latency_ms: u64,
    pub text: String,
    pub thinking: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub thinking_tokens: u64,
    pub error: Option<String>,
}

struct RunningServer {
    host: String,
    port: u16,
    started_at: i64,
    generation: u64,
    shutdown_tx: watch::Sender<bool>,
    accept_handles: Vec<tokio::task::JoinHandle<()>>,
}

struct CooldownEntry {
    account_id: String,
    email: String,
    scope: String,
    until_ts: i64,
    reason: String,
}

static SERVER_STATE: OnceLock<Mutex<Option<RunningServer>>> = OnceLock::new();
static LIFECYCLE_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
static SERVER_GEN_SEQ: AtomicU64 = AtomicU64::new(1);
/// 0 means server is stopped; non-zero matches the active RunningServer generation.
static SERVER_ACTIVE_GEN: AtomicU64 = AtomicU64::new(0);
static ACTIVE_REQUESTS: AtomicUsize = AtomicUsize::new(0);
static RR_COUNTER: AtomicUsize = AtomicUsize::new(0);
/// Keyed by "(account_id, scope)" where scope is "all" | "gemini" | "claude" | "argon" | "carbon"
static COOLDOWNS: OnceLock<Mutex<HashMap<(String, String), CooldownEntry>>> = OnceLock::new();
/// Runtime-learned model incompatibilities: (account_id, lower_model_id) -> expires_at_ts
static UNSUPPORTED_MODELS: OnceLock<Mutex<HashMap<(String, String), i64>>> = OnceLock::new();
static STICKY_MAP: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
/// Cached shared upstream HTTP/2+TLS connection pool keyed by (timeout_secs, proxy_url)
static UPSTREAM_CLIENT_CACHE: OnceLock<Mutex<Option<((u64, String), reqwest::Client)>>> =
    OnceLock::new();
static API_SCHEMA_READY: OnceLock<()> = OnceLock::new();
static CONFIG_CACHE: OnceLock<RwLock<Option<ApiServiceConfig>>> = OnceLock::new();
static API_KEYS_CACHE: OnceLock<RwLock<Option<Vec<ApiKeyRecord>>>> = OnceLock::new();
/// Sliding 60s request timestamps per API key for RPM rate limiting
static KEY_RPM_BUCKETS: OnceLock<Mutex<HashMap<String, VecDeque<Instant>>>> = OnceLock::new();
/// Cached per-key token totals for current UTC date ("YYYY-MM-DD")
static KEY_DAILY_TOKENS: OnceLock<Mutex<HashMap<String, (String, u64)>>> = OnceLock::new();
/// Debounce post-429 background quota refreshes per account (min 90s interval)
static POST_429_REFRESH_DEBOUNCE: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();

fn upstream_client_cache() -> &'static Mutex<Option<((u64, String), reqwest::Client)>> {
    UPSTREAM_CLIENT_CACHE.get_or_init(|| Mutex::new(None))
}

fn config_cache() -> &'static RwLock<Option<ApiServiceConfig>> {
    CONFIG_CACHE.get_or_init(|| RwLock::new(None))
}

fn api_keys_cache() -> &'static RwLock<Option<Vec<ApiKeyRecord>>> {
    API_KEYS_CACHE.get_or_init(|| RwLock::new(None))
}

fn key_rpm_buckets() -> &'static Mutex<HashMap<String, VecDeque<Instant>>> {
    KEY_RPM_BUCKETS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn key_daily_tokens() -> &'static Mutex<HashMap<String, (String, u64)>> {
    KEY_DAILY_TOKENS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn post_429_refresh_debounce() -> &'static Mutex<HashMap<String, Instant>> {
    POST_429_REFRESH_DEBOUNCE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn invalidate_api_keys_cache() {
    if let Ok(mut guard) = api_keys_cache().write() {
        *guard = None;
    }
}

fn utc_today_key() -> String {
    Utc::now().format("%Y-%m-%d").to_string()
}

fn utc_today_start_ts() -> i64 {
    let now = Utc::now();
    now.date_naive()
        .and_hms_opt(0, 0, 0)
        .map(|dt| dt.and_utc().timestamp())
        .unwrap_or_else(|| now.timestamp() - 86_400)
}

fn query_key_today_tokens_db(key_id: &str) -> u64 {
    let Ok(conn) = open_db() else {
        return 0;
    };
    if ensure_api_schema(&conn).is_err() {
        return 0;
    }
    let start_ts = utc_today_start_ts();
    conn.query_row(
        "SELECT COALESCE(SUM(total_tokens), 0) FROM api_service_logs WHERE api_key_id = ?1 AND timestamp >= ?2",
        params![key_id, start_ts],
        |r| r.get::<_, i64>(0),
    )
    .unwrap_or(0)
    .max(0) as u64
}

fn get_key_daily_tokens(key_id: &str) -> u64 {
    let today = utc_today_key();
    if let Ok(map) = key_daily_tokens().lock() {
        if let Some((day, used)) = map.get(key_id) {
            if day == &today {
                return *used;
            }
        }
    }
    let seeded = query_key_today_tokens_db(key_id);
    if let Ok(mut map) = key_daily_tokens().lock() {
        map.insert(key_id.to_string(), (today, seeded));
    }
    seeded
}

fn bump_key_daily_tokens(key_id: &str, added: u64) {
    if added == 0 || key_id.is_empty() || key_id == "invalid" {
        return;
    }
    let today = utc_today_key();
    if let Ok(mut map) = key_daily_tokens().lock() {
        match map.get_mut(key_id) {
            Some((day, used)) if day == &today => {
                *used = used.saturating_add(added);
            }
            _ => {
                map.insert(key_id.to_string(), (today, added));
            }
        }
    }
}

/// Enforces per-key expiry, sliding 60s RPM limit, and daily token quota.
fn check_api_key_limits(key: &ApiKeyRecord) -> Result<(), (u16, String, &'static str)> {
    if let Some(exp) = key.expires_at {
        if exp > 0 && Utc::now().timestamp() >= exp {
            return Err((
                401,
                format!("API key '{}' đã hết hạn sử dụng.", key.label),
                "authentication_error",
            ));
        }
    }
    if key.rate_limit_rpm > 0 {
        let now = Instant::now();
        let window = Duration::from_secs(60);
        if let Ok(mut buckets) = key_rpm_buckets().lock() {
            let q = buckets.entry(key.id.clone()).or_default();
            while let Some(&front) = q.front() {
                if now.duration_since(front) >= window {
                    q.pop_front();
                } else {
                    break;
                }
            }
            if q.len() >= key.rate_limit_rpm as usize {
                return Err((
                    429,
                    format!(
                        "API key '{}' vượt giới hạn tốc độ ({} req/phút). Vui lòng thử lại sau ít giây.",
                        key.label, key.rate_limit_rpm
                    ),
                    "rate_limit_error",
                ));
            }
            q.push_back(now);
        }
    }
    if key.daily_token_limit > 0 {
        let used_today = get_key_daily_tokens(&key.id);
        if used_today >= key.daily_token_limit {
            return Err((
                429,
                format!(
                    "API key '{}' đã đạt hạn mức token trong ngày ({} / {} tokens).",
                    key.label, used_today, key.daily_token_limit
                ),
                "rate_limit_error",
            ));
        }
    }
    Ok(())
}

fn server_lock() -> &'static Mutex<Option<RunningServer>> {
    SERVER_STATE.get_or_init(|| Mutex::new(None))
}

fn lifecycle_lock() -> &'static tokio::sync::Mutex<()> {
    LIFECYCLE_LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

pub fn is_server_running() -> bool {
    SERVER_ACTIVE_GEN.load(Ordering::SeqCst) != 0
}

fn cooldown_lock() -> &'static Mutex<HashMap<(String, String), CooldownEntry>> {
    COOLDOWNS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn unsupported_models_lock() -> &'static Mutex<HashMap<(String, String), i64>> {
    UNSUPPORTED_MODELS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn sticky_lock() -> &'static Mutex<HashMap<String, String>> {
    STICKY_MAP.get_or_init(|| Mutex::new(HashMap::new()))
}

// ============================================================================
// SQLite Schema & Persistence
// ============================================================================

fn ensure_api_schema(conn: &Connection) -> Result<(), String> {
    if API_SCHEMA_READY.get().is_some() {
        return Ok(());
    }
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS api_service_keys (
            id TEXT PRIMARY KEY,
            label TEXT NOT NULL,
            key_value TEXT NOT NULL UNIQUE,
            enabled INTEGER NOT NULL DEFAULT 1,
            is_master INTEGER NOT NULL DEFAULT 0,
            allowed_models_json TEXT NOT NULL DEFAULT '[]',
            pinned_account_ids_json TEXT NOT NULL DEFAULT '[]',
            allowed_profile TEXT NOT NULL DEFAULT 'all',
            rate_limit_rpm INTEGER NOT NULL DEFAULT 0,
            daily_token_limit INTEGER NOT NULL DEFAULT 0,
            expires_at INTEGER,
            created_at INTEGER NOT NULL,
            last_used_at INTEGER,
            total_requests INTEGER NOT NULL DEFAULT 0,
            total_tokens INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS api_service_logs (
            id TEXT PRIMARY KEY,
            timestamp INTEGER NOT NULL,
            api_key_id TEXT NOT NULL,
            api_key_label TEXT NOT NULL,
            account_id TEXT NOT NULL,
            account_email TEXT NOT NULL,
            protocol TEXT NOT NULL,
            request_model TEXT NOT NULL,
            resolved_model TEXT NOT NULL,
            stream INTEGER NOT NULL DEFAULT 0,
            status_code INTEGER NOT NULL,
            latency_ms INTEGER NOT NULL,
            input_tokens INTEGER NOT NULL DEFAULT 0,
            output_tokens INTEGER NOT NULL DEFAULT 0,
            thinking_tokens INTEGER NOT NULL DEFAULT 0,
            cache_read_tokens INTEGER NOT NULL DEFAULT 0,
            total_tokens INTEGER NOT NULL DEFAULT 0,
            error_message TEXT,
            request_preview TEXT NOT NULL DEFAULT '',
            response_preview TEXT NOT NULL DEFAULT '',
            client_ip TEXT NOT NULL DEFAULT ''
        );
        CREATE INDEX IF NOT EXISTS idx_api_logs_ts ON api_service_logs(timestamp DESC);
        ",
    )
    .map_err(|e| format!("Init api_service schema: {e}"))?;

    // Idempotent column migrations for existing databases upgraded from <= v1.1.6
    let _ = conn.execute(
        "ALTER TABLE api_service_keys ADD COLUMN allowed_profile TEXT NOT NULL DEFAULT 'all'",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE api_service_keys ADD COLUMN rate_limit_rpm INTEGER NOT NULL DEFAULT 0",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE api_service_keys ADD COLUMN daily_token_limit INTEGER NOT NULL DEFAULT 0",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE api_service_keys ADD COLUMN expires_at INTEGER",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE api_service_logs ADD COLUMN client_ip TEXT NOT NULL DEFAULT ''",
        [],
    );

    // Ensure at least one master API key exists
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM api_service_keys", [], |r| r.get(0))
        .unwrap_or(0);
    if count == 0 {
        let now = Utc::now().timestamp();
        let key_val = generate_api_key_value();
        let _ = conn.execute(
            "INSERT INTO api_service_keys
             (id, label, key_value, enabled, is_master, allowed_models_json, pinned_account_ids_json,
              allowed_profile, rate_limit_rpm, daily_token_limit, expires_at, created_at)
             VALUES (?1, ?2, ?3, 1, 1, '[]', '[]', 'all', 0, 0, NULL, ?4)",
            params!["master", "Default Master Key", key_val, now],
        );
    }
    let _ = API_SCHEMA_READY.set(());
    Ok(())
}

fn generate_api_key_value() -> String {
    let raw = Uuid::new_v4().to_string().replace('-', "");
    let raw2 = Uuid::new_v4().to_string().replace('-', "");
    format!("agt_onlydevs_{}{}", &raw[..16], &raw2[..12])
}

pub fn load_config() -> Result<ApiServiceConfig, String> {
    if let Ok(guard) = config_cache().read() {
        if let Some(cfg) = guard.as_ref() {
            return Ok(cfg.clone());
        }
    }
    let conn = open_db()?;
    ensure_api_schema(&conn)?;
    let raw: Option<String> = conn
        .query_row(
            "SELECT value FROM app_settings WHERE key = 'api_service_config'",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| format!("Read api_service_config: {e}"))?;
    let cfg = raw
        .and_then(|s| serde_json::from_str::<ApiServiceConfig>(&s).ok())
        .unwrap_or_default();
    if let Ok(mut guard) = config_cache().write() {
        *guard = Some(cfg.clone());
    }
    Ok(cfg)
}

pub fn save_config(config: ApiServiceConfig) -> Result<ApiServiceConfig, String> {
    let conn = open_db()?;
    ensure_api_schema(&conn)?;
    let mut cfg = config;
    if cfg.port < 1024 {
        cfg.port = 57120;
    }
    if cfg.timeout_seconds < 10 {
        cfg.timeout_seconds = 180;
    }
    if cfg.auto_refresh_quota_minutes > 1440 {
        cfg.auto_refresh_quota_minutes = 1440;
    }
    let raw = serde_json::to_string(&cfg).map_err(|e| format!("Serialize config: {e}"))?;
    conn.execute(
        "INSERT INTO app_settings (key, value) VALUES ('api_service_config', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![raw],
    )
    .map_err(|e| format!("Save api_service_config: {e}"))?;
    if let Ok(mut guard) = config_cache().write() {
        *guard = Some(cfg.clone());
    }
    Ok(cfg)
}

pub fn list_api_keys() -> Result<Vec<ApiKeyRecord>, String> {
    if let Ok(guard) = api_keys_cache().read() {
        if let Some(keys) = guard.as_ref() {
            return Ok(keys.clone());
        }
    }
    let conn = open_db()?;
    ensure_api_schema(&conn)?;
    let mut stmt = conn
        .prepare(
            "SELECT id, label, key_value, enabled, is_master, allowed_models_json,
                    pinned_account_ids_json, allowed_profile, rate_limit_rpm, daily_token_limit,
                    expires_at, created_at, last_used_at, total_requests, total_tokens
             FROM api_service_keys
             ORDER BY is_master DESC, created_at ASC",
        )
        .map_err(|e| format!("Prepare list_api_keys: {e}"))?;
    let rows = stmt
        .query_map([], |r| {
            let allowed_raw: String = r.get(5)?;
            let pinned_raw: String = r.get(6)?;
            let profile_raw: String = r.get::<_, String>(7).unwrap_or_else(|_| "all".into());
            Ok(ApiKeyRecord {
                id: r.get(0)?,
                label: r.get(1)?,
                key: r.get(2)?,
                enabled: r.get::<_, i64>(3)? != 0,
                is_master: r.get::<_, i64>(4)? != 0,
                allowed_models: serde_json::from_str(&allowed_raw).unwrap_or_default(),
                pinned_account_ids: serde_json::from_str(&pinned_raw).unwrap_or_default(),
                allowed_profile: normalize_allowed_profile(Some(&profile_raw)),
                rate_limit_rpm: r.get::<_, i64>(8).unwrap_or(0).max(0) as u32,
                daily_token_limit: r.get::<_, i64>(9).unwrap_or(0).max(0) as u64,
                expires_at: r.get::<_, Option<i64>>(10).unwrap_or(None),
                created_at: r.get(11)?,
                last_used_at: r.get(12)?,
                total_requests: r.get::<_, i64>(13)?.max(0) as u64,
                total_tokens: r.get::<_, i64>(14)?.max(0) as u64,
            })
        })
        .map_err(|e| format!("Query list_api_keys: {e}"))?;
    let keys: Vec<ApiKeyRecord> = rows.flatten().collect();
    if let Ok(mut guard) = api_keys_cache().write() {
        *guard = Some(keys.clone());
    }
    Ok(keys)
}

#[allow(clippy::too_many_arguments)]
pub fn create_api_key(
    label: String,
    allowed_models: Vec<String>,
    pinned_account_ids: Vec<String>,
    allowed_profile: Option<String>,
    rate_limit_rpm: Option<u32>,
    daily_token_limit: Option<u64>,
    expires_at: Option<i64>,
) -> Result<ApiKeyRecord, String> {
    let clean_label = label.trim();
    if clean_label.is_empty() {
        return Err("Tên API Key không được để trống.".into());
    }
    let conn = open_db()?;
    ensure_api_schema(&conn)?;
    let id = Uuid::new_v4().to_string();
    let key = generate_api_key_value();
    let now = Utc::now().timestamp();
    let allowed_json = serde_json::to_string(&allowed_models).unwrap_or_else(|_| "[]".into());
    let pinned_json = serde_json::to_string(&pinned_account_ids).unwrap_or_else(|_| "[]".into());
    let profile = normalize_allowed_profile(allowed_profile.as_deref());
    let rpm = rate_limit_rpm.unwrap_or(0);
    let daily_cap = daily_token_limit.unwrap_or(0);
    let exp = expires_at.filter(|&v| v > 0);
    conn.execute(
        "INSERT INTO api_service_keys
         (id, label, key_value, enabled, is_master, allowed_models_json, pinned_account_ids_json,
          allowed_profile, rate_limit_rpm, daily_token_limit, expires_at, created_at)
         VALUES (?1, ?2, ?3, 1, 0, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            id,
            clean_label,
            key,
            allowed_json,
            pinned_json,
            profile,
            rpm as i64,
            daily_cap as i64,
            exp,
            now
        ],
    )
    .map_err(|e| format!("Create API key: {e}"))?;
    invalidate_api_keys_cache();

    Ok(ApiKeyRecord {
        id,
        label: clean_label.to_string(),
        key,
        enabled: true,
        is_master: false,
        allowed_models,
        pinned_account_ids,
        allowed_profile: profile,
        rate_limit_rpm: rpm,
        daily_token_limit: daily_cap,
        expires_at: exp,
        created_at: now,
        last_used_at: None,
        total_requests: 0,
        total_tokens: 0,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn update_api_key(
    id: String,
    label: Option<String>,
    enabled: Option<bool>,
    allowed_models: Option<Vec<String>>,
    pinned_account_ids: Option<Vec<String>>,
    allowed_profile: Option<String>,
    rate_limit_rpm: Option<u32>,
    daily_token_limit: Option<u64>,
    expires_at: Option<Option<i64>>,
) -> Result<ApiKeyRecord, String> {
    let conn = open_db()?;
    ensure_api_schema(&conn)?;
    let keys = list_api_keys()?;
    let mut target = keys
        .into_iter()
        .find(|k| k.id == id)
        .ok_or_else(|| format!("Không tìm thấy API Key {id}"))?;
    if let Some(l) = label.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()) {
        target.label = l;
    }
    if let Some(e) = enabled {
        target.enabled = e;
    }
    if let Some(m) = allowed_models {
        target.allowed_models = m;
    }
    if let Some(p) = pinned_account_ids {
        target.pinned_account_ids = p;
    }
    if let Some(prof) = allowed_profile {
        target.allowed_profile = normalize_allowed_profile(Some(&prof));
    }
    if let Some(rpm) = rate_limit_rpm {
        target.rate_limit_rpm = rpm;
    }
    if let Some(cap) = daily_token_limit {
        target.daily_token_limit = cap;
    }
    if let Some(exp_opt) = expires_at {
        target.expires_at = exp_opt.filter(|&v| v > 0);
    }
    let allowed_json =
        serde_json::to_string(&target.allowed_models).unwrap_or_else(|_| "[]".into());
    let pinned_json =
        serde_json::to_string(&target.pinned_account_ids).unwrap_or_else(|_| "[]".into());
    conn.execute(
        "UPDATE api_service_keys
         SET label = ?1, enabled = ?2, allowed_models_json = ?3, pinned_account_ids_json = ?4,
             allowed_profile = ?5, rate_limit_rpm = ?6, daily_token_limit = ?7, expires_at = ?8
         WHERE id = ?9",
        params![
            target.label,
            if target.enabled { 1 } else { 0 },
            allowed_json,
            pinned_json,
            target.allowed_profile,
            target.rate_limit_rpm as i64,
            target.daily_token_limit as i64,
            target.expires_at,
            target.id
        ],
    )
    .map_err(|e| format!("Update API key: {e}"))?;
    invalidate_api_keys_cache();
    Ok(target)
}

pub fn rotate_api_key(id: String) -> Result<ApiKeyRecord, String> {
    let conn = open_db()?;
    ensure_api_schema(&conn)?;
    let new_key = generate_api_key_value();
    let changed = conn
        .execute(
            "UPDATE api_service_keys SET key_value = ?1 WHERE id = ?2",
            params![&new_key, &id],
        )
        .map_err(|e| format!("Rotate API key: {e}"))?;
    if changed == 0 {
        return Err(format!("Không tìm thấy API Key {id}"));
    }
    invalidate_api_keys_cache();
    list_api_keys()?
        .into_iter()
        .find(|k| k.id == id)
        .ok_or_else(|| "Rotate API key thất bại".into())
}

pub fn delete_api_key(id: String) -> Result<(), String> {
    let conn = open_db()?;
    ensure_api_schema(&conn)?;
    let is_master: i64 = conn
        .query_row(
            "SELECT is_master FROM api_service_keys WHERE id = ?1",
            params![&id],
            |r| r.get(0),
        )
        .unwrap_or(0);
    if is_master != 0 {
        return Err("Không thể xóa Master Key mặc định — bạn có thể bấm Xoay khóa (Rotate).".into());
    }
    conn.execute("DELETE FROM api_service_keys WHERE id = ?1", params![&id])
        .map_err(|e| format!("Delete API key: {e}"))?;
    invalidate_api_keys_cache();
    Ok(())
}

fn find_valid_api_key(token: &str) -> Option<ApiKeyRecord> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return None;
    }
    list_api_keys()
        .ok()?
        .into_iter()
        .find(|k| k.enabled && k.key == trimmed)
}

fn record_api_log(item: &ApiRequestLogItem) {
    bump_key_daily_tokens(&item.api_key_id, item.total_tokens);
    let Ok(conn) = open_db() else { return };
    if ensure_api_schema(&conn).is_err() {
        return;
    }
    let _ = conn.execute(
        "INSERT INTO api_service_logs
         (id, timestamp, api_key_id, api_key_label, account_id, account_email, protocol,
          request_model, resolved_model, stream, status_code, latency_ms,
          input_tokens, output_tokens, thinking_tokens, cache_read_tokens, total_tokens,
          error_message, request_preview, response_preview, client_ip)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21)",
        params![
            item.id,
            item.timestamp,
            item.api_key_id,
            item.api_key_label,
            item.account_id,
            item.account_email,
            item.protocol,
            item.request_model,
            item.resolved_model,
            if item.stream { 1 } else { 0 },
            item.status_code as i64,
            item.latency_ms as i64,
            item.input_tokens as i64,
            item.output_tokens as i64,
            item.thinking_tokens as i64,
            item.cache_read_tokens as i64,
            item.total_tokens as i64,
            item.error_message,
            item.request_preview,
            item.response_preview,
            item.client_ip,
        ],
    );
    let _ = conn.execute(
        "UPDATE api_service_keys
         SET last_used_at = ?1,
             total_requests = total_requests + 1,
             total_tokens = total_tokens + ?2
         WHERE id = ?3",
        params![item.timestamp, item.total_tokens as i64, item.api_key_id],
    );
    // Keep cached key request/token counters fresh in RAM without re-querying SQLite
    if let Ok(mut guard) = api_keys_cache().write() {
        if let Some(keys) = guard.as_mut() {
            if let Some(k) = keys.iter_mut().find(|k| k.id == item.api_key_id) {
                k.last_used_at = Some(item.timestamp);
                k.total_requests = k.total_requests.saturating_add(1);
                k.total_tokens = k.total_tokens.saturating_add(item.total_tokens);
            }
        }
    }
    // Keep table bounded to last 5000 logs
    let _ = conn.execute(
        "DELETE FROM api_service_logs WHERE id IN (
            SELECT id FROM api_service_logs ORDER BY timestamp DESC LIMIT -1 OFFSET 5000
         )",
        [],
    );
}

// ============================================================================
// Cooldown, Model-Compatibility & Smart Pool Management
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AccountProfile {
    /// Next-Gen / Full model accounts (Gemini 4 Argon / Neon / Hydrogen, Claude 5.5)
    FullNextgen,
    /// Classic accounts (Gemini 3.8 + Claude 4.6, no Argon / G4 / Claude 5.5)
    Classic,
    /// Quota not yet fetched (no model inventory known)
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ModelSupport {
    /// Quota fetched & does NOT list this model (or runtime 404 cached for this account+model)
    Unsupported = 0,
    /// Quota not fetched yet -> fallback only when no Exact account is available
    Unknown = 1,
    /// Quota fetched & explicitly lists this model
    Exact = 2,
}

fn is_real_model_entry(name: &str) -> bool {
    let n = name.trim();
    if n.is_empty() {
        return false;
    }
    let lower = n.to_ascii_lowercase();
    !(lower.contains(':')
        || lower.ends_with("-5h")
        || lower.ends_with("-weekly")
        || lower.starts_with("tab_")
        || lower.starts_with("chat_")
        || lower.contains("wtu"))
}

fn model_quota_scope(model: &str) -> &'static str {
    let lower = model.trim().to_ascii_lowercase();
    if lower.contains("argon") {
        "argon"
    } else if lower.contains("carbon") {
        "carbon"
    } else if lower.contains("claude") {
        "claude"
    } else {
        "gemini"
    }
}

fn classify_account_profile(acc: &AntigravityAccount) -> AccountProfile {
    let Some(q) = acc.quota.as_ref() else {
        return AccountProfile::Unknown;
    };
    let mut has_any_real_model = false;
    for m in &q.models {
        let lower = m.name.trim().to_ascii_lowercase();
        if lower == "gemini-argon-wtu-weekly"
            || lower == "argon:weekly"
            || lower.starts_with("gemini-4-argon")
            || lower.starts_with("gemini-4-neon")
            || lower.starts_with("claude-sonnet-5-5")
            || lower.starts_with("claude-opus-5-5")
            || (lower.starts_with("gemini-4-hydrogen") && !lower.ends_with("sonar"))
        {
            return AccountProfile::FullNextgen;
        }
        if is_real_model_entry(&m.name) {
            has_any_real_model = true;
        }
    }
    if has_any_real_model {
        AccountProfile::Classic
    } else {
        AccountProfile::Unknown
    }
}

fn is_runtime_unsupported(account_id: &str, model: &str) -> bool {
    let now = Utc::now().timestamp();
    let key = (account_id.to_string(), model.trim().to_ascii_lowercase());
    let Ok(map) = unsupported_models_lock().lock() else {
        return false;
    };
    map.get(&key).is_some_and(|&until| until > now)
}

fn mark_runtime_unsupported(account_id: &str, model: &str, ttl_secs: i64) {
    let until = Utc::now().timestamp() + ttl_secs.max(60);
    let key = (account_id.to_string(), model.trim().to_ascii_lowercase());
    if let Ok(mut map) = unsupported_models_lock().lock() {
        map.insert(key, until);
    }
}

fn account_model_support(acc: &AntigravityAccount, model: &str) -> ModelSupport {
    let target = model.trim().to_ascii_lowercase();
    if is_runtime_unsupported(&acc.id, &target) {
        return ModelSupport::Unsupported;
    }
    let Some(q) = acc.quota.as_ref() else {
        return ModelSupport::Unknown;
    };
    if q.is_forbidden {
        return ModelSupport::Unsupported;
    }
    let mut has_any_real_model = false;
    for m in &q.models {
        if !is_real_model_entry(&m.name) {
            continue;
        }
        has_any_real_model = true;
        if m.name.trim().eq_ignore_ascii_case(&target) {
            return ModelSupport::Exact;
        }
    }
    if !has_any_real_model {
        ModelSupport::Unknown
    } else {
        ModelSupport::Unsupported
    }
}

fn active_cooldowns() -> Vec<AccountCooldownInfo> {
    let now = Utc::now().timestamp();
    let Ok(mut map) = cooldown_lock().lock() else {
        return Vec::new();
    };
    map.retain(|_, v| v.until_ts > now);
    map.values()
        .map(|e| AccountCooldownInfo {
            account_id: e.account_id.clone(),
            email: e.email.clone(),
            scope: e.scope.clone(),
            until_ts: e.until_ts,
            remaining_seconds: (e.until_ts - now).max(0),
            reason: e.reason.clone(),
        })
        .collect()
}

fn mark_account_cooldown(
    account_id: &str,
    email: &str,
    scope: &str,
    seconds: u64,
    reason: String,
) {
    let until_ts = Utc::now().timestamp() + (seconds.max(5) as i64);
    let scope_clean = if scope.trim().is_empty() { "all" } else { scope.trim() };
    if let Ok(mut map) = cooldown_lock().lock() {
        map.insert(
            (account_id.to_string(), scope_clean.to_string()),
            CooldownEntry {
                account_id: account_id.to_string(),
                email: email.to_string(),
                scope: scope_clean.to_string(),
                until_ts,
                reason,
            },
        );
    }
}

pub fn clear_account_cooldowns(account_id: Option<String>) -> Result<(), String> {
    if let Ok(mut map) = cooldown_lock().lock() {
        if let Some(id) = account_id.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            map.retain(|(acc_id, _), _| acc_id != id);
        } else {
            map.clear();
        }
    }
    if let Ok(mut umap) = unsupported_models_lock().lock() {
        if let Some(id) = account_id.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            umap.retain(|(acc_id, _), _| acc_id != id);
        } else {
            umap.clear();
        }
    }
    Ok(())
}

fn is_on_cooldown_for_model(account_id: &str, model: &str) -> bool {
    let now = Utc::now().timestamp();
    let scope = model_quota_scope(model);
    let Ok(map) = cooldown_lock().lock() else {
        return false;
    };
    map.get(&(account_id.to_string(), "all".to_string()))
        .is_some_and(|e| e.until_ts > now)
        || map
            .get(&(account_id.to_string(), scope.to_string()))
            .is_some_and(|e| e.until_ts > now)
}

fn is_fully_cooled_down(account_id: &str) -> bool {
    let now = Utc::now().timestamp();
    let Ok(map) = cooldown_lock().lock() else {
        return false;
    };
    map.get(&(account_id.to_string(), "all".to_string()))
        .is_some_and(|e| e.until_ts > now)
}

fn parse_retry_seconds_from_body(body: &str, default_secs: u64) -> u64 {
    // 1. Structured Google ErrorInfo/RetryInfo: details[].metadata.quotaResetTimeStamp or details[].retryDelay
    if let Ok(val) = serde_json::from_str::<Value>(body) {
        if let Some(details) = val.pointer("/error/details").and_then(Value::as_array) {
            for d in details {
                if let Some(ts_str) = d
                    .pointer("/metadata/quotaResetTimeStamp")
                    .and_then(Value::as_str)
                {
                    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts_str) {
                        let diff = dt.timestamp() - Utc::now().timestamp();
                        if diff > 0 {
                            return (diff as u64).min(3600);
                        }
                    }
                }
                if let Some(delay_str) = d.get("retryDelay").and_then(Value::as_str) {
                    let trimmed = delay_str.trim().trim_end_matches('s');
                    if let Ok(secs) = trimmed.parse::<f64>() {
                        if secs > 0.0 {
                            return (secs.ceil() as u64).clamp(5, 3600);
                        }
                    }
                }
            }
            // Detail-free 429 (often transient/burst) -> shorter cooldown so accounts recover quickly
            if details.is_empty() {
                return default_secs.min(20).max(10);
            }
        }
    }
    // 2. Text quota reset format: "Your quota will reset after 2h7m23s"
    if let Ok(re) = Regex::new(r"(?i)reset after (?:(\d+)h)?(?:(\d+)m)?(?:(\d+)s)?") {
        if let Some(caps) = re.captures(body) {
            let h: u64 = caps.get(1).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);
            let m: u64 = caps.get(2).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);
            let s: u64 = caps.get(3).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);
            let total = h * 3600 + m * 60 + s;
            if total > 0 {
                return total.min(3600);
            }
        }
    }
    default_secs.max(15)
}

fn account_quota_score(acc: &AntigravityAccount, model: &str) -> i32 {
    let Some(q) = acc.quota.as_ref() else {
        return 50;
    };
    if q.is_forbidden {
        return -100;
    }
    let lower = model.trim().to_ascii_lowercase();
    let scope = model_quota_scope(&lower);

    let mut exact_pct: Option<i32> = None;
    let mut bucket_5h: Option<i32> = None;
    let mut bucket_weekly: Option<i32> = None;
    let mut bucket_exhausted = false;

    for m in &q.models {
        let name = m.name.trim().to_ascii_lowercase();
        if name == lower {
            exact_pct = Some(m.percentage);
            if m.summary_exhausted == Some(true) {
                bucket_exhausted = true;
            }
        }
        match scope {
            "argon" => {
                if name == "gemini-argon-wtu-weekly" || name == "argon:weekly" {
                    bucket_weekly = Some(m.percentage);
                    if m.summary_exhausted == Some(true) {
                        bucket_exhausted = true;
                    }
                }
            }
            "carbon" => {
                if name == "gemini-carbon-wtu-weekly" || name == "carbon:weekly" {
                    bucket_weekly = Some(m.percentage);
                    if m.summary_exhausted == Some(true) {
                        bucket_exhausted = true;
                    }
                }
            }
            "claude" => {
                if name == "3p-5h" || name == "claude:5h" {
                    bucket_5h = Some(m.percentage);
                    if m.summary_exhausted == Some(true) {
                        bucket_exhausted = true;
                    }
                } else if name == "3p-weekly" || name == "claude:weekly" {
                    bucket_weekly = Some(m.percentage);
                    if m.summary_exhausted == Some(true) {
                        bucket_exhausted = true;
                    }
                }
            }
            _ => {
                if name == "gemini-5h" || name == "gemini:5h" {
                    bucket_5h = Some(m.percentage);
                    if m.summary_exhausted == Some(true) {
                        bucket_exhausted = true;
                    }
                } else if name == "gemini-weekly" || name == "gemini:weekly" {
                    bucket_weekly = Some(m.percentage);
                    if m.summary_exhausted == Some(true) {
                        bucket_exhausted = true;
                    }
                }
            }
        }
    }

    if bucket_exhausted {
        return 0;
    }

    // Effective remaining quota is the bottleneck across exact model, 5h, and weekly limits
    let mut candidates: Vec<i32> = Vec::with_capacity(3);
    if let Some(p) = exact_pct {
        candidates.push(p);
    }
    if let Some(p) = bucket_5h {
        candidates.push(p);
    }
    if let Some(p) = bucket_weekly {
        candidates.push(p);
    }
    if let Some(min_pct) = candidates.into_iter().min() {
        return min_pct.clamp(0, 100);
    }

    if q.models.is_empty() {
        return 50;
    }
    let sum: i32 = q.models.iter().map(|m| m.percentage).sum();
    (sum / (q.models.len() as i32)).clamp(0, 100)
}

fn select_pool_members(
    all: Vec<AntigravityAccount>,
    cfg: &ApiServiceConfig,
    key: Option<&ApiKeyRecord>,
) -> Vec<AntigravityAccount> {
    all.into_iter()
        .filter(|a| {
            if a.disabled || a.token.refresh_token.trim().is_empty() {
                return false;
            }
            if let Some(k) = key {
                match k.allowed_profile.as_str() {
                    "classic" => {
                        if classify_account_profile(a) != AccountProfile::Classic {
                            return false;
                        }
                    }
                    "full_nextgen" => {
                        if classify_account_profile(a) != AccountProfile::FullNextgen {
                            return false;
                        }
                    }
                    _ => {}
                }
                if !k.pinned_account_ids.is_empty() {
                    return k.pinned_account_ids.contains(&a.id);
                }
            }
            if !cfg.member_account_ids.is_empty() {
                return cfg.member_account_ids.contains(&a.id);
            }
            cfg.use_all_accounts_when_empty
        })
        .collect()
}

fn rank_pool_candidates(
    all_members: Vec<AntigravityAccount>,
    cfg: &ApiServiceConfig,
    key_id: &str,
    model: &str,
    excluded: &HashSet<String>,
) -> Vec<AntigravityAccount> {
    let base: Vec<AntigravityAccount> = all_members
        .into_iter()
        .filter(|a| !excluded.contains(&a.id))
        .collect();

    if base.is_empty() {
        return Vec::new();
    }

    // 1. STRICT MODEL COMPATIBILITY FILTER:
    //    - Exact accounts (verified to have `model`) are always preferred.
    //    - Unknown accounts (quota never fetched yet) serve only as fallback.
    //    - Unsupported accounts (quota fetched & missing `model`, or runtime 404) are NEVER routed!
    let mut exact_accs: Vec<AntigravityAccount> = Vec::new();
    let mut unknown_accs: Vec<AntigravityAccount> = Vec::new();

    for acc in base {
        match account_model_support(&acc, model) {
            ModelSupport::Exact => exact_accs.push(acc),
            ModelSupport::Unknown => unknown_accs.push(acc),
            ModelSupport::Unsupported => {}
        }
    }

    // Detect if this model is a SHARED model present on both Classic and Full/Next-Gen accounts
    let has_classic_exact = exact_accs
        .iter()
        .any(|a| classify_account_profile(a) == AccountProfile::Classic);
    let has_full_exact = exact_accs
        .iter()
        .any(|a| classify_account_profile(a) == AccountProfile::FullNextgen);
    let is_shared_across_profiles = has_classic_exact && has_full_exact;
    let should_preserve_full = cfg.preserve_exclusive_accounts
        && is_shared_across_profiles
        && matches!(cfg.routing_strategy.as_str(), "smart" | "highest_quota");

    let order_group = |mut group: Vec<AntigravityAccount>| -> Vec<AntigravityAccount> {
        if group.is_empty() {
            return group;
        }
        // Prefer accounts NOT cooling down for this model's bucket; fall back to cooled-down only if all are cooling down
        let (healthy, cooled): (Vec<_>, Vec<_>) = group
            .into_iter()
            .partition(|a| !is_on_cooldown_for_model(&a.id, model));
        group = if !healthy.is_empty() { healthy } else { cooled };

        match cfg.routing_strategy.as_str() {
            "smart" => {
                // Tier 1: has remaining quota > 0 vs exhausted (0)
                // Tier 2: if shared model & preserve_exclusive_accounts -> prefer Classic accounts
                //         with healthy quota (> 15%) so Full/Argon accounts save quota for G4/Argon/C5.5!
                // Tier 3: highest effective quota score for this exact model & bucket
                group.sort_by(|a, b| {
                    let score_a = account_quota_score(a, model);
                    let score_b = account_quota_score(b, model);
                    let alive_a = score_a > 0;
                    let alive_b = score_b > 0;
                    let pref_a = should_preserve_full
                        && classify_account_profile(a) == AccountProfile::Classic
                        && score_a > 15;
                    let pref_b = should_preserve_full
                        && classify_account_profile(b) == AccountProfile::Classic
                        && score_b > 15;

                    alive_b
                        .cmp(&alive_a)
                        .then_with(|| pref_b.cmp(&pref_a))
                        .then_with(|| score_b.cmp(&score_a))
                        .then_with(|| a.email.cmp(&b.email))
                });
            }
            "highest_quota" => {
                group.sort_by(|a, b| {
                    let score_a = account_quota_score(a, model);
                    let score_b = account_quota_score(b, model);
                    let pref_a = should_preserve_full
                        && classify_account_profile(a) == AccountProfile::Classic
                        && score_a > 15;
                    let pref_b = should_preserve_full
                        && classify_account_profile(b) == AccountProfile::Classic
                        && score_b > 15;
                    (score_b > 0)
                        .cmp(&(score_a > 0))
                        .then_with(|| pref_b.cmp(&pref_a))
                        .then_with(|| score_b.cmp(&score_a))
                        .then_with(|| a.email.cmp(&b.email))
                });
            }
            "round_robin" => {
                // Keep >0 quota accounts ahead of 0% exhausted ones, then rotate
                let (mut alive, exhausted): (Vec<_>, Vec<_>) = group
                    .into_iter()
                    .partition(|a| account_quota_score(a, model) > 0);
                if !alive.is_empty() {
                    let idx = RR_COUNTER.fetch_add(1, Ordering::Relaxed) % alive.len();
                    alive.rotate_left(idx);
                    alive.extend(exhausted);
                    group = alive;
                } else {
                    group = exhausted;
                }
            }
            "sticky_session" => {
                let sticky_key = format!("{key_id}:{model}");
                // Sort by quota first, then move sticky account to front if it's alive
                group.sort_by(|a, b| {
                    account_quota_score(b, model)
                        .cmp(&account_quota_score(a, model))
                        .then_with(|| a.email.cmp(&b.email))
                });
                if let Ok(map) = sticky_lock().lock() {
                    if let Some(pinned_id) = map.get(&sticky_key) {
                        if let Some(pos) = group
                            .iter()
                            .position(|a| &a.id == pinned_id && account_quota_score(a, model) > 0)
                        {
                            let chosen = group.remove(pos);
                            group.insert(0, chosen);
                        }
                    }
                }
            }
            _ => {
                // failover: preserve member_account_ids order, keeping >0 quota first
                if !cfg.member_account_ids.is_empty() {
                    group.sort_by(|a, b| {
                        let alive_a = account_quota_score(a, model) > 0;
                        let alive_b = account_quota_score(b, model) > 0;
                        let pos_a = cfg
                            .member_account_ids
                            .iter()
                            .position(|id| id == &a.id)
                            .unwrap_or(usize::MAX);
                        let pos_b = cfg
                            .member_account_ids
                            .iter()
                            .position(|id| id == &b.id)
                            .unwrap_or(usize::MAX);
                        alive_b.cmp(&alive_a).then_with(|| pos_a.cmp(&pos_b))
                    });
                }
            }
        }
        group
    };

    let mut ordered = order_group(exact_accs);
    if !unknown_accs.is_empty() {
        ordered.extend(order_group(unknown_accs));
    }
    ordered
}

fn resolve_pool_accounts(
    cfg: &ApiServiceConfig,
    key: &ApiKeyRecord,
    model: &str,
    excluded: &HashSet<String>,
) -> Vec<AntigravityAccount> {
    let all = accounts::list_accounts_sync().unwrap_or_default();
    let members = select_pool_members(all, cfg, Some(key));
    rank_pool_candidates(members, cfg, &key.id, model, excluded)
}

// ============================================================================
// Model Resolution & Catalog
// ============================================================================

fn glob_match(pattern: &str, text: &str) -> bool {
    let p = pattern.trim().to_ascii_lowercase();
    let t = text.trim().to_ascii_lowercase();
    if p == t {
        return true;
    }
    if let Some(prefix) = p.strip_suffix('*') {
        return t.starts_with(prefix);
    }
    if let Some(suffix) = p.strip_prefix('*') {
        return t.ends_with(suffix);
    }
    false
}

fn resolve_model_alias(cfg: &ApiServiceConfig, requested: &str) -> String {
    let clean = requested
        .trim()
        .strip_prefix("antigravity/")
        .or_else(|| requested.trim().strip_prefix("ag/"))
        .unwrap_or(requested.trim());

    for rule in &cfg.model_rules {
        if rule.enabled && glob_match(&rule.pattern, clean) {
            return rule.target_model.trim().to_string();
        }
    }
    clean.to_string()
}

pub fn list_available_models() -> Result<Vec<ApiAvailableModel>, String> {
    let mut seen = HashSet::new();
    let mut models = Vec::new();
    let cfg = load_config().unwrap_or_default();
    let all_accs = accounts::list_accounts_sync().unwrap_or_default();
    let pool_accs = select_pool_members(all_accs.clone(), &cfg, None);
    let pool_count = pool_accs.len();

    let count_supporting = |target_model: &str| -> usize {
        pool_accs
            .iter()
            .filter(|a| account_model_support(a, target_model) == ModelSupport::Exact)
            .count()
    };

    for (id, display, family, thinking) in BUILTIN_MODELS {
        seen.insert((*id).to_string());
        models.push(ApiAvailableModel {
            id: (*id).to_string(),
            display_name: (*display).to_string(),
            family: (*family).to_string(),
            thinking: *thinking,
            source: "builtin".into(),
            supported_account_count: count_supporting(id),
            pool_account_count: pool_count,
        });
    }

    // Add live discovered models from user's fetched Antigravity accounts
    for acc in all_accs {
        if let Some(q) = acc.quota {
            for m in q.models {
                let n = m.name.trim();
                if !is_real_model_entry(n) || seen.contains(n) {
                    continue;
                }
                seen.insert(n.to_string());
                let lower = n.to_ascii_lowercase();
                let family = if lower.contains("claude") {
                    "claude"
                } else if lower.contains("image") {
                    "image"
                } else if lower.contains("gpt") {
                    "openai"
                } else {
                    "gemini"
                };
                models.push(ApiAvailableModel {
                    id: n.to_string(),
                    display_name: m.display_name.unwrap_or_else(|| n.to_string()),
                    family: family.into(),
                    thinking: lower.contains("thinking")
                        || lower.contains("pro")
                        || lower.contains("argon")
                        || lower.contains("carbon")
                        || lower.contains("high"),
                    source: "account-live".into(),
                    supported_account_count: count_supporting(n),
                    pool_account_count: pool_count,
                });
            }
        }
    }

    // Add enabled custom aliases so clients see them in /v1/models too
    for rule in cfg.model_rules {
        if rule.enabled && !rule.pattern.contains('*') && !seen.contains(&rule.pattern) {
            seen.insert(rule.pattern.clone());
            models.push(ApiAvailableModel {
                id: rule.pattern.clone(),
                display_name: format!("{} → {}", rule.pattern, rule.target_model),
                family: "alias".into(),
                thinking: true,
                source: "alias".into(),
                supported_account_count: count_supporting(&rule.target_model),
                pool_account_count: pool_count,
            });
        }
    }

    Ok(models)
}

// ============================================================================
// Request Translators (OpenAI & Anthropic -> Antigravity Cloud Code)
// ============================================================================

fn sanitize_function_name(name: &str) -> String {
    let mut s: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | ':' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if s.is_empty() {
        return "_unknown".into();
    }
    if !s.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_') {
        s.insert(0, '_');
    }
    s.chars().take(64).collect()
}

fn is_native_ag_tool(name: &str) -> bool {
    AG_NATIVE_TOOL_NAMES.contains(&name)
}

fn cloak_tool_name(sanitized: &str) -> String {
    if is_native_ag_tool(sanitized) || sanitized.ends_with(AG_TOOL_SUFFIX) {
        sanitized.to_string()
    } else {
        let mut s = format!("{sanitized}{AG_TOOL_SUFFIX}");
        if s.len() > 64 {
            let keep = 64 - AG_TOOL_SUFFIX.len();
            s = format!("{}{}", &sanitized[..keep], AG_TOOL_SUFFIX);
        }
        s
    }
}

fn uncloak_tool_name(name: &str, tool_name_map: &HashMap<String, String>) -> String {
    if let Some(orig) = tool_name_map.get(name) {
        return orig.clone();
    }
    name.strip_suffix(AG_TOOL_SUFFIX)
        .unwrap_or(name)
        .to_string()
}

/// Rewrite competing-client branding in system prompts so Antigravity Cloud Code
/// does not flag the request with a fake detail-free 429 RESOURCE_EXHAUSTED.
fn sanitize_antigravity_system_prompt(raw: &str) -> String {
    let mut s = raw.replace(
        "You are a Claude agent, built on Anthropic's Claude Agent SDK.",
        "",
    );
    if let Ok(re_billing) = Regex::new(r"(?im)^x-anthropic-billing-header:[^\n]*(?:\r?\n)*") {
        s = re_billing.replace_all(&s, "").into_owned();
    }
    if let Ok(re_hermes) = Regex::new(
        r"(?i)You are Hermes(?: Agent)?(?:,\s*(?:an intelligent AI assistant|an AI assistant|an AI agent))?(?:,?\s*(?:built|created)\s+by\s+Nous Research)?\.",
    ) {
        s = re_hermes.replace_all(&s, "You are an AI assistant.").into_owned();
    }
    if let Ok(re_cc) = Regex::new(r"(?i)You are Claude Code, Anthropic's official CLI for Claude\.") {
        s = re_cc.replace_all(&s, "You are Antigravity, an AI coding assistant.").into_owned();
    }
    if let Ok(re_oc) = Regex::new(r"(?i)opencode") {
        s = re_oc
            .replace_all(&s, |caps: &regex::Captures| {
                match caps.get(0).map(|m| m.as_str()).unwrap_or("") {
                    "OpenCode" => "Antigravity",
                    "OPENCODE" => "ANTIGRAVITY",
                    _ => "antigravity",
                }
            })
            .into_owned();
    }
    s
}

fn clean_json_schema_for_antigravity(val: Value) -> Value {
    match val {
        Value::Object(map) => {
            let mut out = Map::new();
            for (k, v) in map {
                if matches!(
                    k.as_str(),
                    "$schema"
                        | "$id"
                        | "$defs"
                        | "$ref"
                        | "$comment"
                        | "definitions"
                        | "additionalProperties"
                        | "additionalItems"
                        | "unevaluatedProperties"
                        | "unevaluatedItems"
                        | "contentSchema"
                        | "prefixItems"
                        | "uniqueItems"
                        | "contains"
                        | "default"
                        | "examples"
                        | "title"
                        | "optional"
                        | "~optional"
                        | "deprecated"
                        | "readOnly"
                        | "writeOnly"
                        | "minLength"
                        | "maxLength"
                        | "minItems"
                        | "maxItems"
                        | "minProperties"
                        | "maxProperties"
                        | "multipleOf"
                        | "format"
                        | "exclusiveMinimum"
                        | "exclusiveMaximum"
                        | "const"
                        | "propertyNames"
                        | "patternProperties"
                        | "enumDescriptions"
                        | "anyOf"
                        | "oneOf"
                        | "allOf"
                        | "not"
                        | "dependencies"
                        | "dependentSchemas"
                        | "dependentRequired"
                        | "if"
                        | "then"
                        | "else"
                        | "contentMediaType"
                        | "contentEncoding"
                        | "errorMessage"
                        | "errorMessages"
                        | "x-errorMessage"
                        | "x-errorMessages"
                        | "markdownDescription"
                        | "x-intellij-html-description"
                        | "x-taplo-info"
                        | "x-taplo"
                        | "doNotSuggest"
                        | "suggestSortText"
                        | "cornerRadius"
                        | "fillColor"
                        | "fontFamily"
                        | "fontSize"
                        | "fontWeight"
                        | "gap"
                        | "padding"
                        | "strokeColor"
                        | "strokeThickness"
                        | "textColor"
                ) {
                    continue;
                }
                if k == "type" {
                    if let Value::Array(arr) = &v {
                        if let Some(first) = arr
                            .iter()
                            .filter_map(Value::as_str)
                            .find(|s| *s != "null")
                        {
                            out.insert(k, Value::String(first.to_string()));
                            continue;
                        }
                    }
                }
                out.insert(k, clean_json_schema_for_antigravity(v));
            }
            if out.get("type").and_then(Value::as_str) == Some("object")
                && !out.contains_key("properties")
            {
                out.insert("properties".into(), json!({}));
            }
            Value::Object(out)
        }
        Value::Array(arr) => Value::Array(
            arr.into_iter()
                .map(clean_json_schema_for_antigravity)
                .collect(),
        ),
        other => other,
    }
}

fn extract_text_from_value(val: &Value) -> String {
    match val {
        Value::String(s) => s.clone(),
        Value::Array(arr) => arr
            .iter()
            .filter_map(|item| {
                item.get("text")
                    .and_then(Value::as_str)
                    .or_else(|| item.as_str())
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn convert_openai_content_to_parts(val: &Value) -> Vec<Value> {
    match val {
        Value::String(s) => {
            if s.is_empty() {
                vec![]
            } else {
                vec![json!({ "text": s })]
            }
        }
        Value::Array(arr) => {
            let mut parts = Vec::new();
            for item in arr {
                let kind = item.get("type").and_then(Value::as_str).unwrap_or("");
                if kind == "text" {
                    if let Some(t) = item.get("text").and_then(Value::as_str) {
                        if !t.is_empty() {
                            parts.push(json!({ "text": t }));
                        }
                    }
                } else if kind == "image_url" {
                    if let Some(url) = item.pointer("/image_url/url").and_then(Value::as_str) {
                        if let Some(rest) = url.strip_prefix("data:") {
                            if let Some((meta, b64)) = rest.split_once(";base64,") {
                                parts.push(json!({
                                    "inlineData": {
                                        "mimeType": meta,
                                        "data": b64
                                    }
                                }));
                            }
                        }
                    }
                }
            }
            parts
        }
        _ => vec![],
    }
}

fn merge_adjacent_contents(contents: Vec<Value>) -> Vec<Value> {
    let mut merged: Vec<Value> = Vec::new();
    for item in contents {
        let Some(role) = item.get("role").and_then(Value::as_str) else {
            continue;
        };
        let Some(parts) = item.get("parts").and_then(Value::as_array) else {
            continue;
        };
        if parts.is_empty() {
            continue;
        }
        if let Some(last) = merged.last_mut() {
            if last.get("role").and_then(Value::as_str) == Some(role) {
                if let Some(last_parts) = last.get_mut("parts").and_then(Value::as_array_mut) {
                    last_parts.extend(parts.iter().cloned());
                    continue;
                }
            }
        }
        merged.push(item);
    }
    merged
}

fn openai_body_to_antigravity_envelope(
    body: &Value,
    resolved_model: &str,
    project_id: &str,
    session_seed: &str,
) -> (Value, String, HashMap<String, String>) {
    let mut system_texts: Vec<String> = Vec::new();
    let mut contents: Vec<Value> = Vec::new();
    let mut tool_id_to_name: HashMap<String, String> = HashMap::new();
    let mut tool_name_map: HashMap<String, String> = HashMap::new();
    let mut first_user_preview = String::new();

    let has_tools = body
        .get("tools")
        .and_then(Value::as_array)
        .is_some_and(|a| !a.is_empty());

    if let Some(msgs) = body.get("messages").and_then(Value::as_array) {
        for m in msgs {
            if m.get("role").and_then(Value::as_str) == Some("assistant") {
                if let Some(tcs) = m.get("tool_calls").and_then(Value::as_array) {
                    for tc in tcs {
                        if let (Some(id), Some(name)) = (
                            tc.get("id").and_then(Value::as_str),
                            tc.pointer("/function/name").and_then(Value::as_str),
                        ) {
                            tool_id_to_name.insert(id.to_string(), name.to_string());
                        }
                    }
                }
            }
        }

        for m in msgs {
            let role = m.get("role").and_then(Value::as_str).unwrap_or("user");
            let content = m.get("content").unwrap_or(&Value::Null);
            match role {
                "system" | "developer" => {
                    let t = sanitize_antigravity_system_prompt(&extract_text_from_value(content));
                    if !t.trim().is_empty() {
                        system_texts.push(t);
                    }
                }
                "user" => {
                    let parts = convert_openai_content_to_parts(content);
                    if first_user_preview.is_empty() {
                        first_user_preview = extract_text_from_value(content);
                    }
                    if !parts.is_empty() {
                        contents.push(json!({ "role": "user", "parts": parts }));
                    }
                }
                "assistant" => {
                    let mut parts = Vec::new();
                    let text = extract_text_from_value(content);
                    if !text.is_empty() {
                        parts.push(json!({ "text": text }));
                    }
                    if let Some(tcs) = m.get("tool_calls").and_then(Value::as_array) {
                        for (idx, tc) in tcs.iter().enumerate() {
                            let id = tc.get("id").and_then(Value::as_str).unwrap_or("");
                            let name = tc
                                .pointer("/function/name")
                                .and_then(Value::as_str)
                                .unwrap_or("tool");
                            let args_str = tc
                                .pointer("/function/arguments")
                                .and_then(Value::as_str)
                                .unwrap_or("{}");
                            let args: Value =
                                serde_json::from_str(args_str).unwrap_or_else(|_| json!({}));
                            let sanitized = sanitize_function_name(name);
                            let wire_name = if has_tools {
                                let cloaked = cloak_tool_name(&sanitized);
                                tool_name_map.entry(cloaked.clone()).or_insert_with(|| name.to_string());
                                cloaked
                            } else {
                                sanitized
                            };
                            let mut part = json!({
                                "functionCall": {
                                    "id": id,
                                    "name": wire_name,
                                    "args": args
                                }
                            });
                            // In parallel calls, only the first call gets fallback thoughtSignature
                            if idx == 0 {
                                if let Some(obj) = part.as_object_mut() {
                                    obj.insert(
                                        "thoughtSignature".into(),
                                        Value::String(DEFAULT_THINKING_AG_SIGNATURE.into()),
                                    );
                                }
                            }
                            parts.push(part);
                        }
                    }
                    if !parts.is_empty() {
                        contents.push(json!({ "role": "model", "parts": parts }));
                    }
                }
                "tool" => {
                    let tc_id = m.get("tool_call_id").and_then(Value::as_str).unwrap_or("");
                    let name = tool_id_to_name
                        .get(tc_id)
                        .cloned()
                        .or_else(|| m.get("name").and_then(Value::as_str).map(str::to_string))
                        .unwrap_or_else(|| "tool".into());
                    let sanitized = sanitize_function_name(&name);
                    let wire_name = if has_tools {
                        let cloaked = cloak_tool_name(&sanitized);
                        tool_name_map.entry(cloaked.clone()).or_insert_with(|| name.clone());
                        cloaked
                    } else {
                        sanitized
                    };
                    let raw_text = extract_text_from_value(content);
                    let parsed = serde_json::from_str::<Value>(&raw_text)
                        .unwrap_or_else(|_| json!({ "result": raw_text }));
                    let resp_obj = json!({ "result": parsed });
                    contents.push(json!({
                        "role": "user",
                        "parts": [{
                            "functionResponse": {
                                "id": tc_id,
                                "name": wire_name,
                                "response": resp_obj
                            }
                        }]
                    }));
                }
                _ => {}
            }
        }
    }

    let merged_contents = merge_adjacent_contents(contents);
    let mut gen_cfg = Map::new();
    if let Some(t) = body.get("temperature").and_then(Value::as_f64) {
        gen_cfg.insert("temperature".into(), json!(t));
    }
    if let Some(p) = body.get("top_p").and_then(Value::as_f64) {
        gen_cfg.insert("topP".into(), json!(p));
    }
    let max_out = body
        .get("max_tokens")
        .or_else(|| body.get("max_completion_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or(8192)
        .min(MAX_ANTIGRAVITY_OUTPUT_TOKENS);
    gen_cfg.insert("maxOutputTokens".into(), json!(max_out));

    let mut req_obj = Map::new();
    req_obj.insert("contents".into(), Value::Array(merged_contents.clone()));
    req_obj.insert("generationConfig".into(), Value::Object(gen_cfg));
    if !system_texts.is_empty() {
        req_obj.insert(
            "systemInstruction".into(),
            json!({
                "role": "user",
                "parts": [{ "text": system_texts.join("\n\n") }]
            }),
        );
    }

    // Tools conversion + Anti-ban Cloaking (_ide suffix + native AG decoy tools)
    if let Some(tools) = body.get("tools").and_then(Value::as_array) {
        let mut decls = Vec::new();
        let mut seen_names = HashSet::new();
        for t in tools {
            let (name, desc, params_val) = if let Some(f) = t.get("function") {
                (
                    f.get("name").and_then(Value::as_str).unwrap_or(""),
                    f.get("description").and_then(Value::as_str).unwrap_or(""),
                    f.get("parameters").cloned(),
                )
            } else {
                (
                    t.get("name").and_then(Value::as_str).unwrap_or(""),
                    t.get("description").and_then(Value::as_str).unwrap_or(""),
                    t.get("input_schema").cloned(),
                )
            };
            if name.is_empty() {
                continue;
            }
            let sanitized = sanitize_function_name(name);
            let wire_name = cloak_tool_name(&sanitized);
            if !seen_names.insert(wire_name.clone()) {
                continue;
            }
            tool_name_map.insert(wire_name.clone(), name.to_string());
            let parameters = params_val
                .map(clean_json_schema_for_antigravity)
                .unwrap_or_else(|| {
                    json!({
                        "type": "object",
                        "properties": {
                            "reason": { "type": "string", "description": "Brief explanation" }
                        },
                        "required": ["reason"]
                    })
                });
            decls.push(json!({
                "name": wire_name,
                "description": desc,
                "parameters": parameters
            }));
        }
        if !decls.is_empty() {
            // Append native AG decoy tools after client tools so payload matches official Antigravity IDE shape
            for decoy_name in AG_NATIVE_TOOL_NAMES {
                if seen_names.insert((*decoy_name).to_string()) {
                    decls.push(json!({
                        "name": decoy_name,
                        "description": "This tool is currently unavailable.",
                        "parameters": { "type": "OBJECT", "properties": {}, "required": [] }
                    }));
                }
            }
            req_obj.insert("tools".into(), json!([{ "functionDeclarations": decls }]));
            req_obj.insert(
                "toolConfig".into(),
                json!({ "functionCallingConfig": { "mode": "VALIDATED" } }),
            );
        }
    }

    for k in BLACKLISTED_FIELDS {
        req_obj.remove(*k);
    }
    let session_id = Utc::now().timestamp_millis().to_string();
    req_obj.insert("sessionId".into(), Value::String(session_id));

    let conv_id = Uuid::new_v4();
    let traj_id = Uuid::new_v4();
    let step = (merged_contents.len() * 2).saturating_sub(1).max(1);
    let request_id = format!(
        "agent/{}/{}/{}/{}",
        conv_id,
        Utc::now().timestamp_millis(),
        traj_id,
        step
    );
    let _ = session_seed;

    // Official Antigravity client omits `requestType` on the chat/agent path.
    // Sending `requestType: "agent"` triggers Google's detail-free 429 RESOURCE_EXHAUSTED
    // even when full quota is available. Only image generation models keep `requestType: "image_gen"`.
    let mut envelope_map = Map::new();
    envelope_map.insert("project".into(), json!(project_id));
    envelope_map.insert("model".into(), json!(resolved_model));
    envelope_map.insert("userAgent".into(), json!("antigravity"));
    if resolved_model.to_ascii_lowercase().contains("image") {
        envelope_map.insert("requestType".into(), json!("image_gen"));
    }
    envelope_map.insert("requestId".into(), json!(request_id));
    envelope_map.insert("request".into(), Value::Object(req_obj));

    let preview: String = first_user_preview.chars().take(240).collect();
    (Value::Object(envelope_map), preview, tool_name_map)
}

fn responses_body_to_openai_body(body: &Value) -> Value {
    let mut messages: Vec<Value> = Vec::new();
    if let Some(instr) = body.get("instructions").and_then(Value::as_str) {
        if !instr.trim().is_empty() {
            messages.push(json!({ "role": "system", "content": instr }));
        }
    }
    if let Some(input) = body.get("input") {
        match input {
            Value::String(s) => {
                messages.push(json!({ "role": "user", "content": s }));
            }
            Value::Array(items) => {
                for item in items {
                    let item_type = item.get("type").and_then(Value::as_str).unwrap_or("");
                    if item_type == "function_call" {
                        let call_id = item
                            .get("call_id")
                            .or_else(|| item.get("id"))
                            .and_then(Value::as_str)
                            .unwrap_or("");
                        let name = item.get("name").and_then(Value::as_str).unwrap_or("tool");
                        let args = item
                            .get("arguments")
                            .and_then(Value::as_str)
                            .unwrap_or("{}");
                        messages.push(json!({
                            "role": "assistant",
                            "content": "",
                            "tool_calls": [{
                                "id": call_id,
                                "type": "function",
                                "function": { "name": name, "arguments": args }
                            }]
                        }));
                        continue;
                    }
                    if item_type == "function_call_output" {
                        let call_id = item
                            .get("call_id")
                            .and_then(Value::as_str)
                            .unwrap_or("");
                        let output_txt = extract_text_from_value(
                            item.get("output").unwrap_or(&Value::Null),
                        );
                        messages.push(json!({
                            "role": "tool",
                            "tool_call_id": call_id,
                            "content": output_txt
                        }));
                        continue;
                    }
                    let role = item
                        .get("role")
                        .and_then(Value::as_str)
                        .unwrap_or("user");
                    let mapped_role = if role == "developer" { "system" } else { role };
                    let content = item.get("content").unwrap_or(&Value::Null);
                    let txt = extract_text_from_value(content);
                    if !txt.trim().is_empty() {
                        messages.push(json!({ "role": mapped_role, "content": txt }));
                    }
                }
            }
            _ => {}
        }
    }
    if messages.is_empty() {
        if let Some(existing) = body.get("messages").and_then(Value::as_array) {
            messages = existing.clone();
        }
    }

    let mut out = Map::new();
    out.insert(
        "model".into(),
        body.get("model")
            .cloned()
            .unwrap_or_else(|| json!("gemini-4-neon-medium")),
    );
    out.insert("messages".into(), Value::Array(messages));
    if let Some(m) = body
        .get("max_output_tokens")
        .or_else(|| body.get("max_tokens"))
    {
        out.insert("max_tokens".into(), m.clone());
    }
    if let Some(t) = body.get("temperature") {
        out.insert("temperature".into(), t.clone());
    }
    if let Some(p) = body.get("top_p") {
        out.insert("top_p".into(), p.clone());
    }
    if let Some(s) = body.get("stream") {
        out.insert("stream".into(), s.clone());
    }
    if let Some(tools) = body.get("tools") {
        out.insert("tools".into(), tools.clone());
    }
    Value::Object(out)
}

fn anthropic_body_to_openai_body(body: &Value) -> Value {
    let mut messages: Vec<Value> = Vec::new();
    if let Some(sys) = body.get("system") {
        let sys_text = extract_text_from_value(sys);
        if !sys_text.trim().is_empty() {
            messages.push(json!({ "role": "system", "content": sys_text }));
        }
    }
    if let Some(src_msgs) = body.get("messages").and_then(Value::as_array) {
        for m in src_msgs {
            let role = m.get("role").and_then(Value::as_str).unwrap_or("user");
            let content = m.get("content").unwrap_or(&Value::Null);
            if let Value::String(s) = content {
                messages.push(json!({ "role": role, "content": s }));
                continue;
            }
            if let Value::Array(blocks) = content {
                let mut text_parts = Vec::new();
                let mut tool_calls = Vec::new();
                let mut tool_results = Vec::new();
                for b in blocks {
                    let btype = b.get("type").and_then(Value::as_str).unwrap_or("");
                    match btype {
                        "text" => {
                            if let Some(t) = b.get("text").and_then(Value::as_str) {
                                text_parts.push(t.to_string());
                            }
                        }
                        "tool_use" => {
                            let id = b.get("id").and_then(Value::as_str).unwrap_or("");
                            let name = b.get("name").and_then(Value::as_str).unwrap_or("tool");
                            let input = b.get("input").cloned().unwrap_or_else(|| json!({}));
                            tool_calls.push(json!({
                                "id": id,
                                "type": "function",
                                "function": {
                                    "name": name,
                                    "arguments": serde_json::to_string(&input).unwrap_or_else(|_| "{}".into())
                                }
                            }));
                        }
                        "tool_result" => {
                            let tid = b.get("tool_use_id").and_then(Value::as_str).unwrap_or("");
                            let txt = extract_text_from_value(b.get("content").unwrap_or(&Value::Null));
                            tool_results.push(json!({
                                "role": "tool",
                                "tool_call_id": tid,
                                "content": txt
                            }));
                        }
                        _ => {}
                    }
                }
                if !tool_results.is_empty() {
                    messages.extend(tool_results);
                }
                if !text_parts.is_empty() || !tool_calls.is_empty() {
                    let mut obj = Map::new();
                    obj.insert("role".into(), json!(role));
                    obj.insert("content".into(), json!(text_parts.join("\n")));
                    if !tool_calls.is_empty() {
                        obj.insert("tool_calls".into(), Value::Array(tool_calls));
                    }
                    messages.push(Value::Object(obj));
                }
            }
        }
    }

    let mut out = Map::new();
    out.insert(
        "model".into(),
        body.get("model")
            .cloned()
            .unwrap_or_else(|| json!("claude-sonnet-4-6")),
    );
    out.insert("messages".into(), Value::Array(messages));
    if let Some(m) = body.get("max_tokens") {
        out.insert("max_tokens".into(), m.clone());
    }
    if let Some(t) = body.get("temperature") {
        out.insert("temperature".into(), t.clone());
    }
    if let Some(p) = body.get("top_p") {
        out.insert("top_p".into(), p.clone());
    }
    if let Some(s) = body.get("stream") {
        out.insert("stream".into(), s.clone());
    }
    if let Some(tools) = body.get("tools") {
        out.insert("tools".into(), tools.clone());
    }
    Value::Object(out)
}

// ============================================================================
// Upstream Antigravity Execution + SSE Parser
// ============================================================================

#[derive(Default)]
struct ParsedUpstreamChunk {
    text: String,
    thinking: String,
    tool_calls: Vec<(String, String, Value)>,
    finish_reason: Option<String>,
    input_tokens: u64,
    output_tokens: u64,
    thinking_tokens: u64,
    cache_read_tokens: u64,
}

fn parse_antigravity_response_payload(val: &Value) -> ParsedUpstreamChunk {
    let mut out = ParsedUpstreamChunk::default();
    let root = val.get("response").unwrap_or(val);

    if let Some(usage) = root.get("usageMetadata") {
        out.input_tokens = usage
            .get("promptTokenCount")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        out.thinking_tokens = usage
            .get("thoughtsTokenCount")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let candidates_tokens = usage
            .get("candidatesTokenCount")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let total_tokens = usage
            .get("totalTokenCount")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        // Antigravity 2.25+ reports candidatesTokenCount separately from thoughtsTokenCount
        // (prompt + candidates + thoughts == total). Only subtract if thoughts are folded in.
        out.output_tokens = if out.thinking_tokens > 0
            && candidates_tokens >= out.thinking_tokens
            && total_tokens > 0
            && out.input_tokens.saturating_add(candidates_tokens) == total_tokens
        {
            candidates_tokens.saturating_sub(out.thinking_tokens)
        } else {
            candidates_tokens
        };
        out.cache_read_tokens = usage
            .get("cachedContentTokenCount")
            .and_then(Value::as_u64)
            .unwrap_or(0);
    }

    if let Some(cands) = root.get("candidates").and_then(Value::as_array) {
        if let Some(first) = cands.first() {
            if let Some(fr) = first.get("finishReason").and_then(Value::as_str) {
                out.finish_reason = Some(match fr {
                    "STOP" => "stop".into(),
                    "MAX_TOKENS" => "length".into(),
                    other => other.to_ascii_lowercase(),
                });
            }
            if let Some(parts) = first.pointer("/content/parts").and_then(Value::as_array) {
                for p in parts {
                    let is_thought = p.get("thought").and_then(Value::as_bool).unwrap_or(false);
                    if let Some(t) = p.get("text").and_then(Value::as_str) {
                        if is_thought {
                            out.thinking.push_str(t);
                        } else {
                            out.text.push_str(t);
                        }
                    }
                    if let Some(fc) = p.get("functionCall") {
                        let id = fc
                            .get("id")
                            .and_then(Value::as_str)
                            .filter(|s| !s.is_empty())
                            .map(str::to_string)
                            .unwrap_or_else(|| format!("call_{}", &Uuid::new_v4().to_string()[..12]));
                        let name = fc
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or("tool")
                            .to_string();
                        let args = fc.get("args").cloned().unwrap_or_else(|| json!({}));
                        out.tool_calls.push((id, name, args));
                    }
                }
            }
        }
    }

    out
}

fn build_upstream_client(cfg: &ApiServiceConfig) -> Result<reqwest::Client, String> {
    let timeout_secs = cfg.timeout_seconds.max(30);
    let proxy_key = cfg
        .proxy_url
        .as_deref()
        .map(str::trim)
        .unwrap_or("")
        .to_string();
    let cache_key = (timeout_secs, proxy_key.clone());

    if let Ok(guard) = upstream_client_cache().lock() {
        if let Some((ref k, ref client)) = *guard {
            if *k == cache_key {
                return Ok(client.clone());
            }
        }
    }

    let mut builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .connect_timeout(Duration::from_secs(15))
        .tcp_nodelay(true)
        .tcp_keepalive(Some(Duration::from_secs(30)))
        .pool_idle_timeout(Duration::from_secs(90))
        .pool_max_idle_per_host(16)
        .user_agent(ANTIGRAVITY_IDE_USER_AGENT);
    if !proxy_key.is_empty() {
        if let Ok(p) = reqwest::Proxy::all(&proxy_key) {
            builder = builder.proxy(p);
        }
    }
    let client = builder
        .build()
        .map_err(|e| format!("Build upstream HTTP client: {e}"))?;

    if let Ok(mut guard) = upstream_client_cache().lock() {
        *guard = Some((cache_key, client.clone()));
    }
    Ok(client)
}

fn generate_fallback_project_id() -> String {
    let id = Uuid::new_v4().to_string();
    format!("bright-spark-{}", &id[..5])
}

async fn send_upstream_with_failover(
    cfg: &ApiServiceConfig,
    key: &ApiKeyRecord,
    resolved_model: &str,
    openai_body: &Value,
    stream: bool,
) -> Result<
    (
        reqwest::Response,
        AntigravityAccount,
        String,
        HashMap<String, String>,
    ),
    (u16, String, String, String, String),
> {
    let client = build_upstream_client(cfg)
        .map_err(|e| (500u16, e, String::new(), String::new(), String::new()))?;
    let all_members = select_pool_members(
        accounts::list_accounts_sync().unwrap_or_default(),
        cfg,
        Some(key),
    );
    let total_members = all_members.len();
    if total_members == 0 {
        return Err((
            503u16,
            "Không có tài khoản Antigravity khả dụng trong Pool.".to_string(),
            String::new(),
            String::new(),
            String::new(),
        ));
    }

    let initial_compatible = rank_pool_candidates(
        all_members,
        cfg,
        &key.id,
        resolved_model,
        &HashSet::new(),
    );
    if initial_compatible.is_empty() {
        return Err((
            404u16,
            format!(
                "Không có tài khoản nào trong Pool (0/{total_members}) hỗ trợ model '{resolved_model}'. Hãy thêm/bật tài khoản có model này (vd: nhóm Full/Argon cho Gemini 4 & Claude 5.5, hoặc nhóm Classic cho Claude 4.6) và làm mới quota."
            ),
            String::new(),
            String::new(),
            String::new(),
        ));
    }

    let scope = model_quota_scope(resolved_model);
    let mut tried = HashSet::new();
    let max_real_attempts = (cfg.max_retries as usize) + 1;
    let mut real_attempts = 0usize;
    let mut total_hops = 0usize;
    let max_total_hops = initial_compatible.len().max(max_real_attempts).min(32);

    let mut last_err = (
        503u16,
        format!("Không có tài khoản Antigravity khả dụng cho model '{resolved_model}'."),
        String::new(),
        String::new(),
        String::new(),
    );

    while real_attempts < max_real_attempts && total_hops < max_total_hops {
        let candidates = resolve_pool_accounts(cfg, key, resolved_model, &tried);
        let Some(candidate) = candidates.into_iter().next() else {
            break;
        };
        tried.insert(candidate.id.clone());
        total_hops += 1;

        let acc = match accounts::ensure_fresh_access_token(&candidate.id, false).await {
            Ok(a) => a,
            Err(e) => {
                mark_account_cooldown(
                    &candidate.id,
                    &candidate.email,
                    "all",
                    cfg.cooldown_seconds,
                    e.clone(),
                );
                last_err = (
                    401,
                    e,
                    candidate.id.clone(),
                    candidate.email.clone(),
                    String::new(),
                );
                real_attempts += 1;
                continue;
            }
        };

        let project_id = acc
            .quota
            .as_ref()
            .and_then(|q| q.project_id.clone())
            .or_else(|| acc.token.project_id.clone())
            .filter(|p| !p.trim().is_empty() && p != "aicode-consumers")
            .unwrap_or_else(generate_fallback_project_id);

        let (envelope, preview, tool_name_map) =
            openai_body_to_antigravity_envelope(openai_body, resolved_model, &project_id, &key.id);

        let force_non_stream = resolved_model.to_ascii_lowercase().contains("image");
        let action = if stream && !force_non_stream {
            "streamGenerateContent?alt=sse"
        } else {
            "generateContent"
        };

        let mut succeeded = None;
        let mut saw_model_not_found = false;
        let mut saw_non_404_error = false;
        for base_url in ANTIGRAVITY_BASE_URLS {
            let url = format!("{base_url}/v1internal:{action}");
            let send_res = client
                .post(&url)
                .bearer_auth(&acc.token.access_token)
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .header(reqwest::header::USER_AGENT, ANTIGRAVITY_IDE_USER_AGENT)
                .json(&envelope)
                .send()
                .await;

            match send_res {
                Ok(resp) => {
                    let status = resp.status().as_u16();
                    if resp.status().is_success() {
                        if cfg.routing_strategy == "sticky_session" {
                            if let Ok(mut map) = sticky_lock().lock() {
                                map.insert(format!("{}:{resolved_model}", key.id), acc.id.clone());
                            }
                        }
                        succeeded = Some((resp, acc.clone(), preview.clone(), tool_name_map.clone()));
                        break;
                    }
                    let err_body = resp.text().await.unwrap_or_default();
                    if status == 401 {
                        if let Ok(re_acc) =
                            accounts::ensure_fresh_access_token(&acc.id, true).await
                        {
                            if let Ok(retry_resp) = client
                                .post(&url)
                                .bearer_auth(&re_acc.token.access_token)
                                .header(reqwest::header::CONTENT_TYPE, "application/json")
                                .header(reqwest::header::USER_AGENT, ANTIGRAVITY_IDE_USER_AGENT)
                                .json(&envelope)
                                .send()
                                .await
                            {
                                if retry_resp.status().is_success() {
                                    succeeded = Some((
                                        retry_resp,
                                        re_acc,
                                        preview.clone(),
                                        tool_name_map.clone(),
                                    ));
                                    break;
                                }
                            }
                        }
                    }
                    if status == 429 || status == 503 || status == 500 || status == 502 {
                        saw_non_404_error = true;
                        let cool = parse_retry_seconds_from_body(&err_body, cfg.cooldown_seconds);
                        mark_account_cooldown(
                            &acc.id,
                            &acc.email,
                            scope,
                            cool,
                            format!("HTTP {status}"),
                        );
                        if status == 429 {
                            spawn_post_429_quota_sync(&acc.id);
                        }
                        last_err = (
                            status,
                            err_body,
                            acc.id.clone(),
                            acc.email.clone(),
                            preview.clone(),
                        );
                        continue;
                    }
                    if status == 404 {
                        saw_model_not_found = true;
                        if !(matches!(last_err.0, 429 | 403 | 500 | 502 | 503) && saw_non_404_error)
                        {
                            last_err = (
                                status,
                                err_body,
                                acc.id.clone(),
                                acc.email.clone(),
                                preview.clone(),
                            );
                        }
                        continue;
                    }
                    saw_non_404_error = true;
                    last_err = (
                        status,
                        err_body,
                        acc.id.clone(),
                        acc.email.clone(),
                        preview.clone(),
                    );
                    if status == 403 {
                        mark_account_cooldown(&acc.id, &acc.email, "all", 300, "HTTP 403".into());
                        break;
                    }
                    if status == 400 {
                        // Bad request payload won't succeed on another account either; return exact 400 immediately
                        return Err(last_err);
                    }
                }
                Err(e) => {
                    saw_non_404_error = true;
                    last_err = (
                        502,
                        e.to_string(),
                        acc.id.clone(),
                        acc.email.clone(),
                        preview.clone(),
                    );
                }
            }
        }

        if let Some(ok) = succeeded {
            return Ok(ok);
        }

        if saw_model_not_found && !saw_non_404_error {
            mark_runtime_unsupported(&acc.id, resolved_model, 1800);
        } else {
            real_attempts += 1;
        }
    }

    Err(last_err)
}

// ============================================================================
// HTTP/1.1 Server Handling (/v1/models, /v1/chat/completions, /v1/messages, /health)
// ============================================================================

struct HttpRequest {
    method: String,
    path: String,
    query: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

async fn read_http_request(stream: &mut TcpStream) -> Result<HttpRequest, String> {
    const READ_TIMEOUT: Duration = Duration::from_secs(30);
    let mut buf = Vec::with_capacity(8192);
    let mut temp = [0u8; 8192];
    let header_end;

    loop {
        let n = tokio::time::timeout(READ_TIMEOUT, stream.read(&mut temp))
            .await
            .map_err(|_| "Read header timeout".to_string())?
            .map_err(|e| format!("Read socket: {e}"))?;
        if n == 0 {
            return Err("EOF".into());
        }
        buf.extend_from_slice(&temp[..n]);
        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            header_end = pos + 4;
            break;
        }
        if buf.len() > 128 * 1024 {
            return Err("Header too large".into());
        }
    }

    let head_str = String::from_utf8_lossy(&buf[..header_end - 4]);
    let mut lines = head_str.lines();
    let req_line = lines.next().ok_or("Empty request line")?;
    let mut parts = req_line.split_whitespace();
    let method = parts.next().unwrap_or("GET").to_uppercase();
    let raw_target = parts.next().unwrap_or("/");
    let (path, query) = match raw_target.split_once('?') {
        Some((p, q)) => (p.to_string(), q.to_string()),
        None => (raw_target.to_string(), String::new()),
    };

    let mut headers = HashMap::new();
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
        }
    }

    let is_chunked = headers
        .get("transfer-encoding")
        .is_some_and(|v| v.to_ascii_lowercase().contains("chunked"));
    if headers
        .get("expect")
        .is_some_and(|v| v.to_ascii_lowercase().contains("100-continue"))
    {
        let _ = stream.write_all(b"HTTP/1.1 100 Continue\r\n\r\n").await;
        let _ = stream.flush().await;
    }

    let mut body = Vec::new();
    if is_chunked {
        let mut raw = buf[header_end..].to_vec();
        let mut pos = 0usize;
        loop {
            while raw[pos..].windows(2).position(|w| w == b"\r\n").is_none() {
                let n = tokio::time::timeout(READ_TIMEOUT, stream.read(&mut temp))
                    .await
                    .map_err(|_| "Read chunked size timeout".to_string())?
                    .map_err(|e| format!("Read chunked size: {e}"))?;
                if n == 0 {
                    break;
                }
                raw.extend_from_slice(&temp[..n]);
                if raw.len() > 24 * 1024 * 1024 {
                    return Err("Chunked payload too large".into());
                }
            }
            let Some(rel_crlf) = raw[pos..].windows(2).position(|w| w == b"\r\n") else {
                break;
            };
            let size_line = String::from_utf8_lossy(&raw[pos..pos + rel_crlf]);
            let hex_part = size_line.split(';').next().unwrap_or("").trim();
            let chunk_size = usize::from_str_radix(hex_part, 16).unwrap_or(0);
            pos += rel_crlf + 2;
            if chunk_size == 0 {
                break;
            }
            while raw.len() < pos + chunk_size + 2 {
                let n = tokio::time::timeout(READ_TIMEOUT, stream.read(&mut temp))
                    .await
                    .map_err(|_| "Read chunked data timeout".to_string())?
                    .map_err(|e| format!("Read chunked data: {e}"))?;
                if n == 0 {
                    break;
                }
                raw.extend_from_slice(&temp[..n]);
                if raw.len() > 24 * 1024 * 1024 {
                    return Err("Chunked payload too large".into());
                }
            }
            let end = (pos + chunk_size).min(raw.len());
            body.extend_from_slice(&raw[pos..end]);
            pos = (pos + chunk_size + 2).min(raw.len());
            if body.len() > 20 * 1024 * 1024 {
                return Err("Payload too large".into());
            }
        }
    } else {
        let content_length: usize = headers
            .get("content-length")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        if content_length > 20 * 1024 * 1024 {
            return Err("Payload too large".into());
        }
        body = buf[header_end..].to_vec();
        while body.len() < content_length {
            let n = tokio::time::timeout(READ_TIMEOUT, stream.read(&mut temp))
                .await
                .map_err(|_| "Read body timeout".to_string())?
                .map_err(|e| format!("Read body: {e}"))?;
            if n == 0 {
                break;
            }
            body.extend_from_slice(&temp[..n]);
        }
        body.truncate(content_length);
    }

    Ok(HttpRequest {
        method,
        path,
        query,
        headers,
        body,
    })
}

fn record_api_log_async(item: ApiRequestLogItem) {
    tokio::task::spawn_blocking(move || {
        record_api_log(&item);
    });
}

async fn write_json_response_ka(
    stream: &mut TcpStream,
    status: u16,
    body: &Value,
    keep_alive: bool,
) -> bool {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        429 => "Too Many Requests",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        _ => "Internal Server Error",
    };
    let payload = serde_json::to_string(body).unwrap_or_else(|_| "{}".into());
    let conn_hdr = if keep_alive {
        "Connection: keep-alive\r\nKeep-Alive: timeout=60, max=64"
    } else {
        "Connection: close"
    };
    let resp = format!(
        "HTTP/1.1 {status} {reason}\r\n\
         Content-Type: application/json; charset=utf-8\r\n\
         Access-Control-Allow-Origin: *\r\n\
         Access-Control-Allow-Headers: *\r\n\
         Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
         Content-Length: {}\r\n\
         {conn_hdr}\r\n\r\n\
         {payload}",
        payload.len()
    );
    stream.write_all(resp.as_bytes()).await.is_ok() && stream.flush().await.is_ok()
}

async fn write_sse_headers(stream: &mut TcpStream) -> Result<(), String> {
    let head = "HTTP/1.1 200 OK\r\n\
        Content-Type: text/event-stream; charset=utf-8\r\n\
        Cache-Control: no-cache, no-transform\r\n\
        X-Accel-Buffering: no\r\n\
        Transfer-Encoding: chunked\r\n\
        Connection: close\r\n\
        Access-Control-Allow-Origin: *\r\n\
        Access-Control-Allow-Headers: *\r\n\r\n";
    stream
        .write_all(head.as_bytes())
        .await
        .map_err(|e| format!("Write SSE headers: {e}"))?;
    let _ = stream.flush().await;
    Ok(())
}

async fn write_sse_chunk(stream: &mut TcpStream, payload: &str) -> Result<(), ()> {
    if payload.is_empty() {
        return Ok(());
    }
    let bytes = payload.as_bytes();
    let header = format!("{:x}\r\n", bytes.len());
    stream.write_all(header.as_bytes()).await.map_err(|_| ())?;
    stream.write_all(bytes).await.map_err(|_| ())?;
    stream.write_all(b"\r\n").await.map_err(|_| ())?;
    stream.flush().await.map_err(|_| ())
}

async fn write_sse_end(stream: &mut TcpStream) {
    let _ = stream.write_all(b"0\r\n\r\n").await;
    let _ = stream.flush().await;
}

fn extract_api_token(req: &HttpRequest) -> Option<String> {
    if let Some(auth) = req.headers.get("authorization") {
        let trimmed = auth.trim();
        if let Some(bearer) = trimmed.strip_prefix("Bearer ").or_else(|| trimmed.strip_prefix("bearer ")) {
            return Some(bearer.trim().to_string());
        }
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    if let Some(key) = req.headers.get("x-api-key") {
        if !key.trim().is_empty() {
            return Some(key.trim().to_string());
        }
    }
    if let Some(key) = req.headers.get("x-goog-api-key") {
        if !key.trim().is_empty() {
            return Some(key.trim().to_string());
        }
    }
    for pair in req.query.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            if k == "key" && !v.trim().is_empty() {
                return Some(v.trim().to_string());
            }
        }
    }
    None
}

fn normalize_peer_ip(raw_ip: &str) -> String {
    raw_ip
        .strip_prefix("::ffff:")
        .unwrap_or(raw_ip)
        .to_string()
}

fn spawn_post_429_quota_sync(account_id: &str) {
    let now = Instant::now();
    if let Ok(mut map) = post_429_refresh_debounce().lock() {
        if let Some(last) = map.get(account_id) {
            if now.duration_since(*last) < Duration::from_secs(90) {
                return;
            }
        }
        map.insert(account_id.to_string(), now);
    }
    let acc_id = account_id.to_string();
    tokio::spawn(async move {
        let _ = accounts::refresh_single_account_quota(acc_id).await;
    });
}

async fn handle_connection(
    mut stream: TcpStream,
    conn_gen: u64,
    mut shutdown_rx: watch::Receiver<bool>,
    client_ip: String,
) {
    let _ = stream.set_nodelay(true);
    for _ in 0..64 {
        if *shutdown_rx.borrow() || SERVER_ACTIVE_GEN.load(Ordering::SeqCst) != conn_gen {
            return;
        }
        let req = tokio::select! {
            _ = shutdown_rx.changed() => {
                // API Router stopped: drop idle keep-alive socket immediately
                return;
            }
            res = tokio::time::timeout(Duration::from_secs(60), read_http_request(&mut stream)) => {
                match res {
                    Ok(Ok(r)) => r,
                    _ => return,
                }
            }
        };
        if *shutdown_rx.borrow() || SERVER_ACTIVE_GEN.load(Ordering::SeqCst) != conn_gen {
            let _ = write_json_response_ka(
                &mut stream,
                503,
                &json!({
                    "error": {
                        "message": "OnlyDEVS Antigravity API Router is stopped.",
                        "type": "service_unavailable"
                    }
                }),
                false,
            )
            .await;
            return;
        }
        let wants_keep_alive = !req
            .headers
            .get("connection")
            .is_some_and(|v| v.to_ascii_lowercase().contains("close"));
        let keep_open = handle_single_request(
            &mut stream,
            req,
            wants_keep_alive,
            conn_gen,
            &mut shutdown_rx,
            &client_ip,
        )
        .await;
        if !keep_open {
            return;
        }
    }
}

async fn handle_single_request(
    stream: &mut TcpStream,
    req: HttpRequest,
    keep_alive: bool,
    conn_gen: u64,
    shutdown_rx: &mut watch::Receiver<bool>,
    client_ip: &str,
) -> bool {
    if *shutdown_rx.borrow() || SERVER_ACTIVE_GEN.load(Ordering::SeqCst) != conn_gen {
        let _ = write_json_response_ka(
            stream,
            503,
            &json!({
                "error": {
                    "message": "OnlyDEVS Antigravity API Router is stopped.",
                    "type": "service_unavailable"
                }
            }),
            false,
        )
        .await;
        return false;
    }
    if req.method == "OPTIONS" {
        let conn_hdr = if keep_alive {
            "Connection: keep-alive"
        } else {
            "Connection: close"
        };
        let resp = format!(
            "HTTP/1.1 204 No Content\r\n\
             Access-Control-Allow-Origin: *\r\n\
             Access-Control-Allow-Headers: *\r\n\
             Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
             Content-Length: 0\r\n\
             {conn_hdr}\r\n\r\n"
        );
        return stream.write_all(resp.as_bytes()).await.is_ok()
            && stream.flush().await.is_ok()
            && keep_alive;
    }

    let norm_path = req.path.trim_end_matches('/');
    let norm_path = if norm_path.is_empty() { "/" } else { norm_path };

    if req.method == "GET"
        && (norm_path == "/"
            || norm_path == "/v1"
            || norm_path == "/health"
            || norm_path == "/v1/health"
            || norm_path == "/ping")
    {
        let active = ACTIVE_REQUESTS.load(Ordering::Relaxed);
        return write_json_response_ka(
            stream,
            200,
            &json!({
                "status": "ok",
                "service": "OnlyDEVS Antigravity API Router",
                "activeRequests": active,
                "endpoints": ["/v1/models", "/v1/chat/completions", "/v1/responses", "/v1/messages", "/health"],
            }),
            keep_alive,
        )
        .await
            && keep_alive;
    }

    // Authenticate API Key
    let token = extract_api_token(&req).unwrap_or_default();
    let Some(api_key) = find_valid_api_key(&token) else {
        if norm_path != "/" && norm_path != "/favicon.ico" {
            record_api_log_async(ApiRequestLogItem {
                id: Uuid::new_v4().to_string(),
                timestamp: Utc::now().timestamp(),
                api_key_id: "invalid".into(),
                api_key_label: "Unauthorized".into(),
                account_id: String::new(),
                account_email: String::new(),
                protocol: "http".into(),
                request_model: format!("{} {}", req.method, norm_path),
                resolved_model: "-".into(),
                stream: false,
                status_code: 401,
                latency_ms: 0,
                input_tokens: 0,
                output_tokens: 0,
                thinking_tokens: 0,
                cache_read_tokens: 0,
                total_tokens: 0,
                error_message: Some("Invalid or disabled API key".into()),
                request_preview: String::new(),
                response_preview: String::new(),
                client_ip: client_ip.to_string(),
            });
        }
        return write_json_response_ka(
            stream,
            401,
            &json!({
                "error": {
                    "message": "Invalid or disabled OnlyDEVS API key. Pass Authorization: Bearer <key> or x-api-key.",
                    "type": "authentication_error"
                }
            }),
            keep_alive,
        )
        .await
            && keep_alive;
    };

    // Enforce expiry, per-key RPM rate limit, and daily token quota
    if let Err((status_code, err_msg, err_type)) = check_api_key_limits(&api_key) {
        record_api_log_async(ApiRequestLogItem {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now().timestamp(),
            api_key_id: api_key.id.clone(),
            api_key_label: api_key.label.clone(),
            account_id: String::new(),
            account_email: String::new(),
            protocol: "http".into(),
            request_model: format!("{} {}", req.method, norm_path),
            resolved_model: "-".into(),
            stream: false,
            status_code,
            latency_ms: 0,
            input_tokens: 0,
            output_tokens: 0,
            thinking_tokens: 0,
            cache_read_tokens: 0,
            total_tokens: 0,
            error_message: Some(err_msg.clone()),
            request_preview: String::new(),
            response_preview: String::new(),
            client_ip: client_ip.to_string(),
        });
        return write_json_response_ka(
            stream,
            status_code,
            &json!({
                "error": {
                    "message": err_msg,
                    "type": err_type
                }
            }),
            keep_alive,
        )
        .await
            && keep_alive;
    }

    if req.method == "GET" && (norm_path == "/v1/models" || norm_path == "/models") {
        let models = list_available_models().unwrap_or_default();
        let profile_members = if api_key.allowed_profile != "all" {
            let cfg_snap = load_config().unwrap_or_default();
            Some(select_pool_members(
                accounts::list_accounts_sync().unwrap_or_default(),
                &cfg_snap,
                Some(&api_key),
            ))
        } else {
            None
        };
        let data: Vec<Value> = models
            .into_iter()
            .filter(|m| {
                api_key.allowed_models.is_empty()
                    || api_key.allowed_models.iter().any(|a| glob_match(a, &m.id))
            })
            .filter(|m| match &profile_members {
                Some(members) => members
                    .iter()
                    .any(|acc| account_model_support(acc, &m.id) != ModelSupport::Unsupported),
                None => true,
            })
            .map(|m| {
                json!({
                    "id": m.id,
                    "object": "model",
                    "created": 1735689600,
                    "owned_by": format!("antigravity-{}", m.family),
                    "display_name": m.display_name
                })
            })
            .collect();
        return write_json_response_ka(
            stream,
            200,
            &json!({ "object": "list", "data": data }),
            keep_alive,
        )
        .await
            && keep_alive;
    }

    let is_openai = req.method == "POST"
        && (norm_path == "/v1/chat/completions" || norm_path == "/chat/completions");
    let is_responses =
        req.method == "POST" && (norm_path == "/v1/responses" || norm_path == "/responses");
    let is_anthropic =
        req.method == "POST" && (norm_path == "/v1/messages" || norm_path == "/messages");

    if !is_openai && !is_responses && !is_anthropic {
        let err_msg = format!(
            "Unsupported endpoint: {} {}. Use /v1/models, /v1/chat/completions, /v1/responses, or /v1/messages.",
            req.method, norm_path
        );
        record_api_log_async(ApiRequestLogItem {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now().timestamp(),
            api_key_id: api_key.id.clone(),
            api_key_label: api_key.label.clone(),
            account_id: String::new(),
            account_email: String::new(),
            protocol: "http".into(),
            request_model: format!("{} {}", req.method, norm_path),
            resolved_model: "-".into(),
            stream: false,
            status_code: 404,
            latency_ms: 0,
            input_tokens: 0,
            output_tokens: 0,
            thinking_tokens: 0,
            cache_read_tokens: 0,
            total_tokens: 0,
            error_message: Some(err_msg.clone()),
            request_preview: String::new(),
            response_preview: String::new(),
            client_ip: client_ip.to_string(),
        });
        return write_json_response_ka(
            stream,
            404,
            &json!({
                "error": {
                    "message": err_msg,
                    "type": "not_found_error"
                }
            }),
            keep_alive,
        )
        .await
            && keep_alive;
    }

    let raw_body: Value = match serde_json::from_slice(&req.body) {
        Ok(v) => v,
        Err(e) => {
            let err_msg = format!("Invalid JSON body ({} bytes): {e}", req.body.len());
            record_api_log_async(ApiRequestLogItem {
                id: Uuid::new_v4().to_string(),
                timestamp: Utc::now().timestamp(),
                api_key_id: api_key.id.clone(),
                api_key_label: api_key.label.clone(),
                account_id: String::new(),
                account_email: String::new(),
                protocol: if is_anthropic { "anthropic" } else if is_responses { "responses" } else { "openai" }.into(),
                request_model: format!("{} {}", req.method, norm_path),
                resolved_model: "-".into(),
                stream: false,
                status_code: 400,
                latency_ms: 0,
                input_tokens: 0,
                output_tokens: 0,
                thinking_tokens: 0,
                cache_read_tokens: 0,
                total_tokens: 0,
                error_message: Some(err_msg.clone()),
                request_preview: String::from_utf8_lossy(&req.body).chars().take(200).collect(),
                response_preview: String::new(),
                client_ip: client_ip.to_string(),
            });
            return write_json_response_ka(
                stream,
                400,
                &json!({ "error": { "message": err_msg, "type": "invalid_request_error" } }),
                keep_alive,
            )
            .await
                && keep_alive;
        }
    };

    let cfg = load_config().unwrap_or_default();
    let protocol = if is_anthropic {
        "anthropic"
    } else if is_responses {
        "responses"
    } else {
        "openai"
    };
    let openai_body = if is_anthropic {
        anthropic_body_to_openai_body(&raw_body)
    } else if is_responses {
        responses_body_to_openai_body(&raw_body)
    } else {
        raw_body.clone()
    };

    let request_model = openai_body
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or("gemini-4-neon-medium")
        .to_string();
    let resolved_model = resolve_model_alias(&cfg, &request_model);
    let is_stream = openai_body
        .get("stream")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    if !api_key.allowed_models.is_empty()
        && !api_key
            .allowed_models
            .iter()
            .any(|p| glob_match(p, &request_model) || glob_match(p, &resolved_model))
    {
        let err_msg = format!(
            "API key '{}' is not allowed to call model '{}'",
            api_key.label, request_model
        );
        record_api_log_async(ApiRequestLogItem {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now().timestamp(),
            api_key_id: api_key.id.clone(),
            api_key_label: api_key.label.clone(),
            account_id: String::new(),
            account_email: String::new(),
            protocol: protocol.into(),
            request_model: request_model.clone(),
            resolved_model: resolved_model.clone(),
            stream: is_stream,
            status_code: 403,
            latency_ms: 0,
            input_tokens: 0,
            output_tokens: 0,
            thinking_tokens: 0,
            cache_read_tokens: 0,
            total_tokens: 0,
            error_message: Some(err_msg.clone()),
            request_preview: String::new(),
            response_preview: String::new(),
            client_ip: client_ip.to_string(),
        });
        return write_json_response_ka(
            stream,
            403,
            &json!({
                "error": {
                    "message": err_msg,
                    "type": "permission_error"
                }
            }),
            keep_alive,
        )
        .await
            && keep_alive;
    }

    ACTIVE_REQUESTS.fetch_add(1, Ordering::Relaxed);
    let started = Instant::now();
    let log_id = Uuid::new_v4().to_string();

    let upstream_res =
        send_upstream_with_failover(&cfg, &api_key, &resolved_model, &openai_body, is_stream).await;

    match upstream_res {
        Err((status, err_msg, acc_id, acc_email, err_preview)) => {
            ACTIVE_REQUESTS.fetch_sub(1, Ordering::Relaxed);
            record_api_log_async(ApiRequestLogItem {
                id: log_id,
                timestamp: Utc::now().timestamp(),
                api_key_id: api_key.id,
                api_key_label: api_key.label,
                account_id: acc_id,
                account_email: acc_email,
                protocol: protocol.into(),
                request_model,
                resolved_model,
                stream: is_stream,
                status_code: status,
                latency_ms: started.elapsed().as_millis() as u64,
                input_tokens: 0,
                output_tokens: 0,
                thinking_tokens: 0,
                cache_read_tokens: 0,
                total_tokens: 0,
                error_message: Some(err_msg.clone()),
                request_preview: err_preview,
                response_preview: String::new(),
                client_ip: client_ip.to_string(),
            });
            write_json_response_ka(
                stream,
                status,
                &json!({
                    "error": {
                        "message": err_msg,
                        "type": "upstream_error"
                    }
                }),
                keep_alive,
            )
            .await
                && keep_alive
        }
        Ok((mut resp, acc, req_preview, tool_name_map)) => {
            let mut total_in = 0u64;
            let mut total_out = 0u64;
            let mut total_think = 0u64;
            let mut total_cache = 0u64;
            let mut collected_text = String::new();
            let mut collected_think = String::new();
            let mut collected_tools: Vec<(String, String, Value)> = Vec::new();
            let mut finish_reason = "stop".to_string();
            let completion_id = format!("chatcmpl-{}", &Uuid::new_v4().to_string()[..12]);
            let msg_id = format!("msg_{}", &Uuid::new_v4().to_string()[..16]);
            let created_ts = Utc::now().timestamp();

            let mut keep_open = false;
            if is_stream {
                if write_sse_headers(stream).await.is_ok() {
                    let mut sse_buf = String::new();
                    let mut anthropic_started = false;
                    let mut anthropic_block_idx = 0usize;
                    let mut anthropic_in_thinking = false;
                    let mut anthropic_in_text = false;
                    let mut client_alive = true;

                    while client_alive {
                        if *shutdown_rx.borrow() || SERVER_ACTIVE_GEN.load(Ordering::SeqCst) != conn_gen {
                            client_alive = false;
                            break;
                        }
                        let chunk = tokio::select! {
                            _ = shutdown_rx.changed() => {
                                client_alive = false;
                                break;
                            }
                            res = tokio::time::timeout(Duration::from_secs(10), resp.chunk()) => {
                                match res {
                                    Ok(Ok(Some(c))) => c,
                                    Ok(Ok(None)) | Ok(Err(_)) => break,
                                    Err(_) => {
                                        // 10s upstream silence (e.g. long thinking): send SSE keepalive comment
                                        if write_sse_chunk(stream, ": keepalive\n\n").await.is_err() {
                                            client_alive = false;
                                        }
                                        continue;
                                    }
                                }
                            }
                        };
                        sse_buf.push_str(&String::from_utf8_lossy(&chunk));
                        while let Some(nl) = sse_buf.find('\n') {
                            let line = sse_buf[..nl].trim().to_string();
                            sse_buf.drain(..=nl);
                            let Some(data_str) = line.strip_prefix("data:") else {
                                continue;
                            };
                            let data_trim = data_str.trim();
                            if data_trim.is_empty() || data_trim == "[DONE]" {
                                continue;
                            }
                            let Ok(val) = serde_json::from_str::<Value>(data_trim) else {
                                continue;
                            };
                            let mut parsed = parse_antigravity_response_payload(&val);
                            if !tool_name_map.is_empty() && !parsed.tool_calls.is_empty() {
                                for (_, name, _) in &mut parsed.tool_calls {
                                    *name = uncloak_tool_name(name, &tool_name_map);
                                }
                            }
                            if parsed.input_tokens > 0 {
                                total_in = parsed.input_tokens;
                            }
                            if parsed.output_tokens > 0 {
                                total_out = parsed.output_tokens;
                            }
                            if parsed.thinking_tokens > 0 {
                                total_think = parsed.thinking_tokens;
                            }
                            if parsed.cache_read_tokens > 0 {
                                total_cache = parsed.cache_read_tokens;
                            }
                            if let Some(fr) = parsed.finish_reason {
                                finish_reason = fr;
                            }

                            if is_openai {
                                let mut delta = Map::new();
                                if !parsed.thinking.is_empty() {
                                    collected_think.push_str(&parsed.thinking);
                                    delta.insert("reasoning_content".into(), json!(parsed.thinking));
                                }
                                if !parsed.text.is_empty() {
                                    collected_text.push_str(&parsed.text);
                                    delta.insert("content".into(), json!(parsed.text));
                                }
                                if !parsed.tool_calls.is_empty() {
                                    let tc_json: Vec<Value> = parsed
                                        .tool_calls
                                        .iter()
                                        .enumerate()
                                        .map(|(i, (id, name, args))| {
                                            collected_tools.push((id.clone(), name.clone(), args.clone()));
                                            json!({
                                                "index": i,
                                                "id": id,
                                                "type": "function",
                                                "function": {
                                                    "name": name,
                                                    "arguments": serde_json::to_string(args).unwrap_or_else(|_| "{}".into())
                                                }
                                            })
                                        })
                                        .collect();
                                    delta.insert("tool_calls".into(), Value::Array(tc_json));
                                    finish_reason = "tool_calls".into();
                                }
                                if !delta.is_empty() {
                                    let frame = json!({
                                        "id": completion_id,
                                        "object": "chat.completion.chunk",
                                        "created": created_ts,
                                        "model": request_model,
                                        "choices": [{
                                            "index": 0,
                                            "delta": Value::Object(delta),
                                            "finish_reason": Value::Null
                                        }]
                                    });
                                    let msg = format!("data: {}\n\n", frame);
                                    if write_sse_chunk(stream, &msg).await.is_err() {
                                        client_alive = false;
                                        break;
                                    }
                                }
                            } else if is_responses {
                                // OpenAI Responses API SSE stream
                                let resp_id = format!("resp_{}", &completion_id[9..]);
                                let item_id = format!("msg_{}", &completion_id[9..]);
                                let mut out_batch = String::new();
                                if !anthropic_started {
                                    anthropic_started = true;
                                    let ev_created = json!({
                                        "type": "response.created",
                                        "response": {
                                            "id": resp_id,
                                            "object": "response",
                                            "created_at": created_ts,
                                            "status": "in_progress",
                                            "model": request_model,
                                            "output": []
                                        }
                                    });
                                    let ev_item = json!({
                                        "type": "response.output_item.added",
                                        "output_index": 0,
                                        "item": {
                                            "id": item_id,
                                            "type": "message",
                                            "status": "in_progress",
                                            "role": "assistant",
                                            "content": []
                                        }
                                    });
                                    out_batch.push_str(&format!(
                                        "event: response.created\ndata: {ev_created}\n\nevent: response.output_item.added\ndata: {ev_item}\n\n"
                                    ));
                                }
                                if !parsed.thinking.is_empty() {
                                    collected_think.push_str(&parsed.thinking);
                                }
                                if !parsed.text.is_empty() {
                                    collected_text.push_str(&parsed.text);
                                    let ev_delta = json!({
                                        "type": "response.output_text.delta",
                                        "item_id": item_id,
                                        "output_index": 0,
                                        "content_index": 0,
                                        "delta": parsed.text
                                    });
                                    out_batch.push_str(&format!(
                                        "event: response.output_text.delta\ndata: {ev_delta}\n\n"
                                    ));
                                }
                                for (id, name, args) in parsed.tool_calls {
                                    collected_tools.push((id, name, args));
                                }
                                if !out_batch.is_empty()
                                    && write_sse_chunk(stream, &out_batch).await.is_err()
                                {
                                    client_alive = false;
                                    break;
                                }
                            } else {
                                // Anthropic SSE stream
                                let mut out_batch = String::new();
                                if !anthropic_started {
                                    anthropic_started = true;
                                    let ev = json!({
                                        "type": "message_start",
                                        "message": {
                                            "id": msg_id,
                                            "type": "message",
                                            "role": "assistant",
                                            "content": [],
                                            "model": request_model,
                                            "stop_reason": Value::Null,
                                            "usage": { "input_tokens": total_in, "output_tokens": 0 }
                                        }
                                    });
                                    out_batch.push_str(&format!("event: message_start\ndata: {ev}\n\n"));
                                }
                                if !parsed.thinking.is_empty() {
                                    collected_think.push_str(&parsed.thinking);
                                    if !anthropic_in_thinking {
                                        anthropic_in_thinking = true;
                                        let ev = json!({
                                            "type": "content_block_start",
                                            "index": anthropic_block_idx,
                                            "content_block": { "type": "thinking", "thinking": "" }
                                        });
                                        out_batch.push_str(&format!("event: content_block_start\ndata: {ev}\n\n"));
                                    }
                                    let ev = json!({
                                        "type": "content_block_delta",
                                        "index": anthropic_block_idx,
                                        "delta": { "type": "thinking_delta", "thinking": parsed.thinking }
                                    });
                                    out_batch.push_str(&format!("event: content_block_delta\ndata: {ev}\n\n"));
                                }
                                if !parsed.text.is_empty() {
                                    collected_text.push_str(&parsed.text);
                                    if anthropic_in_thinking {
                                        let ev = json!({ "type": "content_block_stop", "index": anthropic_block_idx });
                                        out_batch.push_str(&format!("event: content_block_stop\ndata: {ev}\n\n"));
                                        anthropic_in_thinking = false;
                                        anthropic_block_idx += 1;
                                    }
                                    if !anthropic_in_text {
                                        anthropic_in_text = true;
                                        let ev = json!({
                                            "type": "content_block_start",
                                            "index": anthropic_block_idx,
                                            "content_block": { "type": "text", "text": "" }
                                        });
                                        out_batch.push_str(&format!("event: content_block_start\ndata: {ev}\n\n"));
                                    }
                                    let ev = json!({
                                        "type": "content_block_delta",
                                        "index": anthropic_block_idx,
                                        "delta": { "type": "text_delta", "text": parsed.text }
                                    });
                                    out_batch.push_str(&format!("event: content_block_delta\ndata: {ev}\n\n"));
                                }
                                for (id, name, args) in parsed.tool_calls {
                                    collected_tools.push((id.clone(), name.clone(), args.clone()));
                                    if anthropic_in_thinking || anthropic_in_text {
                                        let ev = json!({ "type": "content_block_stop", "index": anthropic_block_idx });
                                        out_batch.push_str(&format!("event: content_block_stop\ndata: {ev}\n\n"));
                                        anthropic_in_thinking = false;
                                        anthropic_in_text = false;
                                        anthropic_block_idx += 1;
                                    }
                                    let ev1 = json!({
                                        "type": "content_block_start",
                                        "index": anthropic_block_idx,
                                        "content_block": { "type": "tool_use", "id": id, "name": name, "input": {} }
                                    });
                                    let ev2 = json!({
                                        "type": "content_block_delta",
                                        "index": anthropic_block_idx,
                                        "delta": { "type": "input_json_delta", "partial_json": serde_json::to_string(&args).unwrap_or_else(|_| "{}".into()) }
                                    });
                                    let ev3 = json!({ "type": "content_block_stop", "index": anthropic_block_idx });
                                    out_batch.push_str(&format!("event: content_block_start\ndata: {ev1}\n\nevent: content_block_delta\ndata: {ev2}\n\nevent: content_block_stop\ndata: {ev3}\n\n"));
                                    anthropic_block_idx += 1;
                                    finish_reason = "tool_use".into();
                                }
                                if !out_batch.is_empty()
                                    && write_sse_chunk(stream, &out_batch).await.is_err()
                                {
                                    client_alive = false;
                                    break;
                                }
                            }
                        }
                    }

                    // Send stream termination frames
                    if client_alive {
                        let mut tail = String::new();
                        if is_openai {
                            let done_frame = json!({
                                "id": completion_id,
                                "object": "chat.completion.chunk",
                                "created": created_ts,
                                "model": request_model,
                                "choices": [{
                                    "index": 0,
                                    "delta": {},
                                    "finish_reason": finish_reason
                                }],
                                "usage": {
                                    "prompt_tokens": total_in,
                                    "completion_tokens": total_out + total_think,
                                    "total_tokens": total_in + total_out + total_think
                                }
                            });
                            tail.push_str(&format!("data: {done_frame}\n\ndata: [DONE]\n\n"));
                        } else if is_responses {
                            let resp_id = format!("resp_{}", &completion_id[9..]);
                            let item_id = format!("msg_{}", &completion_id[9..]);
                            let mut output_items: Vec<Value> = Vec::new();
                            if !collected_text.is_empty() {
                                output_items.push(json!({
                                    "id": item_id,
                                    "type": "message",
                                    "status": "completed",
                                    "role": "assistant",
                                    "content": [{ "type": "output_text", "text": collected_text, "annotations": [] }]
                                }));
                            }
                            for (id, name, args) in &collected_tools {
                                output_items.push(json!({
                                    "id": id,
                                    "call_id": id,
                                    "type": "function_call",
                                    "status": "completed",
                                    "name": name,
                                    "arguments": serde_json::to_string(args).unwrap_or_else(|_| "{}".into())
                                }));
                            }
                            let ev_text_done = json!({
                                "type": "response.output_text.done",
                                "item_id": item_id,
                                "output_index": 0,
                                "content_index": 0,
                                "text": collected_text
                            });
                            let ev_done = json!({
                                "type": "response.completed",
                                "response": {
                                    "id": resp_id,
                                    "object": "response",
                                    "created_at": created_ts,
                                    "status": "completed",
                                    "model": request_model,
                                    "output": output_items,
                                    "output_text": collected_text,
                                    "usage": {
                                        "input_tokens": total_in,
                                        "output_tokens": total_out + total_think,
                                        "total_tokens": total_in + total_out + total_think
                                    }
                                }
                            });
                            tail.push_str(&format!("event: response.output_text.done\ndata: {ev_text_done}\n\nevent: response.completed\ndata: {ev_done}\n\n"));
                        } else {
                            if anthropic_in_thinking || anthropic_in_text {
                                let ev = json!({ "type": "content_block_stop", "index": anthropic_block_idx });
                                tail.push_str(&format!("event: content_block_stop\ndata: {ev}\n\n"));
                            }
                            let stop_reason = if finish_reason == "tool_use" {
                                "tool_use"
                            } else if finish_reason == "length" {
                                "max_tokens"
                            } else {
                                "end_turn"
                            };
                            let ev_delta = json!({
                                "type": "message_delta",
                                "delta": { "stop_reason": stop_reason, "stop_sequence": Value::Null },
                                "usage": { "output_tokens": total_out + total_think }
                            });
                            let ev_stop = json!({ "type": "message_stop" });
                            tail.push_str(&format!("event: message_delta\ndata: {ev_delta}\n\nevent: message_stop\ndata: {ev_stop}\n\n"));
                        }
                        let _ = write_sse_chunk(stream, &tail).await;
                        write_sse_end(stream).await;
                    }
                }
            } else {
                // Non-streaming JSON response
                let raw_text = resp.text().await.unwrap_or_default();
                let mut parsed = serde_json::from_str::<Value>(&raw_text)
                    .map(|v| parse_antigravity_response_payload(&v))
                    .unwrap_or_default();
                if !tool_name_map.is_empty() && !parsed.tool_calls.is_empty() {
                    for (_, name, _) in &mut parsed.tool_calls {
                        *name = uncloak_tool_name(name, &tool_name_map);
                    }
                }
                total_in = parsed.input_tokens;
                total_out = parsed.output_tokens;
                total_think = parsed.thinking_tokens;
                total_cache = parsed.cache_read_tokens;
                collected_text = parsed.text;
                collected_think = parsed.thinking;
                collected_tools = parsed.tool_calls;
                if let Some(fr) = parsed.finish_reason {
                    finish_reason = fr;
                }
                if !collected_tools.is_empty() {
                    finish_reason = if is_anthropic {
                        "tool_use".into()
                    } else {
                        "tool_calls".into()
                    };
                }

                if is_openai {
                    let mut message = Map::new();
                    message.insert("role".into(), json!("assistant"));
                    message.insert("content".into(), json!(collected_text));
                    if !collected_think.is_empty() {
                        message.insert("reasoning_content".into(), json!(collected_think));
                    }
                    if !collected_tools.is_empty() {
                        let tcs: Vec<Value> = collected_tools
                            .iter()
                            .map(|(id, name, args)| {
                                json!({
                                    "id": id,
                                    "type": "function",
                                    "function": {
                                        "name": name,
                                        "arguments": serde_json::to_string(args).unwrap_or_else(|_| "{}".into())
                                    }
                                })
                            })
                            .collect();
                        message.insert("tool_calls".into(), Value::Array(tcs));
                    }
                    let out_json = json!({
                        "id": completion_id,
                        "object": "chat.completion",
                        "created": created_ts,
                        "model": request_model,
                        "choices": [{
                            "index": 0,
                            "message": Value::Object(message),
                            "finish_reason": finish_reason
                        }],
                        "usage": {
                            "prompt_tokens": total_in,
                            "completion_tokens": total_out + total_think,
                            "total_tokens": total_in + total_out + total_think
                        }
                    });
                    keep_open = write_json_response_ka(stream, 200, &out_json, keep_alive).await
                        && keep_alive;
                } else if is_responses {
                    let resp_id = format!("resp_{}", &completion_id[9..]);
                    let item_id = format!("msg_{}", &completion_id[9..]);
                    let mut output_items: Vec<Value> = Vec::new();
                    if !collected_text.is_empty() {
                        output_items.push(json!({
                            "id": item_id,
                            "type": "message",
                            "status": "completed",
                            "role": "assistant",
                            "content": [{ "type": "output_text", "text": collected_text, "annotations": [] }]
                        }));
                    }
                    for (id, name, args) in &collected_tools {
                        output_items.push(json!({
                            "id": id,
                            "call_id": id,
                            "type": "function_call",
                            "status": "completed",
                            "name": name,
                            "arguments": serde_json::to_string(args).unwrap_or_else(|_| "{}".into())
                        }));
                    }
                    let out_json = json!({
                        "id": resp_id,
                        "object": "response",
                        "created_at": created_ts,
                        "status": "completed",
                        "model": request_model,
                        "output": output_items,
                        "output_text": collected_text,
                        "usage": {
                            "input_tokens": total_in,
                            "output_tokens": total_out + total_think,
                            "total_tokens": total_in + total_out + total_think
                        }
                    });
                    keep_open = write_json_response_ka(stream, 200, &out_json, keep_alive).await
                        && keep_alive;
                } else {
                    let mut blocks = Vec::new();
                    if !collected_think.is_empty() {
                        blocks.push(json!({ "type": "thinking", "thinking": collected_think }));
                    }
                    if !collected_text.is_empty() {
                        blocks.push(json!({ "type": "text", "text": collected_text }));
                    }
                    for (id, name, args) in &collected_tools {
                        blocks.push(json!({
                            "type": "tool_use",
                            "id": id,
                            "name": name,
                            "input": args
                        }));
                    }
                    let stop_reason = if !collected_tools.is_empty() {
                        "tool_use"
                    } else if finish_reason == "length" {
                        "max_tokens"
                    } else {
                        "end_turn"
                    };
                    let out_json = json!({
                        "id": msg_id,
                        "type": "message",
                        "role": "assistant",
                        "model": request_model,
                        "content": blocks,
                        "stop_reason": stop_reason,
                        "usage": {
                            "input_tokens": total_in,
                            "output_tokens": total_out + total_think,
                            "cache_read_input_tokens": total_cache
                        }
                    });
                    keep_open = write_json_response_ka(stream, 200, &out_json, keep_alive).await
                        && keep_alive;
                }
            }

            ACTIVE_REQUESTS.fetch_sub(1, Ordering::Relaxed);
            let total_tokens = total_in + total_out + total_think + total_cache;
            let resp_preview: String = collected_text.chars().take(260).collect();
            record_api_log_async(ApiRequestLogItem {
                id: log_id,
                timestamp: Utc::now().timestamp(),
                api_key_id: api_key.id,
                api_key_label: api_key.label,
                account_id: acc.id,
                account_email: acc.email,
                protocol: protocol.into(),
                request_model,
                resolved_model,
                stream: is_stream,
                status_code: 200,
                latency_ms: started.elapsed().as_millis() as u64,
                input_tokens: total_in,
                output_tokens: total_out,
                thinking_tokens: total_think,
                cache_read_tokens: total_cache,
                total_tokens,
                error_message: None,
                request_preview: req_preview,
                response_preview: resp_preview,
                client_ip: client_ip.to_string(),
            });
            keep_open
        }
    }
}

// ============================================================================
// Server Lifecycle & Status
// ============================================================================

fn detect_local_lan_ip() -> Option<String> {
    let sock = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    sock.connect("8.8.8.8:80").ok()?;
    Some(sock.local_addr().ok()?.ip().to_string())
}

fn firewall_rule_name(port: u16) -> String {
    format!("OnlyDEVS Antigravity API Router (TCP {port})")
}

pub fn get_lan_diagnostics(port: Option<u16>) -> Result<LanDiagnostics, String> {
    let resolved_port = port.unwrap_or_else(|| load_config().map(|c| c.port).unwrap_or(57120));
    let lan_ip = detect_local_lan_ip();
    let rule_name = firewall_rule_name(resolved_port);
    let manual_command = format!(
        "netsh advfirewall firewall add rule name=\"{rule_name}\" dir=in action=allow protocol=TCP localport={resolved_port} profile=any"
    );

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        use std::process::Command;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;

        let fw_out = Command::new("netsh")
            .args([
                "advfirewall",
                "firewall",
                "show",
                "rule",
                &format!("name={rule_name}"),
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        let firewall_rule_exists = fw_out
            .map(|o| {
                if !o.status.success() {
                    return false;
                }
                let s = String::from_utf8_lossy(&o.stdout).to_ascii_lowercase();
                s.contains("enabled:") && s.contains("yes") && s.contains("allow")
            })
            .unwrap_or(false);

        let prof_out = Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "(Get-NetConnectionProfile | Select-Object -First 1 -ExpandProperty NetworkCategory) -as [string]",
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        let network_profile = prof_out
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|s| !s.is_empty());

        Ok(LanDiagnostics {
            port: resolved_port,
            lan_ip,
            firewall_rule_exists,
            network_profile,
            manual_command,
        })
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok(LanDiagnostics {
            port: resolved_port,
            lan_ip,
            firewall_rule_exists: true,
            network_profile: None,
            manual_command,
        })
    }
}

pub fn open_lan_firewall(port: Option<u16>) -> Result<LanDiagnostics, String> {
    let resolved_port = port.unwrap_or_else(|| load_config().map(|c| c.port).unwrap_or(57120));
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        use std::process::Command;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;

        let rule_name = firewall_rule_name(resolved_port);
        let ping_rule = "OnlyDEVS Antigravity LAN Ping (ICMPv4)";
        let inner = format!(
            "netsh advfirewall firewall delete rule name=\"{rule_name}\" >nul 2>&1 & \
             netsh advfirewall firewall add rule name=\"{rule_name}\" dir=in action=allow protocol=TCP localport={resolved_port} profile=any & \
             netsh advfirewall firewall delete rule name=\"{ping_rule}\" >nul 2>&1 & \
             netsh advfirewall firewall add rule name=\"{ping_rule}\" dir=in action=allow protocol=icmpv4:8,any profile=any"
        );
        let ps = format!(
            "$p = Start-Process -FilePath 'cmd.exe' -ArgumentList '/c','{}' -Verb RunAs -WindowStyle Hidden -Wait -PassThru; exit $p.ExitCode",
            inner.replace('\'', "''")
        );
        let output = Command::new("powershell")
            .args(["-NoProfile", "-Command", &ps])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map_err(|e| format!("Không thể gọi UAC mở Firewall: {e}"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(if stderr.is_empty() {
                "Người dùng đã hủy UAC hoặc không đủ quyền Admin để mở Windows Firewall.".into()
            } else {
                format!("Mở Windows Firewall thất bại: {stderr}")
            });
        }
    }
    get_lan_diagnostics(Some(resolved_port))
}

async fn bind_listener_with_retry(addr: &str) -> Result<TcpListener, std::io::Error> {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match TcpListener::bind(addr).await {
            Ok(listener) => return Ok(listener),
            Err(err) if err.kind() == std::io::ErrorKind::AddrInUse && Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            Err(err) => return Err(err),
        }
    }
}

async fn stop_server_locked() {
    SERVER_ACTIVE_GEN.store(0, Ordering::SeqCst);
    let running = server_lock().lock().ok().and_then(|mut g| g.take());
    if let Some(running) = running {
        let _ = running.shutdown_tx.send(true);
        for handle in running.accept_handles {
            handle.abort();
            let _ = handle.await;
        }
    }
}

pub async fn start_server() -> Result<ApiServiceStatus, String> {
    let _lifecycle = lifecycle_lock().lock().await;
    let cfg = load_config()?;
    // If already running on same bind/port, return status
    let bind_host = if cfg.access_scope == "lan" {
        "0.0.0.0"
    } else {
        "127.0.0.1"
    };
    {
        let guard = server_lock().lock().map_err(|_| "Lock poisoned")?;
        if let Some(running) = guard.as_ref() {
            if running.host == bind_host
                && running.port == cfg.port
                && SERVER_ACTIVE_GEN.load(Ordering::SeqCst) == running.generation
            {
                drop(guard);
                return get_status();
            }
        }
    }

    stop_server_locked().await;

    let addr = format!("{bind_host}:{}", cfg.port);
    let listener = bind_listener_with_retry(&addr).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::AddrInUse
            || e.kind() == std::io::ErrorKind::PermissionDenied
        {
            format!(
                "Không thể mở cổng {addr}: {e} (có thể cổng đang bị chiếm hoặc nằm trong Windows excludedportrange của Hyper-V/WSL — thử đổi sang cổng khác)."
            )
        } else {
            format!("Không thể mở cổng {addr}: {e}")
        }
    })?;

    let generation = SERVER_GEN_SEQ.fetch_add(1, Ordering::SeqCst).max(1);
    let (shutdown_tx, shutdown_rx_base) = watch::channel(false);
    let started_at = Utc::now().timestamp();
    let mut accept_handles = Vec::new();

    // Activate generation BEFORE spawning accept loops so first request is accepted cleanly
    SERVER_ACTIVE_GEN.store(generation, Ordering::SeqCst);

    {
        let mut rx4 = shutdown_rx_base.clone();
        let conn_rx_tpl = shutdown_rx_base.clone();
        let h4 = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = rx4.changed() => {
                        if *rx4.borrow() {
                            break;
                        }
                    }
                    accepted = listener.accept() => {
                        if *rx4.borrow() || SERVER_ACTIVE_GEN.load(Ordering::SeqCst) != generation {
                            break;
                        }
                        match accepted {
                            Ok((stream, peer)) => {
                                let peer_ip = normalize_peer_ip(&peer.ip().to_string());
                                tokio::spawn(handle_connection(
                                    stream,
                                    generation,
                                    conn_rx_tpl.clone(),
                                    peer_ip,
                                ));
                            }
                            Err(_) => {
                                tokio::time::sleep(Duration::from_millis(50)).await;
                            }
                        }
                    }
                }
            }
        });
        accept_handles.push(h4);
    }

    // Also bind IPv6 loopback/wildcard so `http://localhost:<port>/v1` resolves immediately on Windows 11
    let v6_addr = if cfg.access_scope == "lan" {
        format!("[::]:{}", cfg.port)
    } else {
        format!("[::1]:{}", cfg.port)
    };
    if let Ok(listener_v6) = TcpListener::bind(&v6_addr).await {
        let mut rx6 = shutdown_rx_base.clone();
        let conn_rx_tpl = shutdown_rx_base.clone();
        let h6 = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = rx6.changed() => {
                        if *rx6.borrow() {
                            break;
                        }
                    }
                    accepted = listener_v6.accept() => {
                        if *rx6.borrow() || SERVER_ACTIVE_GEN.load(Ordering::SeqCst) != generation {
                            break;
                        }
                        match accepted {
                            Ok((stream, peer)) => {
                                let peer_ip = normalize_peer_ip(&peer.ip().to_string());
                                tokio::spawn(handle_connection(
                                    stream,
                                    generation,
                                    conn_rx_tpl.clone(),
                                    peer_ip,
                                ));
                            }
                            Err(_) => {
                                tokio::time::sleep(Duration::from_millis(50)).await;
                            }
                        }
                    }
                }
            }
        });
        accept_handles.push(h6);
    }

    {
        let mut guard = server_lock().lock().map_err(|_| "Lock poisoned")?;
        *guard = Some(RunningServer {
            host: bind_host.to_string(),
            port: cfg.port,
            started_at,
            generation,
            shutdown_tx,
            accept_handles,
        });
    }

    get_status()
}

pub async fn stop_server() -> Result<ApiServiceStatus, String> {
    let _lifecycle = lifecycle_lock().lock().await;
    stop_server_locked().await;
    get_status()
}

/// Immediate non-blocking teardown used on app exit hooks.
pub fn stop_server_sync() {
    SERVER_ACTIVE_GEN.store(0, Ordering::SeqCst);
    if let Ok(mut guard) = server_lock().lock() {
        if let Some(running) = guard.take() {
            let _ = running.shutdown_tx.send(true);
            for handle in running.accept_handles {
                handle.abort();
            }
        }
    }
}

pub async fn restart_server() -> Result<ApiServiceStatus, String> {
    let _ = stop_server().await;
    tokio::time::sleep(Duration::from_millis(60)).await;
    start_server().await
}

pub fn get_status() -> Result<ApiServiceStatus, String> {
    let cfg = load_config()?;
    let keys = list_api_keys()?;
    let all_accs = accounts::list_accounts_sync().unwrap_or_default();
    let pool_accs = select_pool_members(all_accs, &cfg, None);
    let pool_total = pool_accs.len();
    let pool_healthy = pool_accs
        .iter()
        .filter(|a| !is_fully_cooled_down(&a.id))
        .count();
    let mut pool_full_nextgen = 0usize;
    let mut pool_classic = 0usize;
    let mut pool_unknown = 0usize;
    for a in &pool_accs {
        match classify_account_profile(a) {
            AccountProfile::FullNextgen => pool_full_nextgen += 1,
            AccountProfile::Classic => pool_classic += 1,
            AccountProfile::Unknown => pool_unknown += 1,
        }
    }
    let cooldowns = active_cooldowns();

    let (running, host, port, started_at) = {
        let guard = server_lock().lock().map_err(|_| "Lock poisoned")?;
        match guard.as_ref() {
            Some(r) => (true, r.host.clone(), r.port, Some(r.started_at)),
            None => (
                false,
                if cfg.access_scope == "lan" {
                    "0.0.0.0".to_string()
                } else {
                    "127.0.0.1".to_string()
                },
                cfg.port,
                None,
            ),
        }
    };

    let base_url = format!("http://localhost:{port}/v1");
    let lan_url = if cfg.access_scope == "lan" {
        detect_local_lan_ip().map(|ip| format!("http://{ip}:{port}/v1"))
    } else {
        None
    };

    Ok(ApiServiceStatus {
        running,
        host,
        port,
        base_url,
        lan_url,
        started_at,
        active_requests: ACTIVE_REQUESTS.load(Ordering::Relaxed),
        pool_total,
        pool_healthy,
        pool_full_nextgen,
        pool_classic,
        pool_unknown,
        cooldowns,
        config: cfg,
        keys,
    })
}

// ============================================================================
// Request Logs & API Usage Analytics
// ============================================================================

pub fn list_request_logs(limit: Option<usize>) -> Result<Vec<ApiRequestLogItem>, String> {
    let conn = open_db()?;
    ensure_api_schema(&conn)?;
    let lim = limit.unwrap_or(200).min(1000) as i64;
    let mut stmt = conn
        .prepare(
            "SELECT id, timestamp, api_key_id, api_key_label, account_id, account_email,
                    protocol, request_model, resolved_model, stream, status_code, latency_ms,
                    input_tokens, output_tokens, thinking_tokens, cache_read_tokens, total_tokens,
                    error_message, request_preview, response_preview, client_ip
             FROM api_service_logs
             ORDER BY timestamp DESC
             LIMIT ?1",
        )
        .map_err(|e| format!("Prepare list logs: {e}"))?;
    let rows = stmt
        .query_map(params![lim], |r| {
            Ok(ApiRequestLogItem {
                id: r.get(0)?,
                timestamp: r.get(1)?,
                api_key_id: r.get(2)?,
                api_key_label: r.get(3)?,
                account_id: r.get(4)?,
                account_email: r.get(5)?,
                protocol: r.get(6)?,
                request_model: r.get(7)?,
                resolved_model: r.get(8)?,
                stream: r.get::<_, i64>(9)? != 0,
                status_code: r.get::<_, i64>(10)?.max(0) as u16,
                latency_ms: r.get::<_, i64>(11)?.max(0) as u64,
                input_tokens: r.get::<_, i64>(12)?.max(0) as u64,
                output_tokens: r.get::<_, i64>(13)?.max(0) as u64,
                thinking_tokens: r.get::<_, i64>(14)?.max(0) as u64,
                cache_read_tokens: r.get::<_, i64>(15)?.max(0) as u64,
                total_tokens: r.get::<_, i64>(16)?.max(0) as u64,
                error_message: r.get(17)?,
                request_preview: r.get(18)?,
                response_preview: r.get(19)?,
                client_ip: r.get::<_, String>(20).unwrap_or_default(),
            })
        })
        .map_err(|e| format!("Query list logs: {e}"))?;
    Ok(rows.flatten().collect())
}

pub fn clear_request_logs() -> Result<(), String> {
    let conn = open_db()?;
    ensure_api_schema(&conn)?;
    conn.execute("DELETE FROM api_service_logs", [])
        .map_err(|e| format!("Clear api logs: {e}"))?;
    conn.execute(
        "UPDATE api_service_keys SET total_requests = 0, total_tokens = 0",
        [],
    )
    .ok();
    if let Ok(mut map) = key_daily_tokens().lock() {
        map.clear();
    }
    invalidate_api_keys_cache();
    Ok(())
}

pub fn get_usage_summary(range: Option<String>) -> Result<ApiServiceUsageSummary, String> {
    let conn = open_db()?;
    ensure_api_schema(&conn)?;
    let range_key = range.unwrap_or_else(|| "30d".into());
    let now = Utc::now().timestamp();
    let cutoff = match range_key.as_str() {
        "today" => now - 86_400,
        "7d" => now - 7 * 86_400,
        "30d" => now - 30 * 86_400,
        _ => 0,
    };

    let mut stmt = conn
        .prepare(
            "SELECT timestamp, api_key_id, api_key_label, account_id, account_email,
                    resolved_model, status_code, latency_ms,
                    input_tokens, output_tokens, thinking_tokens, cache_read_tokens, total_tokens
             FROM api_service_logs
             WHERE timestamp >= ?1
             ORDER BY timestamp ASC",
        )
        .map_err(|e| format!("Prepare usage summary: {e}"))?;

    let mut total_requests = 0u64;
    let mut success_requests = 0u64;
    let mut error_requests = 0u64;
    let mut latency_sum = 0u64;
    let mut in_sum = 0u64;
    let mut think_sum = 0u64;
    let mut out_sum = 0u64;
    let mut cache_sum = 0u64;
    let mut tok_sum = 0u64;

    let mut by_model: HashMap<String, (ApiUsageBreakdownRow, u64)> = HashMap::new();
    let mut by_account: HashMap<String, (ApiUsageBreakdownRow, u64)> = HashMap::new();
    let mut by_key: HashMap<String, (ApiUsageBreakdownRow, u64)> = HashMap::new();
    let mut series: BTreeMap<String, ApiUsageTimePoint> = BTreeMap::new();

    let rows = stmt
        .query_map(params![cutoff], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, i64>(6)?,
                r.get::<_, i64>(7)?.max(0) as u64,
                r.get::<_, i64>(8)?.max(0) as u64,
                r.get::<_, i64>(9)?.max(0) as u64,
                r.get::<_, i64>(10)?.max(0) as u64,
                r.get::<_, i64>(11)?.max(0) as u64,
                r.get::<_, i64>(12)?.max(0) as u64,
            ))
        })
        .map_err(|e| format!("Query usage summary: {e}"))?;

    for row in rows.flatten() {
        let (ts, key_id, key_label, acc_id, acc_email, model, status, lat, inp, out, think, cache, tot) =
            row;
        let is_ok = (200..300).contains(&status);
        total_requests += 1;
        if is_ok {
            success_requests += 1;
        } else {
            error_requests += 1;
        }
        latency_sum += lat;
        in_sum += inp;
        think_sum += think;
        out_sum += out;
        cache_sum += cache;
        tok_sum += tot;

        let bump = |map: &mut HashMap<String, (ApiUsageBreakdownRow, u64)>, k: String, lbl: String| {
            let entry = map.entry(k.clone()).or_insert_with(|| {
                (
                    ApiUsageBreakdownRow {
                        key: k,
                        label: lbl,
                        requests: 0,
                        errors: 0,
                        input_tokens: 0,
                        thinking_tokens: 0,
                        output_tokens: 0,
                        cache_read_tokens: 0,
                        total_tokens: 0,
                        avg_latency_ms: 0,
                    },
                    0,
                )
            });
            entry.0.requests += 1;
            if !is_ok {
                entry.0.errors += 1;
            }
            entry.0.input_tokens += inp;
            entry.0.thinking_tokens += think;
            entry.0.output_tokens += out;
            entry.0.cache_read_tokens += cache;
            entry.0.total_tokens += tot;
            entry.1 += lat;
            entry.0.avg_latency_ms = entry.1 / entry.0.requests.max(1);
        };

        bump(&mut by_model, model.clone(), model);
        let acc_k = if acc_id.is_empty() { "none".into() } else { acc_id };
        let acc_l = if acc_email.is_empty() { "No Account".into() } else { acc_email };
        bump(&mut by_account, acc_k, acc_l);
        bump(&mut by_key, key_id, key_label);

        let bucket = chrono::DateTime::from_timestamp(ts, 0)
            .map(|dt| {
                if range_key == "today" {
                    dt.format("%H:00").to_string()
                } else {
                    dt.format("%m-%d").to_string()
                }
            })
            .unwrap_or_else(|| "unknown".into());
        let pt = series.entry(bucket.clone()).or_insert(ApiUsageTimePoint {
            bucket,
            requests: 0,
            errors: 0,
            input_tokens: 0,
            thinking_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            total_tokens: 0,
        });
        pt.requests += 1;
        if !is_ok {
            pt.errors += 1;
        }
        pt.input_tokens += inp;
        pt.thinking_tokens += think;
        pt.output_tokens += out;
        pt.cache_read_tokens += cache;
        pt.total_tokens += tot;
    }

    let finalize = |map: HashMap<String, (ApiUsageBreakdownRow, u64)>| -> Vec<ApiUsageBreakdownRow> {
        let mut v: Vec<ApiUsageBreakdownRow> = map.into_values().map(|(r, _)| r).collect();
        v.sort_by(|a, b| b.total_tokens.cmp(&a.total_tokens).then_with(|| b.requests.cmp(&a.requests)));
        v
    };

    Ok(ApiServiceUsageSummary {
        range: range_key,
        total_requests,
        success_requests,
        error_requests,
        success_rate: if total_requests > 0 {
            (success_requests as f64 / total_requests as f64) * 100.0
        } else {
            100.0
        },
        avg_latency_ms: if total_requests > 0 {
            latency_sum / total_requests
        } else {
            0
        },
        input_tokens: in_sum,
        thinking_tokens: think_sum,
        output_tokens: out_sum,
        cache_read_tokens: cache_sum,
        total_tokens: tok_sum,
        by_model: finalize(by_model),
        by_account: finalize(by_account),
        by_key: finalize(by_key),
        time_series: series.into_values().collect(),
    })
}

// ============================================================================
// Built-in Live Playground Test Chat (uses real Antigravity upstream + logs it)
// ============================================================================

pub async fn test_chat_completion(
    model: String,
    prompt: String,
) -> Result<ApiTestChatResult, String> {
    if !is_server_running() {
        return Err("API Router đang tắt. Hãy bấm 'Chạy API' trước khi test.".into());
    }
    let cfg = load_config()?;
    let keys = list_api_keys()?;
    let key = keys
        .into_iter()
        .find(|k| k.enabled)
        .ok_or_else(|| "Không có API key đang bật.".to_string())?;
    let resolved_model = resolve_model_alias(&cfg, &model);
    let body = json!({
        "model": model,
        "messages": [{ "role": "user", "content": prompt }],
        "max_tokens": 1024,
        "stream": false
    });

    let started = Instant::now();
    match send_upstream_with_failover(&cfg, &key, &resolved_model, &body, false).await {
        Ok((resp, acc, req_preview, _tool_name_map)) => {
            let raw_text = resp.text().await.unwrap_or_default();
            let parsed = serde_json::from_str::<Value>(&raw_text)
                .map(|v| parse_antigravity_response_payload(&v))
                .unwrap_or_default();
            let latency_ms = started.elapsed().as_millis() as u64;
            let total_tokens = parsed.input_tokens
                + parsed.output_tokens
                + parsed.thinking_tokens
                + parsed.cache_read_tokens;
            record_api_log(&ApiRequestLogItem {
                id: Uuid::new_v4().to_string(),
                timestamp: Utc::now().timestamp(),
                api_key_id: key.id,
                api_key_label: format!("{} (Test)", key.label),
                account_id: acc.id,
                account_email: acc.email.clone(),
                protocol: "playground".into(),
                request_model: model,
                resolved_model: resolved_model.clone(),
                stream: false,
                status_code: 200,
                latency_ms,
                input_tokens: parsed.input_tokens,
                output_tokens: parsed.output_tokens,
                thinking_tokens: parsed.thinking_tokens,
                cache_read_tokens: parsed.cache_read_tokens,
                total_tokens,
                error_message: None,
                request_preview: req_preview,
                response_preview: parsed.text.chars().take(260).collect(),
                client_ip: "127.0.0.1".into(),
            });
            Ok(ApiTestChatResult {
                ok: true,
                status_code: 200,
                account_email: acc.email,
                resolved_model,
                latency_ms,
                text: parsed.text,
                thinking: parsed.thinking,
                input_tokens: parsed.input_tokens,
                output_tokens: parsed.output_tokens,
                thinking_tokens: parsed.thinking_tokens,
                error: None,
            })
        }
        Err((status, err, acc_id, acc_email, _err_preview)) => {
            let latency_ms = started.elapsed().as_millis() as u64;
            record_api_log(&ApiRequestLogItem {
                id: Uuid::new_v4().to_string(),
                timestamp: Utc::now().timestamp(),
                api_key_id: key.id,
                api_key_label: format!("{} (Test)", key.label),
                account_id: acc_id,
                account_email: acc_email.clone(),
                protocol: "playground".into(),
                request_model: model,
                resolved_model: resolved_model.clone(),
                stream: false,
                status_code: status,
                latency_ms,
                input_tokens: 0,
                output_tokens: 0,
                thinking_tokens: 0,
                cache_read_tokens: 0,
                total_tokens: 0,
                error_message: Some(err.clone()),
                request_preview: prompt.chars().take(240).collect(),
                response_preview: String::new(),
                client_ip: "127.0.0.1".into(),
            });
            Ok(ApiTestChatResult {
                ok: false,
                status_code: status,
                account_email: acc_email,
                resolved_model,
                latency_ms,
                text: String::new(),
                thinking: String::new(),
                input_tokens: 0,
                output_tokens: 0,
                thinking_tokens: 0,
                error: Some(err),
            })
        }
    }
}

pub fn spawn_auto_start_if_enabled() {
    if let Ok(cfg) = load_config() {
        if cfg.auto_start {
            tauri::async_runtime::spawn(async {
                let _ = start_server().await;
            });
        }
    }
}

/// Periodically refreshes pool account quotas in background while API Router is running,
/// keeping Total Pool Quota and smart routing scores fresh without manual clicks.
pub fn spawn_background_quota_worker(app_handle: tauri::AppHandle) {
    use tauri::Emitter;
    tauri::async_runtime::spawn(async move {
        let mut last_sweep = Instant::now();
        loop {
            tokio::time::sleep(Duration::from_secs(30)).await;
            if !is_server_running() {
                continue;
            }
            let Ok(cfg) = load_config() else {
                continue;
            };
            if cfg.auto_refresh_quota_minutes == 0 {
                continue;
            }
            let interval = Duration::from_secs(cfg.auto_refresh_quota_minutes.max(1) * 60);
            if last_sweep.elapsed() < interval {
                continue;
            }
            last_sweep = Instant::now();
            let all = accounts::list_accounts_sync().unwrap_or_default();
            let members = select_pool_members(all, &cfg, None);
            let mut updated = 0usize;
            for acc in members {
                if !is_server_running() {
                    break;
                }
                if accounts::refresh_single_account_quota(acc.id).await.is_ok() {
                    updated += 1;
                }
                tokio::time::sleep(Duration::from_millis(350)).await;
            }
            if updated > 0 {
                let _ = app_handle.emit("api-pool-quota-updated", updated);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::{ModelQuota, QuotaData, TokenData};

    fn make_acc(id: &str, email: &str, models: Vec<(&str, i32)>) -> AntigravityAccount {
        AntigravityAccount {
            id: id.into(),
            email: email.into(),
            name: None,
            picture: None,
            tags: vec![],
            notes: None,
            two_factor_secret: None,
            account_password: None,
            phone_number: None,
            mail_url: None,
            aux_email: None,
            pending_oauth: false,
            disabled: false,
            disabled_reason: None,
            token: TokenData {
                access_token: "tok".into(),
                refresh_token: "ref".into(),
                expires_in: 3600,
                expiry_timestamp: 9999999999,
                token_type: "Bearer".into(),
                email: Some(email.into()),
                project_id: None,
                is_gcp_tos: None,
                id_token: None,
            },
            quota: Some(QuotaData {
                models: models
                    .into_iter()
                    .map(|(n, pct)| ModelQuota {
                        name: n.into(),
                        display_name: Some(n.into()),
                        percentage: pct,
                        reset_time: String::new(),
                        summary_exhausted: Some(pct == 0),
                    })
                    .collect(),
                last_updated: 1,
                is_forbidden: false,
                subscription_tier: Some("g1-pro-tier".into()),
                credits: vec![],
                is_gcp_tos: None,
                project_id: None,
                quota_summary_stale: false,
                quota_summary_updated_at: None,
            }),
            quota_error: None,
            created_at: 1,
            last_used: 1,
            last_switched_at: None,
            is_current: false,
        }
    }

    fn full_acc(id: &str, email: &str, g_pct: i32, c_pct: i32, argon_pct: i32) -> AntigravityAccount {
        make_acc(
            id,
            email,
            vec![
                ("gemini-5h", g_pct),
                ("gemini-weekly", g_pct),
                ("3p-5h", c_pct),
                ("3p-weekly", c_pct),
                ("gemini-argon-wtu-weekly", argon_pct),
                ("gemini-4-argon-high", argon_pct),
                ("gemini-4-neon-medium", g_pct),
                ("gemini-3.8-flash-high", g_pct),
                ("claude-sonnet-5-5-medium", c_pct),
            ],
        )
    }

    fn classic_acc(id: &str, email: &str, g_pct: i32, c_pct: i32) -> AntigravityAccount {
        make_acc(
            id,
            email,
            vec![
                ("gemini-5h", g_pct),
                ("gemini-weekly", g_pct),
                ("3p-5h", c_pct),
                ("3p-weekly", c_pct),
                ("gemini-3.8-flash-high", g_pct),
                ("claude-sonnet-4-6", c_pct),
                ("claude-opus-4-6-thinking", c_pct),
            ],
        )
    }

    #[test]
    fn smart_routing_separates_argon_and_claude_46_and_preserves_full_on_shared() {
        let cfg = ApiServiceConfig::default(); // smart + preserve_exclusive_accounts = true
        let members = vec![
            full_acc("f_low_argon", "full-low@test.com", 100, 100, 5),
            full_acc("f_high_argon", "full-high@test.com", 90, 80, 95),
            classic_acc("c_high", "classic-high@test.com", 70, 90),
            classic_acc("c_low", "classic-low@test.com", 10, 20),
        ];

        // 1. Argon request -> ONLY Full accounts, highest Argon weekly first (95 > 5), NEVER Classic
        let argon_ranked = rank_pool_candidates(
            members.clone(),
            &cfg,
            "k1",
            "gemini-4-argon-high",
            &HashSet::new(),
        );
        let argon_ids: Vec<&str> = argon_ranked.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(argon_ids, vec!["f_high_argon", "f_low_argon"]);

        // 2. Claude 4.6 request -> ONLY Classic accounts, highest Claude quota first (90 > 20), NEVER Full
        let c46_ranked = rank_pool_candidates(
            members.clone(),
            &cfg,
            "k1",
            "claude-sonnet-4-6",
            &HashSet::new(),
        );
        let c46_ids: Vec<&str> = c46_ranked.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(c46_ids, vec!["c_high", "c_low"]);

        // 3. Shared model (gemini-3.8-flash-high) -> prefers healthy Classic (>15%) first to preserve Full/Argon quota!
        let shared_ranked = rank_pool_candidates(
            members.clone(),
            &cfg,
            "k1",
            "gemini-3.8-flash-high",
            &HashSet::new(),
        );
        assert_eq!(shared_ranked[0].id, "c_high");
        assert_eq!(shared_ranked[1].id, "f_low_argon"); // 100% > 90%
        assert_eq!(shared_ranked[2].id, "f_high_argon");
        assert_eq!(shared_ranked[3].id, "c_low"); // <= 15% overflows after healthy Full

        // 4. Scoped cooldown: cooling down Claude on c_high must NOT block c_high for Gemini 3.8!
        mark_account_cooldown("c_high", "classic-high@test.com", "claude", 120, "HTTP 429".into());
        let c46_after_cd = rank_pool_candidates(
            members.clone(),
            &cfg,
            "k1",
            "claude-sonnet-4-6",
            &HashSet::new(),
        );
        assert_eq!(c46_after_cd[0].id, "c_low");
        let g38_after_cd = rank_pool_candidates(
            members,
            &cfg,
            "k1",
            "gemini-3.8-flash-high",
            &HashSet::new(),
        );
        assert_eq!(g38_after_cd[0].id, "c_high");
        let _ = clear_account_cooldowns(None);
    }

    #[test]
    fn anti_429_envelope_omits_request_type_sanitizes_branding_and_cloaks_tools() {
        let body = json!({
            "model": "gemini-4-neon-medium",
            "messages": [
                {
                    "role": "system",
                    "content": "x-anthropic-billing-header: cc_version=2.1\nYou are OpenCode, an AI coding agent. You are a Claude agent, built on Anthropic's Claude Agent SDK."
                },
                { "role": "user", "content": "Run ls" }
            ],
            "tools": [
                {
                    "type": "function",
                    "function": {
                        "name": "bash",
                        "description": "Run shell command",
                        "parameters": {
                            "type": "object",
                            "additionalProperties": false,
                            "properties": {
                                "cmd": { "type": "string", "minLength": 1, "format": "plain" }
                            }
                        }
                    }
                }
            ]
        });

        let (env, preview, tool_map) =
            openai_body_to_antigravity_envelope(&body, "gemini-4-neon-medium", "p1", "k1");

        // 1. Chat envelope MUST omit requestType (prevents Google fake 429)
        assert!(env.get("requestType").is_none());
        assert_eq!(preview, "Run ls");

        // 2. System instruction strips billing header, SDK branding, and rewrites OpenCode -> Antigravity
        let sys = env
            .pointer("/request/systemInstruction/parts/0/text")
            .and_then(Value::as_str)
            .unwrap_or("");
        assert!(!sys.to_lowercase().contains("opencode"));
        assert!(!sys.contains("x-anthropic-billing-header"));
        assert!(!sys.contains("Claude Agent SDK"));
        assert!(sys.contains("Antigravity"));

        // 3. Tool cloaking: bash -> bash_ide + native decoys appended, uncloak restores "bash"
        let decls = env
            .pointer("/request/tools/0/functionDeclarations")
            .and_then(Value::as_array)
            .unwrap();
        assert_eq!(decls[0].get("name").and_then(Value::as_str), Some("bash_ide"));
        assert!(decls.iter().any(|d| d.get("name").and_then(Value::as_str) == Some("view_file")));
        assert_eq!(uncloak_tool_name("bash_ide", &tool_map), "bash");
        // Unsupported schema keywords stripped
        assert!(decls[0].pointer("/parameters/additionalProperties").is_none());
        assert!(decls[0].pointer("/parameters/properties/cmd/minLength").is_none());
    }

    #[tokio::test]
    async fn http_keep_alive_and_v1_ping_reuse_same_socket_fast() {
        let _guard = lifecycle_lock().lock().await;
        let test_gen = SERVER_GEN_SEQ.fetch_add(1, Ordering::SeqCst).max(1);
        SERVER_ACTIVE_GEN.store(test_gen, Ordering::SeqCst);
        let (_tx, rx) = watch::channel(false);

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            handle_connection(stream, test_gen, rx, "127.0.0.1".into()).await;
        });

        let mut client = TcpStream::connect(addr).await.unwrap();
        // 1st request: GET /v1 with keep-alive
        client
            .write_all(b"GET /v1 HTTP/1.1\r\nHost: 192.168.1.107:57120\r\n\r\n")
            .await
            .unwrap();
        let mut buf1 = [0u8; 1024];
        let n1 = client.read(&mut buf1).await.unwrap();
        let resp1 = String::from_utf8_lossy(&buf1[..n1]);
        assert!(resp1.starts_with("HTTP/1.1 200 OK"));
        assert!(resp1.contains("Connection: keep-alive"));
        assert!(resp1.contains("\"status\":\"ok\""));

        // 2nd request on the EXACT same TCP socket with Connection: close
        client
            .write_all(b"GET /health HTTP/1.1\r\nHost: 192.168.1.107:57120\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();
        let mut buf2 = [0u8; 1024];
        let n2 = client.read(&mut buf2).await.unwrap();
        let resp2 = String::from_utf8_lossy(&buf2[..n2]);
        assert!(resp2.starts_with("HTTP/1.1 200 OK"));
        assert!(resp2.contains("Connection: close"));
        let _ = server.await;
        SERVER_ACTIVE_GEN.store(0, Ordering::SeqCst);
    }

    #[tokio::test]
    async fn stop_signal_immediately_cuts_active_keep_alive_connection() {
        let _guard = lifecycle_lock().lock().await;
        let test_gen = SERVER_GEN_SEQ.fetch_add(1, Ordering::SeqCst).max(1);
        SERVER_ACTIVE_GEN.store(test_gen, Ordering::SeqCst);
        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            handle_connection(stream, test_gen, shutdown_rx, "127.0.0.1".into()).await;
        });

        let mut client = TcpStream::connect(addr).await.unwrap();
        // 1st request succeeds while running
        client
            .write_all(b"GET /v1 HTTP/1.1\r\nHost: 127.0.0.1:57120\r\n\r\n")
            .await
            .unwrap();
        let mut buf1 = [0u8; 1024];
        let n1 = client.read(&mut buf1).await.unwrap();
        let resp1 = String::from_utf8_lossy(&buf1[..n1]);
        assert!(resp1.starts_with("HTTP/1.1 200 OK"));
        assert!(resp1.contains("Connection: keep-alive"));

        // Simulate clicking "Stop API"
        SERVER_ACTIVE_GEN.store(0, Ordering::SeqCst);
        let _ = shutdown_tx.send(true);
        // Server handler must exit immediately (not wait 60s idle timeout!)
        tokio::time::timeout(Duration::from_millis(500), server)
            .await
            .expect("handle_connection should exit immediately on stop")
            .unwrap();

        // Subsequent request on the old keep-alive socket MUST NOT return 200 OK
        let _ = client
            .write_all(b"GET /v1 HTTP/1.1\r\nHost: 127.0.0.1:57120\r\n\r\n")
            .await;
        let mut buf2 = [0u8; 512];
        let n2 = client.read(&mut buf2).await.unwrap_or(0);
        let resp2 = String::from_utf8_lossy(&buf2[..n2]);
        assert!(
            n2 == 0 || resp2.starts_with("HTTP/1.1 503"),
            "Expected EOF or 503 after stop, got: {resp2}"
        );
    }

    #[tokio::test]
    async fn sse_chunked_encoding_emits_rfc9112_frames_and_terminator() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            write_sse_headers(&mut stream).await.unwrap();
            write_sse_chunk(&mut stream, "data: {\"ok\":true}\n\n")
                .await
                .unwrap();
            write_sse_end(&mut stream).await;
        });

        let mut client = TcpStream::connect(addr).await.unwrap();
        let mut raw = Vec::new();
        client.read_to_end(&mut raw).await.unwrap();
        let text = String::from_utf8_lossy(&raw);
        assert!(text.contains("Transfer-Encoding: chunked"));
        assert!(text.contains("X-Accel-Buffering: no"));
        assert!(text.contains("13\r\ndata: {\"ok\":true}\n\n\r\n"));
        assert!(text.ends_with("0\r\n\r\n"));
        let _ = server.await;
    }

    #[test]
    fn key_profile_lock_and_limits_and_ip_normalization_work() {
        // 1. Peer IP normalization strips IPv4-mapped IPv6 prefix
        assert_eq!(normalize_peer_ip("::ffff:192.168.1.50"), "192.168.1.50");
        assert_eq!(normalize_peer_ip("192.168.1.107"), "192.168.1.107");

        // 2. Classic-locked API key MUST exclude Full/Argon accounts from pool selection
        let full_acc = make_acc(
            "acc-full",
            "full@example.com",
            vec![("gemini-4-argon", 100), ("gemini-3.8-flash-tiered", 100)],
        );
        let classic_acc = make_acc(
            "acc-classic",
            "classic@example.com",
            vec![("claude-opus-4-6-thinking", 100), ("gemini-3.8-flash-tiered", 100)],
        );
        let cfg = ApiServiceConfig::default();
        let classic_key = ApiKeyRecord {
            id: "key-friend-classic".into(),
            label: "Friend Classic".into(),
            key: "agt_test_1".into(),
            enabled: true,
            is_master: false,
            allowed_models: vec![],
            pinned_account_ids: vec![],
            allowed_profile: "classic".into(),
            rate_limit_rpm: 2,
            daily_token_limit: 500,
            expires_at: None,
            created_at: 0,
            last_used_at: None,
            total_requests: 0,
            total_tokens: 0,
        };
        let selected = select_pool_members(vec![full_acc, classic_acc], &cfg, Some(&classic_key));
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].id, "acc-classic");

        // 3. Sliding 60s RPM rate limiter allows 2 reqs then blocks 3rd with HTTP 429
        if let Ok(mut map) = key_rpm_buckets().lock() {
            map.remove(&classic_key.id);
        }
        if let Ok(mut map) = key_daily_tokens().lock() {
            map.insert(classic_key.id.clone(), (utc_today_key(), 100));
        }
        assert!(check_api_key_limits(&classic_key).is_ok());
        assert!(check_api_key_limits(&classic_key).is_ok());
        let err_rpm = check_api_key_limits(&classic_key).unwrap_err();
        assert_eq!(err_rpm.0, 429);

        // 4. Daily token limit blocks with HTTP 429
        let mut cap_key = classic_key.clone();
        cap_key.id = "key-cap-test".into();
        cap_key.rate_limit_rpm = 0;
        cap_key.daily_token_limit = 1000;
        if let Ok(mut map) = key_daily_tokens().lock() {
            map.insert(cap_key.id.clone(), (utc_today_key(), 1000));
        }
        let err_cap = check_api_key_limits(&cap_key).unwrap_err();
        assert_eq!(err_cap.0, 429);

        // 5. Expired key rejects with HTTP 401
        let mut exp_key = cap_key.clone();
        exp_key.expires_at = Some(Utc::now().timestamp() - 10);
        let err_exp = check_api_key_limits(&exp_key).unwrap_err();
        assert_eq!(err_exp.0, 401);
    }
}
