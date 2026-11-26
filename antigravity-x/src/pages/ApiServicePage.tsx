import { useCallback, useEffect, useMemo, useState } from "react";
import {
  Activity,
  AlertTriangle,
  BarChart3,
  BookOpen,
  Check,
  Clock,
  Code2,
  Copy,
  Cpu,
  Eye,
  EyeOff,
  FolderPlus,
  KeyRound,
  Layers,
  Loader2,
  Play,
  Plus,
  Power,
  Radio,
  RefreshCw,
  RotateCcw,
  Save,
  Search,
  Send,
  ShieldAlert,
  ShieldCheck,
  SlidersHorizontal,
  Sparkles,
  Trash2,
  Users,
  X,
  Zap,
} from "lucide-react";
import type {
  AntigravityAccount,
  ApiAvailableModel,
  ApiKeyRecord,
  ApiRequestLogItem,
  ApiServiceConfig,
  ApiServiceStatus,
  ApiServiceUsageSummary,
  ApiTestChatResult,
  AppLanguage,
  LanDiagnostics,
  ModelAliasRule,
  RefreshStats,
} from "../types";
import { agxInvoke } from "../platform/invoke";
import {
  formatExactNumber,
  formatTokensShort,
} from "../components/AntigravityUsageTrend";
import {
  formatResetCountdown,
  getAntigravityOverallScore,
  getAntigravityQuotaDisplayItems,
  getQuotaTone,
  normalizeTierBadge,
} from "../components/AntigravityQuotaSection";

type ApiServicePageProps = {
  lang: AppLanguage;
  accounts: AntigravityAccount[];
  onRefreshAccounts: () => Promise<void>;
  onNavigateAccounts: () => void;
  onStatusChange?: (status: ApiServiceStatus | null) => void;
};

type SubTab = "overview" | "models" | "setup" | "logs" | "usage";
type SetupClient = "claude-code" | "codex-cli" | "cursor" | "curl" | "python";

function formatTime(ts?: number | null): string {
  if (!ts) return "—";
  const d = new Date(ts * 1000);
  if (Number.isNaN(d.getTime())) return "—";
  return d.toLocaleString(undefined, {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

function maskKey(key: string, visible: boolean): string {
  if (visible || key.length <= 16) return key;
  return `${key.slice(0, 14)}••••••••••••${key.slice(-4)}`;
}

type AccountModelProfileKind = "full_nextgen" | "classic" | "unknown";

function classifyAccountModelProfile(acc: AntigravityAccount): {
  kind: AccountModelProfileKind;
  summary: string;
} {
  const models = acc.quota?.models ?? [];
  if (models.length === 0) {
    return { kind: "unknown", summary: "" };
  }
  let isNextgen = false;
  let hasReal = false;
  let g5h: number | null = null;
  let c5h: number | null = null;
  let cWk: number | null = null;
  let argonWk: number | null = null;

  for (const m of models) {
    const n = m.name.trim().toLowerCase();
    if (
      n === "gemini-argon-wtu-weekly" ||
      n === "argon:weekly" ||
      n === "gemini-carbon-wtu-weekly" ||
      n === "carbon:weekly" ||
      n.startsWith("gemini-4-argon") ||
      n.startsWith("gemini-4-neon") ||
      n.startsWith("gemini-4-carbon") ||
      n.startsWith("claude-sonnet-5-5") ||
      n.startsWith("claude-opus-5-5")
    ) {
      isNextgen = true;
    }
    if (
      !n.includes(":") &&
      !n.endsWith("-5h") &&
      !n.endsWith("-weekly") &&
      !n.startsWith("tab_") &&
      !n.startsWith("chat_") &&
      !n.includes("wtu")
    ) {
      hasReal = true;
    }
    if (n === "gemini-5h" || n === "gemini:5h") g5h = m.percentage;
    if (n === "3p-5h" || n === "claude:5h") c5h = m.percentage;
    if (n === "3p-weekly" || n === "claude:weekly") cWk = m.percentage;
    if (n === "gemini-argon-wtu-weekly" || n === "argon:weekly") argonWk = m.percentage;
  }

  const cEff = c5h != null && cWk != null ? Math.min(c5h, cWk) : (c5h ?? cWk);
  if (isNextgen) {
    const parts: string[] = [];
    if (g5h != null) parts.push(`G4 ${g5h}%`);
    if (cEff != null) parts.push(`C5.5 ${cEff}%`);
    if (argonWk != null) parts.push(`Argon ${argonWk}%`);
    return { kind: "full_nextgen", summary: parts.join(" · ") };
  }
  if (hasReal) {
    const parts: string[] = [];
    if (g5h != null) parts.push(`G3.8 ${g5h}%`);
    if (cEff != null) parts.push(`C4.6 ${cEff}%`);
    return { kind: "classic", summary: parts.join(" · ") };
  }
  return { kind: "unknown", summary: "" };
}

export function ApiServicePage({
  lang,
  accounts,
  onRefreshAccounts,
  onNavigateAccounts,
  onStatusChange,
}: ApiServicePageProps) {
  const vi = lang === "vi";

  const [subTab, setSubTab] = useState<SubTab>("overview");
  const [status, setStatus] = useState<ApiServiceStatus | null>(null);
  const [models, setModels] = useState<ApiAvailableModel[]>([]);
  const [logs, setLogs] = useState<ApiRequestLogItem[]>([]);
  const [usage, setUsage] = useState<ApiServiceUsageSummary | null>(null);
  const [usageRange, setUsageRange] = useState<"today" | "7d" | "30d" | "all">("30d");

  const [busyKey, setBusyKey] = useState<string | null>(null);
  const [banner, setBanner] = useState<{ tone: "ok" | "err"; text: string } | null>(null);
  const [copiedKey, setCopiedKey] = useState<string | null>(null);
  const [revealMasterKey, setRevealMasterKey] = useState(false);
  const [lanDiag, setLanDiag] = useState<LanDiagnostics | null>(null);

  // Editable config draft
  const [cfgDraft, setCfgDraft] = useState<ApiServiceConfig | null>(null);

  // Modals
  const [memberModalOpen, setMemberModalOpen] = useState(false);
  const [memberDraftIds, setMemberDraftIds] = useState<Set<string>>(new Set());
  const [useAllWhenEmptyDraft, setUseAllWhenEmptyDraft] = useState(true);

  const [keyModalOpen, setKeyModalOpen] = useState(false);
  const [newKeyLabel, setNewKeyLabel] = useState("");
  const [newKeyModels, setNewKeyModels] = useState("");
  const [newKeyProfile, setNewKeyProfile] = useState<"all" | "classic" | "full_nextgen">("all");
  const [newKeyRpm, setNewKeyRpm] = useState("0");
  const [newKeyDailyTokens, setNewKeyDailyTokens] = useState("0");
  const [newKeyExpireDays, setNewKeyExpireDays] = useState("0");
  const [newKeyPinned, setNewKeyPinned] = useState<Set<string>>(new Set());

  const [addRulePattern, setAddRulePattern] = useState("");
  const [addRuleTarget, setAddRuleTarget] = useState("gemini-pro-agent");

  // Models filter
  const [modelSearch, setModelSearch] = useState("");
  const [modelFamilyFilter, setModelFamilyFilter] = useState("ALL");

  // Setup & Playground
  const [setupClient, setSetupClient] = useState<SetupClient>("claude-code");
  const [testModel, setTestModel] = useState("gemini-3-flash-agent");
  const [testPrompt, setTestPrompt] = useState(
    vi ? "Chào bạn! Trả lời ngắn gọn 1 câu để kiểm tra kết nối Antigravity API." : "Hello! Reply in 1 short sentence to verify the Antigravity API router.",
  );
  const [testResult, setTestResult] = useState<ApiTestChatResult | null>(null);

  // Logs filter & detail
  const [logSearch, setLogSearch] = useState("");
  const [logStatusFilter, setLogStatusFilter] = useState<"all" | "ok" | "err">("all");
  const [autoRefreshLogs, setAutoRefreshLogs] = useState(true);
  const [inspectLog, setInspectLog] = useState<ApiRequestLogItem | null>(null);

  const flash = (tone: "ok" | "err", text: string) => {
    setBanner({ tone, text });
    window.setTimeout(() => setBanner(null), 4500);
  };

  const copyText = async (text: string, id: string) => {
    try {
      await navigator.clipboard.writeText(text);
      setCopiedKey(id);
      window.setTimeout(() => setCopiedKey(null), 1600);
    } catch {
      // ignore
    }
  };

  const loadAll = useCallback(async () => {
    try {
      const [st, mdl, lg, usg] = await Promise.all([
        agxInvoke<ApiServiceStatus>("api_service_get_status"),
        agxInvoke<ApiAvailableModel[]>("api_service_list_models"),
        agxInvoke<ApiRequestLogItem[]>("api_service_list_logs", { limit: 250 }),
        agxInvoke<ApiServiceUsageSummary>("api_service_get_usage", { range: usageRange }),
      ]);
      setStatus(st);
      setCfgDraft(st.config);
      setModels(mdl ?? []);
      setLogs(lg ?? []);
      setUsage(usg ?? null);
      onStatusChange?.(st);
    } catch (err) {
      flash("err", err instanceof Error ? err.message : String(err));
    }
  }, [onStatusChange, usageRange]);

  useEffect(() => {
    void loadAll();
  }, [loadAll]);

  useEffect(() => {
    if (status?.config.accessScope !== "lan") {
      setLanDiag(null);
      return;
    }
    let cancelled = false;
    void agxInvoke<LanDiagnostics>("api_service_lan_diagnostics", {
      port: status.port,
    })
      .then((diag) => {
        if (!cancelled) setLanDiag(diag);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [status?.config.accessScope, status?.port]);

  useEffect(() => {
    if (!autoRefreshLogs || (subTab !== "logs" && subTab !== "overview")) return;
    const t = window.setInterval(() => {
      void agxInvoke<ApiServiceStatus>("api_service_get_status")
        .then((st) => {
          setStatus(st);
          onStatusChange?.(st);
        })
        .catch(() => {});
      if (subTab === "logs") {
        void agxInvoke<ApiRequestLogItem[]>("api_service_list_logs", { limit: 250 })
          .then((lg) => setLogs(lg ?? []))
          .catch(() => {});
      }
    }, 3500);
    return () => window.clearInterval(t);
  }, [autoRefreshLogs, onStatusChange, subTab]);

  const masterKey = useMemo<ApiKeyRecord | null>(() => {
    if (!status?.keys?.length) return null;
    return status.keys.find((k) => k.isMaster) ?? status.keys[0];
  }, [status]);

  const baseUrl = status?.baseUrl || "http://localhost:57120/v1";
  const anthropicBaseUrl = baseUrl.replace(/\/v1$/, "");
  const activeKeyValue = masterKey?.key || "agt_onlydevs_key";

  const poolAccounts = useMemo(() => {
    if (!status) return [];
    const cfg = status.config;
    return accounts.filter((a) => {
      if (a.disabled || !a.token.refreshToken) return false;
      if (cfg.memberAccountIds.length > 0) {
        return cfg.memberAccountIds.includes(a.id);
      }
      return cfg.useAllAccountsWhenEmpty;
    });
  }, [accounts, status]);

  const cooldownMap = useMemo(() => {
    const m = new Map<string, { scope: string; remainingSeconds: number; reason: string }[]>();
    for (const c of status?.cooldowns ?? []) {
      const list = m.get(c.accountId) ?? [];
      list.push({
        scope: c.scope || "all",
        remainingSeconds: c.remainingSeconds,
        reason: c.reason,
      });
      m.set(c.accountId, list);
    }
    return m;
  }, [status]);

  type QuotaBucketAgg = {
    key: string;
    label: string;
    sumPct: number;
    maxPct: number;
    avgPct: number;
    availableCount: number;
    totalCount: number;
    earliestReset: string;
  };

  type PoolQuotaGroupSummary = {
    accountCount: number;
    readyCount: number;
    tierBreakdown: string;
    totalEquivAcc: number;
    buckets: QuotaBucketAgg[];
  };

  const poolQuotaSummary = useMemo(() => {
    const fullProAccs: AntigravityAccount[] = [];
    const classicProAccs: AntigravityAccount[] = [];
    const otherAccs: AntigravityAccount[] = [];

    for (const acc of poolAccounts) {
      const tier = normalizeTierBadge(acc.quota?.subscriptionTier, acc);
      const profile = classifyAccountModelProfile(acc);
      if (profile.kind === "full_nextgen" && tier !== "FREE") {
        fullProAccs.push(acc);
      } else if (profile.kind === "classic" && tier !== "FREE") {
        classicProAccs.push(acc);
      } else {
        otherAccs.push(acc);
      }
    }

    const buildGroup = (
      list: AntigravityAccount[],
      specs: ReadonlyArray<{ key: string; label: string; optional?: boolean }>,
    ): PoolQuotaGroupSummary => {
      const tierCounts = new Map<string, number>();
      let readyCount = 0;
      const itemMaps = list.map((acc) => {
        const t = normalizeTierBadge(acc.quota?.subscriptionTier, acc);
        tierCounts.set(t, (tierCounts.get(t) ?? 0) + 1);
        if ((cooldownMap.get(acc.id)?.length ?? 0) === 0) {
          readyCount += 1;
        }
        const map = new Map<string, { pct: number; resetTime: string }>();
        for (const item of getAntigravityQuotaDisplayItems(acc)) {
          map.set(item.key, {
            pct: Math.max(0, Math.min(100, item.percentage)),
            resetTime: item.resetTime,
          });
        }
        return map;
      });

      const buckets: QuotaBucketAgg[] = [];
      for (const spec of specs) {
        let sumPct = 0;
        let totalCount = 0;
        let availableCount = 0;
        let earliestMs = Number.POSITIVE_INFINITY;
        let earliestReset = "";

        for (const m of itemMaps) {
          const hit = m.get(spec.key);
          if (!hit) continue;
          totalCount += 1;
          sumPct += hit.pct;
          if (hit.pct > 0) availableCount += 1;
          if (hit.resetTime) {
            const ms = Date.parse(hit.resetTime);
            if (!Number.isNaN(ms) && ms > Date.now() && ms < earliestMs) {
              earliestMs = ms;
              earliestReset = hit.resetTime;
            }
          }
        }
        if (totalCount === 0 && spec.optional) continue;
        const maxPct = totalCount * 100;
        const avgPct = totalCount > 0 ? Math.round(sumPct / totalCount) : 0;
        buckets.push({
          key: spec.key,
          label: spec.label,
          sumPct,
          maxPct,
          avgPct,
          availableCount,
          totalCount,
          earliestReset,
        });
      }

      // Equivalent full-account score across primary 5h buckets (min of available buckets per acc)
      let totalEquivAcc = 0;
      for (const m of itemMaps) {
        const g5 = m.get("gemini:5h")?.pct;
        const c5 = m.get("claude:5h")?.pct;
        const gw = m.get("gemini:weekly")?.pct;
        const cw = m.get("claude:weekly")?.pct;
        const vals = [g5, c5, gw, cw].filter((v): v is number => v != null);
        if (vals.length > 0) {
          totalEquivAcc += vals.reduce((a, b) => a + b, 0) / (vals.length * 100);
        }
      }

      const tierBreakdown = Array.from(tierCounts.entries())
        .map(([k, v]) => `${v} ${k}`)
        .join(" · ");

      return {
        accountCount: list.length,
        readyCount,
        tierBreakdown,
        totalEquivAcc,
        buckets,
      };
    };

    const fullPro = buildGroup(fullProAccs, [
      { key: "gemini:5h", label: "Gemini 4 · 5h" },
      { key: "gemini:weekly", label: "Gemini 4 · Weekly" },
      { key: "claude:5h", label: "Claude 5.5 · 5h" },
      { key: "claude:weekly", label: "Claude 5.5 · Weekly" },
    ]);

    const classicPro = buildGroup(classicProAccs, [
      { key: "gemini:5h", label: "Gemini 3.8 · 5h" },
      { key: "gemini:weekly", label: "Gemini 3.8 · Weekly" },
      { key: "claude:5h", label: "Claude 4.6 · 5h" },
      { key: "claude:weekly", label: "Claude 4.6 · Weekly" },
    ]);

    return {
      fullPro,
      classicPro,
      otherCount: otherAccs.length,
      otherFreeCount: otherAccs.filter(
        (a) => normalizeTierBadge(a.quota?.subscriptionTier) === "FREE",
      ).length,
      otherUnscannedCount: otherAccs.filter((a) => (a.quota?.models?.length ?? 0) === 0).length,
    };
  }, [cooldownMap, poolAccounts]);

  const handleOpenLanFirewall = async () => {
    if (!status) return;
    setBusyKey("lan-fw");
    try {
      const diag = await agxInvoke<LanDiagnostics>("api_service_open_lan_firewall", {
        port: status.port,
      });
      setLanDiag(diag);
      flash(
        diag.firewallRuleExists ? "ok" : "err",
        diag.firewallRuleExists
          ? vi
            ? `Đã mở Windows Firewall cho LAN (TCP ${status.port} + Ping ICMPv4)!`
            : `Opened Windows Firewall for LAN (TCP ${status.port} + ICMPv4 Ping)!`
          : vi
            ? "Đã chạy lệnh mở Firewall nhưng chưa thấy rule — hãy chạy lệnh netsh bằng quyền Admin."
            : "Firewall rule not detected yet — run the netsh command as Administrator.",
      );
    } catch (err) {
      flash("err", err instanceof Error ? err.message : String(err));
    } finally {
      setBusyKey(null);
    }
  };

  const handleRefreshPoolQuotas = async () => {
    setBusyKey("refresh-pool-quota");
    try {
      const stats = await agxInvoke<RefreshStats>("refresh_all_quotas");
      await onRefreshAccounts();
      await loadAll();
      flash(
        "ok",
        vi
          ? `Đã quét lại quota ${stats.success}/${stats.total} tài khoản`
          : `Refreshed quota for ${stats.success}/${stats.total} accounts`,
      );
    } catch (err) {
      flash("err", err instanceof Error ? err.message : String(err));
    } finally {
      setBusyKey(null);
    }
  };

  // Actions
  const handleToggleServer = async () => {
    const isRunning = Boolean(status?.running);
    setBusyKey("power");
    try {
      const next = await agxInvoke<ApiServiceStatus>(
        isRunning ? "api_service_stop" : "api_service_start",
      );
      setStatus(next);
      setCfgDraft(next.config);
      onStatusChange?.(next);
      flash(
        "ok",
        next.running
          ? vi
            ? `Đã khởi động API Router tại ${next.baseUrl}`
            : `API Router running at ${next.baseUrl}`
          : vi
            ? "Đã dừng API Router"
            : "API Router stopped",
      );
    } catch (err) {
      flash("err", err instanceof Error ? err.message : String(err));
    } finally {
      setBusyKey(null);
    }
  };

  const handleRestartServer = async () => {
    setBusyKey("restart");
    try {
      const next = await agxInvoke<ApiServiceStatus>("api_service_restart");
      setStatus(next);
      setCfgDraft(next.config);
      onStatusChange?.(next);
      flash("ok", vi ? `Đã khởi động lại tại ${next.baseUrl}` : `Restarted at ${next.baseUrl}`);
    } catch (err) {
      flash("err", err instanceof Error ? err.message : String(err));
    } finally {
      setBusyKey(null);
    }
  };

  const handleSaveConfig = async (nextCfg: ApiServiceConfig, restartIfRunning = false) => {
    setBusyKey("save-cfg");
    try {
      await agxInvoke<ApiServiceConfig>("api_service_save_config", { config: nextCfg });
      if (restartIfRunning && status?.running) {
        const restarted = await agxInvoke<ApiServiceStatus>("api_service_restart");
        setStatus(restarted);
        setCfgDraft(restarted.config);
        onStatusChange?.(restarted);
      } else {
        await loadAll();
      }
      flash("ok", vi ? "Đã lưu cấu hình API Service" : "Saved API Service config");
    } catch (err) {
      flash("err", err instanceof Error ? err.message : String(err));
    } finally {
      setBusyKey(null);
    }
  };

  const openMemberModal = () => {
    if (!status) return;
    setMemberDraftIds(new Set(status.config.memberAccountIds));
    setUseAllWhenEmptyDraft(status.config.useAllAccountsWhenEmpty);
    setMemberModalOpen(true);
  };

  const handleSaveMembers = async () => {
    if (!cfgDraft) return;
    const nextCfg: ApiServiceConfig = {
      ...cfgDraft,
      memberAccountIds: Array.from(memberDraftIds),
      useAllAccountsWhenEmpty: useAllWhenEmptyDraft,
    };
    await handleSaveConfig(nextCfg, false);
    setMemberModalOpen(false);
  };

  const handleCreateKey = async () => {
    if (!newKeyLabel.trim()) return;
    setBusyKey("create-key");
    try {
      const allowed = newKeyModels
        .split(",")
        .map((s) => s.trim())
        .filter(Boolean);
      const rpmNum = Math.max(0, Math.floor(Number(newKeyRpm) || 0));
      const dailyCapNum = Math.max(0, Math.floor(Number(newKeyDailyTokens) || 0));
      const expDays = Math.max(0, Number(newKeyExpireDays) || 0);
      const expiresAt =
        expDays > 0 ? Math.floor(Date.now() / 1000 + expDays * 86400) : null;
      await agxInvoke("api_service_create_key", {
        label: newKeyLabel.trim(),
        allowedModels: allowed,
        pinnedAccountIds: Array.from(newKeyPinned),
        allowedProfile: newKeyProfile,
        rateLimitRpm: rpmNum,
        dailyTokenLimit: dailyCapNum,
        expiresAt,
      });
      setNewKeyLabel("");
      setNewKeyModels("");
      setNewKeyProfile("all");
      setNewKeyRpm("0");
      setNewKeyDailyTokens("0");
      setNewKeyExpireDays("0");
      setNewKeyPinned(new Set());
      setKeyModalOpen(false);
      await loadAll();
      flash("ok", vi ? "Đã tạo API Key mới" : "Created new API Key");
    } catch (err) {
      flash("err", err instanceof Error ? err.message : String(err));
    } finally {
      setBusyKey(null);
    }
  };

  const handleAddAliasRule = async () => {
    if (!cfgDraft || !addRulePattern.trim() || !addRuleTarget.trim()) return;
    const rule: ModelAliasRule = {
      id: `rule-${Date.now()}`,
      pattern: addRulePattern.trim(),
      targetModel: addRuleTarget.trim(),
      enabled: true,
    };
    await handleSaveConfig(
      { ...cfgDraft, modelRules: [...cfgDraft.modelRules, rule] },
      false,
    );
    setAddRulePattern("");
  };

  const handleRunTestChat = async () => {
    if (!testPrompt.trim()) return;
    setBusyKey("test-chat");
    setTestResult(null);
    try {
      const res = await agxInvoke<ApiTestChatResult>("api_service_test_chat", {
        model: testModel,
        prompt: testPrompt.trim(),
      });
      setTestResult(res);
      await loadAll();
    } catch (err) {
      flash("err", err instanceof Error ? err.message : String(err));
    } finally {
      setBusyKey(null);
    }
  };

  const filteredModels = useMemo(() => {
    const q = modelSearch.trim().toLowerCase();
    return models.filter((m) => {
      if (modelFamilyFilter !== "ALL" && m.family !== modelFamilyFilter) return false;
      if (q && !m.id.toLowerCase().includes(q) && !m.displayName.toLowerCase().includes(q)) {
        return false;
      }
      return true;
    });
  }, [modelFamilyFilter, modelSearch, models]);

  const filteredLogs = useMemo(() => {
    const q = logSearch.trim().toLowerCase();
    return logs.filter((l) => {
      if (logStatusFilter === "ok" && (l.statusCode < 200 || l.statusCode >= 300)) return false;
      if (logStatusFilter === "err" && l.statusCode >= 200 && l.statusCode < 300) return false;
      if (q) {
        const hay = `${l.requestModel} ${l.resolvedModel} ${l.accountEmail} ${l.apiKeyLabel} ${l.clientIp ?? ""} ${l.errorMessage ?? ""}`.toLowerCase();
        if (!hay.includes(q)) return false;
      }
      return true;
    });
  }, [logSearch, logStatusFilter, logs]);

  const setupSnippet = useMemo(() => {
    switch (setupClient) {
      case "claude-code":
        return `# PowerShell (Windows) — Claude Code CLI via Antigravity Router
$env:ANTHROPIC_BASE_URL="${anthropicBaseUrl}"
$env:ANTHROPIC_AUTH_TOKEN="${activeKeyValue}"
$env:ANTHROPIC_MODEL="claude-sonnet-4-6"
$env:ANTHROPIC_SMALL_FAST_MODEL="gemini-3-flash-agent"
claude

# Hoặc ~/.claude/settings.json:
{
  "env": {
    "ANTHROPIC_BASE_URL": "${anthropicBaseUrl}",
    "ANTHROPIC_AUTH_TOKEN": "${activeKeyValue}",
    "ANTHROPIC_MODEL": "claude-sonnet-4-6"
  }
}`;
      case "codex-cli":
        return `# PowerShell (Windows) — OpenAI / Codex CLI compatible
$env:OPENAI_BASE_URL="${baseUrl}"
$env:OPENAI_API_KEY="${activeKeyValue}"

# ~/.codex/config.toml
model_provider = "onlydevs-antigravity"
model = "gemini-pro-agent"

[model_providers.onlydevs-antigravity]
name = "OnlyDEVS Antigravity Router"
base_url = "${baseUrl}"
wire_api = "chat"`;
      case "cursor":
        return `# Cursor / Cline / Roo Code / Continue (OpenAI-Compatible)
Base URL: ${baseUrl}
API Key:  ${activeKeyValue}
Model:    gemini-pro-agent  (hoặc claude-sonnet-4-6, gemini-3-flash-agent, claude-opus-4-6-thinking)`;
      case "curl":
        return `# 1. Liệt kê danh sách model khả dụng:
curl ${baseUrl}/models -H "Authorization: Bearer ${activeKeyValue}"

# 2. Gọi OpenAI Chat Completions (Streaming SSE):
curl ${baseUrl}/chat/completions \\
  -H "Content-Type: application/json" \\
  -H "Authorization: Bearer ${activeKeyValue}" \\
  -d '{
    "model": "gemini-3-flash-agent",
    "stream": true,
    "messages": [{"role": "user", "content": "Hello from OnlyDEVS Antigravity!"}]
  }'

# 3. Gọi Anthropic Messages API (/v1/messages):
curl ${baseUrl}/messages \\
  -H "Content-Type: application/json" \\
  -H "x-api-key: ${activeKeyValue}" \\
  -H "anthropic-version: 2023-06-01" \\
  -d '{
    "model": "claude-sonnet-4-6",
    "max_tokens": 1024,
    "messages": [{"role": "user", "content": "Ping!"}]
  }'`;
      case "python":
        return `from openai import OpenAI

client = OpenAI(
    base_url="${baseUrl}",
    api_key="${activeKeyValue}",
)

resp = client.chat.completions.create(
    model="gemini-pro-agent",
    messages=[{"role": "user", "content": "Giải thích ngắn gọn kiến trúc Antigravity Router"}],
)
print(resp.choices[0].message.content)`;
    }
  }, [activeKeyValue, anthropicBaseUrl, baseUrl, setupClient]);

  return (
    <div className="agx-page od-page">
      <header className="od-header">
        <div className="od-header__left">
          <h1>{vi ? "API Service" : "API Service"}</h1>
        </div>

        <nav className="od-subtabs" role="tablist">
          <button
            type="button"
            className={`od-subtab${subTab === "overview" ? " is-active" : ""}`}
            onClick={() => setSubTab("overview")}
          >
            <Radio size={14} />
            <span>{vi ? "Tổng quan" : "Overview"}</span>
          </button>
          <button
            type="button"
            className={`od-subtab${subTab === "models" ? " is-active" : ""}`}
            onClick={() => setSubTab("models")}
          >
            <Cpu size={14} />
            <span>{vi ? `Mô hình (${models.length})` : `Models (${models.length})`}</span>
          </button>
          <button
            type="button"
            className={`od-subtab${subTab === "setup" ? " is-active" : ""}`}
            onClick={() => setSubTab("setup")}
          >
            <BookOpen size={14} />
            <span>{vi ? "Hướng dẫn" : "Setup"}</span>
          </button>
          <button
            type="button"
            className={`od-subtab${subTab === "logs" ? " is-active" : ""}`}
            onClick={() => setSubTab("logs")}
          >
            <Activity size={14} />
            <span>{vi ? `Nhật ký (${logs.length})` : `Logs (${logs.length})`}</span>
          </button>
          <button
            type="button"
            className={`od-subtab${subTab === "usage" ? " is-active" : ""}`}
            onClick={() => setSubTab("usage")}
          >
            <BarChart3 size={14} />
            <span>{vi ? "Thống kê" : "Usage"}</span>
          </button>
        </nav>

        <div className="od-header__right">
          <button
            type="button"
            className={`agx-btn agx-btn--sm ${status?.running ? "agx-btn--danger" : "agx-btn--primary"}`}
            disabled={busyKey === "power"}
            onClick={() => void handleToggleServer()}
          >
            {busyKey === "power" ? (
              <Loader2 size={15} className="spin" />
            ) : (
              <Power size={15} />
            )}
            <span>
              {status?.running
                ? vi
                  ? "Dừng API"
                  : "Stop API"
                : vi
                  ? "Chạy API"
                  : "Start API"}
            </span>
          </button>
        </div>
      </header>

      {banner ? (
        <div className={`od-banner is-${banner.tone}`}>
          <span>{banner.text}</span>
          <button type="button" className="agx-icon-btn" onClick={() => setBanner(null)}>
            <X size={14} />
          </button>
        </div>
      ) : null}

      {/* SUB-TAB 1: OVERVIEW, POOL & API KEYS */}
      {subTab === "overview" && status && cfgDraft ? (
        <div className="ag-api-overview-grid">
          {/* Full-width Split Pro Pool Quota Summary */}
          <section className="ag-api-card ag-api-quota-card">
            <div className="ag-api-card__head">
              <div className="ag-api-card__title-group">
                <h3>
                  <Layers size={15} />{" "}
                  {vi
                    ? "Tổng Quota Pool (Phân loại 2 nhóm Pro)"
                    : "Total Pool Quota (Split by Pro Account Tier)"}
                </h3>
                <span className="ag-api-protocol-pill">
                  {poolAccounts.length} acc trong pool
                </span>
              </div>
              <button
                type="button"
                className="agx-btn agx-btn--secondary agx-btn--sm"
                disabled={busyKey === "refresh-pool-quota"}
                onClick={() => void handleRefreshPoolQuotas()}
              >
                {busyKey === "refresh-pool-quota" ? (
                  <Loader2 size={13} className="spin" />
                ) : (
                  <RefreshCw size={13} />
                )}
                <span>{vi ? "Quét lại Quota Pool" : "Refresh Pool Quota"}</span>
              </button>
            </div>

            <div className="ag-api-quota-groups">
              {/* Group 1: Pro Full Next-Gen (Argon / G4 / C5.5) */}
              <div className="ag-api-quota-group is-full">
                <div className="ag-api-quota-group__head">
                  <div className="ag-api-quota-group__title">
                    <strong>
                      <Sparkles size={14} />
                      <span>Pro · Full Next-Gen (Argon / G4 / C5.5)</span>
                    </strong>
                    <span>
                      {poolQuotaSummary.fullPro.accountCount > 0
                        ? `${poolQuotaSummary.fullPro.readyCount}/${poolQuotaSummary.fullPro.accountCount} ${vi ? "acc sẵn sàng" : "ready"} · ${poolQuotaSummary.fullPro.tierBreakdown}`
                        : vi
                          ? "Chưa có acc Full Next-Gen trong pool"
                          : "No Full Next-Gen accounts in pool"}
                    </span>
                  </div>
                  <div className="ag-api-quota-group__total">
                    <strong>
                      {poolQuotaSummary.fullPro.totalEquivAcc.toFixed(2)} /{" "}
                      {poolQuotaSummary.fullPro.accountCount} acc
                    </strong>
                    <span>
                      {vi ? "Tổng dung lượng quy đổi" : "Total equiv. capacity"}
                    </span>
                  </div>
                </div>

                {poolQuotaSummary.fullPro.accountCount > 0 ? (
                  <div className="ag-api-quota-buckets">
                    {poolQuotaSummary.fullPro.buckets.map((b) => {
                      const tone = getQuotaTone(b.avgPct);
                      const resetText = formatResetCountdown(b.earliestReset, vi);
                      return (
                        <div key={b.key} className="ag-api-quota-row">
                          <div className="ag-api-quota-row__top">
                            <span className="ag-api-quota-row__label">
                              <span>{b.label}</span>
                              {resetText ? (
                                <span className="ag-api-quota-row__reset">
                                  · {resetText}
                                </span>
                              ) : null}
                            </span>
                            <span className="ag-api-quota-row__metrics">
                              <span>
                                {b.sumPct}% / {b.maxPct}% ({b.availableCount}/{b.totalCount} acc)
                              </span>
                              <span className={`ag-api-quota-row__pct is-${tone}`}>
                                {b.avgPct}%
                              </span>
                            </span>
                          </div>
                          <div className="ag-api-quota-bar">
                            <div
                              className={`ag-api-quota-bar__fill is-${tone}`}
                              style={{ width: `${Math.max(0, Math.min(100, b.avgPct))}%` }}
                            />
                          </div>
                        </div>
                      );
                    })}
                  </div>
                ) : null}
              </div>

              {/* Group 2: ProLite Classic (G4 Hydrogen / G3.8 / Claude 4.6) */}
              <div className="ag-api-quota-group is-classic">
                <div className="ag-api-quota-group__head">
                  <div className="ag-api-quota-group__title">
                    <strong>
                      <Cpu size={14} />
                      <span>ProLite (Hydrogen / G3.8 / Claude 4.6)</span>
                    </strong>
                    <span>
                      {poolQuotaSummary.classicPro.accountCount > 0
                        ? `${poolQuotaSummary.classicPro.readyCount}/${poolQuotaSummary.classicPro.accountCount} ${vi ? "acc sẵn sàng" : "ready"} · ${poolQuotaSummary.classicPro.tierBreakdown}`
                        : vi
                          ? "Chưa có acc ProLite trong pool"
                          : "No ProLite accounts in pool"}
                    </span>
                  </div>
                  <div className="ag-api-quota-group__total">
                    <strong>
                      {poolQuotaSummary.classicPro.totalEquivAcc.toFixed(2)} /{" "}
                      {poolQuotaSummary.classicPro.accountCount} acc
                    </strong>
                    <span>
                      {vi ? "Tổng dung lượng quy đổi" : "Total equiv. capacity"}
                    </span>
                  </div>
                </div>

                {poolQuotaSummary.classicPro.accountCount > 0 ? (
                  <div className="ag-api-quota-buckets">
                    {poolQuotaSummary.classicPro.buckets.map((b) => {
                      const tone = getQuotaTone(b.avgPct);
                      const resetText = formatResetCountdown(b.earliestReset, vi);
                      return (
                        <div key={b.key} className="ag-api-quota-row">
                          <div className="ag-api-quota-row__top">
                            <span className="ag-api-quota-row__label">
                              <span>{b.label}</span>
                              {resetText ? (
                                <span className="ag-api-quota-row__reset">
                                  · {resetText}
                                </span>
                              ) : null}
                            </span>
                            <span className="ag-api-quota-row__metrics">
                              <span>
                                {b.sumPct}% / {b.maxPct}% ({b.availableCount}/{b.totalCount} acc)
                              </span>
                              <span className={`ag-api-quota-row__pct is-${tone}`}>
                                {b.avgPct}%
                              </span>
                            </span>
                          </div>
                          <div className="ag-api-quota-bar">
                            <div
                              className={`ag-api-quota-bar__fill is-${tone}`}
                              style={{ width: `${Math.max(0, Math.min(100, b.avgPct))}%` }}
                            />
                          </div>
                        </div>
                      );
                    })}
                  </div>
                ) : null}
              </div>
            </div>

            {poolQuotaSummary.otherCount > 0 ? (
              <div className="ag-api-quota-other-strip">
                <span>
                  {vi
                    ? `Nhóm khác (tách riêng khỏi 2 loại Pro): ${poolQuotaSummary.otherCount} acc`
                    : `Other accounts (excluded from Pro groups): ${poolQuotaSummary.otherCount}`}
                  {poolQuotaSummary.otherFreeCount > 0
                    ? ` · ${poolQuotaSummary.otherFreeCount} FREE`
                    : ""}
                  {poolQuotaSummary.otherUnscannedCount > 0
                    ? ` · ${poolQuotaSummary.otherUnscannedCount} ${vi ? "chưa quét quota" : "unscanned"}`
                    : ""}
                </span>
              </div>
            ) : null}
          </section>

          <section className="ag-api-card ag-api-hero-card">
            <div className="ag-api-card__head">
              <div className="ag-api-card__title-group">
                <h3>{vi ? "API Router" : "API Router"}</h3>
                <span className="ag-api-protocol-pill">
                  <ShieldCheck size={12} /> OpenAI · Anthropic
                </span>
              </div>
              <span
                className={`ag-api-status-pill ${status.running ? "is-on" : "is-off"}`}
              >
                <span className="ag-api-status-dot" />
                {status.running
                  ? vi
                    ? "Đang chạy"
                    : "Running"
                  : vi
                    ? "Đã tắt"
                    : "Stopped"}
              </span>
            </div>

            {/* Endpoint URL Row */}
            <div className="ag-api-endpoint-row">
              <select
                className="agx-select ag-toolbar-select"
                value={cfgDraft.accessScope}
                onChange={(e) => {
                  const next = { ...cfgDraft, accessScope: e.target.value };
                  setCfgDraft(next);
                  void handleSaveConfig(next, true);
                }}
              >
                <option value="local">{vi ? "Chỉ cục bộ" : "Local Only"}</option>
                <option value="lan">{vi ? "Mạng LAN (0.0.0.0)" : "LAN (0.0.0.0)"}</option>
              </select>

              <code className="ag-api-url-code">{status.baseUrl}</code>

              <button
                type="button"
                className="ag-card-icon-btn"
                title={vi ? "Sao chép Base URL" : "Copy Base URL"}
                onClick={() => void copyText(status.baseUrl, "base-url")}
              >
                {copiedKey === "base-url" ? <Check size={14} /> : <Copy size={14} />}
              </button>
            </div>

            {status.lanUrl ? (
              <>
                <div className="ag-api-lan-note">
                  <span>LAN URL:</span>
                  <code>{status.lanUrl}</code>
                  <button
                    type="button"
                    className="ag-card-icon-btn"
                    onClick={() => void copyText(status.lanUrl!, "lan-url")}
                  >
                    {copiedKey === "lan-url" ? <Check size={13} /> : <Copy size={13} />}
                  </button>
                </div>

                {lanDiag ? (
                  <div
                    className={`ag-api-lan-fw-row ${
                      lanDiag.firewallRuleExists ? "is-ok" : "is-warn"
                    }`}
                  >
                    <div className="ag-api-lan-fw-info">
                      {lanDiag.firewallRuleExists ? (
                        <ShieldCheck size={14} style={{ color: "#10b981", flexShrink: 0 }} />
                      ) : (
                        <ShieldAlert size={14} style={{ color: "#f59e0b", flexShrink: 0 }} />
                      )}
                      <span>
                        {lanDiag.firewallRuleExists
                          ? vi
                            ? `Firewall LAN đã mở (TCP ${lanDiag.port}${lanDiag.networkProfile ? ` · Mạng ${lanDiag.networkProfile}` : ""})`
                            : `LAN Firewall open (TCP ${lanDiag.port}${lanDiag.networkProfile ? ` · ${lanDiag.networkProfile}` : ""})`
                          : vi
                            ? `Windows Firewall đang chặn LAN (Mạng: ${lanDiag.networkProfile || "Public"}) — máy khác ping/gọi sẽ bị timeout!`
                            : `Windows Firewall is blocking LAN (${lanDiag.networkProfile || "Public"} profile) — remote ping/requests will time out!`}
                      </span>
                    </div>
                    <div className="ag-api-lan-fw-actions">
                      {!lanDiag.firewallRuleExists ? (
                        <button
                          type="button"
                          className="agx-btn agx-btn--primary agx-btn--sm"
                          disabled={busyKey === "lan-fw"}
                          onClick={() => void handleOpenLanFirewall()}
                        >
                          {busyKey === "lan-fw" ? (
                            <Loader2 size={13} className="spin" />
                          ) : (
                            <AlertTriangle size={13} />
                          )}
                          <span>{vi ? "Mở Firewall LAN" : "Open LAN Firewall"}</span>
                        </button>
                      ) : null}
                      <button
                        type="button"
                        className="agx-btn agx-btn--ghost agx-btn--sm"
                        title={lanDiag.manualCommand}
                        onClick={() => void copyText(lanDiag.manualCommand, "lan-fw-cmd")}
                      >
                        {copiedKey === "lan-fw-cmd" ? <Check size={12} /> : <Copy size={12} />}
                        <span>netsh</span>
                      </button>
                    </div>
                  </div>
                ) : null}
              </>
            ) : null}

            {/* Master Key Row */}
            {masterKey ? (
              <div className="ag-api-key-row">
                <span className="ag-api-key-label">{vi ? "Khóa API" : "API Key"}</span>
                <code className="ag-api-key-val">
                  {maskKey(masterKey.key, revealMasterKey)}
                </code>
                <button
                  type="button"
                  className="ag-card-icon-btn"
                  onClick={() => setRevealMasterKey((v) => !v)}
                  title={revealMasterKey ? "Hide" : "Reveal"}
                >
                  {revealMasterKey ? <EyeOff size={14} /> : <Eye size={14} />}
                </button>
                <button
                  type="button"
                  className="ag-card-icon-btn"
                  onClick={() => void copyText(masterKey.key, "master-key")}
                  title={vi ? "Sao chép khóa" : "Copy key"}
                >
                  {copiedKey === "master-key" ? <Check size={14} /> : <Copy size={14} />}
                </button>
                <button
                  type="button"
                  className="ag-card-icon-btn"
                  title={vi ? "Xoay khóa mới (Rotate)" : "Rotate key"}
                  onClick={async () => {
                    if (!window.confirm(vi ? "Tạo khóa mới thay thế Master Key hiện tại?" : "Rotate Master API Key?")) return;
                    await agxInvoke("api_service_rotate_key", { id: masterKey.id });
                    await loadAll();
                    flash("ok", vi ? "Đã xoay Master API Key mới" : "Rotated Master API Key");
                  }}
                >
                  <RotateCcw size={14} />
                </button>
              </div>
            ) : null}

            {/* Service Members Pool Box */}
            <div className="ag-api-pool-box">
              <div className="ag-api-pool-head">
                <div>
                  <strong>
                    {poolAccounts.length} {vi ? "tài khoản trong Pool" : "accounts in Pool"}
                  </strong>
                  <span className="agx-muted">
                    {status.poolHealthy}/{status.poolTotal}{" "}
                    {vi ? "sẵn sàng" : "healthy"}
                    {status.cooldowns.length > 0
                      ? ` · ${status.cooldowns.length} cooldown`
                      : ""}
                  </span>
                  <div className="ag-api-pool-split">
                    <span className="ag-api-profile-pill is-full">
                      PRO · G4/Argon/C5.5: {status.poolFullNextgen}
                    </span>
                    <span className="ag-api-profile-pill is-classic">
                      ProLite · Hydrogen/C4.6: {status.poolClassic}
                    </span>
                    {status.poolUnknown > 0 ? (
                      <span className="ag-api-profile-pill is-unknown">
                        {vi ? "Chưa quét" : "Unscanned"}: {status.poolUnknown}
                      </span>
                    ) : null}
                  </div>
                </div>
                <div style={{ display: "flex", gap: 6 }}>
                  <button
                    type="button"
                    className="agx-btn agx-btn--secondary agx-btn--sm"
                    onClick={openMemberModal}
                  >
                    <FolderPlus size={13} />
                    <span>{vi ? "Quản lý thành viên" : "Manage Members"}</span>
                  </button>
                  <button
                    type="button"
                    className="agx-btn agx-btn--secondary agx-btn--sm"
                    onClick={onNavigateAccounts}
                  >
                    <Users size={13} />
                    <span>{vi ? "Thêm TK" : "Add Account"}</span>
                  </button>
                </div>
              </div>

              {poolAccounts.length === 0 ? (
                <p className="agx-muted" style={{ margin: "6px 0 0" }}>
                  {vi
                    ? "Chưa có tài khoản Antigravity trong pool. Hãy thêm tài khoản ở tab Tài khoản (OAuth / Đồng bộ trên máy) rồi bật lại."
                    : "No Antigravity accounts in pool yet. Add an account in the Account tab first."}
                </p>
              ) : (
                <div className="ag-api-pool-list">
                  {poolAccounts.map((acc) => {
                    const score = getAntigravityOverallScore(acc);
                    const profile = classifyAccountModelProfile(acc);
                    const cds = cooldownMap.get(acc.id) ?? [];
                    return (
                      <div key={acc.id} className="ag-api-pool-member">
                        <div className="ag-api-pool-member__info">
                          <div className="ag-api-pool-member__top">
                            <strong>{acc.email}</strong>
                            <span
                              className={`ag-api-profile-pill ${
                                profile.kind === "full_nextgen"
                                  ? "is-full"
                                  : profile.kind === "classic"
                                    ? "is-classic"
                                    : "is-unknown"
                              }`}
                            >
                              {profile.kind === "full_nextgen"
                                ? "PRO · Argon/G4"
                                : profile.kind === "classic"
                                  ? "ProLite · Hydrogen/C4.6"
                                  : vi
                                    ? "Chưa quét"
                                    : "Unscanned"}
                            </span>
                          </div>
                          <span>
                            {profile.summary
                              ? profile.summary
                              : score >= 0
                                ? `Quota TB: ${score}%`
                                : vi
                                  ? "Chưa có quota"
                                  : "No quota"}
                          </span>
                        </div>
                        <div className="ag-api-pool-member__right">
                          {cds.length > 0 ? (
                            <>
                              {cds.map((cd) => (
                                <span key={cd.scope} className="ag-badge-warn">
                                  {cd.scope !== "all" ? `${cd.scope} ` : ""}
                                  {cd.remainingSeconds}s ({cd.reason})
                                </span>
                              ))}
                              <button
                                type="button"
                                className="agx-btn agx-btn--ghost agx-btn--sm"
                                onClick={async () => {
                                  await agxInvoke("api_service_clear_cooldowns", {
                                    accountId: acc.id,
                                  });
                                  await loadAll();
                                }}
                              >
                                {vi ? "Gỡ" : "Clear"}
                              </button>
                            </>
                          ) : (
                            <span className="ag-badge-current">
                              {vi ? "Sẵn sàng" : "Ready"}
                            </span>
                          )}
                        </div>
                      </div>
                    );
                  })}
                </div>
              )}
            </div>

            {/* Footer Strip */}
            <div className="ag-api-card__foot">
              <span className="agx-muted">
                {vi ? "Đang xử lý:" : "Active:"} <strong>{status.activeRequests}</strong>
              </span>
              <div style={{ display: "flex", gap: 6 }}>
                <button
                  type="button"
                  className="ag-toolbar-icon-btn"
                  title={vi ? "Làm mới" : "Refresh"}
                  onClick={() => {
                    void loadAll();
                    void onRefreshAccounts();
                  }}
                >
                  <RefreshCw size={14} />
                </button>
                {status.running ? (
                  <button
                    type="button"
                    className="ag-toolbar-icon-btn"
                    title={vi ? "Khởi động lại Router" : "Restart Router"}
                    disabled={busyKey === "restart"}
                    onClick={() => void handleRestartServer()}
                  >
                    <Play size={14} />
                  </button>
                ) : null}
              </div>
            </div>
          </section>

          {/* Right: Advanced Routing & Network Configuration */}
          <section className="ag-api-card">
            <div className="ag-api-card__head">
              <div className="ag-api-card__title-group">
                <h3>
                  <SlidersHorizontal size={15} />{" "}
                  {vi ? "Cấu hình định tuyến & mạng" : "Routing & Network Settings"}
                </h3>
              </div>
              <button
                type="button"
                className="agx-btn agx-btn--primary agx-btn--sm"
                disabled={busyKey === "save-cfg"}
                onClick={() => void handleSaveConfig(cfgDraft, true)}
              >
                <Save size={13} />
                <span>{vi ? "Lưu & Áp dụng" : "Save & Apply"}</span>
              </button>
            </div>

            <div className="ag-vault-grid">
              <label className="agx-field">
                <span>{vi ? "Cổng lắng nghe (Port)" : "Listen Port"}</span>
                <input
                  type="number"
                  min={1024}
                  max={65535}
                  className="agx-input"
                  value={cfgDraft.port}
                  onChange={(e) =>
                    setCfgDraft({ ...cfgDraft, port: Number(e.target.value) || 57120 })
                  }
                />
              </label>

              <label className="agx-field">
                <span>{vi ? "Chiến lược xoay tài khoản" : "Routing Strategy"}</span>
                <select
                  className="agx-input"
                  value={cfgDraft.routingStrategy}
                  onChange={(e) =>
                    setCfgDraft({ ...cfgDraft, routingStrategy: e.target.value })
                  }
                >
                  <option value="smart">
                    {vi
                      ? "Thông minh theo Model & Quota (Khuyên dùng)"
                      : "Smart Model-Aware (Recommended)"}
                  </option>
                  <option value="highest_quota">
                    {vi ? "Ưu tiên Quota cao nhất" : "Highest Quota First"}
                  </option>
                  <option value="round_robin">
                    {vi ? "Xoay vòng đều (Round Robin)" : "Round Robin"}
                  </option>
                  <option value="sticky_session">
                    {vi ? "Ghim phiên (Sticky Session)" : "Sticky Session"}
                  </option>
                  <option value="failover">
                    {vi ? "Dự phòng tuần tự (Failover)" : "Priority Failover"}
                  </option>
                </select>
              </label>

              <label className="agx-field">
                <span>{vi ? "Số lần thử lại (Failover Retries)" : "Failover Retries"}</span>
                <input
                  type="number"
                  min={0}
                  max={10}
                  className="agx-input"
                  value={cfgDraft.maxRetries}
                  onChange={(e) =>
                    setCfgDraft({ ...cfgDraft, maxRetries: Number(e.target.value) || 0 })
                  }
                />
              </label>

              <label className="agx-field">
                <span>{vi ? "Thời gian Cooldown khi 429 (giây)" : "429 Cooldown (seconds)"}</span>
                <input
                  type="number"
                  min={5}
                  max={3600}
                  className="agx-input"
                  value={cfgDraft.cooldownSeconds}
                  onChange={(e) =>
                    setCfgDraft({ ...cfgDraft, cooldownSeconds: Number(e.target.value) || 60 })
                  }
                />
              </label>

              <label className="agx-field">
                <span>{vi ? "Timeout gọi Upstream (giây)" : "Upstream Timeout (s)"}</span>
                <input
                  type="number"
                  min={15}
                  max={600}
                  className="agx-input"
                  value={cfgDraft.timeoutSeconds}
                  onChange={(e) =>
                    setCfgDraft({ ...cfgDraft, timeoutSeconds: Number(e.target.value) || 180 })
                  }
                />
              </label>

              <label className="agx-field">
                <span>{vi ? "HTTP / SOCKS Proxy (tùy chọn)" : "Upstream Proxy (optional)"}</span>
                <input
                  type="text"
                  className="agx-input"
                  value={cfgDraft.proxyUrl ?? ""}
                  onChange={(e) =>
                    setCfgDraft({ ...cfgDraft, proxyUrl: e.target.value || null })
                  }
                  placeholder="http://127.0.0.1:7890"
                />
              </label>

              <label className="agx-field">
                <span>
                  {vi
                    ? "Tự quét Quota nền (phút, 0 = tắt)"
                    : "Background Quota Refresh (min, 0 = off)"}
                </span>
                <input
                  type="number"
                  min={0}
                  max={1440}
                  className="agx-input"
                  value={cfgDraft.autoRefreshQuotaMinutes ?? 10}
                  onChange={(e) =>
                    setCfgDraft({
                      ...cfgDraft,
                      autoRefreshQuotaMinutes: Math.max(0, Math.min(1440, Number(e.target.value) || 0)),
                    })
                  }
                />
              </label>
            </div>

            <div className="ag-api-switches">
              <label className="ag-select-all-label">
                <input
                  type="checkbox"
                  checked={cfgDraft.autoStart}
                  onChange={(e) =>
                    setCfgDraft({ ...cfgDraft, autoStart: e.target.checked })
                  }
                />
                <span>
                  {vi
                    ? "Tự động khởi chạy API Router khi mở OnlyDEVS"
                    : "Auto-start API Router on app launch"}
                </span>
              </label>

              <label className="ag-select-all-label">
                <input
                  type="checkbox"
                  checked={cfgDraft.minimizeToTray ?? true}
                  onChange={(e) =>
                    setCfgDraft({ ...cfgDraft, minimizeToTray: e.target.checked })
                  }
                />
                <span>
                  {vi
                    ? "Thu nhỏ xuống khay hệ thống (System Tray) khi bấm đóng cửa sổ — giữ API chạy nền"
                    : "Minimize to System Tray on close — keep API Router running in background"}
                </span>
              </label>

              <label className="ag-select-all-label">
                <input
                  type="checkbox"
                  checked={cfgDraft.promptCacheAffinity ?? true}
                  onChange={(e) =>
                    setCfgDraft({
                      ...cfgDraft,
                      promptCacheAffinity: e.target.checked,
                    })
                  }
                />
                <span>
                  {vi
                    ? "Bật KV Prompt Cache (cố định sessionId & ghim acc theo hội thoại — giảm độ trễ 3–5x)"
                    : "Enable KV Prompt Cache (stable sessionId & conversation account pinning — 3–5x faster TTFT)"}
                </span>
              </label>

              <label className="ag-select-all-label">
                <input
                  type="checkbox"
                  checked={cfgDraft.priorityTier ?? true}
                  onChange={(e) =>
                    setCfgDraft({
                      ...cfgDraft,
                      priorityTier: e.target.checked,
                    })
                  }
                />
                <span>
                  {vi
                    ? "Bật đường ưu tiên Priority Tier (SERVICE_TIER_PRIORITY — tự fallback an toàn nếu model không hỗ trợ)"
                    : "Enable Priority Service Tier (SERVICE_TIER_PRIORITY with automatic safe fallback)"}
                </span>
              </label>

              <label className="ag-select-all-label">
                <input
                  type="checkbox"
                  checked={cfgDraft.fastMode ?? false}
                  onChange={(e) =>
                    setCfgDraft({
                      ...cfgDraft,
                      fastMode: e.target.checked,
                    })
                  }
                />
                <span>
                  {vi
                    ? "Chế độ Fast / Low-Thinking (ép thinkingLevel=LOW khi client không gửi reasoning_effort)"
                    : "Fast / Low-Thinking Mode (default thinkingLevel=LOW unless client sets reasoning_effort)"}
                </span>
              </label>

              <label className="ag-select-all-label">
                <input
                  type="checkbox"
                  checked={!(cfgDraft.cleanToolSchema ?? false)}
                  onChange={(e) =>
                    setCfgDraft({
                      ...cfgDraft,
                      cleanToolSchema: !e.target.checked,
                    })
                  }
                />
                <span>
                  {vi
                    ? "Ngụy trang Tool chống 429 (kèm 21 Decoy Tools & _ide — mặc định BẬT; tắt để gửi tên tool IDE gốc)"
                    : "Tool Cloaking Anti-429 (21 decoy tools & _ide — default ON; turn off for clean native IDE tool names)"}
                </span>
              </label>

              <label className="ag-select-all-label">
                <input
                  type="checkbox"
                  checked={cfgDraft.preserveExclusiveAccounts ?? true}
                  onChange={(e) =>
                    setCfgDraft({
                      ...cfgDraft,
                      preserveExclusiveAccounts: e.target.checked,
                    })
                  }
                />
                <span>
                  {vi
                    ? "Giữ quota acc Full/Argon: ưu tiên acc Classic (3.8) cho model dùng chung (3.8 / 3-Flash)"
                    : "Preserve Full/Argon accounts: prefer Classic (3.8) accounts for shared models"}
                </span>
              </label>

              <label className="ag-select-all-label">
                <input
                  type="checkbox"
                  checked={cfgDraft.useAllAccountsWhenEmpty}
                  onChange={(e) =>
                    setCfgDraft({
                      ...cfgDraft,
                      useAllAccountsWhenEmpty: e.target.checked,
                    })
                  }
                />
                <span>
                  {vi
                    ? "Tự dùng tất cả tài khoản hợp lệ khi chưa chọn danh sách thành viên riêng"
                    : "Auto-use all valid accounts when member list is empty"}
                </span>
              </label>
            </div>

            {status.cooldowns.length > 0 ? (
              <div style={{ marginTop: 10 }}>
                <button
                  type="button"
                  className="agx-btn agx-btn--secondary agx-btn--sm"
                  onClick={async () => {
                    await agxInvoke("api_service_clear_cooldowns", { accountId: null });
                    await loadAll();
                    flash("ok", vi ? "Đã gỡ toàn bộ cooldown" : "Cleared all cooldowns");
                  }}
                >
                  <RotateCcw size={13} />
                  <span>{vi ? "Gỡ toàn bộ Cooldown" : "Clear All Cooldowns"}</span>
                </button>
              </div>
            ) : null}
          </section>

          {/* Bottom Full-Width: Multi API Keys Manager */}
          <section className="ag-api-card ag-api-card--full">
            <div className="ag-api-card__head">
              <div className="ag-api-card__title-group">
                <h3>
                  <KeyRound size={15} />{" "}
                  {vi ? "Quản lý API Keys" : "API Keys Management"} ({status.keys.length})
                </h3>
              </div>
              <button
                type="button"
                className="agx-btn agx-btn--primary agx-btn--sm"
                onClick={() => setKeyModalOpen(true)}
              >
                <Plus size={14} />
                <span>{vi ? "Tạo API Key mới" : "Create API Key"}</span>
              </button>
            </div>

            <div className="ag-token-table-wrap">
              <table className="ag-token-table">
                <thead>
                  <tr>
                    <th>{vi ? "Nhãn" : "Label"}</th>
                    <th>Key</th>
                    <th>{vi ? "Nhóm Pro & Giới hạn" : "Pool Tier & Limits"}</th>
                    <th className="is-num">{vi ? "Lượt gọi" : "Requests"}</th>
                    <th className="is-num">Tokens</th>
                    <th>{vi ? "Trạng thái" : "Status"}</th>
                    <th className="is-num">{vi ? "Thao tác" : "Actions"}</th>
                  </tr>
                </thead>
                <tbody>
                  {status.keys.map((k) => {
                    const isExpired =
                      Boolean(k.expiresAt) &&
                      (k.expiresAt ?? 0) > 0 &&
                      Math.floor(Date.now() / 1000) >= (k.expiresAt ?? 0);
                    return (
                    <tr key={k.id}>
                      <td>
                        <strong>{k.label}</strong>
                        {k.isMaster ? (
                          <span className="ag-badge-tier is-pro" style={{ marginLeft: 6 }}>
                            MASTER
                          </span>
                        ) : null}
                      </td>
                      <td>
                        <code>{maskKey(k.key, revealMasterKey)}</code>
                      </td>
                      <td>
                        <div style={{ display: "flex", flexWrap: "wrap", gap: 4, alignItems: "center" }}>
                          {k.allowedProfile === "classic" ? (
                            <span className="ag-api-profile-pill is-classic">
                              Chỉ Classic (3.8/C4.6)
                            </span>
                          ) : k.allowedProfile === "full_nextgen" ? (
                            <span className="ag-api-profile-pill is-full">
                              Chỉ Full (Argon/G4/C5.5)
                            </span>
                          ) : (
                            <span className="agx-muted" style={{ fontSize: 12 }}>
                              {vi ? "Tất cả Pro" : "All tiers"}
                            </span>
                          )}
                          {k.rateLimitRpm > 0 ? (
                            <span className="ag-api-protocol-pill">
                              {k.rateLimitRpm} RPM
                            </span>
                          ) : null}
                          {k.dailyTokenLimit > 0 ? (
                            <span className="ag-api-protocol-pill">
                              {formatTokensShort(k.dailyTokenLimit, 1)}/ngày
                            </span>
                          ) : null}
                          {k.expiresAt ? (
                            <span className={isExpired ? "ag-badge-err" : "ag-badge-warn"}>
                              {isExpired
                                ? vi
                                  ? "Đã hết hạn"
                                  : "Expired"
                                : `HSD ${formatTime(k.expiresAt)}`}
                            </span>
                          ) : null}
                          {k.allowedModels.length > 0 ? (
                            <span className="ag-token-cell-sub">
                              Model: {k.allowedModels.join(", ")}
                            </span>
                          ) : null}
                        </div>
                      </td>
                      <td className="is-num">{formatExactNumber(k.totalRequests)}</td>
                      <td className="is-num">{formatTokensShort(k.totalTokens, 2)}</td>
                      <td>
                        <span
                          className={
                            k.enabled && !isExpired ? "ag-badge-current" : "ag-badge-err"
                          }
                        >
                          {isExpired
                            ? vi
                              ? "Hết hạn"
                              : "Expired"
                            : k.enabled
                              ? vi
                                ? "Bật"
                                : "Active"
                              : vi
                                ? "Tắt"
                                : "Disabled"}
                        </span>
                      </td>
                      <td className="is-num">
                        <div style={{ display: "inline-flex", gap: 4 }}>
                          <button
                            type="button"
                            className="ag-card-icon-btn"
                            title={vi ? "Sao chép" : "Copy"}
                            onClick={() => void copyText(k.key, `k-${k.id}`)}
                          >
                            {copiedKey === `k-${k.id}` ? <Check size={13} /> : <Copy size={13} />}
                          </button>
                          <button
                            type="button"
                            className="ag-card-icon-btn"
                            title={vi ? "Bật / Tắt khóa" : "Toggle key"}
                            onClick={async () => {
                              await agxInvoke("api_service_update_key", {
                                id: k.id,
                                enabled: !k.enabled,
                              });
                              await loadAll();
                            }}
                          >
                            <Power size={13} />
                          </button>
                          <button
                            type="button"
                            className="ag-card-icon-btn"
                            title={vi ? "Xoay khóa" : "Rotate"}
                            onClick={async () => {
                              await agxInvoke("api_service_rotate_key", { id: k.id });
                              await loadAll();
                            }}
                          >
                            <RotateCcw size={13} />
                          </button>
                          {!k.isMaster ? (
                            <button
                              type="button"
                              className="ag-card-icon-btn is-danger"
                              title={vi ? "Xóa" : "Delete"}
                              onClick={async () => {
                                await agxInvoke("api_service_delete_key", { id: k.id });
                                await loadAll();
                              }}
                            >
                              <Trash2 size={13} />
                            </button>
                          ) : null}
                        </div>
                      </td>
                    </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          </section>
        </div>
      ) : null}

      {/* ====================================================================
          SUB-TAB 2: AVAILABLE MODELS & ALIAS ROUTING RULES
         ==================================================================== */}
      {subTab === "models" && cfgDraft ? (
        <div className="ag-overview-wrap">
          <section className="ag-api-card">
            <div className="ag-api-card__head">
              <div>
                <h3>{vi ? "Mô hình khả dụng" : "Available Models"}</h3>
              </div>
              <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
                <div className="ag-conv-search">
                  <Search size={14} />
                  <input
                    type="text"
                    value={modelSearch}
                    onChange={(e) => setModelSearch(e.target.value)}
                    placeholder={vi ? "Tìm model ID..." : "Search model ID..."}
                  />
                </div>
                <select
                  className="agx-select ag-toolbar-select"
                  value={modelFamilyFilter}
                  onChange={(e) => setModelFamilyFilter(e.target.value)}
                >
                  <option value="ALL">{vi ? "Tất cả nhóm" : "All families"}</option>
                  <option value="gemini">Gemini</option>
                  <option value="claude">Claude</option>
                  <option value="openai">OpenAI / OSS</option>
                  <option value="image">Image</option>
                  <option value="alias">Alias</option>
                </select>
              </div>
            </div>

            <div className="ag-api-model-grid">
              {filteredModels.map((m) => {
                const formatLimitShort = (val: number) => {
                  if (val >= 1_000_000) return "1M";
                  if (val === 131_072) return "128K";
                  if (val === 65_536 || val === 65_535) return "64K";
                  if (val === 32_768) return "32K";
                  if (val >= 1_000) return `${Math.round(val / 1_000)}K`;
                  return String(val);
                };
                const ideJsonSnippet = JSON.stringify(
                  {
                    [m.id]: {
                      name: m.displayName,
                      limit: {
                        context: m.contextWindow,
                        output: m.maxOutputTokens,
                      },
                    },
                  },
                  null,
                  2,
                );
                return (
                <div key={m.id} className="ag-api-model-tile">
                  <div className="ag-api-model-tile__top">
                    <strong>{m.displayName}</strong>
                    <span className={`ag-kind-badge is-${m.family === "claude" ? "swarm" : m.family === "alias" ? "subagent" : "main"}`}>
                      {m.family.toUpperCase()}
                    </span>
                  </div>
                  <code className="ag-api-model-id">{m.id}</code>
                  <div
                    className="ag-api-model-limits"
                    title={
                      vi
                        ? `Context Window: ${m.contextWindow.toLocaleString()} tokens | Max Output: ${m.maxOutputTokens.toLocaleString()} tokens`
                        : `Context Window: ${m.contextWindow.toLocaleString()} tokens | Max Output: ${m.maxOutputTokens.toLocaleString()} tokens`
                    }
                  >
                    <span className="ag-api-model-limit-chip">
                      <strong>Ctx</strong> {m.contextWindow.toLocaleString()} ({formatLimitShort(m.contextWindow)})
                    </span>
                    <span className="ag-api-model-limit-chip">
                      <strong>Out</strong> {m.maxOutputTokens.toLocaleString()} ({formatLimitShort(m.maxOutputTokens)})
                    </span>
                  </div>
                  <div className="ag-api-model-tile__foot">
                    <div style={{ display: "flex", alignItems: "center", gap: 6, flexWrap: "wrap" }}>
                      <span
                        className={`ag-api-model-support ${
                          m.supportedAccountCount > 0 ? "is-ok" : "is-none"
                        }`}
                        title={
                          vi
                            ? `${m.supportedAccountCount}/${m.poolAccountCount} tài khoản trong Pool hỗ trợ model này`
                            : `${m.supportedAccountCount}/${m.poolAccountCount} pool accounts support this model`
                        }
                      >
                        {m.supportedAccountCount}/{m.poolAccountCount} acc
                      </span>
                      <span className="agx-muted">
                        {m.thinking ? "Thinking" : "Standard"} · {m.source}
                      </span>
                    </div>
                    <div style={{ display: "flex", gap: 4 }}>
                      <button
                        type="button"
                        className="ag-card-icon-btn"
                        title={vi ? "Sao chép Model ID" : "Copy Model ID"}
                        onClick={() => void copyText(m.id, `m-${m.id}`)}
                      >
                        {copiedKey === `m-${m.id}` ? <Check size={13} /> : <Copy size={13} />}
                      </button>
                      <button
                        type="button"
                        className="ag-card-icon-btn"
                        title={vi ? "Sao chép JSON cấu hình IDE (context/output)" : "Copy IDE JSON config (context/output)"}
                        onClick={() => void copyText(ideJsonSnippet, `json-${m.id}`)}
                      >
                        {copiedKey === `json-${m.id}` ? <Check size={13} /> : <Code2 size={13} />}
                      </button>
                      <button
                        type="button"
                        className="ag-card-icon-btn"
                        title={vi ? "Test nhanh model này" : "Quick test this model"}
                        onClick={() => {
                          setTestModel(m.id);
                          setSubTab("setup");
                        }}
                      >
                        <Play size={13} />
                      </button>
                    </div>
                  </div>
                </div>
                );
              })}
            </div>
          </section>

          {/* Custom Model Alias / Rewrite Rules */}
          <section className="ag-api-card">
            <div className="ag-api-card__head">
              <div>
                <h3>
                  <Layers size={15} />{" "}
                  {vi ? "Quy tắc ánh xạ Model (Alias / Rewrite Rules)" : "Model Alias & Rewrite Rules"}
                </h3>
                <span className="agx-muted">
                  {vi
                    ? "Tự động chuyển đổi tên model từ client (vd: gpt-4o, claude-3-5-sonnet*) sang model Antigravity tương ứng."
                    : "Automatically remap incoming client model names (e.g. gpt-4o) to Antigravity upstream models."}
                </span>
              </div>
            </div>

            <div className="ag-api-rule-add">
              <input
                type="text"
                className="agx-input"
                value={addRulePattern}
                onChange={(e) => setAddRulePattern(e.target.value)}
                placeholder={vi ? "Pattern (vd: gpt-4*, o3-mini)" : "Pattern (e.g. gpt-4*, o3-mini)"}
              />
              <span className="agx-muted">→</span>
              <select
                className="agx-input"
                value={addRuleTarget}
                onChange={(e) => setAddRuleTarget(e.target.value)}
              >
                {models
                  .filter((m) => m.family !== "alias")
                  .map((m) => (
                    <option key={m.id} value={m.id}>
                      {m.id} ({m.displayName})
                    </option>
                  ))}
              </select>
              <button
                type="button"
                className="agx-btn agx-btn--primary"
                disabled={!addRulePattern.trim()}
                onClick={() => void handleAddAliasRule()}
              >
                <Plus size={14} />
                <span>{vi ? "Thêm quy tắc" : "Add Rule"}</span>
              </button>
            </div>

            <div className="ag-token-table-wrap">
              <table className="ag-token-table">
                <thead>
                  <tr>
                    <th>{vi ? "Bật" : "On"}</th>
                    <th>{vi ? "Client Model Pattern" : "Client Model Pattern"}</th>
                    <th>{vi ? "Chuyển sang Antigravity Model" : "Target Antigravity Model"}</th>
                    <th className="is-num">{vi ? "Xóa" : "Remove"}</th>
                  </tr>
                </thead>
                <tbody>
                  {cfgDraft.modelRules.map((r) => (
                    <tr key={r.id}>
                      <td>
                        <input
                          type="checkbox"
                          checked={r.enabled}
                          onChange={() => {
                            const nextRules = cfgDraft.modelRules.map((x) =>
                              x.id === r.id ? { ...x, enabled: !x.enabled } : x,
                            );
                            void handleSaveConfig({ ...cfgDraft, modelRules: nextRules }, false);
                          }}
                        />
                      </td>
                      <td>
                        <code>{r.pattern}</code>
                      </td>
                      <td>
                        <code>{r.targetModel}</code>
                      </td>
                      <td className="is-num">
                        <button
                          type="button"
                          className="ag-card-icon-btn is-danger"
                          onClick={() => {
                            const nextRules = cfgDraft.modelRules.filter((x) => x.id !== r.id);
                            void handleSaveConfig({ ...cfgDraft, modelRules: nextRules }, false);
                          }}
                        >
                          <Trash2 size={13} />
                        </button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </section>
        </div>
      ) : null}

      {/* ====================================================================
          SUB-TAB 3: SETUP GUIDES & LIVE PLAYGROUND TESTER
         ==================================================================== */}
      {subTab === "setup" ? (
        <div className="ag-api-overview-grid">
          <section className="ag-api-card">
            <div className="ag-api-card__head">
              <h3>{vi ? "Hướng dẫn kết nối" : "Client Setup"}</h3>
              <button
                type="button"
                className="agx-btn agx-btn--primary agx-btn--sm"
                onClick={() => void copyText(setupSnippet, "setup-code")}
              >
                {copiedKey === "setup-code" ? <Check size={14} /> : <Copy size={14} />}
                <span>{vi ? "Sao chép" : "Copy"}</span>
              </button>
            </div>

            <div className="ag-add-tabs" style={{ padding: 0 }}>
              {(
                [
                  ["claude-code", "Claude Code"],
                  ["codex-cli", "Codex CLI"],
                  ["cursor", "Cursor / Cline / Roo"],
                  ["curl", "cURL"],
                  ["python", "Python SDK"],
                ] as const
              ).map(([id, label]) => (
                <button
                  key={id}
                  type="button"
                  className={setupClient === id ? "is-active" : ""}
                  onClick={() => setSetupClient(id)}
                >
                  {label}
                </button>
              ))}
            </div>

            <pre className="ag-api-code-block">{setupSnippet}</pre>
          </section>

          {/* Live Playground Tester */}
          <section className="ag-api-card">
            <div className="ag-api-card__head">
              <h3>
                <Zap size={15} /> {vi ? "Test API" : "Playground"}
              </h3>
            </div>

            <label className="agx-field">
              <span>{vi ? "Chọn mô hình kiểm tra" : "Model"}</span>
              <select
                className="agx-input"
                value={testModel}
                onChange={(e) => setTestModel(e.target.value)}
              >
                {models.map((m) => (
                  <option key={m.id} value={m.id}>
                    {m.id} — {m.displayName} [{m.supportedAccountCount}/{m.poolAccountCount} acc]
                  </option>
                ))}
              </select>
            </label>

            <label className="agx-field">
              <span>Prompt</span>
              <textarea
                className="agx-textarea"
                rows={4}
                value={testPrompt}
                onChange={(e) => setTestPrompt(e.target.value)}
              />
            </label>

            <div>
              <button
                type="button"
                className="agx-btn agx-btn--primary"
                disabled={busyKey === "test-chat" || !testPrompt.trim() || !status?.running}
                title={
                  !status?.running
                    ? vi
                      ? "Hãy bấm Chạy API trước khi test"
                      : "Start API Router before testing"
                    : undefined
                }
                onClick={() => void handleRunTestChat()}
              >
                {busyKey === "test-chat" ? (
                  <Loader2 size={14} className="spin" />
                ) : (
                  <Send size={14} />
                )}
                <span>
                  {!status?.running
                    ? vi
                      ? "API Router đang tắt (Bật API để test)"
                      : "API Router is OFF (Start to test)"
                    : vi
                      ? "Gửi yêu cầu thử"
                      : "Send Test Request"}
                </span>
              </button>
            </div>

            {testResult ? (
              <div className="ag-api-test-output">
                <div className="ag-api-test-meta">
                  <span className={testResult.ok ? "ag-badge-current" : "ag-badge-err"}>
                    HTTP {testResult.statusCode}
                  </span>
                  <span>{testResult.resolvedModel}</span>
                  {testResult.accountEmail ? <span>{testResult.accountEmail}</span> : null}
                  <span>{testResult.latencyMs} ms</span>
                  <span>
                    In: {testResult.inputTokens} · Think: {testResult.thinkingTokens} · Out:{" "}
                    {testResult.outputTokens}
                  </span>
                </div>
                {testResult.error ? (
                  <div className="ag-token-usage__error">{testResult.error}</div>
                ) : null}
                {testResult.thinking ? (
                  <details className="ag-api-thinking-box">
                    <summary>{vi ? "Quá trình suy luận (Thinking)" : "Thinking trace"}</summary>
                    <pre>{testResult.thinking}</pre>
                  </details>
                ) : null}
                {testResult.text ? (
                  <pre className="ag-api-reply-box">{testResult.text}</pre>
                ) : null}
              </div>
            ) : null}
          </section>
        </div>
      ) : null}

      {/* ====================================================================
          SUB-TAB 4: REQUEST LOGS
         ==================================================================== */}
      {subTab === "logs" ? (
        <section className="ag-api-card">
          <div className="ag-api-card__head">
            <div>
              <h3>
                {vi ? "Nhật ký gọi API" : "API Request Logs"} ({filteredLogs.length})
              </h3>
            </div>
            <div style={{ display: "flex", alignItems: "center", gap: 8, flexWrap: "wrap" }}>
              <div className="ag-conv-search">
                <Search size={14} />
                <input
                  type="text"
                  value={logSearch}
                  onChange={(e) => setLogSearch(e.target.value)}
                  placeholder={vi ? "Tìm model, email, lỗi..." : "Search model, account, error..."}
                />
              </div>
              <select
                className="agx-select ag-toolbar-select"
                value={logStatusFilter}
                onChange={(e) => setLogStatusFilter(e.target.value as "all" | "ok" | "err")}
              >
                <option value="all">{vi ? "Tất cả trạng thái" : "All status"}</option>
                <option value="ok">2xx OK</option>
                <option value="err">{vi ? "Lỗi (4xx/5xx)" : "Errors (4xx/5xx)"}</option>
              </select>
              <label className="ag-select-all-label">
                <input
                  type="checkbox"
                  checked={autoRefreshLogs}
                  onChange={(e) => setAutoRefreshLogs(e.target.checked)}
                />
                <span>{vi ? "Tự cập nhật" : "Live"}</span>
              </label>
              <button
                type="button"
                className="agx-btn agx-btn--secondary agx-btn--sm"
                onClick={() => void loadAll()}
              >
                <RefreshCw size={13} />
              </button>
              <button
                type="button"
                className="agx-btn agx-btn--danger agx-btn--sm"
                onClick={async () => {
                  if (!window.confirm(vi ? "Xóa toàn bộ nhật ký gọi API?" : "Clear all API logs?")) return;
                  await agxInvoke("api_service_clear_logs");
                  await loadAll();
                }}
              >
                <Trash2 size={13} />
                <span>{vi ? "Xóa nhật ký" : "Clear"}</span>
              </button>
            </div>
          </div>

          <div className="ag-token-table-wrap">
            <table className="ag-token-table">
              <thead>
                <tr>
                  <th>{vi ? "Thời gian" : "Time"}</th>
                  <th>Client IP</th>
                  <th>Status</th>
                  <th>Protocol</th>
                  <th>Model</th>
                  <th>{vi ? "Tài khoản" : "Account"}</th>
                  <th className="is-num">Latency</th>
                  <th className="is-num">Input</th>
                  <th className="is-num">Think</th>
                  <th className="is-num">Output</th>
                  <th className="is-num">{vi ? "Tổng" : "Total"}</th>
                </tr>
              </thead>
              <tbody>
                {filteredLogs.length === 0 ? (
                  <tr>
                    <td colSpan={11} style={{ textAlign: "center", padding: 28 }}>
                      {vi
                        ? "Chưa có lượt gọi API nào — thử gửi request ở tab Hướng dẫn & Test!"
                        : "No API requests logged yet."}
                    </td>
                  </tr>
                ) : (
                  filteredLogs.map((l) => (
                    <tr
                      key={l.id}
                      className="is-clickable"
                      onClick={() => setInspectLog(l)}
                    >
                      <td>
                        <Clock size={11} style={{ marginRight: 4, opacity: 0.6 }} />
                        {formatTime(l.timestamp)}
                      </td>
                      <td>
                        <code style={{ fontSize: 11.5 }}>{l.clientIp || "—"}</code>
                      </td>
                      <td>
                        <span
                          className={
                            l.statusCode >= 200 && l.statusCode < 300
                              ? "ag-badge-current"
                              : "ag-badge-err"
                          }
                        >
                          {l.statusCode}
                        </span>
                      </td>
                      <td>
                        <span className="ag-kind-badge is-main">
                          {l.protocol}
                          {l.stream ? " · SSE" : ""}
                        </span>
                      </td>
                      <td>
                        <strong>{l.requestModel}</strong>
                        {l.resolvedModel !== l.requestModel ? (
                          <div className="ag-token-cell-sub">→ {l.resolvedModel}</div>
                        ) : null}
                      </td>
                      <td>{l.accountEmail || "—"}</td>
                      <td className="is-num">{l.latencyMs} ms</td>
                      <td className="is-num">{formatExactNumber(l.inputTokens)}</td>
                      <td className="is-num">{formatExactNumber(l.thinkingTokens)}</td>
                      <td className="is-num">{formatExactNumber(l.outputTokens)}</td>
                      <td className="is-num">
                        <strong>{formatExactNumber(l.totalTokens)}</strong>
                      </td>
                    </tr>
                  ))
                )}
              </tbody>
            </table>
          </div>
        </section>
      ) : null}

      {/* ====================================================================
          SUB-TAB 5: API SERVICE USAGE ANALYTICS
         ==================================================================== */}
      {subTab === "usage" ? (
        <div className="ag-overview-wrap">
          <section className="ag-token-usage__surface">
            <div className="ag-token-usage__head">
              <div>
                <h3>{vi ? "Thống kê sử dụng API Router" : "API Router Usage Analytics"}</h3>
                <p>
                  {vi
                    ? "Thống kê riêng toàn bộ lượt gọi, độ trễ và lượng Token đi qua OnlyDEVS Antigravity API Service."
                    : "Tracks all requests, latency, and token consumption routed through OnlyDEVS Antigravity API Service."}
                </p>
              </div>
              <div style={{ display: "flex", gap: 6, flexWrap: "wrap" }}>
                {(["today", "7d", "30d", "all"] as const).map((r) => (
                  <button
                    key={r}
                    type="button"
                    className={`ag-token-strip__range${usageRange === r ? " is-active" : ""}`}
                    onClick={() => setUsageRange(r)}
                  >
                    {r === "today"
                      ? vi
                        ? "Hôm nay"
                        : "Today"
                      : r === "7d"
                        ? vi
                          ? "7 ngày"
                          : "7d"
                        : r === "30d"
                          ? vi
                            ? "30 ngày"
                            : "30d"
                          : vi
                            ? "Tất cả"
                            : "All"}
                  </button>
                ))}
                <button
                  type="button"
                  className="agx-btn agx-btn--secondary agx-btn--sm"
                  onClick={() => void loadAll()}
                >
                  <RefreshCw size={13} />
                </button>
              </div>
            </div>

            {usage ? (
              <>
                <section className="ag-token-usage__hero">
                  <div className="ag-token-usage__hero-head">
                    <div className="ag-token-usage__hero-total">
                      <div className="ag-token-usage__hero-icon">
                        <Zap size={24} />
                      </div>
                      <div className="ag-token-usage__hero-total-meta">
                        <span>{vi ? "Tổng Token qua API Router" : "Total API Router Tokens"}</span>
                        <strong>{formatTokensShort(usage.totalTokens, 2)}</strong>
                        <span className="ag-token-usage__hero-sub">
                          {formatExactNumber(usage.totalTokens)} tokens
                        </span>
                      </div>
                    </div>

                    <div className="ag-token-usage__hero-side">
                      <span>{vi ? "Tổng Request" : "Total Requests"}</span>
                      <strong>{formatExactNumber(usage.totalRequests)}</strong>
                      <span>{vi ? "Tỷ lệ thành công" : "Success Rate"}</span>
                      <strong>{usage.successRate.toFixed(1)}%</strong>
                      <span>{vi ? "Độ trễ trung bình" : "Avg Latency"}</span>
                      <strong>{usage.avgLatencyMs} ms</strong>
                      <span>{vi ? "Lỗi" : "Errors"}</span>
                      <strong>{formatExactNumber(usage.errorRequests)}</strong>
                    </div>
                  </div>

                  <div className="ag-token-usage__cards">
                    <article className="ag-token-card is-input">
                      <div className="ag-token-card__head">
                        <span>Input Tokens</span>
                      </div>
                      <strong>{formatTokensShort(usage.inputTokens, 2)}</strong>
                      <span className="ag-token-card__foot">
                        {formatExactNumber(usage.inputTokens)}
                      </span>
                    </article>
                    <article className="ag-token-card is-thinking">
                      <div className="ag-token-card__head">
                        <span>Thinking Tokens</span>
                      </div>
                      <strong>{formatTokensShort(usage.thinkingTokens, 1)}</strong>
                      <span className="ag-token-card__foot">
                        {formatExactNumber(usage.thinkingTokens)}
                      </span>
                    </article>
                    <article className="ag-token-card is-output">
                      <div className="ag-token-card__head">
                        <span>Output Tokens</span>
                      </div>
                      <strong>{formatTokensShort(usage.outputTokens, 1)}</strong>
                      <span className="ag-token-card__foot">
                        {formatExactNumber(usage.outputTokens)}
                      </span>
                    </article>
                    <article className="ag-token-card is-cache">
                      <div className="ag-token-card__head">
                        <span>Cache Read</span>
                      </div>
                      <strong>{formatTokensShort(usage.cacheReadTokens, 2)}</strong>
                      <span className="ag-token-card__foot">
                        {formatExactNumber(usage.cacheReadTokens)}
                      </span>
                    </article>
                  </div>
                </section>

                {/* Time-series mini bar chart */}
                {usage.timeSeries.length > 0 ? (
                  <section className="ag-token-section">
                    <div className="ag-token-section__head">
                      <h4>{vi ? "Lưu lượng theo thời gian" : "Traffic Over Time"}</h4>
                    </div>
                    <div className="ag-api-bars-wrap">
                      {(() => {
                        const maxVal = Math.max(
                          1,
                          ...usage.timeSeries.map((p) => p.totalTokens || p.requests),
                        );
                        return usage.timeSeries.map((pt) => {
                          const metric = pt.totalTokens || pt.requests;
                          const pct = Math.max(6, Math.round((metric / maxVal) * 100));
                          return (
                            <div
                              key={pt.bucket}
                              className="ag-api-bar-col"
                              title={`${pt.bucket}: ${pt.requests} reqs · ${formatExactNumber(pt.totalTokens)} tokens`}
                            >
                              <div className="ag-api-bar-track">
                                <div className="ag-api-bar-fill" style={{ height: `${pct}%` }} />
                              </div>
                              <span>{pt.bucket}</span>
                            </div>
                          );
                        });
                      })()}
                    </div>
                  </section>
                ) : null}

                {/* Breakdown by Model & by Account */}
                <div className="ag-token-usage__grids">
                  <section className="ag-token-section">
                    <div className="ag-token-section__head">
                      <h4>{vi ? "Theo mô hình" : "By Model"}</h4>
                    </div>
                    <div className="ag-token-table-wrap">
                      <table className="ag-token-table">
                        <thead>
                          <tr>
                            <th>Model</th>
                            <th className="is-num">Reqs</th>
                            <th className="is-num">Avg ms</th>
                            <th className="is-num">Tokens</th>
                          </tr>
                        </thead>
                        <tbody>
                          {usage.byModel.map((row) => (
                            <tr key={row.key}>
                              <td><strong>{row.label}</strong></td>
                              <td className="is-num">{row.requests}</td>
                              <td className="is-num">{row.avgLatencyMs}</td>
                              <td className="is-num">
                                <strong>{formatTokensShort(row.totalTokens, 2)}</strong>
                              </td>
                            </tr>
                          ))}
                        </tbody>
                      </table>
                    </div>
                  </section>

                  <section className="ag-token-section">
                    <div className="ag-token-section__head">
                      <h4>{vi ? "Theo tài khoản Antigravity" : "By Antigravity Account"}</h4>
                    </div>
                    <div className="ag-token-table-wrap">
                      <table className="ag-token-table">
                        <thead>
                          <tr>
                            <th>{vi ? "Tài khoản" : "Account"}</th>
                            <th className="is-num">Reqs</th>
                            <th className="is-num">Err</th>
                            <th className="is-num">Tokens</th>
                          </tr>
                        </thead>
                        <tbody>
                          {usage.byAccount.map((row) => (
                            <tr key={row.key}>
                              <td><strong>{row.label}</strong></td>
                              <td className="is-num">{row.requests}</td>
                              <td className="is-num">{row.errors}</td>
                              <td className="is-num">
                                <strong>{formatTokensShort(row.totalTokens, 2)}</strong>
                              </td>
                            </tr>
                          ))}
                        </tbody>
                      </table>
                    </div>
                  </section>
                </div>
              </>
            ) : null}
          </section>
        </div>
      ) : null}

      {/* ====================================================================
          MODALS (Member Pool Picker, Create API Key, Inspect Log)
         ==================================================================== */}
      {memberModalOpen ? (
        <div className="agx-modal-backdrop" onClick={() => setMemberModalOpen(false)}>
          <div className="agx-modal" onClick={(e) => e.stopPropagation()}>
            <div className="agx-modal-head">
              <h3>{vi ? "Quản lý thành viên Pool API" : "Manage API Pool Members"}</h3>
              <button
                type="button"
                className="agx-icon-btn"
                onClick={() => setMemberModalOpen(false)}
              >
                <X size={16} />
              </button>
            </div>
            <div className="agx-modal-body">
              <label className="ag-select-all-label" style={{ marginBottom: 10 }}>
                <input
                  type="checkbox"
                  checked={useAllWhenEmptyDraft}
                  onChange={(e) => setUseAllWhenEmptyDraft(e.target.checked)}
                />
                <span>
                  {vi
                    ? "Nếu không chọn cụ thể, tự động dùng toàn bộ tài khoản hợp lệ"
                    : "Use all valid accounts when none selected"}
                </span>
              </label>

              <div className="ag-group-member-list">
                {accounts.map((acc) => {
                  const prof = classifyAccountModelProfile(acc);
                  return (
                    <label key={acc.id} className="ag-group-member-item">
                      <input
                        type="checkbox"
                        checked={memberDraftIds.has(acc.id)}
                        onChange={() => {
                          setMemberDraftIds((prev) => {
                            const next = new Set(prev);
                            if (next.has(acc.id)) next.delete(acc.id);
                            else next.add(acc.id);
                            return next;
                          });
                        }}
                      />
                      <span style={{ flex: 1, minWidth: 0, overflow: "hidden", textOverflow: "ellipsis" }}>
                        {acc.email}
                      </span>
                      <span
                        className={`ag-api-profile-pill ${
                          prof.kind === "full_nextgen"
                            ? "is-full"
                            : prof.kind === "classic"
                              ? "is-classic"
                              : "is-unknown"
                        }`}
                      >
                        {prof.kind === "full_nextgen"
                          ? "PRO · Argon"
                          : prof.kind === "classic"
                            ? "ProLite · Hydrogen/C4.6"
                            : vi
                              ? "Chưa quét"
                              : "Unscanned"}
                      </span>
                    </label>
                  );
                })}
              </div>
            </div>
            <div className="agx-modal-foot">
              <button
                type="button"
                className="agx-btn agx-btn--secondary"
                onClick={() => setMemberModalOpen(false)}
              >
                {vi ? "Hủy" : "Cancel"}
              </button>
              <button
                type="button"
                className="agx-btn agx-btn--primary"
                onClick={() => void handleSaveMembers()}
              >
                <Save size={14} />
                <span>{vi ? "Lưu thành viên" : "Save Members"}</span>
              </button>
            </div>
          </div>
        </div>
      ) : null}

      {keyModalOpen ? (
        <div className="agx-modal-backdrop" onClick={() => setKeyModalOpen(false)}>
          <div className="agx-modal" onClick={(e) => e.stopPropagation()}>
            <div className="agx-modal-head">
              <h3>{vi ? "Tạo API Key mới" : "Create New API Key"}</h3>
              <button
                type="button"
                className="agx-icon-btn"
                onClick={() => setKeyModalOpen(false)}
              >
                <X size={16} />
              </button>
            </div>
            <div className="agx-modal-body">
              <div className="ag-modal-stack">
                <label className="agx-field">
                  <span>{vi ? "Tên khóa (Label)" : "Key Label"}</span>
                  <input
                    type="text"
                    className="agx-input"
                    value={newKeyLabel}
                    onChange={(e) => setNewKeyLabel(e.target.value)}
                    placeholder="Claude Code / Cursor / Team..."
                  />
                </label>
                <label className="agx-field">
                  <span>
                    {vi
                      ? "Khóa theo nhóm Pro (bảo vệ quota Argon / G4)"
                      : "Pro Pool Tier Lock"}
                  </span>
                  <select
                    className="agx-input"
                    value={newKeyProfile}
                    onChange={(e) =>
                      setNewKeyProfile(e.target.value as "all" | "classic" | "full_nextgen")
                    }
                  >
                    <option value="all">
                      {vi ? "Tất cả nhóm Pro (Mặc định)" : "All Pro tiers (Default)"}
                    </option>
                    <option value="classic">
                      {vi
                        ? "Chỉ ProLite (Hydrogen / G3.8 / Claude 4.6 — giữ nguyên Argon/Carbon)"
                        : "ProLite only (Hydrogen / G3.8 / Claude 4.6)"}
                    </option>
                    <option value="full_nextgen">
                      {vi
                        ? "Chỉ PRO · Full Next-Gen (Argon / G4 / Claude 5.5)"
                        : "PRO · Full Next-Gen only (Argon / G4 / C5.5)"}
                    </option>
                  </select>
                </label>

                <div className="ag-vault-grid">
                  <label className="agx-field">
                    <span>
                      {vi ? "Giới hạn tốc độ (Req/phút, 0 = không)" : "Rate Limit (RPM, 0 = none)"}
                    </span>
                    <input
                      type="number"
                      min={0}
                      max={600}
                      className="agx-input"
                      value={newKeyRpm}
                      onChange={(e) => setNewKeyRpm(e.target.value)}
                      placeholder="0"
                    />
                  </label>
                  <label className="agx-field">
                    <span>
                      {vi ? "Hạn mức Token/ngày (0 = không)" : "Daily Token Cap (0 = none)"}
                    </span>
                    <input
                      type="number"
                      min={0}
                      step={50000}
                      className="agx-input"
                      value={newKeyDailyTokens}
                      onChange={(e) => setNewKeyDailyTokens(e.target.value)}
                      placeholder="0"
                    />
                  </label>
                  <label className="agx-field">
                    <span>
                      {vi ? "Hết hạn sau (ngày, 0 = vĩnh viễn)" : "Expires in (days, 0 = never)"}
                    </span>
                    <input
                      type="number"
                      min={0}
                      max={365}
                      className="agx-input"
                      value={newKeyExpireDays}
                      onChange={(e) => setNewKeyExpireDays(e.target.value)}
                      placeholder="0"
                    />
                  </label>
                </div>

                <label className="agx-field">
                  <span>
                    {vi
                      ? "Giới hạn Model (phân tách dấu phẩy, để trống = tất cả)"
                      : "Allowed Models (comma-separated, empty = all)"}
                  </span>
                  <input
                    type="text"
                    className="agx-input"
                    value={newKeyModels}
                    onChange={(e) => setNewKeyModels(e.target.value)}
                    placeholder="claude-sonnet-4-6, gemini-*"
                  />
                </label>
                <div className="ag-group-member-picker">
                  <span className="agx-muted">
                    {vi
                      ? "Ghim tài khoản riêng cho Key này (để trống = dùng chung Pool):"
                      : "Pin specific accounts for this key (empty = use global pool):"}
                  </span>
                  <div className="ag-group-member-list">
                    {accounts.map((a) => (
                      <label key={a.id} className="ag-group-member-item">
                        <input
                          type="checkbox"
                          checked={newKeyPinned.has(a.id)}
                          onChange={() => {
                            setNewKeyPinned((prev) => {
                              const next = new Set(prev);
                              if (next.has(a.id)) next.delete(a.id);
                              else next.add(a.id);
                              return next;
                            });
                          }}
                        />
                        <span>{a.email}</span>
                      </label>
                    ))}
                  </div>
                </div>
              </div>
            </div>
            <div className="agx-modal-foot">
              <button
                type="button"
                className="agx-btn agx-btn--secondary"
                onClick={() => setKeyModalOpen(false)}
              >
                {vi ? "Hủy" : "Cancel"}
              </button>
              <button
                type="button"
                className="agx-btn agx-btn--primary"
                disabled={!newKeyLabel.trim() || busyKey === "create-key"}
                onClick={() => void handleCreateKey()}
              >
                <Plus size={14} />
                <span>{vi ? "Tạo khóa" : "Create Key"}</span>
              </button>
            </div>
          </div>
        </div>
      ) : null}

      {inspectLog ? (
        <div className="agx-modal-backdrop" onClick={() => setInspectLog(null)}>
          <div
            className="agx-modal agx-modal--wide"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="agx-modal-head">
              <div>
                <h3>
                  HTTP {inspectLog.statusCode} · {inspectLog.requestModel} →{" "}
                  {inspectLog.resolvedModel}
                </h3>
                <span className="agx-muted">
                  {formatTime(inspectLog.timestamp)}
                  {inspectLog.clientIp ? ` · IP ${inspectLog.clientIp}` : ""} ·{" "}
                  {inspectLog.accountEmail || "—"} · {inspectLog.apiKeyLabel} ·{" "}
                  {inspectLog.latencyMs}ms
                </span>
              </div>
              <button
                type="button"
                className="agx-icon-btn"
                onClick={() => setInspectLog(null)}
              >
                <X size={16} />
              </button>
            </div>
            <div className="agx-modal-body">
              <div className="ag-conv-detail__kpis" style={{ marginBottom: 12 }}>
                <div>
                  <span>Input</span>
                  <strong>{formatExactNumber(inspectLog.inputTokens)}</strong>
                </div>
                <div>
                  <span>Thinking</span>
                  <strong>{formatExactNumber(inspectLog.thinkingTokens)}</strong>
                </div>
                <div>
                  <span>Output</span>
                  <strong>{formatExactNumber(inspectLog.outputTokens)}</strong>
                </div>
                <div>
                  <span>Cache</span>
                  <strong>{formatExactNumber(inspectLog.cacheReadTokens)}</strong>
                </div>
                <div>
                  <span>Total</span>
                  <strong>{formatExactNumber(inspectLog.totalTokens)}</strong>
                </div>
              </div>
              {inspectLog.errorMessage ? (
                <div className="ag-token-usage__error" style={{ marginBottom: 10 }}>
                  {inspectLog.errorMessage}
                </div>
              ) : null}
              {inspectLog.requestPreview ? (
                <div style={{ marginBottom: 10 }}>
                  <span className="agx-muted">Request preview:</span>
                  <pre className="ag-api-reply-box">{inspectLog.requestPreview}</pre>
                </div>
              ) : null}
              {inspectLog.responsePreview ? (
                <div>
                  <span className="agx-muted">Response preview:</span>
                  <pre className="ag-api-reply-box">{inspectLog.responsePreview}</pre>
                </div>
              ) : null}
            </div>
          </div>
        </div>
      ) : null}
    </div>
  );
}
