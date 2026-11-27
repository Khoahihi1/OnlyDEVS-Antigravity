import { useCallback, useEffect, useMemo, useState } from "react";
import { AppShell } from "./components/AppShell";
import { UpdateNotification } from "./components/UpdateNotification";
import { AccountsPage } from "./pages/AccountsPage";
import { ApiServicePage } from "./pages/ApiServicePage";
import { PluginsPage } from "./pages/PluginsPage";
import { PromptsPage, type PromptDraft } from "./pages/PromptsPage";
import { SessionsPage } from "./pages/SessionsPage";
import { McpSkillsPage } from "./pages/McpSkillsPage";
import { AboutPage } from "./pages/AboutPage";
import { agxInvoke } from "./platform/invoke";
import type {
  AntigravityAccount,
  ApiServiceStatus,
  AppLanguage,
  AppTab,
  AppTheme,
  AppThemeColor,
  InstructionMode,
  McpSkillsState,
  OAuthPrepareResult,
  OverviewState,
  PluginEntry,
  PromptCategory,
  RefreshStats,
  SavedPrompt,
  SessionScanResult,
  SessionStepItem,
  SwitchHistoryItem,
  TokenUsageSummary,
  TotpCodeResult,
  UpdateCheckResult,
  VersionJumpInfo,
} from "./types";

const APP_VERSION = "1.2.17";

const VALID_LANGS: AppLanguage[] = ["vi", "en", "zh-CN", "zh-TW", "ja", "ko", "ru", "de"];
const VALID_THEME_COLORS: AppThemeColor[] = [
  "default",
  "nord",
  "tokyo-night",
  "catppuccin",
  "gruvbox",
  "everforest",
  "oled",
];

function readSavedLang(): AppLanguage {
  try {
    const raw = window.localStorage.getItem("onlydevs.lang") as AppLanguage | null;
    if (raw && VALID_LANGS.includes(raw)) return raw;
  } catch {
    // ignore
  }
  return "vi";
}

function readSavedTheme(): AppTheme {
  try {
    const raw = window.localStorage.getItem("onlydevs.theme");
    if (raw === "light" || raw === "dark") return raw;
  } catch {
    // ignore
  }
  return "dark";
}

function readSavedThemeColor(): AppThemeColor {
  try {
    const raw = window.localStorage.getItem("onlydevs.themeColor") as AppThemeColor | null;
    if (raw && VALID_THEME_COLORS.includes(raw)) return raw;
  } catch {
    // ignore
  }
  return "default";
}

export default function App() {
  const [tab, setTab] = useState<AppTab>("accounts");
  const [lang, setLang] = useState<AppLanguage>(readSavedLang);
  const [theme, setTheme] = useState<AppTheme>(readSavedTheme);
  const [themeColor, setThemeColor] = useState<AppThemeColor>(readSavedThemeColor);
  const [loading, setLoading] = useState(false);

  const [overview, setOverview] = useState<OverviewState | null>(null);
  const [accounts, setAccounts] = useState<AntigravityAccount[]>([]);
  const [tokenUsage, setTokenUsage] = useState<TokenUsageSummary | null>(null);
  const [switchHistory, setSwitchHistory] = useState<SwitchHistoryItem[]>([]);
  const [plugins, setPlugins] = useState<PluginEntry[]>([]);
  const [prompts, setPrompts] = useState<SavedPrompt[]>([]);
  const [categories, setCategories] = useState<PromptCategory[]>([]);
  const [configRaw, setConfigRaw] = useState("{}");
  const [agentsMd, setAgentsMd] = useState("");
  const [instructionMode, setInstructionMode] = useState<InstructionMode>("replace");
  const [sessionScan, setSessionScan] = useState<SessionScanResult | null>(null);
  const [mcpSkills, setMcpSkills] = useState<McpSkillsState | null>(null);
  const [apiServiceStatus, setApiServiceStatus] = useState<ApiServiceStatus | null>(null);

  const [toast, setToast] = useState<string | null>(null);
  const [pendingUpdate, setPendingUpdate] = useState<UpdateCheckResult | null>(null);

  const showToast = useCallback((message: string) => {
    setToast(message);
    window.setTimeout(() => setToast(null), 3400);
  }, []);

  const refreshAll = useCallback(async () => {
    setLoading(true);
    try {
      const results = await Promise.allSettled([
        agxInvoke<OverviewState>("get_overview_state"),
        agxInvoke<AntigravityAccount[]>("list_accounts"),
        agxInvoke<TokenUsageSummary>("get_antigravity_token_usage"),
        agxInvoke<SwitchHistoryItem[]>("get_switch_history"),
        agxInvoke<PluginEntry[]>("list_plugins"),
        agxInvoke<SavedPrompt[]>("list_prompts"),
        agxInvoke<PromptCategory[]>("list_prompt_categories"),
        agxInvoke<string>("get_config_raw"),
        agxInvoke<string>("get_agents_md"),
        agxInvoke<string>("get_instruction_mode"),
        agxInvoke<SessionScanResult>("list_sessions"),
        agxInvoke<McpSkillsState>("list_mcp_and_skills"),
        agxInvoke<ApiServiceStatus>("api_service_get_status"),
      ]);

      const [
        ovRes,
        accsRes,
        usageRes,
        histRes,
        plugRes,
        prRes,
        catsRes,
        cfgRes,
        agentsRes,
        modeRes,
        sessRes,
        msRes,
        apiStRes,
      ] = results;

      if (ovRes.status === "fulfilled") setOverview(ovRes.value);
      if (accsRes.status === "fulfilled") setAccounts(accsRes.value);
      if (usageRes.status === "fulfilled") setTokenUsage(usageRes.value);
      if (histRes.status === "fulfilled") setSwitchHistory(histRes.value);
      if (plugRes.status === "fulfilled") setPlugins(plugRes.value);
      if (prRes.status === "fulfilled") setPrompts(prRes.value);
      if (catsRes.status === "fulfilled") setCategories(catsRes.value);
      if (cfgRes.status === "fulfilled") setConfigRaw(cfgRes.value);
      if (agentsRes.status === "fulfilled") setAgentsMd(agentsRes.value);
      if (modeRes.status === "fulfilled") {
        setInstructionMode(modeRes.value === "append" ? "append" : "replace");
      }
      if (sessRes.status === "fulfilled") setSessionScan(sessRes.value);
      if (msRes.status === "fulfilled") setMcpSkills(msRes.value);
      if (apiStRes.status === "fulfilled") setApiServiceStatus(apiStRes.value);
      else setApiServiceStatus(null);

      // Surface first non-API error without blocking any other tab/feature
      const firstErr = results
        .slice(0, 12)
        .find((r): r is PromiseRejectedResult => r.status === "rejected");
      if (firstErr) {
        const err = firstErr.reason;
        showToast(err instanceof Error ? err.message : String(err));
      }
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    } finally {
      setLoading(false);
    }
  }, [showToast]);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    try {
      window.localStorage.setItem("onlydevs.theme", theme);
    } catch {
      // ignore
    }
  }, [theme]);

  useEffect(() => {
    document.documentElement.dataset.themeColor = themeColor;
    try {
      window.localStorage.setItem("onlydevs.themeColor", themeColor);
    } catch {
      // ignore
    }
  }, [themeColor]);

  useEffect(() => {
    document.documentElement.lang = lang;
    try {
      window.localStorage.setItem("onlydevs.lang", lang);
    } catch {
      // ignore
    }
  }, [lang]);

  useEffect(() => {
    void refreshAll();
  }, [refreshAll]);

  // Listen to System Tray API toggle & background quota auto-refresh events
  useEffect(() => {
    let unlistenStatus: (() => void) | null = null;
    let unlistenQuota: (() => void) | null = null;
    void (async () => {
      try {
        const { listen } = await import("@tauri-apps/api/event");
        unlistenStatus = await listen<ApiServiceStatus>(
          "api-service-status-changed",
          (ev) => {
            if (ev.payload) {
              setApiServiceStatus(ev.payload);
            }
          },
        );
        unlistenQuota = await listen<number>("api-pool-quota-updated", () => {
          void refreshAll();
        });
      } catch {
        // non-Tauri web preview mode
      }
    })();
    return () => {
      unlistenStatus?.();
      unlistenQuota?.();
    };
  }, [refreshAll]);

  // Startup version jump notice + mandatory update check on launch & every 15m
  useEffect(() => {
    let active = true;
    const runMandatoryCheck = async () => {
      try {
        const res = await agxInvoke<UpdateCheckResult>("check_for_updates", {
          force: true,
        });
        if (active && res.hasUpdate) {
          setPendingUpdate(res);
        }
      } catch {
        // offline or GitHub unreachable — do not block startup
      }
    };

    void (async () => {
      try {
        const jump = await agxInvoke<VersionJumpInfo | null>("check_version_jump");
        if (active && jump) {
          showToast(
            `Đã cập nhật OnlyDEVS từ v${jump.previousVersion} → v${jump.currentVersion}`,
          );
        }
      } catch {
        // ignore
      }
      await runMandatoryCheck();
    })();

    const timer = window.setInterval(() => {
      void runMandatoryCheck();
    }, 15 * 60 * 1000);

    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, [showToast]);

  const handleToggleAccountInApiPool = useCallback(
    async (accountId: string) => {
      const st =
        apiServiceStatus ??
        (await agxInvoke<ApiServiceStatus>("api_service_get_status"));
      const cfg = st.config;
      let nextIds: string[];
      let added: boolean;

      if (cfg.memberAccountIds.length === 0 && cfg.useAllAccountsWhenEmpty) {
        // Currently implicitly using all accounts -> switching to explicit list without this account
        nextIds = accounts.map((a) => a.id).filter((id) => id !== accountId);
        added = false;
      } else if (cfg.memberAccountIds.includes(accountId)) {
        nextIds = cfg.memberAccountIds.filter((id) => id !== accountId);
        added = false;
      } else {
        nextIds = [...cfg.memberAccountIds, accountId];
        added = true;
      }

      const nextCfg = {
        ...cfg,
        memberAccountIds: nextIds,
        useAllAccountsWhenEmpty: false,
      };
      await agxInvoke("api_service_save_config", { config: nextCfg });
      const refreshed = await agxInvoke<ApiServiceStatus>("api_service_get_status");
      setApiServiceStatus(refreshed);
      const email = accounts.find((a) => a.id === accountId)?.email ?? accountId;
      showToast(
        added
          ? `Đã thêm ${email} vào API Pool`
          : `Đã gỡ ${email} khỏi API Pool`,
      );
    },
    [accounts, apiServiceStatus, showToast],
  );

  const navBadges = useMemo(() => {
    const enabledPlugs = plugins.filter((p) => p.enabled).length;
    const enabledPrompts = prompts.filter((p) => p.enabled).length;
    const totalSess = sessionScan?.totalSessions ?? 0;
    const totalMcpSkills =
      (mcpSkills?.mcpServers.filter((m) => m.enabled).length ?? 0) +
      (mcpSkills?.skills.filter((s) => s.enabled).length ?? 0);

    return {
      accounts: accounts.length,
      "api-service": apiServiceStatus?.running ? "ON" : "OFF",
      plugins: `${enabledPlugs}/${plugins.length}`,
      prompts: enabledPrompts > 0 ? "ON" : prompts.length,
      sessions: totalSess,
      "mcp-skills": totalMcpSkills,
    };
  }, [accounts, apiServiceStatus, plugins, prompts, sessionScan, mcpSkills]);

  const page = (() => {
    switch (tab) {
      case "accounts":
        return (
          <AccountsPage
            lang={lang}
            accounts={accounts}
            tokenUsage={tokenUsage}
            switchHistory={switchHistory}
            loading={loading}
            apiServiceStatus={apiServiceStatus}
            onToggleApiPool={handleToggleAccountInApiPool}
            onRefreshAllData={refreshAll}
            onImportLocalAccount={async () => {
              const acc = await agxInvoke<AntigravityAccount>("import_local_account");
              showToast(`Đã đồng bộ tài khoản: ${acc.email}`);
              await refreshAll();
            }}
            onAddByToken={async (refreshTokenInput) => {
              const added = await agxInvoke<AntigravityAccount[]>("add_accounts_by_token", {
                refreshTokenInput,
              });
              showToast(`Đã thêm ${added.length} tài khoản`);
              await refreshAll();
            }}
            onImportContent={async (content) => {
              const added = await agxInvoke<AntigravityAccount[]>("import_accounts_content", {
                content,
              });
              showToast(`Đã import ${added.length} tài khoản`);
              await refreshAll();
            }}
            onExportAccounts={(accountIds, includeSensitive) =>
              agxInvoke<string>("export_accounts", { accountIds, includeSensitive })
            }
            onFetchQuota={async (accountId) => {
              const acc = await agxInvoke<AntigravityAccount>("fetch_account_quota", {
                accountId,
              });
              showToast(`Đã cập nhật Quota: ${acc.email}`);
              await refreshAll();
            }}
            onRefreshAllQuotas={async () => {
              const stats = await agxInvoke<RefreshStats>("refresh_all_quotas");
              showToast(
                `Quota: ${stats.success}/${stats.total} thành công${
                  stats.failed > 0 ? ` (${stats.failed} lỗi)` : ""
                }`,
              );
              await refreshAll();
              return stats;
            }}
            onSwitchAccount={async (accountId, switchTarget, launch, customPath, workingDir) => {
              const acc = await agxInvoke<AntigravityAccount>("switch_account", {
                accountId,
                switchTarget,
                launch,
                customPath: customPath ?? null,
                workingDir: workingDir ?? null,
              });
              let seamlessOk = false;
              if (!launch && switchTarget !== "cli") {
                try {
                  const st = await agxInvoke<{ connectedCount?: number; settings?: { seamlessPriority?: boolean } }>(
                    "get_ide_bridge_status",
                  );
                  seamlessOk = Boolean(st?.connectedCount && st?.settings?.seamlessPriority !== false);
                } catch {
                  // ignore
                }
              }
              showToast(
                launch
                  ? `Đã chuyển & mở: ${acc.email}`
                  : seamlessOk
                    ? `Đã chuyển nóng vào Antigravity IDE (không restart): ${acc.email}`
                    : `Đã chuyển tài khoản: ${acc.email}`,
              );
              await refreshAll();
            }}
            onUpdateMetadata={async (payload) => {
              await agxInvoke("update_account_metadata", payload);
              showToast("Đã lưu thông tin Vault & Tag");
              await refreshAll();
            }}
            onDeleteAccounts={async (accountIds) => {
              const count = await agxInvoke<number>("delete_accounts", { accountIds });
              showToast(`Đã xóa ${count} tài khoản`);
              await refreshAll();
            }}
            onPrepareOAuth={() => agxInvoke<OAuthPrepareResult>("prepare_oauth_session")}
            onStartOAuthLogin={async () => {
              const acc = await agxInvoke<AntigravityAccount>("start_oauth_login");
              showToast(`OAuth thành công: ${acc.email}`);
              await refreshAll();
            }}
            onCompleteOAuthLogin={async () => {
              const acc = await agxInvoke<AntigravityAccount>("complete_oauth_login");
              showToast(`OAuth thành công: ${acc.email}`);
              await refreshAll();
            }}
            onSubmitOAuthCallback={async (callbackUrl) => {
              await agxInvoke<void>("submit_oauth_callback_url", {
                callbackUrl,
              });
            }}
            onCancelOAuthLogin={async () => {
              await agxInvoke<void>("cancel_oauth_login");
            }}
            onGenerateTotp={(secret) =>
              agxInvoke<TotpCodeResult>("generate_totp_code", { secret })
            }
            onClearSwitchHistory={async () => {
              await agxInvoke("clear_switch_history");
              showToast("Đã xóa lịch sử chuyển đổi");
              await refreshAll();
            }}
          />
        );

      case "api-service":
        return (
          <ApiServicePage
            lang={lang}
            accounts={accounts}
            onRefreshAccounts={refreshAll}
            onNavigateAccounts={() => setTab("accounts")}
            onStatusChange={setApiServiceStatus}
          />
        );

      case "plugins":
        return (
          <PluginsPage
            lang={lang}
            plugins={plugins}
            overview={overview}
            configRaw={configRaw}
            loading={loading}
            onRefresh={refreshAll}
            onInstallBundled={async (customPath) => {
              const msg = await agxInvoke<string>("install_filter_bypass_plugin", {
                sourcePath: customPath ?? null,
              });
              showToast(msg);
              await refreshAll();
            }}
            onInstallFromFolder={async (sourcePath, pluginId) => {
              const msg = await agxInvoke<string>("install_plugin_from_folder", {
                sourcePath,
                pluginId: pluginId ?? null,
              });
              showToast(msg);
              await refreshAll();
            }}
            onInstallFromZip={async (payload) => {
              const msg = await agxInvoke<string>("install_plugin_from_zip", payload);
              showToast(msg);
              await refreshAll();
            }}
            onInstallFromGit={async (repoUrl, pluginId) => {
              const msg = await agxInvoke<string>("install_plugin_from_git", {
                repoUrl,
                pluginId: pluginId ?? null,
              });
              showToast(msg);
              await refreshAll();
            }}
            onCreateCustomPlugin={async (payload) => {
              const msg = await agxInvoke<string>("create_custom_plugin", payload);
              showToast(msg);
              await refreshAll();
            }}
            onUninstallPlugin={async (pluginId) => {
              const msg = await agxInvoke<string>("uninstall_plugin", { pluginId });
              showToast(msg);
              await refreshAll();
            }}
            onToggle={async (id, enabled) => {
              await agxInvoke("set_plugin_enabled", { pluginId: id, enabled });
              showToast(`${id}: ${enabled ? "Enabled" : "Disabled"}`);
              await refreshAll();
            }}
            onRunSelfTest={async () => {
              const res = await agxInvoke<string>("run_filter_bypass_test");
              showToast("Self-test PASSED");
              return res;
            }}
            onGetHooksRaw={(pluginId) =>
              agxInvoke<string>("get_plugin_hooks_raw", { pluginId })
            }
            onSaveHooksRaw={async (pluginId, raw) => {
              await agxInvoke("save_plugin_hooks_raw", { pluginId, raw });
              showToast("Đã lưu hooks.json");
              await refreshAll();
            }}
            onSaveConfigRaw={async (raw) => {
              await agxInvoke("save_config_raw", { raw });
              showToast("Đã lưu config.json");
              await refreshAll();
            }}
          />
        );

      case "prompts":
        return (
          <PromptsPage
            lang={lang}
            prompts={prompts}
            categories={categories}
            instructionMode={instructionMode}
            agentsPreview={agentsMd}
            loading={loading}
            onRefresh={refreshAll}
            onSyncSeedPrompts={async () => {
              const added = await agxInvoke<number>("sync_seed_prompts");
              showToast(
                added > 0
                  ? `Đã đồng bộ thêm ${added} prompt từ thư mục Prompt`
                  : "Đã đồng bộ nội dung mới nhất từ thư mục Prompt",
              );
            }}
            onSavePrompt={async (draft: PromptDraft) => {
              await agxInvoke("upsert_prompt", {
                id: draft.id ?? null,
                category: draft.category,
                title: draft.title,
                filename: draft.filename,
                description: draft.description,
                content: draft.content,
                enabled: draft.enabled,
              });
              showToast(
                draft.enabled
                  ? `Đã lưu & ghi "${draft.title}" vào ~/.gemini/config/AGENTS.md`
                  : `Đã lưu prompt "${draft.title}"`,
              );
              await refreshAll();
            }}
            onDelete={async (id) => {
              await agxInvoke("delete_prompt", { id });
              showToast("Đã xóa prompt");
              await refreshAll();
            }}
            onToggle={async (id, enabled) => {
              await agxInvoke("set_prompt_enabled", { id, enabled });
              const target = prompts.find((p) => p.id === id);
              showToast(
                enabled
                  ? `Đã áp dụng "${target?.title ?? id}" vào ~/.gemini/config/AGENTS.md`
                  : `Đã tắt "${target?.title ?? id}"`,
              );
              await refreshAll();
            }}
            onSaveAgents={async (content) => {
              await agxInvoke("save_agents_md", { content });
              showToast("Đã lưu ~/.gemini/config/AGENTS.md");
              await refreshAll();
            }}
            onModeChange={async (mode) => {
              await agxInvoke("set_instruction_mode", { mode });
              setInstructionMode(mode);
              showToast(
                mode === "replace"
                  ? "Chế độ: Thay thế trực tiếp vào ~/.gemini/config/AGENTS.md"
                  : "Chế độ: Giữ rule gốc + nối thêm vào AGENTS.md",
              );
              await refreshAll();
            }}
            onCreateCategory={async (name) => {
              await agxInvoke("create_prompt_category", { name });
              await refreshAll();
            }}
            onDeleteCategory={async (name) => {
              await agxInvoke("delete_prompt_category", { name });
              await refreshAll();
            }}
          />
        );

      case "sessions":
        return (
          <SessionsPage
            lang={lang}
            scan={sessionScan}
            loading={loading}
            onRefresh={refreshAll}
            onSyncSessions={async () => {
              const msg = await agxInvoke<string>("sync_sessions");
              showToast(msg);
              await refreshAll();
            }}
            onDeleteSessions={async (ids) => {
              const msg = await agxInvoke<string>("delete_sessions", { ids });
              showToast(msg);
              await refreshAll();
            }}
            onExportSessions={async (ids) => {
              const outPath = await agxInvoke<string>("export_sessions_markdown", { ids });
              showToast(`Đã xuất Markdown: ${outPath}`);
            }}
            onGetTranscript={(sessionId) =>
              agxInvoke<SessionStepItem[]>("get_session_transcript", { sessionId })
            }
          />
        );

      case "mcp-skills":
        return (
          <McpSkillsPage
            lang={lang}
            state={mcpSkills}
            loading={loading}
            onRefresh={refreshAll}
            onToggleMcp={async (id, enabled) => {
              await agxInvoke("set_mcp_enabled", { id, enabled });
              showToast(`MCP ${id}: ${enabled ? "Enabled" : "Disabled"}`);
              await refreshAll();
            }}
            onUpsertMcp={async (payload) => {
              await agxInvoke("upsert_mcp_server", payload);
              showToast(`Đã lưu MCP server ${payload.id}`);
              await refreshAll();
            }}
            onDeleteMcp={async (id) => {
              await agxInvoke("delete_mcp_server", { id });
              showToast(`Đã xóa MCP server ${id}`);
              await refreshAll();
            }}
            onGetMcpConfigRaw={() => agxInvoke<string>("get_mcp_config_raw")}
            onSaveMcpConfigRaw={async (raw) => {
              await agxInvoke("save_mcp_config_raw", { raw });
              showToast("Đã lưu mcp_config.json");
              await refreshAll();
            }}
            onToggleSkill={async (id, enabled) => {
              await agxInvoke("set_skill_enabled", { id, enabled });
              showToast(`Skill ${id}: ${enabled ? "Enabled" : "Disabled"}`);
              await refreshAll();
            }}
            onGetSkillContent={(id, source) =>
              agxInvoke<string>("get_skill_content", { id, source })
            }
            onUpsertSkill={async (payload) => {
              await agxInvoke("upsert_skill", payload);
              showToast(`Đã lưu Skill ${payload.id}`);
              await refreshAll();
            }}
            onDeleteSkill={async (id) => {
              await agxInvoke("delete_skill", { id });
              showToast(`Đã xóa Skill ${id}`);
              await refreshAll();
            }}
            onSaveNote={async (kind, id, note) => {
              await agxInvoke("save_item_note", { kind, id, note });
              showToast("Đã lưu ghi chú");
              await refreshAll();
            }}
          />
        );

      case "about":
        return (
          <AboutPage
            lang={lang}
            overview={overview}
            appVersion={APP_VERSION}
            onRefresh={refreshAll}
            onOpenUpdateModal={(res) => setPendingUpdate(res)}
          />
        );
    }
  })();

  return (
    <>
      <AppShell
        activeTab={tab}
        onTabChange={setTab}
        lang={lang}
        onLangChange={setLang}
        theme={theme}
        onToggleTheme={() => setTheme((t) => (t === "dark" ? "light" : "dark"))}
        themeColor={themeColor}
        onThemeColorChange={setThemeColor}
        appVersion={APP_VERSION}
        badges={navBadges}
      >
        {page}
      </AppShell>
      <UpdateNotification
        lang={lang}
        update={pendingUpdate}
        onClose={() => setPendingUpdate(null)}
        onSkipped={(ver) => showToast(`Đã bỏ qua phiên bản v${ver}`)}
      />
      {toast && <div className="agx-toast">{toast}</div>}
    </>
  );
}
