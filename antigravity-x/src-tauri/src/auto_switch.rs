use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Emitter};

use crate::accounts::{self, AntigravityAccount};
use crate::paths;

static WATCHER_STARTED: AtomicBool = AtomicBool::new(false);

fn eval_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

fn last_alert_state() -> &'static Mutex<Option<(String, i64)>> {
    static STATE: OnceLock<Mutex<Option<(String, i64)>>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(None))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AutoSwitchConfig {
    pub enabled: bool,
    pub threshold: i32,
    pub split_window_thresholds: bool,
    pub five_hour_threshold: i32,
    pub weekly_threshold: i32,
    pub credits_enabled: bool,
    pub credits_threshold: i32,
    /// "any_group" | "all_groups" | "selected_groups"
    pub scope_mode: String,
    /// Subset of ["gemini-5h", "gemini-weekly", "claude-5h", "claude-weekly"]
    pub selected_group_ids: Vec<String>,
    /// "all_accounts" | "selected_accounts"
    pub account_scope_mode: String,
    pub selected_account_ids: Vec<String>,
    pub quota_alert_enabled: bool,
    pub quota_alert_threshold: i32,
    pub poll_interval_minutes: u64,
    /// "ide" | "cli" | "both"
    pub switch_target: String,
}

impl Default for AutoSwitchConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            threshold: 5,
            split_window_thresholds: false,
            five_hour_threshold: 5,
            weekly_threshold: 5,
            credits_enabled: false,
            credits_threshold: 100,
            scope_mode: "any_group".into(),
            selected_group_ids: vec![
                "gemini-5h".into(),
                "gemini-weekly".into(),
                "claude-5h".into(),
                "claude-weekly".into(),
            ],
            account_scope_mode: "all_accounts".into(),
            selected_account_ids: Vec::new(),
            quota_alert_enabled: false,
            quota_alert_threshold: 20,
            poll_interval_minutes: 3,
            switch_target: "ide".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoSwitchEvalResult {
    pub switched: bool,
    pub from_email: Option<String>,
    pub to_email: Option<String>,
    pub reason: String,
    pub alert_triggered: bool,
    pub alert_message: Option<String>,
}

fn config_path() -> Result<PathBuf, String> {
    let dir = paths::home_dir()?.join(".antigravity-x");
    fs::create_dir_all(&dir).map_err(|e| format!("Tạo thư mục cấu hình: {e}"))?;
    Ok(dir.join("auto_switch_config.json"))
}

pub fn normalize_config(mut cfg: AutoSwitchConfig) -> AutoSwitchConfig {
    cfg.threshold = cfg.threshold.clamp(0, 95);
    cfg.five_hour_threshold = cfg.five_hour_threshold.clamp(0, 95);
    cfg.weekly_threshold = cfg.weekly_threshold.clamp(0, 95);
    cfg.credits_threshold = cfg.credits_threshold.clamp(0, 100_000);
    cfg.quota_alert_threshold = cfg.quota_alert_threshold.clamp(1, 95);
    cfg.poll_interval_minutes = cfg.poll_interval_minutes.clamp(1, 60);
    cfg.scope_mode = match cfg.scope_mode.trim().to_ascii_lowercase().as_str() {
        "all_groups" => "all_groups".into(),
        "selected_groups" => "selected_groups".into(),
        _ => "any_group".into(),
    };
    cfg.account_scope_mode = match cfg.account_scope_mode.trim().to_ascii_lowercase().as_str() {
        "selected_accounts" => "selected_accounts".into(),
        _ => "all_accounts".into(),
    };
    cfg.switch_target = match cfg.switch_target.trim().to_ascii_lowercase().as_str() {
        "cli" => "cli".into(),
        "both" => "both".into(),
        _ => "ide".into(),
    };
    if cfg.selected_group_ids.is_empty() {
        cfg.selected_group_ids = vec![
            "gemini-5h".into(),
            "gemini-weekly".into(),
            "claude-5h".into(),
            "claude-weekly".into(),
        ];
    }
    cfg
}

pub fn load_config() -> AutoSwitchConfig {
    let Ok(path) = config_path() else {
        return AutoSwitchConfig::default();
    };
    if !path.is_file() {
        return AutoSwitchConfig::default();
    }
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str::<AutoSwitchConfig>(&s).ok())
        .map(normalize_config)
        .unwrap_or_default()
}

fn write_config_only(cfg: AutoSwitchConfig) -> Result<AutoSwitchConfig, String> {
    let normalized = normalize_config(cfg);
    let path = config_path()?;
    let json = serde_json::to_string_pretty(&normalized)
        .map_err(|e| format!("Serialize auto_switch_config: {e}"))?;
    fs::write(&path, json).map_err(|e| format!("Ghi auto_switch_config: {e}"))?;
    Ok(normalized)
}

pub fn save_config(cfg: AutoSwitchConfig) -> Result<AutoSwitchConfig, String> {
    let normalized = write_config_only(cfg)?;
    if normalized.enabled {
        crate::ide_bridge::disable_seamless_mode();
    }
    Ok(normalized)
}

pub fn disable_classic_mode() {
    let mut cfg = load_config();
    if cfg.enabled {
        cfg.enabled = false;
        let _ = write_config_only(cfg);
    }
}

#[derive(Debug, Clone, Copy)]
struct BucketSnapshot {
    id: &'static str,
    label: &'static str,
    is_weekly: bool,
    pct: i32,
}

fn extract_monitored_buckets(acc: &AntigravityAccount) -> Vec<BucketSnapshot> {
    let Some(q) = acc.quota.as_ref() else {
        return Vec::new();
    };
    let mut g5h: Option<i32> = None;
    let mut gwk: Option<i32> = None;
    let mut c5h: Option<i32> = None;
    let mut cwk: Option<i32> = None;

    for m in &q.models {
        let n = m.name.trim().to_ascii_lowercase();
        if n == "gemini-5h" || n == "gemini:5h" {
            g5h = Some(m.percentage);
        } else if n == "gemini-weekly" || n == "gemini:weekly" {
            gwk = Some(m.percentage);
        } else if n == "3p-5h" || n == "claude:5h" {
            c5h = Some(m.percentage);
        } else if n == "3p-weekly" || n == "claude:weekly" {
            cwk = Some(m.percentage);
        }
    }

    // Fallback to model-level percentages if summary buckets aren't present
    if g5h.is_none() && c5h.is_none() {
        let mut gem_min: Option<i32> = None;
        let mut cl_min: Option<i32> = None;
        for m in &q.models {
            let n = m.name.trim().to_ascii_lowercase();
            if n.contains(':') || n.ends_with("-5h") || n.ends_with("-weekly") || n.contains("wtu")
            {
                continue;
            }
            if n.contains("claude") {
                cl_min = Some(cl_min.map_or(m.percentage, |cur| cur.min(m.percentage)));
            } else if n.contains("gemini") {
                gem_min = Some(gem_min.map_or(m.percentage, |cur| cur.min(m.percentage)));
            }
        }
        g5h = gem_min;
        c5h = cl_min;
    }

    let mut out = Vec::new();
    if let Some(p) = g5h {
        out.push(BucketSnapshot {
            id: "gemini-5h",
            label: "Gemini 5h",
            is_weekly: false,
            pct: p,
        });
    }
    if let Some(p) = gwk {
        out.push(BucketSnapshot {
            id: "gemini-weekly",
            label: "Gemini Weekly",
            is_weekly: true,
            pct: p,
        });
    }
    if let Some(p) = c5h {
        out.push(BucketSnapshot {
            id: "claude-5h",
            label: "Claude 5h",
            is_weekly: false,
            pct: p,
        });
    }
    if let Some(p) = cwk {
        out.push(BucketSnapshot {
            id: "claude-weekly",
            label: "Claude Weekly",
            is_weekly: true,
            pct: p,
        });
    }
    out
}

fn available_credits_total(acc: &AntigravityAccount) -> Option<i32> {
    let credits = &acc.quota.as_ref()?.credits;
    if credits.is_empty() {
        return None;
    }
    let total: f64 = credits
        .iter()
        .filter_map(|c| {
            c.credit_amount
                .as_deref()
                .and_then(|s| s.trim().parse::<f64>().ok())
        })
        .sum();
    Some(total.round() as i32)
}

fn bucket_threshold(cfg: &AutoSwitchConfig, bucket: &BucketSnapshot) -> i32 {
    if cfg.split_window_thresholds {
        if bucket.is_weekly {
            cfg.weekly_threshold
        } else {
            cfg.five_hour_threshold
        }
    } else {
        cfg.threshold
    }
}

fn filter_active_buckets<'a>(
    cfg: &AutoSwitchConfig,
    buckets: &'a [BucketSnapshot],
) -> Vec<&'a BucketSnapshot> {
    if cfg.scope_mode == "selected_groups" {
        let selected: HashSet<&str> = cfg.selected_group_ids.iter().map(String::as_str).collect();
        buckets
            .iter()
            .filter(|b| selected.contains(b.id))
            .collect()
    } else {
        buckets.iter().collect()
    }
}

fn evaluate_trigger_reason(cfg: &AutoSwitchConfig, current: &AntigravityAccount) -> Option<String> {
    if current.quota_error.is_some() || current.quota.as_ref().is_some_and(|q| q.is_forbidden) {
        return Some("Tài khoản hiện tại gặp lỗi quota / 403".into());
    }

    let all_buckets = extract_monitored_buckets(current);
    let active = filter_active_buckets(cfg, &all_buckets);

    if !active.is_empty() {
        if cfg.scope_mode == "all_groups" {
            if active.iter().all(|b| b.pct <= bucket_threshold(cfg, b)) {
                return Some(format!(
                    "Tất cả nhóm mô hình đều chạm ngưỡng (<= {}%)",
                    cfg.threshold
                ));
            }
        } else if let Some(hit) = active.iter().find(|b| b.pct <= bucket_threshold(cfg, b)) {
            let th = bucket_threshold(cfg, hit);
            return Some(format!("{} còn {}% (<= {}%)", hit.label, hit.pct, th));
        }
    }

    if cfg.credits_enabled {
        if let Some(rem) = available_credits_total(current) {
            if rem < cfg.credits_threshold {
                return Some(format!(
                    "Credits còn {} (< {})",
                    rem, cfg.credits_threshold
                ));
            }
        }
    }

    None
}

fn is_candidate_healthy(cfg: &AutoSwitchConfig, acc: &AntigravityAccount, current_id: &str) -> Option<i32> {
    if acc.id == current_id || acc.disabled || acc.pending_oauth || acc.quota_error.is_some() {
        return None;
    }
    let q = acc.quota.as_ref()?;
    if q.is_forbidden {
        return None;
    }
    let all_buckets = extract_monitored_buckets(acc);
    let active = filter_active_buckets(cfg, &all_buckets);
    if active.is_empty() {
        return None;
    }
    if active.iter().any(|b| b.pct <= bucket_threshold(cfg, b)) {
        return None;
    }
    if cfg.credits_enabled {
        if let Some(rem) = available_credits_total(acc) {
            if rem < cfg.credits_threshold {
                return None;
            }
        }
    }
    let min_pct = active.iter().map(|b| b.pct).min().unwrap_or(0);
    let sum_pct: i32 = active.iter().map(|b| b.pct).sum();
    Some(min_pct * 1000 + sum_pct)
}

fn check_quota_alert(cfg: &AutoSwitchConfig, current: &AntigravityAccount) -> Option<String> {
    if !cfg.quota_alert_enabled {
        return None;
    }
    let all_buckets = extract_monitored_buckets(current);
    let active = filter_active_buckets(cfg, &all_buckets);
    let low = active
        .into_iter()
        .filter(|b| b.pct <= cfg.quota_alert_threshold)
        .min_by_key(|b| b.pct)?;
    Some(format!(
        "{}: {} chỉ còn {}% (<= {}%)",
        current.email, low.label, low.pct, cfg.quota_alert_threshold
    ))
}

pub fn evaluate_accounts_in_memory(
    cfg: &AutoSwitchConfig,
    accounts: &[AntigravityAccount],
) -> (Option<(String, String)>, Option<String>) {
    let Some(current) = accounts.iter().find(|a| a.is_current) else {
        return (None, None);
    };

    let alert = check_quota_alert(cfg, current);

    if !cfg.enabled {
        return (None, alert);
    }

    let in_scope = |acc: &AntigravityAccount| -> bool {
        if cfg.account_scope_mode == "selected_accounts" {
            cfg.selected_account_ids.iter().any(|id| id == &acc.id)
        } else {
            true
        }
    };

    if !in_scope(current) {
        return (None, alert);
    }

    let Some(reason) = evaluate_trigger_reason(cfg, current) else {
        return (None, alert);
    };

    let mut ranked: Vec<(&AntigravityAccount, i32)> = accounts
        .iter()
        .filter(|a| in_scope(a))
        .filter_map(|a| is_candidate_healthy(cfg, a, &current.id).map(|score| (a, score)))
        .collect();

    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| b.0.last_used.cmp(&a.0.last_used)));
    let Some((best, _)) = ranked.first() else {
        return (None, alert);
    };

    (Some((best.id.clone(), reason)), alert)
}

pub async fn evaluate_and_run_if_enabled(
    app: Option<&AppHandle>,
    force_check: bool,
) -> Result<AutoSwitchEvalResult, String> {
    let _guard = eval_lock().lock().await;
    let cfg = load_config();
    let seamless_on = crate::ide_bridge::is_seamless_auto_switch_enabled();
    let effective_enabled = cfg.enabled || seamless_on;
    if !force_check && !effective_enabled && !cfg.quota_alert_enabled {
        return Ok(AutoSwitchEvalResult {
            switched: false,
            from_email: None,
            to_email: None,
            reason: "Tự động chuyển đổi đang tắt".into(),
            alert_triggered: false,
            alert_message: None,
        });
    }

    let accounts = accounts::list_accounts_sync()?;
    let current_email = accounts
        .iter()
        .find(|a| a.is_current)
        .map(|a| a.email.clone());

    let mut eval_cfg = cfg.clone();
    if effective_enabled {
        eval_cfg.enabled = true;
    }
    let (switch_plan, alert_msg) = evaluate_accounts_in_memory(&eval_cfg, &accounts);

    let mut alert_triggered = false;
    if let Some(ref msg) = alert_msg {
        let now = chrono::Utc::now().timestamp();
        let should_emit = if let Ok(mut guard) = last_alert_state().lock() {
            let repeated = guard
                .as_ref()
                .is_some_and(|(prev, ts)| prev == msg && now - *ts < 300);
            if !repeated {
                *guard = Some((msg.clone(), now));
                true
            } else {
                false
            }
        } else {
            true
        };
        if should_emit {
            alert_triggered = true;
            if let Some(handle) = app {
                let _ = handle.emit("auto-switch-alert", msg.clone());
            }
        }
    }

    if let Some((target_id, mut reason)) = switch_plan {
        crate::ide_bridge::auto_heal_if_enabled();
        let bridge_ready = cfg.switch_target != "cli"
            && crate::ide_bridge::is_seamless_priority_enabled()
            && crate::ide_bridge::has_connected_clients().await;
        let should_relaunch = cfg.switch_target != "cli"
            && !bridge_ready
            && crate::ide_bridge::is_fallback_relaunch_enabled();

        let switched_acc = accounts::switch_antigravity_account(
            target_id,
            Some(cfg.switch_target.clone()),
            Some(should_relaunch),
            None,
            None,
        )
        .await?;

        if bridge_ready {
            reason = format!("{reason} · Seamless IDE Bridge");
        } else if should_relaunch {
            reason = format!("{reason} · Relaunched IDE");
        }

        let result = AutoSwitchEvalResult {
            switched: true,
            from_email: current_email,
            to_email: Some(switched_acc.email.clone()),
            reason,
            alert_triggered,
            alert_message: alert_msg,
        };
        if let Some(handle) = app {
            let _ = handle.emit("auto-switch-executed", &result);
        }
        return Ok(result);
    }

    Ok(AutoSwitchEvalResult {
        switched: false,
        from_email: current_email,
        to_email: None,
        reason: if effective_enabled {
            "Tài khoản hiện tại vẫn đủ hạn mức hoặc chưa có tài khoản dự phòng đạt ngưỡng".into()
        } else {
            "Tự động chuyển đổi đang tắt".into()
        },
        alert_triggered,
        alert_message: alert_msg,
    })
}

pub fn spawn_auto_switch_watcher(app: AppHandle) {
    if WATCHER_STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(12)).await;
        loop {
            let cfg = load_config();
            let interval_secs = cfg.poll_interval_minutes.clamp(1, 60) * 60;
            let seamless_on = crate::ide_bridge::is_seamless_auto_switch_enabled();
            if cfg.enabled || seamless_on || cfg.quota_alert_enabled {
                if let Ok(accs) = accounts::list_accounts_sync() {
                    if let Some(current) = accs.iter().find(|a| a.is_current) {
                        let _ = accounts::refresh_single_account_quota(current.id.clone()).await;
                    }
                }
                let _ = evaluate_and_run_if_enabled(Some(&app), false).await;
            }
            tokio::time::sleep(std::time::Duration::from_secs(interval_secs)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::{CreditInfo, ModelQuota, QuotaData, TokenData};

    fn mk_acc(id: &str, email: &str, is_current: bool, buckets: &[(&str, i32)], credits: Option<f64>) -> AntigravityAccount {
        let mut models: Vec<ModelQuota> = buckets
            .iter()
            .map(|(n, pct)| ModelQuota {
                name: (*n).to_string(),
                display_name: None,
                percentage: *pct,
                reset_time: String::new(),
                summary_exhausted: Some(*pct <= 0),
                max_tokens: None,
                max_output_tokens: None,
            })
            .collect();
        models.push(ModelQuota {
            name: "gemini-4-argon-high".into(),
            display_name: None,
            percentage: buckets.first().map(|b| b.1).unwrap_or(100),
            reset_time: String::new(),
            summary_exhausted: None,
            max_tokens: None,
            max_output_tokens: None,
        });
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
                access_token: "t".into(),
                refresh_token: "r".into(),
                expires_in: 3600,
                expiry_timestamp: 9999999999,
                token_type: "Bearer".into(),
                email: Some(email.into()),
                project_id: None,
                is_gcp_tos: None,
                id_token: None,
            },
            quota: Some(QuotaData {
                models,
                last_updated: 1,
                is_forbidden: false,
                subscription_tier: Some("g1-pro-tier".into()),
                credits: credits
                    .map(|c| {
                        vec![CreditInfo {
                            credit_type: "AI_CREDITS".into(),
                            credit_amount: Some(c.to_string()),
                            minimum_credit_amount_for_usage: None,
                        }]
                    })
                    .unwrap_or_default(),
                is_gcp_tos: None,
                project_id: None,
                quota_summary_stale: false,
                quota_summary_updated_at: None,
            }),
            quota_error: None,
            created_at: 1,
            last_used: 1,
            last_switched_at: None,
            is_current,
        }
    }

    #[test]
    fn auto_switch_triggers_on_any_group_split_window_credits_and_scope() {
        let mut cfg = AutoSwitchConfig::default();
        cfg.enabled = true;
        cfg.threshold = 5;

        let a1 = mk_acc(
            "a1",
            "cur@gmail.com",
            true,
            &[("gemini-5h", 4), ("gemini-weekly", 80), ("3p-5h", 90)],
            Some(500.0),
        );
        let a2 = mk_acc(
            "a2",
            "low@gmail.com",
            false,
            &[("gemini-5h", 30), ("gemini-weekly", 60), ("3p-5h", 40)],
            Some(500.0),
        );
        let a3 = mk_acc(
            "a3",
            "best@gmail.com",
            false,
            &[("gemini-5h", 95), ("gemini-weekly", 90), ("3p-5h", 85)],
            Some(500.0),
        );

        let (plan, _) = evaluate_accounts_in_memory(&cfg, &[a1.clone(), a2.clone(), a3.clone()]);
        assert_eq!(plan.as_ref().map(|p| p.0.as_str()), Some("a3"));

        // Selected accounts scope excludes a3 -> picks a2
        cfg.account_scope_mode = "selected_accounts".into();
        cfg.selected_account_ids = vec!["a1".into(), "a2".into()];
        let (plan_scoped, _) =
            evaluate_accounts_in_memory(&cfg, &[a1.clone(), a2.clone(), a3.clone()]);
        assert_eq!(plan_scoped.as_ref().map(|p| p.0.as_str()), Some("a2"));

        // Split window: weekly threshold 85 triggers on a1 (weekly 80 <= 85) even if 5h threshold is 0
        cfg.account_scope_mode = "all_accounts".into();
        cfg.split_window_thresholds = true;
        cfg.five_hour_threshold = 0;
        cfg.weekly_threshold = 85;
        let (plan_split, _) =
            evaluate_accounts_in_memory(&cfg, &[a1.clone(), a2.clone(), a3.clone()]);
        assert_eq!(plan_split.as_ref().map(|p| p.0.as_str()), Some("a3"));

        // Quota alert fires when <= quota_alert_threshold
        cfg.quota_alert_enabled = true;
        cfg.quota_alert_threshold = 20;
        let (_, alert) = evaluate_accounts_in_memory(&cfg, &[a1]);
        assert!(alert.is_some());
    }
}
