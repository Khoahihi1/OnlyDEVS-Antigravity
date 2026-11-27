use crate::paths;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::OnceLock;
use tauri::AppHandle;
#[cfg(not(test))]
use tauri::Emitter;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot, Mutex, RwLock};
use uuid::Uuid;

pub const BRIDGE_EXTENSION_ID: &str = "onlydevs-auth-bridge";
pub const BRIDGE_EXTENSION_VERSION: &str = "1.0.2";
const BRIDGE_PORT_START: u16 = 57128;
const BRIDGE_PORT_END: u16 = 57148;
const WS_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
const MAX_WS_FRAME_LEN: usize = 2 * 1024 * 1024;
const WS_HELLO_TIMEOUT_SECS: u64 = 10;
const WS_IDLE_PING_SECS: u64 = 15;
const WS_PONG_DEAD_SECS: u64 = 12;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct IdeBridgeSettings {
    pub seamless_auto_switch: bool,
    pub seamless_priority: bool,
    pub auto_heal_inject: bool,
    pub fallback_relaunch: bool,
}

impl Default for IdeBridgeSettings {
    fn default() -> Self {
        Self {
            seamless_auto_switch: false,
            seamless_priority: true,
            auto_heal_inject: false,
            fallback_relaunch: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectedIdeClientInfo {
    pub client_id: u64,
    pub extension_version: String,
    pub vscode_version: String,
    pub ide_name: String,
    pub connected_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdeBridgeStatus {
    pub server_running: bool,
    pub ws_port: Option<u16>,
    pub ide_found: bool,
    pub ide_root_path: Option<String>,
    pub extension_dir: Option<String>,
    pub extension_injected: bool,
    pub extension_version: Option<String>,
    pub needs_update: bool,
    pub connected_count: usize,
    pub connected_clients: Vec<ConnectedIdeClientInfo>,
    pub settings: IdeBridgeSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BridgeDiscoveryFile {
    port: u16,
    token: String,
    pid: u32,
    version: String,
    updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct SeamlessSwitchPayload {
    pub email: String,
    pub access_token: String,
    pub refresh_token: String,
    pub expiry_date_seconds: i64,
    pub is_gcp_tos: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeamlessSwitchReply {
    pub success: bool,
    pub email: Option<String>,
    pub effective_mode: Option<String>,
    pub message: Option<String>,
}

struct ConnectedClientHandle {
    info: ConnectedIdeClientInfo,
    tx: mpsc::UnboundedSender<WsOutbound>,
    session_id: Option<String>,
    host_pid: Option<u32>,
}

enum WsOutbound {
    Text(String),
    Ping(Vec<u8>),
    Pong(Vec<u8>),
    Close,
}

static SERVER_STARTED: AtomicBool = AtomicBool::new(false);
static SERVER_PORT: OnceLock<RwLock<Option<u16>>> = OnceLock::new();
static AUTH_TOKEN: OnceLock<String> = OnceLock::new();
static CLIENT_SEQ: AtomicU64 = AtomicU64::new(1);
static CLIENTS: OnceLock<RwLock<HashMap<u64, ConnectedClientHandle>>> = OnceLock::new();
static PENDING_SWITCHES: OnceLock<Mutex<HashMap<String, oneshot::Sender<SeamlessSwitchReply>>>> =
    OnceLock::new();
static APP_HANDLE: OnceLock<std::sync::Mutex<Option<AppHandle>>> = OnceLock::new();

fn server_port_lock() -> &'static RwLock<Option<u16>> {
    SERVER_PORT.get_or_init(|| RwLock::new(None))
}

fn auth_token() -> &'static str {
    AUTH_TOKEN
        .get_or_init(|| Uuid::new_v4().to_string().replace('-', ""))
        .as_str()
}

fn clients_map() -> &'static RwLock<HashMap<u64, ConnectedClientHandle>> {
    CLIENTS.get_or_init(|| RwLock::new(HashMap::new()))
}

fn pending_map() -> &'static Mutex<HashMap<String, oneshot::Sender<SeamlessSwitchReply>>> {
    PENDING_SWITCHES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn app_handle_slot() -> &'static std::sync::Mutex<Option<AppHandle>> {
    APP_HANDLE.get_or_init(|| std::sync::Mutex::new(None))
}

fn settings_file_path() -> Result<PathBuf, String> {
    Ok(paths::app_home()?.join("ide-bridge-settings.json"))
}

fn discovery_file_path() -> Result<PathBuf, String> {
    Ok(paths::app_home()?.join("ide-bridge.json"))
}

pub fn load_settings() -> IdeBridgeSettings {
    let Ok(path) = settings_file_path() else {
        return IdeBridgeSettings::default();
    };
    let Ok(raw) = fs::read_to_string(path) else {
        return IdeBridgeSettings::default();
    };
    serde_json::from_str::<IdeBridgeSettings>(&raw).unwrap_or_default()
}

fn write_settings_only(settings: &IdeBridgeSettings) -> Result<(), String> {
    let path = settings_file_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Tạo thư mục cài đặt bridge: {e}"))?;
    }
    let body = serde_json::to_string_pretty(settings)
        .map_err(|e| format!("Serialize bridge settings: {e}"))?;
    fs::write(&path, body).map_err(|e| format!("Ghi bridge settings: {e}"))?;
    Ok(())
}

pub fn save_settings(mut settings: IdeBridgeSettings) -> Result<IdeBridgeSettings, String> {
    if settings.seamless_auto_switch {
        settings.seamless_priority = true;
        crate::auto_switch::disable_classic_mode();
    }
    write_settings_only(&settings)?;
    emit_status_changed();
    Ok(settings)
}

pub fn disable_seamless_mode() {
    let mut cfg = load_settings();
    if cfg.seamless_auto_switch || cfg.seamless_priority {
        cfg.seamless_auto_switch = false;
        cfg.seamless_priority = false;
        let _ = write_settings_only(&cfg);
        emit_status_changed();
    }
}

fn emit_status_changed() {
    #[cfg(not(test))]
    {
        if let Ok(guard) = app_handle_slot().lock() {
            if let Some(handle) = guard.as_ref() {
                let h = handle.clone();
                tauri::async_runtime::spawn(async move {
                    let status = get_status().await;
                    let _ = h.emit("ide-bridge-status-changed", status);
                });
            }
        }
    }
}

pub fn is_seamless_auto_switch_enabled() -> bool {
    load_settings().seamless_auto_switch
}

pub fn is_seamless_priority_enabled() -> bool {
    let s = load_settings();
    s.seamless_auto_switch || s.seamless_priority
}

pub fn is_fallback_relaunch_enabled() -> bool {
    load_settings().fallback_relaunch
}

fn is_os_pid_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    #[cfg(target_os = "windows")]
    {
        const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
        const STILL_ACTIVE: u32 = 259;
        unsafe extern "system" {
            fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut std::ffi::c_void;
            fn GetExitCodeProcess(handle: *mut std::ffi::c_void, exit_code: *mut u32) -> i32;
            fn CloseHandle(handle: *mut std::ffi::c_void) -> i32;
        }
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if h.is_null() {
                // ERROR_ACCESS_DENIED (5) means process exists; ERROR_INVALID_PARAMETER (87) means gone.
                return std::io::Error::last_os_error().raw_os_error() == Some(5);
            }
            let mut exit_code: u32 = 0;
            let ok = GetExitCodeProcess(h, &mut exit_code);
            CloseHandle(h);
            ok != 0 && exit_code == STILL_ACTIVE
        }
    }
    #[cfg(target_os = "linux")]
    {
        Path::new(&format!("/proc/{pid}")).exists()
    }
    #[cfg(target_os = "macos")]
    {
        if pid > i32::MAX as u32 {
            return false;
        }
        unsafe extern "C" {
            fn kill(pid: i32, sig: i32) -> i32;
        }
        let rc = unsafe { kill(pid as i32, 0) };
        rc == 0 || std::io::Error::last_os_error().raw_os_error() == Some(1)
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        let _ = pid;
        true
    }
}

fn is_handle_alive(handle: &ConnectedClientHandle) -> bool {
    !handle.tx.is_closed() && handle.host_pid.map_or(true, is_os_pid_alive)
}

async fn collect_alive_clients() -> Vec<ConnectedIdeClientInfo> {
    let stale_ids: Vec<u64> = {
        let guard = clients_map().read().await;
        let dead: Vec<u64> = guard
            .iter()
            .filter_map(|(&cid, handle)| (!is_handle_alive(handle)).then_some(cid))
            .collect();
        if dead.is_empty() {
            let mut list: Vec<ConnectedIdeClientInfo> =
                guard.values().map(|c| c.info.clone()).collect();
            list.sort_by_key(|c| c.connected_at);
            return list;
        }
        dead
    };

    let mut guard = clients_map().write().await;
    for cid in stale_ids {
        if let Some(old) = guard.remove(&cid) {
            let _ = old.tx.send(WsOutbound::Close);
        }
    }
    guard.retain(|_, handle| is_handle_alive(handle));
    let mut list: Vec<ConnectedIdeClientInfo> = guard.values().map(|c| c.info.clone()).collect();
    list.sort_by_key(|c| c.connected_at);
    list
}

pub async fn has_connected_clients() -> bool {
    !collect_alive_clients().await.is_empty()
}

fn find_antigravity_ide_app_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        let mut candidates: Vec<PathBuf> = Vec::new();
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            let base = PathBuf::from(&local);
            candidates.push(base.join("Programs").join("Antigravity IDE"));
            candidates.push(base.join("Antigravity IDE"));
        }
        if let Ok(pf) = std::env::var("ProgramFiles") {
            candidates.push(PathBuf::from(pf).join("Antigravity IDE"));
        }
        if let Ok(pf86) = std::env::var("ProgramFiles(x86)") {
            candidates.push(PathBuf::from(pf86).join("Antigravity IDE"));
        }
        for root in candidates {
            let app_dir = root.join("resources").join("app");
            if app_dir.join("extensions").is_dir() && app_dir.join("product.json").is_file() {
                return Some(app_dir);
            }
        }
        None
    }

    #[cfg(target_os = "macos")]
    {
        let mut candidates = vec![PathBuf::from(
            "/Applications/Antigravity IDE.app/Contents/Resources/app",
        )];
        if let Some(home) = dirs::home_dir() {
            candidates.push(home.join("Applications/Antigravity IDE.app/Contents/Resources/app"));
        }
        for app_dir in candidates {
            if app_dir.join("extensions").is_dir() && app_dir.join("product.json").is_file() {
                return Some(app_dir);
            }
        }
        None
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let mut candidates = vec![
            PathBuf::from("/usr/share/antigravity-ide/resources/app"),
            PathBuf::from("/opt/Antigravity IDE/resources/app"),
            PathBuf::from("/opt/antigravity-ide/resources/app"),
        ];
        if let Some(home) = dirs::home_dir() {
            candidates.push(home.join(".local/share/antigravity-ide/resources/app"));
        }
        for app_dir in candidates {
            if app_dir.join("extensions").is_dir() && app_dir.join("product.json").is_file() {
                return Some(app_dir);
            }
        }
        None
    }
}

fn builtin_extension_cache_paths() -> Vec<PathBuf> {
    let mut out = Vec::new();
    #[cfg(target_os = "windows")]
    if let Ok(appdata) = std::env::var("APPDATA") {
        out.push(
            PathBuf::from(appdata)
                .join("Antigravity IDE")
                .join("CachedProfilesData")
                .join("__default__profile__")
                .join("extensions.builtin.cache"),
        );
    }
    #[cfg(target_os = "macos")]
    if let Some(home) = dirs::home_dir() {
        out.push(
            home.join("Library")
                .join("Application Support")
                .join("Antigravity IDE")
                .join("CachedProfilesData")
                .join("__default__profile__")
                .join("extensions.builtin.cache"),
        );
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    if let Some(home) = dirs::home_dir() {
        out.push(
            home.join(".config")
                .join("Antigravity IDE")
                .join("CachedProfilesData")
                .join("__default__profile__")
                .join("extensions.builtin.cache"),
        );
    }
    out
}

fn invalidate_builtin_extension_cache() {
    for p in builtin_extension_cache_paths() {
        if p.is_file() {
            let _ = fs::remove_file(p);
        }
    }
}

fn bridge_extension_package_json() -> String {
    serde_json::to_string_pretty(&json!({
        "name": BRIDGE_EXTENSION_ID,
        "displayName": "OnlyDEVS Seamless Auth Bridge",
        "description": "Local WebSocket bridge for seamless Antigravity IDE account switching without restarting.",
        "version": BRIDGE_EXTENSION_VERSION,
        "publisher": "onlydevs",
        "type": "commonjs",
        "engines": {
            "vscode": "^1.80.0"
        },
        "enabledApiProposals": [
            "antigravityUnifiedStateSync"
        ],
        "activationEvents": [
            "onStartupFinished"
        ],
        "main": "./extension.js",
        "contributes": {
            "commands": [
                {
                    "command": "onlydevsBridge.reconnect",
                    "title": "OnlyDEVS Bridge: Reconnect"
                },
                {
                    "command": "onlydevsBridge.status",
                    "title": "OnlyDEVS Bridge: Show Status"
                }
            ]
        }
    }))
    .unwrap_or_default()
}

pub fn bridge_extension_js_source() -> &'static str {
    r#"/* eslint-disable */
"use strict";

const vscode = require("vscode");
const http = require("node:http");
const crypto = require("node:crypto");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");

const EXTENSION_VERSION = "1.0.2";
const RECONNECT_DELAY_MS = 2500;
const HEARTBEAT_INTERVAL_MS = 10000;

let activeSocket = null;
let activeReq = null;
let connectSeq = 0;
let reconnectTimer = null;
let heartbeatTimer = null;
let disposed = false;
let connectedState = false;
let currentPort = null;
let statusBarItem = null;

function getDiscoveryPath() {
  const customHome = (process.env.ANTIGRAVITY_X_HOME || "").trim();
  const baseDir = customHome || path.join(os.homedir(), ".antigravity-x");
  return path.join(baseDir, "ide-bridge.json");
}

function readDiscovery() {
  try {
    const raw = fs.readFileSync(getDiscoveryPath(), "utf8");
    const parsed = JSON.parse(raw);
    if (parsed && typeof parsed.port === "number" && typeof parsed.token === "string") {
      return parsed;
    }
  } catch {
    // Ignore when OnlyDEVS app hasn't written discovery yet
  }
  return null;
}

function updateStatusBar() {
  if (!statusBarItem) return;
  if (connectedState) {
    statusBarItem.text = "$(zap) OnlyDEVS";
    statusBarItem.tooltip = `OnlyDEVS Seamless Bridge đã kết nối (ws://127.0.0.1:${currentPort || "?"})`;
  } else {
    statusBarItem.text = "$(debug-disconnect) OnlyDEVS";
    statusBarItem.tooltip = "Đang chờ OnlyDEVS-Antigravity (bấm để kết nối lại)";
  }
}

function buildWsFrame(opcode, payloadBuf) {
  const len = payloadBuf.length;
  const maskKey = crypto.randomBytes(4);
  let headerLen = 2;
  if (len >= 126 && len <= 65535) {
    headerLen = 4;
  } else if (len > 65535) {
    headerLen = 10;
  }
  const out = Buffer.allocUnsafe(headerLen + 4 + len);
  out[0] = 0x80 | (opcode & 0x0f);
  let offset = 2;
  if (len < 126) {
    out[1] = 0x80 | len;
  } else if (len <= 65535) {
    out[1] = 0x80 | 126;
    out.writeUInt16BE(len, 2);
    offset = 4;
  } else {
    out[1] = 0x80 | 127;
    out.writeBigUInt64BE(BigInt(len), 2);
    offset = 10;
  }
  maskKey.copy(out, offset);
  offset += 4;
  for (let i = 0; i < len; i++) {
    out[offset + i] = payloadBuf[i] ^ maskKey[i % 4];
  }
  return out;
}

function sendJson(obj) {
  if (!activeSocket || activeSocket.destroyed) return false;
  try {
    const payload = Buffer.from(JSON.stringify(obj), "utf8");
    activeSocket.write(buildWsFrame(0x1, payload));
    return true;
  } catch {
    return false;
  }
}

function sendPong(dataBuf) {
  if (!activeSocket || activeSocket.destroyed) return;
  try {
    activeSocket.write(buildWsFrame(0x0a, dataBuf || Buffer.alloc(0)));
  } catch {
    // ignore
  }
}

function sendPing() {
  if (!activeSocket || activeSocket.destroyed) return;
  try {
    activeSocket.write(buildWsFrame(0x09, Buffer.from("ping")));
  } catch {
    // ignore
  }
}

async function handleSwitchAccount(msg) {
  const requestId = String(msg.requestId || "");
  const targetEmail = String(msg.email || "");
  try {
    const syncApi = vscode.antigravityUnifiedStateSync;
    if (!syncApi || !syncApi.OAuthPreferences || typeof syncApi.OAuthPreferences.setOAuthTokenInfo !== "function") {
      throw new Error("Antigravity IDE không khả dụng API OAuthPreferences.setOAuthTokenInfo");
    }

    const tokenPayload = {
      accessToken: String(msg.accessToken || ""),
      refreshToken: String(msg.refreshToken || ""),
      expiryDateSeconds: Number(msg.expiryDateSeconds || 0),
      tokenType: String(msg.tokenType || "Bearer"),
      isGcpTos: Boolean(msg.isGcpTos),
    };

    if (!tokenPayload.accessToken || !tokenPayload.refreshToken) {
      throw new Error("Thiếu accessToken hoặc refreshToken trong payload chuyển đổi");
    }

    await syncApi.OAuthPreferences.setOAuthTokenInfo(tokenPayload);

    // Allow Electron's uss-oauth sync/subscriber to flush before restarting Extension Host
    // so the spawned agy.exe agent backend reloads the newly written credentials cleanly
    // without closing or reloading the Antigravity IDE window.
    await new Promise((resolve) => setTimeout(resolve, 150));

    // Best-effort refresh status message without disturbing the active editor/chat panel
    vscode.window.setStatusBarMessage(
      `$(zap) OnlyDEVS: Đã chuyển nóng sang ${targetEmail} — đang làm mới agent...`,
      5000,
    );

    sendJson({
      type: "switch_result",
      requestId,
      success: true,
      email: targetEmail,
      effectiveMode: "seamless",
      message: `Đã nạp OAuth token mới & làm mới Extension Host cho ${targetEmail}`,
    });

    if (vscode.commands && typeof vscode.commands.executeCommand === "function") {
      const restartTimer = setTimeout(() => {
        void Promise.resolve(
          vscode.commands.executeCommand("workbench.action.restartExtensionHost"),
        ).catch(() => {});
      }, 120);
      if (typeof restartTimer.unref === "function") {
        restartTimer.unref();
      }
    }
  } catch (err) {
    sendJson({
      type: "switch_result",
      requestId,
      success: false,
      email: targetEmail,
      effectiveMode: "error",
      message: err && err.message ? String(err.message) : String(err),
    });
  }
}

function handleIncomingMessage(text) {
  let msg;
  try {
    msg = JSON.parse(text);
  } catch {
    return;
  }
  if (!msg || typeof msg !== "object") return;
  if (msg.type === "hello_ack") {
    connectedState = true;
    updateStatusBar();
    return;
  }
  if (msg.type === "switch_account") {
    void handleSwitchAccount(msg);
  }
}

function cleanupSocket() {
  if (heartbeatTimer) {
    clearInterval(heartbeatTimer);
    heartbeatTimer = null;
  }
  if (activeReq) {
    try {
      activeReq.destroy();
    } catch {
      // ignore
    }
    activeReq = null;
  }
  if (activeSocket) {
    try {
      activeSocket.destroy();
    } catch {
      // ignore
    }
    activeSocket = null;
  }
  connectedState = false;
  updateStatusBar();
}

function scheduleReconnect() {
  if (disposed || reconnectTimer) return;
  reconnectTimer = setTimeout(() => {
    reconnectTimer = null;
    connectToBridge();
  }, RECONNECT_DELAY_MS);
}

function connectToBridge() {
  if (disposed) return;
  if (reconnectTimer) {
    clearTimeout(reconnectTimer);
    reconnectTimer = null;
  }
  const mySeq = ++connectSeq;
  cleanupSocket();

  const discovery = readDiscovery();
  if (!discovery) {
    scheduleReconnect();
    return;
  }

  currentPort = discovery.port;
  const wsKey = crypto.randomBytes(16).toString("base64");
  const expectedAccept = crypto
    .createHash("sha1")
    .update(wsKey + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11")
    .digest("base64");

  const req = http.request({
    hostname: "127.0.0.1",
    port: discovery.port,
    path: "/",
    method: "GET",
    headers: {
      Connection: "Upgrade",
      Upgrade: "websocket",
      "Sec-WebSocket-Key": wsKey,
      "Sec-WebSocket-Version": "13",
    },
  });
  activeReq = req;

  req.on("upgrade", (res, socket, head) => {
    if (activeReq === req) {
      activeReq = null;
    }
    if (disposed || mySeq !== connectSeq) {
      try {
        socket.destroy();
      } catch {
        // ignore
      }
      return;
    }

    const acceptHeader = res.headers["sec-websocket-accept"];
    if (acceptHeader !== expectedAccept) {
      socket.destroy();
      scheduleReconnect();
      return;
    }

    if (activeSocket && activeSocket !== socket) {
      try {
        activeSocket.destroy();
      } catch {
        // ignore
      }
    }
    activeSocket = socket;
    let rxBuf = head && head.length ? Buffer.from(head) : Buffer.alloc(0);
    let fragmentedChunks = [];

    const drainFrames = () => {
      while (rxBuf.length >= 2) {
        const b0 = rxBuf[0];
        const b1 = rxBuf[1];
        const fin = (b0 & 0x80) !== 0;
        const opcode = b0 & 0x0f;
        const masked = (b1 & 0x80) !== 0;
        let payloadLen = b1 & 0x7f;
        let offset = 2;

        if (payloadLen === 126) {
          if (rxBuf.length < 4) return;
          payloadLen = rxBuf.readUInt16BE(2);
          offset = 4;
        } else if (payloadLen === 127) {
          if (rxBuf.length < 10) return;
          const bigLen = rxBuf.readBigUInt64BE(2);
          if (bigLen > BigInt(4 * 1024 * 1024)) {
            socket.destroy();
            return;
          }
          payloadLen = Number(bigLen);
          offset = 10;
        }

        const maskBytes = masked ? 4 : 0;
        if (rxBuf.length < offset + maskBytes + payloadLen) return;

        let payload = rxBuf.subarray(offset + maskBytes, offset + maskBytes + payloadLen);
        if (masked) {
          const mask = rxBuf.subarray(offset, offset + 4);
          const unmasked = Buffer.allocUnsafe(payloadLen);
          for (let i = 0; i < payloadLen; i++) {
            unmasked[i] = payload[i] ^ mask[i % 4];
          }
          payload = unmasked;
        } else {
          payload = Buffer.from(payload);
        }

        rxBuf = rxBuf.subarray(offset + maskBytes + payloadLen);

        if (opcode === 0x8) {
          if (activeSocket === socket) {
            cleanupSocket();
            scheduleReconnect();
          }
          return;
        }
        if (opcode === 0x9) {
          sendPong(payload);
          continue;
        }
        if (opcode === 0x0a) {
          continue;
        }
        if (opcode === 0x1 || opcode === 0x0) {
          fragmentedChunks.push(payload);
          if (fin) {
            const fullText = Buffer.concat(fragmentedChunks).toString("utf8");
            fragmentedChunks = [];
            handleIncomingMessage(fullText);
          }
        }
      }
    };

    socket.on("data", (chunk) => {
      rxBuf = Buffer.concat([rxBuf, chunk]);
      drainFrames();
    });

    socket.on("close", () => {
      if (activeSocket === socket) {
        cleanupSocket();
        scheduleReconnect();
      }
    });

    socket.on("error", () => {
      if (activeSocket === socket) {
        cleanupSocket();
        scheduleReconnect();
      }
    });

    const sessionId =
      vscode.env && vscode.env.sessionId ? String(vscode.env.sessionId).trim() : "";

    sendJson({
      type: "hello",
      token: discovery.token,
      extensionVersion: EXTENSION_VERSION,
      vscodeVersion: String(vscode.version || ""),
      ideName: String(vscode.env.appName || "Antigravity IDE"),
      sessionId,
      hostPid: Number(process.pid || 0),
    });

    heartbeatTimer = setInterval(() => {
      sendPing();
    }, HEARTBEAT_INTERVAL_MS);

    drainFrames();
  });

  req.on("response", () => {
    if (activeReq === req) {
      activeReq = null;
    }
    if (mySeq === connectSeq) {
      scheduleReconnect();
    }
  });

  req.on("error", () => {
    if (activeReq === req) {
      activeReq = null;
    }
    if (mySeq === connectSeq) {
      scheduleReconnect();
    }
  });

  req.setTimeout(5000, () => {
    req.destroy();
  });

  req.end();
}

function activate(context) {
  disposed = false;
  statusBarItem = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Right, 50);
  statusBarItem.command = "onlydevsBridge.status";
  statusBarItem.show();
  updateStatusBar();

  const onProcessSignal = () => {
    deactivate();
  };
  process.once("beforeExit", onProcessSignal);
  process.once("SIGTERM", onProcessSignal);
  process.once("SIGINT", onProcessSignal);

  context.subscriptions.push(
    statusBarItem,
    {
      dispose: () => {
        process.removeListener("beforeExit", onProcessSignal);
        process.removeListener("SIGTERM", onProcessSignal);
        process.removeListener("SIGINT", onProcessSignal);
        deactivate();
      },
    },
    vscode.commands.registerCommand("onlydevsBridge.reconnect", () => {
      connectToBridge();
      vscode.window.setStatusBarMessage("OnlyDEVS Bridge: Đang kết nối lại...", 2500);
    }),
    vscode.commands.registerCommand("onlydevsBridge.status", async () => {
      const msg = connectedState
        ? `OnlyDEVS Seamless Bridge đang kết nối (port ${currentPort}).`
        : "OnlyDEVS Seamless Bridge chưa kết nối tới app OnlyDEVS-Antigravity.";
      const action = await vscode.window.showInformationMessage(msg, "Kết nối lại");
      if (action === "Kết nối lại") {
        connectToBridge();
      }
    }),
  );

  connectToBridge();
}

function deactivate() {
  disposed = true;
  connectSeq++;
  if (reconnectTimer) {
    clearTimeout(reconnectTimer);
    reconnectTimer = null;
  }
  if (activeSocket && !activeSocket.destroyed) {
    try {
      activeSocket.write(buildWsFrame(0x8, Buffer.alloc(0)));
    } catch {
      // ignore
    }
  }
  cleanupSocket();
}

module.exports = {
  activate,
  deactivate,
};
"#
}

fn inspect_extension_in_dir(ext_dir: &Path) -> (bool, Option<String>, bool) {
    let pkg_path = ext_dir.join("package.json");
    let js_path = ext_dir.join("extension.js");
    if !pkg_path.is_file() || !js_path.is_file() {
        return (false, None, false);
    }
    let version = fs::read_to_string(&pkg_path)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| {
            v.get("version")
                .and_then(|x| x.as_str())
                .map(str::to_string)
        });
    let current_js = fs::read_to_string(&js_path).unwrap_or_default();
    let needs_update = version.as_deref() != Some(BRIDGE_EXTENSION_VERSION)
        || current_js != bridge_extension_js_source();
    (true, version, needs_update)
}

fn inject_extension_into_dir(ext_dir: &Path) -> Result<(), String> {
    fs::create_dir_all(ext_dir)
        .map_err(|e| format!("Không thể tạo thư mục extension {}: {e}", ext_dir.display()))?;
    fs::write(ext_dir.join("package.json"), bridge_extension_package_json())
        .map_err(|e| format!("Ghi package.json thất bại: {e}"))?;
    fs::write(ext_dir.join("extension.js"), bridge_extension_js_source())
        .map_err(|e| format!("Ghi extension.js thất bại: {e}"))?;
    Ok(())
}

pub async fn inject_extension() -> Result<IdeBridgeStatus, String> {
    let app_dir = find_antigravity_ide_app_dir()
        .ok_or_else(|| "Không tìm thấy thư mục cài đặt Antigravity IDE trên máy.".to_string())?;
    let ext_dir = app_dir.join("extensions").join(BRIDGE_EXTENSION_ID);
    inject_extension_into_dir(&ext_dir)?;
    invalidate_builtin_extension_cache();

    let mut cfg = load_settings();
    cfg.seamless_auto_switch = true;
    cfg.auto_heal_inject = true;
    cfg.seamless_priority = true;
    let _ = save_settings(cfg);

    let status = get_status().await;
    emit_status_changed();
    Ok(status)
}

pub async fn remove_extension() -> Result<IdeBridgeStatus, String> {
    if let Some(app_dir) = find_antigravity_ide_app_dir() {
        let ext_dir = app_dir.join("extensions").join(BRIDGE_EXTENSION_ID);
        if ext_dir.exists() {
            fs::remove_dir_all(&ext_dir)
                .map_err(|e| format!("Gỡ extension thất bại: {e}"))?;
            invalidate_builtin_extension_cache();
        }
    }

    let mut cfg = load_settings();
    cfg.seamless_auto_switch = false;
    cfg.auto_heal_inject = false;
    let _ = write_settings_only(&cfg);

    let clients: Vec<mpsc::UnboundedSender<WsOutbound>> = {
        let guard = clients_map().read().await;
        guard.values().map(|c| c.tx.clone()).collect()
    };
    for tx in clients {
        let _ = tx.send(WsOutbound::Close);
    }

    let status = get_status().await;
    emit_status_changed();
    Ok(status)
}

pub fn auto_heal_if_enabled() {
    let cfg = load_settings();
    if !cfg.auto_heal_inject {
        return;
    }
    if let Some(app_dir) = find_antigravity_ide_app_dir() {
        let ext_dir = app_dir.join("extensions").join(BRIDGE_EXTENSION_ID);
        let (injected, _ver, needs_update) = inspect_extension_in_dir(&ext_dir);
        if !injected || needs_update {
            if inject_extension_into_dir(&ext_dir).is_ok() {
                invalidate_builtin_extension_cache();
            }
        }
    }
}

pub async fn get_status() -> IdeBridgeStatus {
    let ws_port = *server_port_lock().read().await;
    let ide_app_dir = find_antigravity_ide_app_dir();
    let (ide_found, ide_root_path, extension_dir, extension_injected, extension_version, needs_update) =
        if let Some(app_dir) = ide_app_dir {
            let ext_dir = app_dir.join("extensions").join(BRIDGE_EXTENSION_ID);
            let (injected, ver, needs_upd) = inspect_extension_in_dir(&ext_dir);
            (
                true,
                Some(app_dir.to_string_lossy().to_string()),
                Some(ext_dir.to_string_lossy().to_string()),
                injected,
                ver,
                needs_upd,
            )
        } else {
            (false, None, None, false, None, false)
        };

    let connected_clients = collect_alive_clients().await;

    IdeBridgeStatus {
        server_running: ws_port.is_some(),
        ws_port,
        ide_found,
        ide_root_path,
        extension_dir,
        extension_injected,
        extension_version,
        needs_update,
        connected_count: connected_clients.len(),
        connected_clients,
        settings: load_settings(),
    }
}

pub async fn request_seamless_switch(
    payload: SeamlessSwitchPayload,
    timeout_ms: u64,
) -> Result<SeamlessSwitchReply, String> {
    let _ = collect_alive_clients().await;
    let txs: Vec<(u64, mpsc::UnboundedSender<WsOutbound>)> = {
        let guard = clients_map().read().await;
        guard.iter().map(|(&cid, c)| (cid, c.tx.clone())).collect()
    };
    if txs.is_empty() {
        return Err("Extension IDE chưa kết nối qua WebSocket.".into());
    }

    let request_id = format!("sw_{}", Uuid::new_v4().simple());
    let (tx_reply, rx_reply) = oneshot::channel::<SeamlessSwitchReply>();
    {
        let mut pending = pending_map().lock().await;
        pending.insert(request_id.clone(), tx_reply);
    }

    let msg = json!({
        "type": "switch_account",
        "requestId": request_id,
        "email": payload.email,
        "accessToken": payload.access_token,
        "refreshToken": payload.refresh_token,
        "expiryDateSeconds": payload.expiry_date_seconds,
        "tokenType": "Bearer",
        "isGcpTos": payload.is_gcp_tos,
    });
    let text = serde_json::to_string(&msg).map_err(|e| format!("Serialize switch msg: {e}"))?;

    let mut sent = 0usize;
    let mut failed_cids = Vec::new();
    for (cid, client_tx) in txs {
        if client_tx.send(WsOutbound::Text(text.clone())).is_ok() {
            sent += 1;
        } else {
            failed_cids.push(cid);
        }
    }
    if !failed_cids.is_empty() {
        let mut map = clients_map().write().await;
        for cid in failed_cids {
            map.remove(&cid);
        }
        drop(map);
        emit_status_changed();
    }
    if sent == 0 {
        let mut pending = pending_map().lock().await;
        pending.remove(&request_id);
        return Err("Không gửi được lệnh sang Antigravity IDE.".into());
    }

    match tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), rx_reply).await {
        Ok(Ok(reply)) => {
            if reply.success {
                Ok(reply)
            } else {
                Err(reply
                    .message
                    .unwrap_or_else(|| "IDE Extension trả về lỗi khi nạp token.".into()))
            }
        }
        Ok(Err(_)) => Err("Kênh chờ phản hồi từ IDE Extension bị đóng.".into()),
        Err(_) => {
            let mut pending = pending_map().lock().await;
            pending.remove(&request_id);
            Err("Hết thời gian chờ phản hồi từ Antigravity IDE Extension.".into())
        }
    }
}

fn write_discovery_file(port: u16, token: &str) -> Result<(), String> {
    let path = discovery_file_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Tạo thư mục discovery: {e}"))?;
    }
    let body = serde_json::to_string_pretty(&BridgeDiscoveryFile {
        port,
        token: token.to_string(),
        pid: std::process::id(),
        version: BRIDGE_EXTENSION_VERSION.to_string(),
        updated_at: Utc::now().timestamp(),
    })
    .map_err(|e| format!("Serialize discovery: {e}"))?;
    fs::write(&path, body).map_err(|e| format!("Ghi discovery: {e}"))?;
    Ok(())
}

pub fn cleanup_discovery_file_sync() {
    if let Ok(path) = discovery_file_path() {
        let _ = fs::remove_file(path);
    }
}

pub fn spawn_bridge_server(app: AppHandle) {
    if let Ok(mut guard) = app_handle_slot().lock() {
        *guard = Some(app);
    }
    if SERVER_STARTED.swap(true, Ordering::SeqCst) {
        return;
    }

    tauri::async_runtime::spawn(async move {
        auto_heal_if_enabled();

        let mut bound_listener: Option<(TcpListener, u16)> = None;
        for port in BRIDGE_PORT_START..=BRIDGE_PORT_END {
            if let Ok(listener) = TcpListener::bind(("127.0.0.1", port)).await {
                bound_listener = Some((listener, port));
                break;
            }
        }
        let Some((listener, port)) = bound_listener else {
            return;
        };

        {
            let mut guard = server_port_lock().write().await;
            *guard = Some(port);
        }
        let token = auth_token().to_string();
        let _ = write_discovery_file(port, &token);
        emit_status_changed();

        loop {
            let Ok((stream, _addr)) = listener.accept().await else {
                continue;
            };
            let expected_token = token.clone();
            tauri::async_runtime::spawn(async move {
                let _ = handle_ws_client(stream, expected_token).await;
            });
        }
    });
}

async fn handle_ws_client(mut stream: TcpStream, expected_token: String) -> Result<(), String> {
    let mut header_buf = Vec::with_capacity(2048);
    let mut byte_buf = [0u8; 1024];
    let header_end = loop {
        let n = stream
            .read(&mut byte_buf)
            .await
            .map_err(|e| format!("WS handshake read: {e}"))?;
        if n == 0 {
            return Err("EOF during handshake".into());
        }
        header_buf.extend_from_slice(&byte_buf[..n]);
        if let Some(pos) = find_double_crlf(&header_buf) {
            break pos;
        }
        if header_buf.len() > 16 * 1024 {
            let _ = stream.write_all(b"HTTP/1.1 431 Header Too Large\r\n\r\n").await;
            return Err("Handshake header too large".into());
        }
    };

    let header_str = String::from_utf8_lossy(&header_buf[..header_end]);
    let ws_key = extract_ws_key(&header_str).ok_or_else(|| {
        let _ = futures_lite_ignore();
        "Missing Sec-WebSocket-Key".to_string()
    })?;
    let accept_val = compute_ws_accept(&ws_key);

    let resp = format!(
        "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept_val}\r\n\r\n"
    );
    stream
        .write_all(resp.as_bytes())
        .await
        .map_err(|e| format!("WS handshake write: {e}"))?;

    let leftover = header_buf[header_end + 4..].to_vec();
    let (mut reader, mut writer) = tokio::io::split(stream);
    let (tx, mut rx) = mpsc::unbounded_channel::<WsOutbound>();

    let writer_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            let frame = match msg {
                WsOutbound::Text(s) => encode_ws_frame(0x1, s.as_bytes()),
                WsOutbound::Ping(p) => encode_ws_frame(0x9, &p),
                WsOutbound::Pong(p) => encode_ws_frame(0xA, &p),
                WsOutbound::Close => {
                    let _ = writer.write_all(&encode_ws_frame(0x8, &[])).await;
                    break;
                }
            };
            if writer.write_all(&frame).await.is_err() {
                break;
            }
        }
    });

    let mut rx_buf = leftover;
    let mut registered_client_id: Option<u64> = None;
    let mut fragmented = Vec::new();
    let mut awaiting_pong_since: Option<tokio::time::Instant> = None;

    let read_result: Result<(), String> = async {
        'read_loop: loop {
            while let Some(frame) = try_decode_ws_frame(&mut rx_buf)? {
                awaiting_pong_since = None;
                match frame.opcode {
                    0x8 => break 'read_loop,
                    0x9 => {
                        if tx.send(WsOutbound::Pong(frame.payload)).is_err() {
                            break 'read_loop;
                        }
                    }
                    0xA => {}
                    0x0 | 0x1 => {
                        fragmented.extend_from_slice(&frame.payload);
                        if frame.fin {
                            let text = String::from_utf8_lossy(&fragmented).to_string();
                            fragmented.clear();
                            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&text) {
                                let msg_type = val.get("type").and_then(|v| v.as_str()).unwrap_or("");
                                if msg_type == "hello" {
                                    let client_tok =
                                        val.get("token").and_then(|v| v.as_str()).unwrap_or("");
                                    if client_tok != expected_token {
                                        let _ = tx.send(WsOutbound::Close);
                                        break 'read_loop;
                                    }
                                    let cid = registered_client_id.unwrap_or_else(|| {
                                        CLIENT_SEQ.fetch_add(1, Ordering::SeqCst)
                                    });
                                    registered_client_id = Some(cid);
                                    let session_id = val
                                        .get("sessionId")
                                        .and_then(|v| v.as_str())
                                        .map(str::trim)
                                        .filter(|s| !s.is_empty())
                                        .map(str::to_string);
                                    let host_pid = val
                                        .get("hostPid")
                                        .and_then(|v| v.as_u64())
                                        .filter(|&p| p > 0 && p <= u32::MAX as u64)
                                        .map(|p| p as u32);
                                    let info = ConnectedIdeClientInfo {
                                        client_id: cid,
                                        extension_version: val
                                            .get("extensionVersion")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("1.0.0")
                                            .to_string(),
                                        vscode_version: val
                                            .get("vscodeVersion")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("")
                                            .to_string(),
                                        ide_name: val
                                            .get("ideName")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("Antigravity IDE")
                                            .to_string(),
                                        connected_at: Utc::now().timestamp(),
                                    };
                                    let evicted_txs = {
                                        let mut map = clients_map().write().await;
                                        let mut evicted = Vec::new();
                                        let stale_cids: Vec<u64> = map
                                            .iter()
                                            .filter_map(|(&existing_cid, existing)| {
                                                if existing_cid == cid {
                                                    return None;
                                                }
                                                let dead = !is_handle_alive(existing);
                                                let same_session = session_id.is_some()
                                                    && existing.session_id == session_id;
                                                let same_pid = host_pid.is_some()
                                                    && existing.host_pid == host_pid;
                                                (dead || same_session || same_pid)
                                                    .then_some(existing_cid)
                                            })
                                            .collect();
                                        for stale_cid in stale_cids {
                                            if let Some(old) = map.remove(&stale_cid) {
                                                evicted.push(old.tx);
                                            }
                                        }
                                        map.insert(
                                            cid,
                                            ConnectedClientHandle {
                                                info,
                                                tx: tx.clone(),
                                                session_id,
                                                host_pid,
                                            },
                                        );
                                        evicted
                                    };
                                    for old_tx in evicted_txs {
                                        let _ = old_tx.send(WsOutbound::Close);
                                    }
                                    let ack = json!({
                                        "type": "hello_ack",
                                        "bridgeVersion": BRIDGE_EXTENSION_VERSION,
                                    })
                                    .to_string();
                                    let _ = tx.send(WsOutbound::Text(ack));
                                    emit_status_changed();
                                } else if msg_type == "switch_result"
                                    && registered_client_id.is_some()
                                {
                                    if let Some(req_id) =
                                        val.get("requestId").and_then(|v| v.as_str())
                                    {
                                        let reply = SeamlessSwitchReply {
                                            success: val
                                                .get("success")
                                                .and_then(|v| v.as_bool())
                                                .unwrap_or(false),
                                            email: val
                                                .get("email")
                                                .and_then(|v| v.as_str())
                                                .map(str::to_string),
                                            effective_mode: val
                                                .get("effectiveMode")
                                                .and_then(|v| v.as_str())
                                                .map(str::to_string),
                                            message: val
                                                .get("message")
                                                .and_then(|v| v.as_str())
                                                .map(str::to_string),
                                        };
                                        let sender = {
                                            let mut pending = pending_map().lock().await;
                                            pending.remove(req_id)
                                        };
                                        if let Some(ch) = sender {
                                            let _ = ch.send(reply);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }

            let wait_secs = if registered_client_id.is_none() {
                WS_HELLO_TIMEOUT_SECS
            } else if awaiting_pong_since.is_some() {
                WS_PONG_DEAD_SECS
            } else {
                WS_IDLE_PING_SECS
            };

            let mut chunk = [0u8; 4096];
            match tokio::time::timeout(
                std::time::Duration::from_secs(wait_secs),
                reader.read(&mut chunk),
            )
            .await
            {
                Ok(Ok(0)) => break 'read_loop,
                Ok(Ok(n)) => {
                    rx_buf.extend_from_slice(&chunk[..n]);
                    if rx_buf.len() > MAX_WS_FRAME_LEN + 64 {
                        break 'read_loop;
                    }
                }
                Ok(Err(e)) => {
                    // ConnectionReset / ConnectionAborted must still clean up `registered_client_id` below.
                    return Err(format!("WS read: {e}"));
                }
                Err(_) => {
                    if registered_client_id.is_none() {
                        break 'read_loop;
                    }
                    if let Some(ping_at) = awaiting_pong_since {
                        if ping_at.elapsed() >= std::time::Duration::from_secs(WS_PONG_DEAD_SECS)
                        {
                            break 'read_loop;
                        }
                    } else {
                        if tx.send(WsOutbound::Ping(b"hb".to_vec())).is_err() {
                            break 'read_loop;
                        }
                        awaiting_pong_since = Some(tokio::time::Instant::now());
                    }
                }
            }
        }
        Ok(())
    }
    .await;

    // Unconditional cleanup: runs whether loop ended cleanly, timed out, errored (WSAECONNRESET),
    // or failed frame decoding — preventing stale client ghosts from ever accumulating.
    if let Some(cid) = registered_client_id {
        let removed = {
            let mut map = clients_map().write().await;
            map.remove(&cid).is_some()
        };
        if removed {
            emit_status_changed();
        }
    }
    let _ = tx.send(WsOutbound::Close);
    if tokio::time::timeout(std::time::Duration::from_millis(250), writer_task)
        .await
        .is_err()
    {
        // Writer timed out on dead socket; dropping future cleans up.
    }
    read_result
}

fn futures_lite_ignore() {}

fn find_double_crlf(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

fn extract_ws_key(headers: &str) -> Option<String> {
    for line in headers.lines() {
        if let Some((name, val)) = line.split_once(':') {
            if name.trim().eq_ignore_ascii_case("sec-websocket-key") {
                let k = val.trim();
                if !k.is_empty() {
                    return Some(k.to_string());
                }
            }
        }
    }
    None
}

struct DecodedWsFrame {
    fin: bool,
    opcode: u8,
    payload: Vec<u8>,
}

fn encode_ws_frame(opcode: u8, payload: &[u8]) -> Vec<u8> {
    let len = payload.len();
    let mut out = Vec::with_capacity(len + 10);
    out.push(0x80 | (opcode & 0x0f));
    if len < 126 {
        out.push(len as u8);
    } else if len <= 65535 {
        out.push(126);
        out.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        out.push(127);
        out.extend_from_slice(&(len as u64).to_be_bytes());
    }
    out.extend_from_slice(payload);
    out
}

fn try_decode_ws_frame(buf: &mut Vec<u8>) -> Result<Option<DecodedWsFrame>, String> {
    if buf.len() < 2 {
        return Ok(None);
    }
    let b0 = buf[0];
    let b1 = buf[1];
    let fin = (b0 & 0x80) != 0;
    let opcode = b0 & 0x0f;
    let masked = (b1 & 0x80) != 0;
    let mut len = (b1 & 0x7f) as usize;
    let mut offset = 2usize;

    if len == 126 {
        if buf.len() < 4 {
            return Ok(None);
        }
        len = u16::from_be_bytes([buf[2], buf[3]]) as usize;
        offset = 4;
    } else if len == 127 {
        if buf.len() < 10 {
            return Ok(None);
        }
        let raw_len = u64::from_be_bytes([
            buf[2], buf[3], buf[4], buf[5], buf[6], buf[7], buf[8], buf[9],
        ]);
        if raw_len > MAX_WS_FRAME_LEN as u64 {
            return Err("WebSocket frame exceeds limit".into());
        }
        len = raw_len as usize;
        offset = 10;
    }

    let mask_len = if masked { 4 } else { 0 };
    if buf.len() < offset + mask_len + len {
        return Ok(None);
    }

    let mut payload = buf[offset + mask_len..offset + mask_len + len].to_vec();
    if masked {
        let mask = [
            buf[offset],
            buf[offset + 1],
            buf[offset + 2],
            buf[offset + 3],
        ];
        for (i, b) in payload.iter_mut().enumerate() {
            *b ^= mask[i % 4];
        }
    }
    buf.drain(..offset + mask_len + len);

    Ok(Some(DecodedWsFrame {
        fin,
        opcode,
        payload,
    }))
}

pub fn compute_ws_accept(client_key: &str) -> String {
    let mut input = client_key.as_bytes().to_vec();
    input.extend_from_slice(WS_GUID.as_bytes());
    let digest = sha1_digest(&input);
    BASE64.encode(digest)
}

/// Pure RFC 3174 SHA-1 for RFC 6455 Sec-WebSocket-Accept handshake.
fn sha1_digest(data: &[u8]) -> [u8; 20] {
    let mut h0: u32 = 0x6745_2301;
    let mut h1: u32 = 0xEFCD_AB89;
    let mut h2: u32 = 0x98BA_DCFE;
    let mut h3: u32 = 0x1032_5476;
    let mut h4: u32 = 0xC3D2_E1F0;

    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut msg = data.to_vec();
    msg.push(0x80);
    while (msg.len() % 64) != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.chunks_exact(64) {
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
        for (i, &wi) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A82_7999),
                20..=39 => (b ^ c ^ d, 0x6ED9_EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC),
                _ => (b ^ c ^ d, 0xCA62_C1D6),
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(wi);
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn rfc6455_websocket_accept_vector() {
        assert_eq!(
            compute_ws_accept("dGhlIHNhbXBsZSBub25jZQ=="),
            "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
        );
    }

    #[test]
    fn websocket_frame_roundtrip_masked_and_unmasked() {
        let raw = br#"{"type":"hello","token":"abc"}"#;
        let mut frame = encode_ws_frame(0x1, raw);
        let decoded = try_decode_ws_frame(&mut frame).unwrap().unwrap();
        assert!(decoded.fin);
        assert_eq!(decoded.opcode, 0x1);
        assert_eq!(decoded.payload, raw);
        assert!(frame.is_empty());

        // Masked client frame
        let mask = [0x12u8, 0x34, 0x56, 0x78];
        let mut masked_frame = vec![0x81u8, 0x80 | (raw.len() as u8)];
        masked_frame.extend_from_slice(&mask);
        for (i, b) in raw.iter().enumerate() {
            masked_frame.push(*b ^ mask[i % 4]);
        }
        let decoded_masked = try_decode_ws_frame(&mut masked_frame).unwrap().unwrap();
        assert_eq!(decoded_masked.payload, raw);
    }

    #[test]
    fn inject_extension_files_and_detect_up_to_date() {
        let dir = TempDir::new().unwrap();
        let ext_dir = dir.path().join(BRIDGE_EXTENSION_ID);
        let (found, _, _) = inspect_extension_in_dir(&ext_dir);
        assert!(!found);

        inject_extension_into_dir(&ext_dir).unwrap();
        let (found2, ver2, needs_upd) = inspect_extension_in_dir(&ext_dir);
        assert!(found2);
        assert_eq!(ver2.as_deref(), Some(BRIDGE_EXTENSION_VERSION));
        assert!(!needs_upd);

        let js = fs::read_to_string(ext_dir.join("extension.js")).unwrap();
        assert!(js.contains("workbench.action.restartExtensionHost"));
    }

    fn encode_masked_client_text(text: &str) -> Vec<u8> {
        let payload = text.as_bytes();
        let mask = [0x41u8, 0x42, 0x43, 0x44];
        let mut out = Vec::with_capacity(payload.len() + 10);
        out.push(0x81);
        if payload.len() < 126 {
            out.push(0x80 | (payload.len() as u8));
        } else {
            out.push(0x80 | 126);
            out.extend_from_slice(&(payload.len() as u16).to_be_bytes());
        }
        out.extend_from_slice(&mask);
        for (i, b) in payload.iter().enumerate() {
            out.push(*b ^ mask[i % 4]);
        }
        out
    }

    static TEST_CLIENTS_LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> =
        std::sync::OnceLock::new();

    fn test_clients_lock() -> &'static tokio::sync::Mutex<()> {
        TEST_CLIENTS_LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
    }

    async fn open_test_ws_client(
        port: u16,
        token: &str,
        session_id: Option<&str>,
    ) -> TcpStream {
        let mut client = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        let key = "dGhlIHNhbXBsZSBub25jZQ==";
        let upgrade_req = format!(
            "GET / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n"
        );
        client.write_all(upgrade_req.as_bytes()).await.unwrap();

        let mut rx = Vec::new();
        let mut tmp = [0u8; 1024];
        let hdr_end = loop {
            let n = client.read(&mut tmp).await.unwrap();
            assert!(n > 0);
            rx.extend_from_slice(&tmp[..n]);
            if let Some(pos) = find_double_crlf(&rx) {
                break pos;
            }
        };
        let hdr = String::from_utf8_lossy(&rx[..hdr_end]);
        assert!(hdr.contains("101 Switching Protocols"));
        rx.drain(..hdr_end + 4);

        let hello = json!({
            "type": "hello",
            "token": token,
            "extensionVersion": BRIDGE_EXTENSION_VERSION,
            "vscodeVersion": "1.107.0",
            "ideName": "Antigravity IDE",
            "sessionId": session_id.unwrap_or(""),
        })
        .to_string();
        client
            .write_all(&encode_masked_client_text(&hello))
            .await
            .unwrap();

        let ack_frame = loop {
            if let Some(f) = try_decode_ws_frame(&mut rx).unwrap() {
                break f;
            }
            let n = client.read(&mut tmp).await.unwrap();
            assert!(n > 0);
            rx.extend_from_slice(&tmp[..n]);
        };
        let ack_json: serde_json::Value = serde_json::from_slice(&ack_frame.payload).unwrap();
        assert_eq!(ack_json["type"], "hello_ack");
        client
    }

    #[tokio::test]
    async fn local_websocket_hello_and_switch_roundtrip() {
        let _guard = test_clients_lock().lock().await;
        clients_map().write().await.clear();

        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let token = "test-bridge-token-123".to_string();
        let server_token = token.clone();

        let server_task = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let _ = handle_ws_client(stream, server_token).await;
        });

        let mut client = open_test_ws_client(port, &token, None).await;
        assert!(has_connected_clients().await);

        let mut rx = Vec::new();
        let mut tmp = [0u8; 1024];

        let switch_task = tokio::spawn(async {
            request_seamless_switch(
                SeamlessSwitchPayload {
                    email: "new-user@example.com".into(),
                    access_token: "ya29.fresh".into(),
                    refresh_token: "1//fresh".into(),
                    expiry_date_seconds: 1_800_000_000,
                    is_gcp_tos: false,
                },
                5_000,
            )
            .await
        });

        let switch_req_frame = loop {
            if let Some(f) = try_decode_ws_frame(&mut rx).unwrap() {
                break f;
            }
            let n = client.read(&mut tmp).await.unwrap();
            assert!(n > 0);
            rx.extend_from_slice(&tmp[..n]);
        };
        let req_json: serde_json::Value =
            serde_json::from_slice(&switch_req_frame.payload).unwrap();
        assert_eq!(req_json["type"], "switch_account");
        assert_eq!(req_json["email"], "new-user@example.com");
        assert_eq!(req_json["accessToken"], "ya29.fresh");

        let reply_msg = json!({
            "type": "switch_result",
            "requestId": req_json["requestId"],
            "success": true,
            "email": "new-user@example.com",
            "effectiveMode": "seamless",
            "message": "OK"
        })
        .to_string();
        client
            .write_all(&encode_masked_client_text(&reply_msg))
            .await
            .unwrap();

        let switch_reply = switch_task.await.unwrap().unwrap();
        assert!(switch_reply.success);
        assert_eq!(switch_reply.effective_mode.as_deref(), Some("seamless"));

        drop(client);
        let _ = server_task.await;
        assert_eq!(collect_alive_clients().await.len(), 0);
    }

    #[tokio::test]
    async fn deduplicates_same_window_session_and_cleans_up_on_frame_error() {
        let _guard = test_clients_lock().lock().await;
        clients_map().write().await.clear();

        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let token = "test-bridge-dedup-token".to_string();

        let accept_token = token.clone();
        let accept_task = tokio::spawn(async move {
            let mut handles = Vec::new();
            for _ in 0..3 {
                let (stream, _) = listener.accept().await.unwrap();
                let t = accept_token.clone();
                handles.push(tokio::spawn(async move { handle_ws_client(stream, t).await }));
            }
            handles
        });

        let c1 = open_test_ws_client(port, &token, Some("win-session-A")).await;
        assert_eq!(collect_alive_clients().await.len(), 1);

        // Same IDE window reconnects (e.g., Extension Host restart before old socket finished closing)
        let mut c2 = open_test_ws_client(port, &token, Some("win-session-A")).await;
        assert_eq!(collect_alive_clients().await.len(), 1);
        drop(c1);

        // A genuine second IDE window connects with a distinct sessionId
        let c3 = open_test_ws_client(port, &token, Some("win-session-B")).await;
        assert_eq!(collect_alive_clients().await.len(), 2);

        let server_handles = accept_task.await.unwrap();
        let mut iter = server_handles.into_iter();
        let h1 = iter.next().unwrap();
        let h2 = iter.next().unwrap();
        let h3 = iter.next().unwrap();
        let _ = h1.await;

        // Force a decode error (oversized length 127 frame > MAX_WS_FRAME_LEN) on c2 to prove cleanup still runs!
        let bad_frame = [0x81u8, 0x7F, 0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
        let _ = c2.write_all(&bad_frame).await;
        drop(c2);
        let res2 = h2.await.unwrap();
        assert!(res2.is_err());
        assert_eq!(collect_alive_clients().await.len(), 1);

        drop(c3);
        let _ = h3.await;
        assert_eq!(collect_alive_clients().await.len(), 0);
    }

    #[test]
    fn process_liveness_detects_current_and_bogus_pid() {
        assert!(is_os_pid_alive(std::process::id()));
        assert!(!is_os_pid_alive(0));
        assert!(!is_os_pid_alive(u32::MAX - 1));
    }
}
