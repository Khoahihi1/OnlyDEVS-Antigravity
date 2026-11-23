mod accounts;
mod antigravity_token_usage;
mod api_service;
mod config;
mod mcp_skills;
mod paths;
mod plugins;
mod prompts;
mod sessions;
mod updater;

use prompts::InstructionMode;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager, WindowEvent};

static ALLOW_APP_EXIT: AtomicBool = AtomicBool::new(false);

#[tauri::command]
fn get_overview_state() -> Result<config::OverviewState, String> {
    config::build_overview()
}

#[tauri::command]
fn get_config_raw() -> Result<String, String> {
    config::get_config_raw()
}

#[tauri::command]
fn save_config_raw(raw: String) -> Result<(), String> {
    config::save_config_raw(&raw)
}

#[tauri::command]
fn list_plugins() -> Result<Vec<config::PluginEntry>, String> {
    config::list_plugins()
}

#[tauri::command]
fn set_plugin_enabled(plugin_id: String, enabled: bool) -> Result<(), String> {
    config::set_plugin_enabled(&plugin_id, enabled)
}

#[tauri::command]
fn install_filter_bypass_plugin(source_path: Option<String>) -> Result<String, String> {
    plugins::install_filter_bypass(source_path)
}

#[tauri::command]
fn install_plugin_from_folder(
    source_path: String,
    plugin_id: Option<String>,
) -> Result<String, String> {
    plugins::install_plugin_from_folder(source_path, plugin_id)
}

#[tauri::command]
fn install_plugin_from_zip(
    zip_path: Option<String>,
    zip_name: Option<String>,
    zip_bytes: Option<Vec<u8>>,
    plugin_id: Option<String>,
) -> Result<String, String> {
    plugins::install_plugin_from_zip(zip_path, zip_name, zip_bytes, plugin_id)
}

#[tauri::command]
fn install_plugin_from_git(
    repo_url: String,
    plugin_id: Option<String>,
) -> Result<String, String> {
    plugins::install_plugin_from_git(repo_url, plugin_id)
}

#[tauri::command]
fn create_custom_plugin(
    plugin_id: String,
    name: String,
    version: String,
    description: String,
    hooks_raw: Option<String>,
    register_only: bool,
) -> Result<String, String> {
    plugins::create_custom_plugin(
        plugin_id,
        name,
        version,
        description,
        hooks_raw,
        register_only,
    )
}

#[tauri::command]
fn uninstall_plugin(plugin_id: String) -> Result<String, String> {
    plugins::uninstall_plugin(&plugin_id)
}

#[tauri::command]
fn get_plugin_hooks_raw(plugin_id: String) -> Result<String, String> {
    plugins::get_plugin_hooks_raw(&plugin_id)
}

#[tauri::command]
fn save_plugin_hooks_raw(plugin_id: String, raw: String) -> Result<(), String> {
    plugins::save_plugin_hooks_raw(&plugin_id, &raw)
}

#[tauri::command]
fn run_filter_bypass_test() -> Result<String, String> {
    plugins::run_filter_bypass_test()
}

#[tauri::command]
fn list_prompts() -> Result<Vec<prompts::SavedPrompt>, String> {
    prompts::list_prompts()
}

#[tauri::command]
fn list_prompt_categories() -> Result<Vec<prompts::PromptCategory>, String> {
    prompts::list_categories()
}

#[tauri::command]
fn create_prompt_category(name: String) -> Result<Vec<prompts::PromptCategory>, String> {
    prompts::create_category(name)
}

#[tauri::command]
fn delete_prompt_category(name: String) -> Result<Vec<prompts::PromptCategory>, String> {
    prompts::delete_category(name)
}

#[tauri::command]
fn sync_seed_prompts() -> Result<usize, String> {
    prompts::sync_seed_prompts()
}

#[tauri::command]
fn upsert_prompt(
    id: Option<String>,
    category: String,
    title: String,
    filename: String,
    description: Option<String>,
    content: String,
    enabled: bool,
) -> Result<prompts::SavedPrompt, String> {
    prompts::upsert_prompt(id, category, title, filename, description, content, enabled)
}

#[tauri::command]
fn delete_prompt(id: String) -> Result<(), String> {
    prompts::delete_prompt(&id)
}

#[tauri::command]
fn set_prompt_enabled(id: String, enabled: bool) -> Result<(), String> {
    prompts::set_prompt_enabled(&id, enabled)
}

#[tauri::command]
fn get_agents_md() -> Result<String, String> {
    prompts::get_agents_md()
}

#[tauri::command]
fn save_agents_md(content: String) -> Result<(), String> {
    prompts::save_agents_md(&content)
}

#[tauri::command]
fn apply_prompts_to_agents(mode: String) -> Result<String, String> {
    let instruction_mode = match mode.as_str() {
        "append" => InstructionMode::Append,
        _ => InstructionMode::Replace,
    };
    prompts::apply_prompts_to_agents(instruction_mode)
}

#[tauri::command]
fn get_instruction_mode() -> Result<String, String> {
    match prompts::get_instruction_mode()? {
        InstructionMode::Append => Ok("append".into()),
        InstructionMode::Replace => Ok("replace".into()),
    }
}

#[tauri::command]
fn set_instruction_mode(mode: String) -> Result<(), String> {
    let instruction_mode = match mode.as_str() {
        "append" => InstructionMode::Append,
        _ => InstructionMode::Replace,
    };
    prompts::set_instruction_mode(instruction_mode)
}

#[tauri::command]
fn list_sessions() -> Result<sessions::SessionScanResult, String> {
    sessions::list_sessions()
}

#[tauri::command]
fn get_session_transcript(session_id: String) -> Result<Vec<sessions::SessionStepItem>, String> {
    sessions::get_session_transcript(&session_id)
}

#[tauri::command]
fn sync_sessions() -> Result<String, String> {
    sessions::sync_sessions()
}

#[tauri::command]
fn delete_sessions(ids: Vec<String>) -> Result<String, String> {
    sessions::delete_sessions(ids)
}

#[tauri::command]
fn export_sessions_markdown(ids: Vec<String>) -> Result<String, String> {
    sessions::export_sessions_markdown(ids)
}

#[tauri::command]
fn list_mcp_and_skills() -> Result<mcp_skills::McpSkillsState, String> {
    mcp_skills::list_mcp_and_skills()
}

#[tauri::command]
fn set_mcp_enabled(id: String, enabled: bool) -> Result<(), String> {
    mcp_skills::set_mcp_enabled(&id, enabled)
}

#[tauri::command]
fn upsert_mcp_server(
    id: String,
    transport: String,
    url: Option<String>,
    command: Option<String>,
    args: Vec<String>,
    env: BTreeMap<String, String>,
    enabled: bool,
) -> Result<(), String> {
    mcp_skills::upsert_mcp_server(id, transport, url, command, args, env, enabled)
}

#[tauri::command]
fn delete_mcp_server(id: String) -> Result<(), String> {
    mcp_skills::delete_mcp_server(&id)
}

#[tauri::command]
fn get_mcp_config_raw() -> Result<String, String> {
    mcp_skills::get_mcp_config_raw()
}

#[tauri::command]
fn save_mcp_config_raw(raw: String) -> Result<(), String> {
    mcp_skills::save_mcp_config_raw(&raw)
}

#[tauri::command]
fn set_skill_enabled(id: String, enabled: bool) -> Result<(), String> {
    mcp_skills::set_skill_enabled(&id, enabled)
}

#[tauri::command]
fn get_skill_content(id: String, source: String) -> Result<String, String> {
    mcp_skills::get_skill_content(&id, &source)
}

#[tauri::command]
fn upsert_skill(
    id: String,
    name: String,
    description: String,
    body: String,
) -> Result<(), String> {
    mcp_skills::upsert_skill(id, name, description, body)
}

#[tauri::command]
fn delete_skill(id: String) -> Result<(), String> {
    mcp_skills::delete_skill(&id)
}

#[tauri::command]
fn save_item_note(kind: String, id: String, note: String) -> Result<(), String> {
    mcp_skills::save_item_note(&kind, &id, &note)
}

// ============================================================================
// Antigravity Account Commands
// ============================================================================

#[tauri::command]
fn list_accounts() -> Result<Vec<accounts::AntigravityAccount>, String> {
    accounts::list_accounts_sync()
}

#[tauri::command]
async fn import_local_account() -> Result<accounts::AntigravityAccount, String> {
    accounts::import_local_account().await
}

#[tauri::command]
async fn add_accounts_by_token(
    refresh_token_input: String,
) -> Result<Vec<accounts::AntigravityAccount>, String> {
    accounts::add_account_by_refresh_token(refresh_token_input).await
}

#[tauri::command]
async fn import_accounts_content(
    content: String,
) -> Result<Vec<accounts::AntigravityAccount>, String> {
    accounts::import_accounts_from_content(content).await
}

#[tauri::command]
fn export_accounts(account_ids: Vec<String>, include_sensitive: bool) -> Result<String, String> {
    accounts::export_accounts_json(account_ids, include_sensitive)
}

#[tauri::command]
async fn fetch_account_quota(account_id: String) -> Result<accounts::AntigravityAccount, String> {
    accounts::refresh_single_account_quota(account_id).await
}

#[tauri::command]
async fn refresh_all_quotas() -> Result<accounts::RefreshStats, String> {
    accounts::refresh_all_account_quotas().await
}

#[tauri::command]
fn detect_launch_targets() -> Vec<accounts::LaunchTargetInfo> {
    accounts::detect_antigravity_launch_targets()
}

#[tauri::command]
async fn switch_account(
    account_id: String,
    switch_target: Option<String>,
    launch: Option<bool>,
    custom_path: Option<String>,
    working_dir: Option<String>,
) -> Result<accounts::AntigravityAccount, String> {
    accounts::switch_antigravity_account(account_id, switch_target, launch, custom_path, working_dir)
        .await
}

#[tauri::command]
fn update_account_metadata(
    account_id: String,
    tags: Vec<String>,
    notes: Option<String>,
    two_factor_secret: Option<String>,
    account_password: Option<String>,
    phone_number: Option<String>,
    mail_url: Option<String>,
    aux_email: Option<String>,
) -> Result<accounts::AntigravityAccount, String> {
    accounts::update_account_metadata(
        account_id,
        tags,
        notes,
        two_factor_secret,
        account_password,
        phone_number,
        mail_url,
        aux_email,
    )
}

#[tauri::command]
fn delete_accounts(account_ids: Vec<String>) -> Result<usize, String> {
    accounts::delete_accounts_by_ids(account_ids)
}

#[tauri::command]
async fn prepare_oauth_session(
    app: tauri::AppHandle,
) -> Result<accounts::OAuthPrepareResult, String> {
    accounts::prepare_oauth_session(app).await
}

#[tauri::command]
async fn start_oauth_login(app: tauri::AppHandle) -> Result<accounts::AntigravityAccount, String> {
    accounts::start_oauth_login(app).await
}

#[tauri::command]
async fn complete_oauth_login(
    app: tauri::AppHandle,
) -> Result<accounts::AntigravityAccount, String> {
    accounts::complete_oauth_login(app).await
}

#[tauri::command]
async fn submit_oauth_callback_url(
    app: tauri::AppHandle,
    callback_url: String,
) -> Result<(), String> {
    accounts::submit_oauth_callback_url(app, callback_url).await
}

#[tauri::command]
fn cancel_oauth_login() -> Result<(), String> {
    accounts::cancel_oauth_login();
    Ok(())
}

#[tauri::command]
fn generate_totp_code(secret: String) -> Result<accounts::TotpCodeResult, String> {
    accounts::generate_totp_code(&secret)
}

#[tauri::command]
fn get_switch_history() -> Result<Vec<accounts::SwitchHistoryItem>, String> {
    accounts::get_switch_history()
}

#[tauri::command]
fn clear_switch_history() -> Result<(), String> {
    accounts::clear_switch_history()
}

#[tauri::command]
fn get_antigravity_token_usage() -> Result<accounts::TokenUsageSummary, String> {
    accounts::get_antigravity_token_usage()
}

#[tauri::command]
fn list_account_groups() -> Result<Vec<accounts::AccountGroup>, String> {
    accounts::list_account_groups()
}

#[tauri::command]
fn save_account_group(
    id: Option<String>,
    name: String,
    account_ids: Vec<String>,
) -> Result<accounts::AccountGroup, String> {
    accounts::save_account_group(id, name, account_ids)
}

#[tauri::command]
fn delete_account_group(group_id: String) -> Result<(), String> {
    accounts::delete_account_group(&group_id)
}

#[tauri::command]
fn assign_accounts_to_group(
    group_id: String,
    account_ids: Vec<String>,
    add_mode: bool,
) -> Result<accounts::AccountGroup, String> {
    accounts::assign_accounts_to_group(&group_id, account_ids, add_mode)
}

#[tauri::command]
fn get_account_custom_sort() -> Result<Vec<String>, String> {
    accounts::get_account_custom_sort()
}

#[tauri::command]
fn save_account_custom_sort(account_ids: Vec<String>) -> Result<Vec<String>, String> {
    accounts::save_account_custom_sort(account_ids)
}

#[tauri::command]
async fn import_accounts_from_directory(
    dir_path: String,
) -> Result<Vec<accounts::AntigravityAccount>, String> {
    accounts::import_accounts_from_directory(dir_path).await
}

#[tauri::command]
async fn pick_folder_dialog(default_path: Option<String>) -> Result<Option<String>, String> {
    tokio::task::spawn_blocking(move || accounts::pick_folder_dialog(default_path))
        .await
        .map_err(|e| format!("Folder dialog task failed: {e}"))?
}

#[tauri::command]
async fn antigravity_query_token_usage(
    query: Option<antigravity_token_usage::types::DashboardQueryRequest>,
) -> Result<antigravity_token_usage::types::DashboardBundleResponse, String> {
    tokio::task::spawn_blocking(move || antigravity_token_usage::query_token_usage(query))
        .await
        .map_err(|e| format!("Antigravity token usage query task failed: {e}"))?
}

#[tauri::command]
async fn antigravity_sync_token_usage(
    rebuild: Option<bool>,
    query: Option<antigravity_token_usage::types::DashboardQueryRequest>,
) -> Result<antigravity_token_usage::types::DashboardBundleResponse, String> {
    tokio::task::spawn_blocking(move || {
        antigravity_token_usage::sync_token_usage(rebuild.unwrap_or(false), query)
    })
    .await
    .map_err(|e| format!("Antigravity token usage sync task failed: {e}"))?
}

#[tauri::command]
async fn antigravity_get_conversation_token_detail(
    conversation_id: String,
) -> Result<antigravity_token_usage::types::ConversationDetailResponse, String> {
    tokio::task::spawn_blocking(move || {
        antigravity_token_usage::get_conversation_detail(&conversation_id)
    })
    .await
    .map_err(|e| format!("Antigravity conversation detail task failed: {e}"))?
}

#[tauri::command]
async fn antigravity_get_token_usage_settings(
) -> Result<antigravity_token_usage::types::AntigravityTokenUsageSettings, String> {
    tokio::task::spawn_blocking(antigravity_token_usage::get_token_usage_settings)
        .await
        .map_err(|e| format!("Antigravity token usage settings task failed: {e}"))?
}

#[tauri::command]
async fn antigravity_set_token_usage_data_dir(
    custom_dir: Option<String>,
    query: Option<antigravity_token_usage::types::DashboardQueryRequest>,
) -> Result<antigravity_token_usage::types::DashboardBundleResponse, String> {
    tokio::task::spawn_blocking(move || {
        antigravity_token_usage::set_custom_data_dir(custom_dir, query)
    })
    .await
    .map_err(|e| format!("Antigravity set token usage data dir task failed: {e}"))?
}

#[tauri::command]
fn get_update_settings() -> Result<updater::UpdateSettings, String> {
    updater::load_update_settings()
}

#[tauri::command]
fn patch_update_settings(
    auto_check: Option<bool>,
    check_interval_hours: Option<u64>,
    auto_install: Option<bool>,
    remind_on_update: Option<bool>,
    skipped_version: Option<String>,
) -> Result<updater::UpdateSettings, String> {
    updater::patch_update_settings(
        auto_check,
        check_interval_hours,
        auto_install,
        remind_on_update,
        skipped_version,
    )
}

#[tauri::command]
fn should_check_updates() -> Result<bool, String> {
    updater::should_check_updates()
}

#[tauri::command]
fn check_version_jump() -> Result<Option<updater::VersionJumpInfo>, String> {
    updater::check_version_jump()
}

#[tauri::command]
async fn check_for_updates(force: Option<bool>) -> Result<updater::UpdateCheckResult, String> {
    updater::check_for_updates(force.unwrap_or(false)).await
}

#[tauri::command]
async fn download_and_install_update(
    app: tauri::AppHandle,
    download_url: String,
    asset_name: Option<String>,
    silent: Option<bool>,
) -> Result<String, String> {
    updater::download_and_install_update(app, download_url, asset_name, silent.unwrap_or(false))
        .await
}

#[tauri::command]
fn api_service_get_status() -> Result<api_service::ApiServiceStatus, String> {
    api_service::get_status()
}

#[tauri::command]
async fn api_service_start() -> Result<api_service::ApiServiceStatus, String> {
    api_service::start_server().await
}

#[tauri::command]
async fn api_service_stop() -> Result<api_service::ApiServiceStatus, String> {
    api_service::stop_server().await
}

#[tauri::command]
async fn api_service_restart() -> Result<api_service::ApiServiceStatus, String> {
    api_service::restart_server().await
}

#[tauri::command]
fn api_service_save_config(
    config: api_service::ApiServiceConfig,
) -> Result<api_service::ApiServiceConfig, String> {
    api_service::save_config(config)
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
fn api_service_create_key(
    label: String,
    allowed_models: Vec<String>,
    pinned_account_ids: Vec<String>,
    allowed_profile: Option<String>,
    rate_limit_rpm: Option<u32>,
    daily_token_limit: Option<u64>,
    expires_at: Option<i64>,
) -> Result<api_service::ApiKeyRecord, String> {
    api_service::create_api_key(
        label,
        allowed_models,
        pinned_account_ids,
        allowed_profile,
        rate_limit_rpm,
        daily_token_limit,
        expires_at,
    )
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
fn api_service_update_key(
    id: String,
    label: Option<String>,
    enabled: Option<bool>,
    allowed_models: Option<Vec<String>>,
    pinned_account_ids: Option<Vec<String>>,
    allowed_profile: Option<String>,
    rate_limit_rpm: Option<u32>,
    daily_token_limit: Option<u64>,
    expires_at: Option<Option<i64>>,
) -> Result<api_service::ApiKeyRecord, String> {
    api_service::update_api_key(
        id,
        label,
        enabled,
        allowed_models,
        pinned_account_ids,
        allowed_profile,
        rate_limit_rpm,
        daily_token_limit,
        expires_at,
    )
}

#[tauri::command]
fn api_service_rotate_key(id: String) -> Result<api_service::ApiKeyRecord, String> {
    api_service::rotate_api_key(id)
}

#[tauri::command]
fn api_service_delete_key(id: String) -> Result<(), String> {
    api_service::delete_api_key(id)
}

#[tauri::command]
fn api_service_clear_cooldowns(account_id: Option<String>) -> Result<(), String> {
    api_service::clear_account_cooldowns(account_id)
}

#[tauri::command]
fn api_service_list_models() -> Result<Vec<api_service::ApiAvailableModel>, String> {
    api_service::list_available_models()
}

#[tauri::command]
fn api_service_list_logs(limit: Option<usize>) -> Result<Vec<api_service::ApiRequestLogItem>, String> {
    api_service::list_request_logs(limit)
}

#[tauri::command]
fn api_service_clear_logs() -> Result<(), String> {
    api_service::clear_request_logs()
}

#[tauri::command]
fn api_service_get_usage(range: Option<String>) -> Result<api_service::ApiServiceUsageSummary, String> {
    api_service::get_usage_summary(range)
}

#[tauri::command]
async fn api_service_test_chat(
    model: String,
    prompt: String,
) -> Result<api_service::ApiTestChatResult, String> {
    api_service::test_chat_completion(model, prompt).await
}

#[tauri::command]
async fn api_service_lan_diagnostics(
    port: Option<u16>,
) -> Result<api_service::LanDiagnostics, String> {
    tokio::task::spawn_blocking(move || api_service::get_lan_diagnostics(port))
        .await
        .map_err(|e| format!("LAN diagnostics task failed: {e}"))?
}

#[tauri::command]
async fn api_service_open_lan_firewall(
    port: Option<u16>,
) -> Result<api_service::LanDiagnostics, String> {
    tokio::task::spawn_blocking(move || api_service::open_lan_firewall(port))
        .await
        .map_err(|e| format!("Open LAN firewall task failed: {e}"))?
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.unminimize();
                let _ = win.show();
                let _ = win.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            api_service::spawn_auto_start_if_enabled();
            api_service::spawn_background_quota_worker(app.handle().clone());

            // Native System Tray so API Router keeps running when closing the window
            if let Ok(item_show) = MenuItem::with_id(
                app,
                "tray_show",
                "Mở OnlyDEVS Antigravity",
                true,
                None::<&str>,
            ) {
                if let (Ok(item_toggle), Ok(sep), Ok(item_quit)) = (
                    MenuItem::with_id(
                        app,
                        "tray_toggle_api",
                        "Bật / Tắt API Router",
                        true,
                        None::<&str>,
                    ),
                    PredefinedMenuItem::separator(app),
                    MenuItem::with_id(
                        app,
                        "tray_quit",
                        "Thoát hẳn (Dừng API)",
                        true,
                        None::<&str>,
                    ),
                ) {
                    if let Ok(menu) =
                        Menu::with_items(app, &[&item_show, &item_toggle, &sep, &item_quit])
                    {
                        let mut builder = TrayIconBuilder::with_id("onlydevs-tray")
                            .tooltip("OnlyDEVS — Antigravity API Router")
                            .menu(&menu)
                            .show_menu_on_left_click(false)
                            .on_menu_event(|app_handle, event| match event.id().as_ref() {
                                "tray_show" => {
                                    if let Some(win) = app_handle.get_webview_window("main") {
                                        let _ = win.unminimize();
                                        let _ = win.show();
                                        let _ = win.set_focus();
                                    }
                                }
                                "tray_toggle_api" => {
                                    let handle = app_handle.clone();
                                    tauri::async_runtime::spawn(async move {
                                        let res = if api_service::is_server_running() {
                                            api_service::stop_server().await
                                        } else {
                                            api_service::start_server().await
                                        };
                                        if let Ok(status) = res {
                                            let _ = handle.emit("api-service-status-changed", status);
                                        }
                                    });
                                }
                                "tray_quit" => {
                                    ALLOW_APP_EXIT.store(true, Ordering::SeqCst);
                                    api_service::stop_server_sync();
                                    app_handle.exit(0);
                                }
                                _ => {}
                            })
                            .on_tray_icon_event(|tray, event| {
                                if let TrayIconEvent::Click {
                                    button: MouseButton::Left,
                                    button_state: MouseButtonState::Up,
                                    ..
                                } = event
                                {
                                    if let Some(win) = tray.app_handle().get_webview_window("main") {
                                        let _ = win.unminimize();
                                        let _ = win.show();
                                        let _ = win.set_focus();
                                    }
                                }
                            });
                        if let Some(icon) = app.default_window_icon() {
                            builder = builder.icon(icon.clone());
                        }
                        let _ = builder.build(app);
                    }
                }
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                if !ALLOW_APP_EXIT.load(Ordering::SeqCst) {
                    let minimize = api_service::load_config()
                        .map(|c| c.minimize_to_tray)
                        .unwrap_or(true);
                    if minimize {
                        api.prevent_close();
                        let _ = window.hide();
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_overview_state,
            get_config_raw,
            save_config_raw,
            list_plugins,
            set_plugin_enabled,
            install_filter_bypass_plugin,
            install_plugin_from_folder,
            install_plugin_from_zip,
            install_plugin_from_git,
            create_custom_plugin,
            uninstall_plugin,
            get_plugin_hooks_raw,
            save_plugin_hooks_raw,
            run_filter_bypass_test,
            list_prompts,
            list_prompt_categories,
            create_prompt_category,
            delete_prompt_category,
            sync_seed_prompts,
            upsert_prompt,
            delete_prompt,
            set_prompt_enabled,
            get_agents_md,
            save_agents_md,
            apply_prompts_to_agents,
            get_instruction_mode,
            set_instruction_mode,
            list_sessions,
            get_session_transcript,
            sync_sessions,
            delete_sessions,
            export_sessions_markdown,
            list_mcp_and_skills,
            set_mcp_enabled,
            upsert_mcp_server,
            delete_mcp_server,
            get_mcp_config_raw,
            save_mcp_config_raw,
            set_skill_enabled,
            get_skill_content,
            upsert_skill,
            delete_skill,
            save_item_note,
            list_accounts,
            import_local_account,
            add_accounts_by_token,
            import_accounts_content,
            export_accounts,
            fetch_account_quota,
            refresh_all_quotas,
            detect_launch_targets,
            switch_account,
            update_account_metadata,
            delete_accounts,
            prepare_oauth_session,
            start_oauth_login,
            complete_oauth_login,
            submit_oauth_callback_url,
            cancel_oauth_login,
            generate_totp_code,
            get_switch_history,
            clear_switch_history,
            get_antigravity_token_usage,
            list_account_groups,
            save_account_group,
            delete_account_group,
            assign_accounts_to_group,
            get_account_custom_sort,
            save_account_custom_sort,
            import_accounts_from_directory,
            pick_folder_dialog,
            antigravity_query_token_usage,
            antigravity_sync_token_usage,
            antigravity_get_conversation_token_detail,
            antigravity_get_token_usage_settings,
            antigravity_set_token_usage_data_dir,
            get_update_settings,
            patch_update_settings,
            should_check_updates,
            check_version_jump,
            check_for_updates,
            download_and_install_update,
            api_service_get_status,
            api_service_start,
            api_service_stop,
            api_service_restart,
            api_service_save_config,
            api_service_create_key,
            api_service_update_key,
            api_service_rotate_key,
            api_service_delete_key,
            api_service_clear_cooldowns,
            api_service_list_models,
            api_service_list_logs,
            api_service_clear_logs,
            api_service_get_usage,
            api_service_test_chat,
            api_service_lan_diagnostics,
            api_service_open_lan_firewall,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app_handle, event| {
            if matches!(
                event,
                tauri::RunEvent::Exit | tauri::RunEvent::ExitRequested { .. }
            ) {
                api_service::stop_server_sync();
            }
        });
}
