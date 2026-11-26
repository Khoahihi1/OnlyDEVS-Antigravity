import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  AlertTriangle,
  ArrowDown,
  ArrowUp,
  BarChart3,
  Check,
  Clock,
  Copy,
  Download,
  ExternalLink,
  Eye,
  EyeOff,
  FileJson,
  FileText,
  FolderOpen,
  FolderPlus,
  GripVertical,
  History,
  Info,
  KeyRound,
  LayoutGrid,
  List,
  Loader2,
  Monitor,
  Play,
  Plus,
  Radio,
  RefreshCw,
  Rows3,
  Save,
  Search,
  Sparkles,
  Tag,
  Terminal,
  Trash2,
  Upload,
  UserCheck,
  Users,
  X,
  Zap,
} from "lucide-react";
import type {
  AccountGroup,
  AgUsageRangePreset,
  AntigravityAccount,
  ApiServiceStatus,
  AppLanguage,
  AutoSwitchConfig,
  AutoSwitchEvalResult,
  DashboardBundleResponse,
  LaunchTargetInfo,
  OAuthPrepareResult,
  RefreshStats,
  SwitchHistoryItem,
  TokenUsageSummary,
  TotpCodeResult,
} from "../types";
import { agxInvoke, isTauri } from "../platform/invoke";
import { tr } from "../i18n";
import {
  AntigravityQuotaSection,
  formatResetCountdown,
  getAntigravityOverallScore,
  getAntigravityQuotaDisplayItems,
  getEarliestResetMs,
  getQuotaBucketPercentage,
  isAccountNeedsReauth,
  normalizeTierBadge,
} from "../components/AntigravityQuotaSection";
import { AntigravityTokenUsagePanel } from "../components/AntigravityTokenUsagePanel";
import {
  formatExactNumber,
  formatTokensShort,
} from "../components/AntigravityUsageTrend";

type AccountsPageProps = {
  lang: AppLanguage;
  accounts: AntigravityAccount[];
  tokenUsage: TokenUsageSummary | null;
  switchHistory: SwitchHistoryItem[];
  loading: boolean;
  apiServiceStatus: ApiServiceStatus | null;
  onToggleApiPool: (accountId: string) => Promise<void>;
  onRefreshAllData: () => Promise<void>;
  onImportLocalAccount: () => Promise<void>;
  onAddByToken: (refreshToken: string, emailHint?: string) => Promise<void>;
  onImportContent: (content: string) => Promise<void>;
  onExportAccounts: (
    accountIds: string[],
    includeSensitive: boolean,
  ) => Promise<string>;
  onFetchQuota: (accountId: string) => Promise<void>;
  onRefreshAllQuotas: () => Promise<RefreshStats>;
  onSwitchAccount: (
    accountId: string,
    switchTarget: string,
    launch: boolean,
    customPath?: string | null,
    workingDir?: string | null,
  ) => Promise<void>;
  onUpdateMetadata: (payload: {
    accountId: string;
    tags?: string[];
    notes?: string;
    twoFactorSecret?: string;
    accountPassword?: string;
    phoneNumber?: string;
    mailUrl?: string;
    auxEmail?: string;
  }) => Promise<void>;
  onDeleteAccounts: (accountIds: string[]) => Promise<void>;
  onPrepareOAuth: () => Promise<OAuthPrepareResult>;
  onStartOAuthLogin: () => Promise<void>;
  onCompleteOAuthLogin: () => Promise<void>;
  onSubmitOAuthCallback: (callbackUrl: string) => Promise<void>;
  onCancelOAuthLogin: () => Promise<void>;
  onGenerateTotp: (secret: string) => Promise<TotpCodeResult>;
  onClearSwitchHistory: () => Promise<void>;
};

type AccountSubTab = "overview" | "usage";
type ViewMode = "grid" | "list" | "compact";
type TierFilter =
  | "ALL"
  | "PRO"
  | "ProLite"
  | "ULTRA"
  | "FREE"
  | "VALID"
  | "REAUTH"
  | "FORBIDDEN";
type SortField =
  | "overall"
  | "created_at"
  | "last_switched_at"
  | "gemini:5h"
  | "gemini:weekly"
  | "claude:5h"
  | "claude:weekly"
  | "argon:weekly"
  | "reset_time"
  | "email"
  | "custom";

const QUICK_RANGES: AgUsageRangePreset[] = [
  "today",
  "7d",
  "30d",
  "month",
  "all",
];

function maskEmail(email: string, privacy: boolean): string {
  if (!privacy) return email;
  const parts = email.split("@");
  if (parts.length !== 2) return "••••••";
  const name = parts[0];
  const visible = name.length <= 2 ? name[0] ?? "•" : name.slice(0, 2);
  return `${visible}••••@${parts[1]}`;
}

function formatDateTimeFull(tsSec?: number | null): string {
  if (!tsSec) return "—";
  const d = new Date(tsSec * 1000);
  if (Number.isNaN(d.getTime())) return "—";
  const dd = String(d.getDate()).padStart(2, "0");
  const mm = String(d.getMonth() + 1).padStart(2, "0");
  const yyyy = d.getFullYear();
  const hh = String(d.getHours()).padStart(2, "0");
  const mi = String(d.getMinutes()).padStart(2, "0");
  return `${dd}/${mm}/${yyyy} ${hh}:${mi}`;
}

export function AccountsPage({
  lang,
  accounts,
  switchHistory,
  loading,
  apiServiceStatus,
  onToggleApiPool,
  onRefreshAllData,
  onImportLocalAccount,
  onAddByToken,
  onImportContent,
  onExportAccounts,
  onFetchQuota,
  onRefreshAllQuotas,
  onSwitchAccount,
  onUpdateMetadata,
  onDeleteAccounts,
  onPrepareOAuth,
  onStartOAuthLogin,
  onCompleteOAuthLogin,
  onSubmitOAuthCallback,
  onCancelOAuthLogin,
  onGenerateTotp,
  onClearSwitchHistory,
}: AccountsPageProps) {
  const vi = lang === "vi";
  const fileInputRef = useRef<HTMLInputElement | null>(null);

  // 2-tab navigation ("Tổng quan tài khoản" | "Mức dùng phiên")
  const [subTab, setSubTab] = useState<AccountSubTab>("overview");

  // Local Token Usage Summary Strip
  const [usageRange, setUsageRange] = useState<AgUsageRangePreset>("30d");
  const [usageBundle, setUsageBundle] =
    useState<DashboardBundleResponse | null>(null);
  const [usageSyncing, setUsageSyncing] = useState<boolean>(false);

  // Overview toolbar & filters
  const [searchQuery, setSearchQuery] = useState("");
  const [viewMode, setViewMode] = useState<ViewMode>("grid");
  const [tierFilter, setTierFilter] = useState<TierFilter>("ALL");
  const [tagFilter, setTagFilter] = useState<string>("ALL");
  const [groupByTag, setGroupByTag] = useState<boolean>(false);
  const [sortBy, setSortBy] = useState<SortField>("overall");
  const [sortDir, setSortDir] = useState<"desc" | "asc">("desc");
  const [privacyMode, setPrivacyMode] = useState(false);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());

  // Account groups & custom sort order
  const [groups, setGroups] = useState<AccountGroup[]>([]);
  const [activeGroupId, setActiveGroupId] = useState<string>("ALL");
  const [customSortOrder, setCustomSortOrder] = useState<string[]>([]);

  // Busy flags & feedback
  const [busyKey, setBusyKey] = useState<string | null>(null);
  const [localMsg, setLocalMsg] = useState<{
    tone: "ok" | "err";
    text: string;
  } | null>(null);

  // Modals
  const [addModalOpen, setAddModalOpen] = useState(false);
  const [addTab, setAddTab] = useState<
    "oauth" | "local" | "token" | "batch" | "json"
  >("oauth");
  const [oauthPrep, setOauthPrep] = useState<OAuthPrepareResult | null>(null);
  const [oauthPrepLoading, setOauthPrepLoading] = useState(false);
  const [oauthWorking, setOauthWorking] = useState(false);
  const [oauthStatusText, setOauthStatusText] = useState<string | null>(null);
  const [oauthError, setOauthError] = useState<string | null>(null);
  const [oauthCallbackUrl, setOauthCallbackUrl] = useState("");
  const addModalOpenRef = useRef(addModalOpen);
  const addTabRef = useRef(addTab);
  const oauthWorkingRef = useRef(oauthWorking);
  const oauthPrepRef = useRef(oauthPrep);
  const oauthPrepInFlightRef = useRef(false);
  const onPrepareOAuthRef = useRef(onPrepareOAuth);
  const onCompleteOAuthLoginRef = useRef(onCompleteOAuthLogin);
  const onCancelOAuthLoginRef = useRef(onCancelOAuthLogin);
  addModalOpenRef.current = addModalOpen;
  addTabRef.current = addTab;
  oauthWorkingRef.current = oauthWorking;
  oauthPrepRef.current = oauthPrep;
  onPrepareOAuthRef.current = onPrepareOAuth;
  onCompleteOAuthLoginRef.current = onCompleteOAuthLogin;
  onCancelOAuthLoginRef.current = onCancelOAuthLogin;
  const [tokenInput, setTokenInput] = useState("");
  const [tokenEmailHint, setTokenEmailHint] = useState("");
  const [batchInput, setBatchInput] = useState("");
  const [jsonInput, setJsonInput] = useState("");

  // Launch / Switch Account Modal (opens ONLY when Play is clicked)
  const [launchAccount, setLaunchAccount] = useState<AntigravityAccount | null>(null);
  const [launchTargets, setLaunchTargets] = useState<LaunchTargetInfo[]>([]);
  const [selectedLaunchTarget, setSelectedLaunchTarget] = useState<string>(() => {
    try {
      return window.localStorage.getItem("onlydevs.launchTarget") || "antigravity_ide";
    } catch {
      return "antigravity_ide";
    }
  });
  const [launchCustomPath, setLaunchCustomPath] = useState("");
  const [launchWorkingDir, setLaunchWorkingDir] = useState("");
  const [launchError, setLaunchError] = useState<string | null>(null);

  const isAccountInApiPool = useCallback(
    (accountId: string): boolean => {
      const cfg = apiServiceStatus?.config;
      if (!cfg) return false;
      if (cfg.memberAccountIds.length === 0 && cfg.useAllAccountsWhenEmpty) {
        return true;
      }
      return cfg.memberAccountIds.includes(accountId);
    },
    [apiServiceStatus],
  );

  const openLaunchPopup = async (acc: AntigravityAccount) => {
    setLaunchAccount(acc);
    setLaunchError(null);
    setLaunchCustomPath("");
    try {
      const targets = await agxInvoke<LaunchTargetInfo[]>("detect_launch_targets");
      setLaunchTargets(targets ?? []);
      if (targets && targets.length > 0) {
        const currentValid = targets.find((t) => t.id === selectedLaunchTarget && t.available);
        if (!currentValid) {
          const firstAvail = targets.find((t) => t.available) ?? targets[0];
          setSelectedLaunchTarget(firstAvail.id);
        }
      }
    } catch {
      setLaunchTargets([]);
    }
  };

  const executeLaunchOrSwitch = async (launch: boolean) => {
    if (!launchAccount) return;
    setBusyKey(launch ? `launch-${launchAccount.id}` : `switch-${launchAccount.id}`);
    setLaunchError(null);
    try {
      try {
        window.localStorage.setItem("onlydevs.launchTarget", selectedLaunchTarget);
      } catch {
        // ignore
      }
      await onSwitchAccount(
        launchAccount.id,
        selectedLaunchTarget,
        launch,
        launchCustomPath.trim() || null,
        selectedLaunchTarget === "cli" ? launchWorkingDir.trim() || null : null,
      );
      setLaunchAccount(null);
    } catch (e) {
      setLaunchError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusyKey(null);
    }
  };

  // Account Info / Full Model Quota modal
  const [infoAccount, setInfoAccount] = useState<AntigravityAccount | null>(
    null,
  );

  // Tag Quick Editor modal
  const [tagAccount, setTagAccount] = useState<AntigravityAccount | null>(null);
  const [tagDraftInput, setTagDraftInput] = useState("");

  // Vault (Notes / Password / 2FA TOTP / Recovery) modal
  const [vaultAccount, setVaultAccount] = useState<AntigravityAccount | null>(
    null,
  );
  const [vaultForm, setVaultForm] = useState({
    tags: "",
    notes: "",
    accountPassword: "",
    twoFactorSecret: "",
    phoneNumber: "",
    mailUrl: "",
    auxEmail: "",
  });
  const [showVaultPassword, setShowVaultPassword] = useState(false);
  const [totpLive, setTotpLive] = useState<TotpCodeResult | null>(null);

  // Group Manager modal
  const [groupModalOpen, setGroupModalOpen] = useState(false);
  const [editingGroupId, setEditingGroupId] = useState<string | null>(null);
  const [groupNameInput, setGroupNameInput] = useState("");
  const [groupMemberIds, setGroupMemberIds] = useState<Set<string>>(new Set());

  // Custom Sort Reorder modal
  const [customSortModalOpen, setCustomSortModalOpen] = useState(false);
  const [sortDraftIds, setSortDraftIds] = useState<string[]>([]);

  // Export JSON modal
  const [exportModalOpen, setExportModalOpen] = useState(false);
  const [exportJsonText, setExportJsonText] = useState("");
  const [exportIncludeSensitive, setExportIncludeSensitive] = useState(true);
  const [exportTargetIds, setExportTargetIds] = useState<string[]>([]);

  // Switch History modal
  const [historyModalOpen, setHistoryModalOpen] = useState(false);

  // Auto-Switch config & modal state
  const [autoSwitchModalOpen, setAutoSwitchModalOpen] = useState(false);
  const [autoSwitchScopeOpen, setAutoSwitchScopeOpen] = useState(false);
  const [autoSwitchSaving, setAutoSwitchSaving] = useState(false);
  const [autoSwitchChecking, setAutoSwitchChecking] = useState(false);
  const [autoSwitchCfg, setAutoSwitchCfg] = useState<AutoSwitchConfig>({
    enabled: false,
    threshold: 5,
    splitWindowThresholds: false,
    fiveHourThreshold: 5,
    weeklyThreshold: 5,
    creditsEnabled: false,
    creditsThreshold: 100,
    scopeMode: "any_group",
    selectedGroupIds: ["gemini-5h", "gemini-weekly", "claude-5h", "claude-weekly"],
    accountScopeMode: "all_accounts",
    selectedAccountIds: [],
    quotaAlertEnabled: false,
    quotaAlertThreshold: 20,
    pollIntervalMinutes: 3,
    switchTarget: "ide",
  });

  const [copiedId, setCopiedId] = useState<string | null>(null);

  const flashMsg = (tone: "ok" | "err", text: string) => {
    setLocalMsg({ tone, text });
    window.setTimeout(() => setLocalMsg(null), 4500);
  };

  const copyText = async (text: string, key: string) => {
    try {
      await navigator.clipboard.writeText(text);
      setCopiedId(key);
      window.setTimeout(() => setCopiedId(null), 1600);
    } catch {
      // ignore clipboard errors
    }
  };

  // Load groups & custom sort order on mount
  const loadGroupsAndSort = useCallback(async () => {
    try {
      const [gList, sortList] = await Promise.all([
        agxInvoke<AccountGroup[]>("list_account_groups"),
        agxInvoke<string[]>("get_account_custom_sort"),
      ]);
      setGroups(gList ?? []);
      setCustomSortOrder(sortList ?? []);
    } catch {
      // ignore
    }
  }, []);

  useEffect(() => {
    void loadGroupsAndSort();
  }, [loadGroupsAndSort]);

  useEffect(() => {
    void agxInvoke<AutoSwitchConfig>("get_auto_switch_config")
      .then((cfg) => {
        if (cfg) setAutoSwitchCfg(cfg);
      })
      .catch(() => {});
  }, []);

  useEffect(() => {
    if (!isTauri()) return;
    let unlistenSwitched: (() => void) | null = null;
    let unlistenAlert: (() => void) | null = null;
    void import("@tauri-apps/api/event").then(async ({ listen }) => {
      unlistenSwitched = await listen<AutoSwitchEvalResult>(
        "auto-switch-executed",
        (ev) => {
          const p = ev.payload;
          flashMsg(
            "ok",
            vi
              ? `Tự động chuyển sang ${p.toEmail ?? "tài khoản mới"} (${p.reason})`
              : `Auto-switched to ${p.toEmail ?? "next account"} (${p.reason})`,
          );
          void onRefreshAllData();
        },
      );
      unlistenAlert = await listen<string>("auto-switch-alert", (ev) => {
        flashMsg("err", vi ? `Cảnh báo hạn mức: ${ev.payload}` : `Quota alert: ${ev.payload}`);
      });
    });
    return () => {
      unlistenSwitched?.();
      unlistenAlert?.();
    };
  }, [onRefreshAllData, vi]);

  // Load Local Token Summary Strip for active range
  const loadTokenSummary = useCallback(
    async (preset: AgUsageRangePreset, forceSync: boolean) => {
      setUsageSyncing(true);
      try {
        const cmd = forceSync
          ? "antigravity_sync_token_usage"
          : "antigravity_query_token_usage";
        const args = forceSync
          ? {
              rebuild: false,
              query: {
                preset,
                pageSize: 1,
                tzOffsetMinutes: new Date().getTimezoneOffset(),
              },
            }
          : {
              query: {
                preset,
                pageSize: 1,
                tzOffsetMinutes: new Date().getTimezoneOffset(),
              },
            };
        const res = await agxInvoke<DashboardBundleResponse>(cmd, args);
        setUsageBundle(res);
      } catch {
        // ignore summary errors
      } finally {
        setUsageSyncing(false);
      }
    },
    [],
  );

  useEffect(() => {
    void loadTokenSummary(usageRange, false);
  }, [loadTokenSummary, usageRange]);

  // Live TOTP countdown in Vault modal
  useEffect(() => {
    if (!vaultAccount || !vaultForm.twoFactorSecret.trim()) {
      setTotpLive(null);
      return;
    }
    let cancelled = false;
    const tick = async () => {
      try {
        const res = await onGenerateTotp(vaultForm.twoFactorSecret.trim());
        if (!cancelled) setTotpLive(res);
      } catch {
        if (!cancelled) setTotpLive(null);
      }
    };
    void tick();
    const timer = window.setInterval(() => void tick(), 1000);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [vaultAccount, vaultForm.twoFactorSecret, onGenerateTotp]);

  // Derived tag & tier stats
  const allTags = useMemo(() => {
    const s = new Set<string>();
    for (const acc of accounts) {
      for (const t of acc.tags) {
        if (t.trim()) s.add(t.trim());
      }
    }
    return Array.from(s).sort();
  }, [accounts]);

  const tierCounts = useMemo(() => {
    let pro = 0;
    let proLite = 0;
    let ultra = 0;
    let free = 0;
    let reauth = 0;
    let forbidden = 0;
    let valid = 0;
    for (const acc of accounts) {
      const t = normalizeTierBadge(acc.quota?.subscriptionTier, acc);
      if (t === "PRO") pro++;
      else if (t === "ProLite") proLite++;
      else if (t === "ULTRA") ultra++;
      else free++;
      const needs = isAccountNeedsReauth(acc);
      const forb = Boolean(acc.quota?.isForbidden);
      if (needs) reauth++;
      if (forb) forbidden++;
      if (!needs && !forb && !acc.disabled) valid++;
    }
    return { all: accounts.length, pro, proLite, ultra, free, valid, reauth, forbidden };
  }, [accounts]);

  const activeGroup = useMemo(
    () => groups.find((g) => g.id === activeGroupId) ?? null,
    [groups, activeGroupId],
  );

  // Filtered & sorted accounts
  const filteredAccounts = useMemo(() => {
    const q = searchQuery.trim().toLowerCase();
    const customRank = new Map<string, number>();
    customSortOrder.forEach((id, idx) => customRank.set(id, idx));

    const list = accounts.filter((acc) => {
      if (activeGroup && !activeGroup.accountIds.includes(acc.id)) return false;

      if (tierFilter !== "ALL") {
        const t = normalizeTierBadge(acc.quota?.subscriptionTier, acc);
        const needs = isAccountNeedsReauth(acc);
        const forb = Boolean(acc.quota?.isForbidden);
        if (tierFilter === "PRO" && t !== "PRO") return false;
        if (tierFilter === "ProLite" && t !== "ProLite") return false;
        if (tierFilter === "ULTRA" && t !== "ULTRA") return false;
        if (tierFilter === "FREE" && t !== "FREE") return false;
        if (tierFilter === "REAUTH" && !needs) return false;
        if (tierFilter === "FORBIDDEN" && !forb) return false;
        if (tierFilter === "VALID" && (needs || forb || acc.disabled))
          return false;
      }

      if (tagFilter !== "ALL" && !acc.tags.includes(tagFilter)) return false;

      if (q) {
        const hay = `${acc.email} ${acc.name ?? ""} ${acc.notes ?? ""} ${acc.tags.join(" ")}`.toLowerCase();
        if (!hay.includes(q)) return false;
      }
      return true;
    });

    const mult = sortDir === "desc" ? -1 : 1;

    list.sort((a, b) => {
      if (sortBy === "custom") {
        const ra = customRank.get(a.id) ?? Number.MAX_SAFE_INTEGER;
        const rb = customRank.get(b.id) ?? Number.MAX_SAFE_INTEGER;
        if (ra !== rb) return ra - rb;
        return a.email.localeCompare(b.email);
      }
      if (a.isCurrent !== b.isCurrent) return a.isCurrent ? -1 : 1;

      switch (sortBy) {
        case "overall":
          return (
            (getAntigravityOverallScore(a) - getAntigravityOverallScore(b)) *
            mult
          );
        case "created_at":
          return (a.createdAt - b.createdAt) * mult;
        case "last_switched_at":
          return ((a.lastSwitchedAt ?? 0) - (b.lastSwitchedAt ?? 0)) * mult;
        case "gemini:5h":
        case "gemini:weekly":
        case "claude:5h":
        case "claude:weekly":
        case "argon:weekly":
          return (
            (getQuotaBucketPercentage(a, sortBy) -
              getQuotaBucketPercentage(b, sortBy)) *
            mult
          );
        case "reset_time":
          return (getEarliestResetMs(a) - getEarliestResetMs(b)) * mult;
        case "email":
          return a.email.localeCompare(b.email) * mult;
        default:
          return 0;
      }
    });

    return list;
  }, [
    accounts,
    activeGroup,
    customSortOrder,
    searchQuery,
    sortBy,
    sortDir,
    tagFilter,
    tierFilter,
  ]);

  // Grouped by tag view when `groupByTag` is active
  const groupedByTagSections = useMemo(() => {
    if (!groupByTag) return null;
    const map = new Map<string, AntigravityAccount[]>();
    for (const acc of filteredAccounts) {
      const key = acc.tags[0] || (vi ? "Chưa gắn thẻ" : "Untagged");
      const arr = map.get(key) ?? [];
      arr.push(acc);
      map.set(key, arr);
    }
    return Array.from(map.entries());
  }, [filteredAccounts, groupByTag, vi]);

  const allSelected =
    filteredAccounts.length > 0 &&
    filteredAccounts.every((a) => selectedIds.has(a.id));

  const toggleSelectAll = () => {
    if (allSelected) {
      setSelectedIds(new Set());
    } else {
      setSelectedIds(new Set(filteredAccounts.map((a) => a.id)));
    }
  };

  const toggleSelectOne = (id: string) => {
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  // Actions
  const handleOpenVault = (acc: AntigravityAccount) => {
    setVaultAccount(acc);
    setShowVaultPassword(false);
    setVaultForm({
      tags: acc.tags.join(", "),
      notes: acc.notes ?? "",
      accountPassword: acc.accountPassword ?? "",
      twoFactorSecret: acc.twoFactorSecret ?? "",
      phoneNumber: acc.phoneNumber ?? "",
      mailUrl: acc.mailUrl ?? "",
      auxEmail: acc.auxEmail ?? "",
    });
  };

  const handleSaveVault = async () => {
    if (!vaultAccount) return;
    setBusyKey("save-vault");
    try {
      const parsedTags = vaultForm.tags
        .split(",")
        .map((s) => s.trim())
        .filter(Boolean);
      await onUpdateMetadata({
        accountId: vaultAccount.id,
        tags: parsedTags,
        notes: vaultForm.notes,
        accountPassword: vaultForm.accountPassword,
        twoFactorSecret: vaultForm.twoFactorSecret,
        phoneNumber: vaultForm.phoneNumber,
        mailUrl: vaultForm.mailUrl,
        auxEmail: vaultForm.auxEmail,
      });
      setVaultAccount(null);
      flashMsg("ok", vi ? "Đã lưu Vault & 2FA" : "Saved Vault & 2FA");
    } catch (e) {
      flashMsg("err", e instanceof Error ? e.message : String(e));
    } finally {
      setBusyKey(null);
    }
  };

  const handleSaveTags = async () => {
    if (!tagAccount) return;
    setBusyKey("save-tags");
    try {
      const nextTags = tagDraftInput
        .split(",")
        .map((s) => s.trim())
        .filter(Boolean);
      await onUpdateMetadata({ accountId: tagAccount.id, tags: nextTags });
      setTagAccount(null);
    } catch (e) {
      flashMsg("err", e instanceof Error ? e.message : String(e));
    } finally {
      setBusyKey(null);
    }
  };

  const handleOpenExport = async (ids: string[], includeSensitive: boolean) => {
    setBusyKey("export");
    try {
      const raw = await onExportAccounts(ids, includeSensitive);
      setExportTargetIds(ids);
      setExportIncludeSensitive(includeSensitive);
      setExportJsonText(raw);
      setExportModalOpen(true);
    } catch (e) {
      flashMsg("err", e instanceof Error ? e.message : String(e));
    } finally {
      setBusyKey(null);
    }
  };

  const handleDownloadExport = () => {
    const blob = new Blob([exportJsonText], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `onlydevs-antigravity-accounts-${new Date().toISOString().slice(0, 10)}.json`;
    a.click();
    URL.revokeObjectURL(url);
  };

  const handleImportDirectory = async () => {
    setBusyKey("import-dir");
    try {
      const picked = await agxInvoke<string | null>("pick_folder_dialog");
      if (!picked) return;
      const imported = await agxInvoke<AntigravityAccount[]>(
        "import_accounts_from_directory",
        { dirPath: picked },
      );
      flashMsg(
        "ok",
        vi
          ? `Đã nhập ${imported.length} tài khoản từ thư mục`
          : `Imported ${imported.length} accounts from folder`,
      );
      setAddModalOpen(false);
      await onRefreshAllData();
    } catch (e) {
      flashMsg("err", e instanceof Error ? e.message : String(e));
    } finally {
      setBusyKey(null);
    }
  };

  // OAuth lifecycle: auto-prepare on open, cancel on close/tab switch,
  // and listen to Tauri `oauth-callback-received` so both browser redirect & manual paste finish automatically once.
  useEffect(() => {
    if (!isTauri()) return;
    let active = true;
    let unlistenUrl: (() => void) | undefined;
    let unlistenCallback: (() => void) | undefined;

    void import("@tauri-apps/api/event").then(async ({ listen }) => {
      if (!active) return;
      unlistenUrl = await listen<string>("oauth-url-generated", (event) => {
        const url = String(event.payload || "");
        if (!url) return;
        setOauthPrep((prev) =>
          prev ? { ...prev, authUrl: url } : { authUrl: url, redirectUri: "", state: "" },
        );
      });
      if (!active) {
        unlistenUrl?.();
        return;
      }
      unlistenCallback = await listen("oauth-callback-received", async () => {
        if (!addModalOpenRef.current || addTabRef.current !== "oauth") return;
        if (oauthWorkingRef.current) {
          setOauthStatusText(
            vi
              ? "Đã nhận callback từ Google, đang đổi token & tải quota..."
              : "Callback received, exchanging token & fetching quota...",
          );
          return;
        }
        if (!oauthPrepRef.current) return;
        oauthWorkingRef.current = true;
        setOauthWorking(true);
        setOauthError(null);
        setOauthStatusText(
          vi
            ? "Đã nhận callback từ Google, đang hoàn tất đăng nhập..."
            : "Callback received, completing sign-in...",
        );
        try {
          await onCompleteOAuthLoginRef.current();
          setOauthCallbackUrl("");
          setOauthPrep(null);
          setOauthStatusText(null);
          setAddModalOpen(false);
        } catch (e) {
          const msg = e instanceof Error ? e.message : String(e);
          setOauthError(msg);
          setOauthPrep(null);
        } finally {
          oauthWorkingRef.current = false;
          setOauthWorking(false);
        }
      });
    });

    return () => {
      active = false;
      unlistenUrl?.();
      unlistenCallback?.();
    };
  }, [vi]);

  useEffect(() => {
    if (!addModalOpen || addTab !== "oauth" || oauthPrep || oauthPrepInFlightRef.current) return;
    let cancelled = false;
    oauthPrepInFlightRef.current = true;
    setOauthPrepLoading(true);
    onPrepareOAuthRef
      .current()
      .then((prep) => {
        if (!cancelled) {
          setOauthPrep(prep);
          setOauthError(null);
        }
      })
      .catch((e) => {
        if (!cancelled) {
          setOauthError(e instanceof Error ? e.message : String(e));
        }
      })
      .finally(() => {
        oauthPrepInFlightRef.current = false;
        setOauthPrepLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [addModalOpen, addTab, oauthPrep]);

  useEffect(() => {
    if (addModalOpen && addTab === "oauth") return;
    if (!oauthPrep && !oauthWorking) return;
    void onCancelOAuthLoginRef.current().catch(() => {});
    setOauthPrep(null);
    setOauthCallbackUrl("");
    setOauthError(null);
    setOauthStatusText(null);
    setOauthWorking(false);
    oauthWorkingRef.current = false;
  }, [addModalOpen, addTab, oauthPrep, oauthWorking]);

  useEffect(() => {
    return () => {
      if (addModalOpenRef.current && addTabRef.current === "oauth") {
        void onCancelOAuthLoginRef.current().catch(() => {});
      }
    };
  }, []);

  const handleStartOAuth = async () => {
    if (oauthWorkingRef.current) return;
    oauthWorkingRef.current = true;
    setOauthWorking(true);
    setOauthError(null);
    setOauthStatusText(
      vi
        ? "Đã mở trình duyệt — hoàn tất đăng nhập Google, app sẽ tự nhận tài khoản..."
        : "Browser opened — finish Google sign-in and the app will auto-complete...",
    );
    try {
      await onStartOAuthLogin();
      setOauthCallbackUrl("");
      setOauthPrep(null);
      setOauthStatusText(null);
      setAddModalOpen(false);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setOauthError(msg);
      setOauthStatusText(null);
      setOauthPrep(null);
      flashMsg("err", msg);
    } finally {
      oauthWorkingRef.current = false;
      setOauthWorking(false);
    }
  };

  const handleCompleteOAuth = async () => {
    if (oauthWorkingRef.current) return;
    oauthWorkingRef.current = true;
    setOauthWorking(true);
    setOauthError(null);
    setOauthStatusText(
      vi
        ? "Đang chờ callback từ trình duyệt..."
        : "Waiting for browser OAuth callback...",
    );
    try {
      await onCompleteOAuthLogin();
      setOauthCallbackUrl("");
      setOauthPrep(null);
      setOauthStatusText(null);
      setAddModalOpen(false);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setOauthError(msg);
      setOauthStatusText(null);
      setOauthPrep(null);
      flashMsg("err", msg);
    } finally {
      oauthWorkingRef.current = false;
      setOauthWorking(false);
    }
  };

  const handleResetOAuth = async () => {
    setOauthError(null);
    setOauthStatusText(null);
    setOauthCallbackUrl("");
    setOauthWorking(false);
    oauthWorkingRef.current = false;
    await onCancelOAuthLogin().catch(() => {});
    setOauthPrep(null);
  };

  const handleSubmitCallback = async () => {
    const trimmed = oauthCallbackUrl.trim();
    if (!trimmed) return;
    setBusyKey("oauth-submit");
    setOauthError(null);
    try {
      await onSubmitOAuthCallback(trimmed);
      setOauthCallbackUrl("");
      // If start_oauth_login / complete_oauth_login isn't already waiting, finish immediately
      // instead of relying solely on the async Tauri event listener.
      if (!oauthWorkingRef.current) {
        void handleCompleteOAuth();
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setOauthError(msg);
      flashMsg("err", msg);
    } finally {
      setBusyKey(null);
    }
  };

  const handleOpenCreateGroup = (existing?: AccountGroup) => {
    if (existing) {
      setEditingGroupId(existing.id);
      setGroupNameInput(existing.name);
      setGroupMemberIds(new Set(existing.accountIds));
    } else {
      setEditingGroupId(null);
      setGroupNameInput("");
      setGroupMemberIds(new Set(selectedIds));
    }
    setGroupModalOpen(true);
  };

  const handleSaveGroup = async () => {
    if (!groupNameInput.trim()) return;
    setBusyKey("save-group");
    try {
      await agxInvoke("save_account_group", {
        id: editingGroupId,
        name: groupNameInput.trim(),
        accountIds: Array.from(groupMemberIds),
      });
      await loadGroupsAndSort();
      setGroupModalOpen(false);
      flashMsg("ok", vi ? "Đã lưu nhóm tài khoản" : "Saved account group");
    } catch (e) {
      flashMsg("err", e instanceof Error ? e.message : String(e));
    } finally {
      setBusyKey(null);
    }
  };

  const handleDeleteGroup = async (groupId: string) => {
    if (
      !window.confirm(
        vi
          ? "Xóa nhóm này? (Các tài khoản bên trong không bị xóa)"
          : "Delete this group? (Accounts inside will NOT be deleted)",
      )
    ) {
      return;
    }
    setBusyKey(`del-group-${groupId}`);
    try {
      await agxInvoke("delete_account_group", { groupId });
      if (activeGroupId === groupId) setActiveGroupId("ALL");
      await loadGroupsAndSort();
      setGroupModalOpen(false);
    } catch (e) {
      flashMsg("err", e instanceof Error ? e.message : String(e));
    } finally {
      setBusyKey(null);
    }
  };

  const handleBatchRefreshSelected = async () => {
    const ids = Array.from(selectedIds);
    if (ids.length === 0) return;
    setBusyKey("batch-refresh");
    try {
      for (const id of ids) {
        await onFetchQuota(id);
      }
      flashMsg(
        "ok",
        vi
          ? `Đã làm mới Quota cho ${ids.length} tài khoản`
          : `Refreshed quota for ${ids.length} accounts`,
      );
    } catch (e) {
      flashMsg("err", e instanceof Error ? e.message : String(e));
    } finally {
      setBusyKey(null);
    }
  };

  const openCustomSortModal = () => {
    const existing = new Set(customSortOrder);
    const merged = [
      ...customSortOrder.filter((id) => accounts.some((a) => a.id === id)),
      ...accounts.filter((a) => !existing.has(a.id)).map((a) => a.id),
    ];
    setSortDraftIds(merged);
    setCustomSortModalOpen(true);
  };

  const moveDraftOrder = (idx: number, delta: -1 | 1) => {
    const target = idx + delta;
    if (target < 0 || target >= sortDraftIds.length) return;
    setSortDraftIds((prev) => {
      const next = [...prev];
      const [item] = next.splice(idx, 1);
      next.splice(target, 0, item);
      return next;
    });
  };

  const saveDraftCustomSort = async () => {
    setBusyKey("save-sort");
    try {
      const saved = await agxInvoke<string[]>("save_account_custom_sort", {
        accountIds: sortDraftIds,
      });
      setCustomSortOrder(saved);
      setSortBy("custom");
      setCustomSortModalOpen(false);
      flashMsg("ok", vi ? "Đã lưu thứ tự sắp xếp tùy chỉnh" : "Saved custom order");
    } catch (e) {
      flashMsg("err", e instanceof Error ? e.message : String(e));
    } finally {
      setBusyKey(null);
    }
  };

  // Summary Strip numbers
  const stripExact = usageBundle?.summary.kpi.exact;
  const stripKpi = usageBundle?.summary.kpi;
  const stripCache =
    (stripExact?.cacheReadTokens ?? 0) + (stripExact?.cacheWriteTokens ?? 0);

  // Render single Account Card (Grid / List / Compact)
  const renderAccountCard = (acc: AntigravityAccount) => {
    const tier = normalizeTierBadge(acc.quota?.subscriptionTier, acc);
    const needsReauth = isAccountNeedsReauth(acc);
    const isForbidden = Boolean(acc.quota?.isForbidden);
    const isSelected = selectedIds.has(acc.id);
    const inApiPool = isAccountInApiPool(acc.id);
    const isBusyCard =
      busyKey === `switch-${acc.id}` ||
      busyKey === `launch-${acc.id}` ||
      busyKey === `quota-${acc.id}` ||
      busyKey === `api-${acc.id}` ||
      busyKey === `del-${acc.id}`;

    if (viewMode === "compact") {
      const buckets = getAntigravityQuotaDisplayItems(acc);
      return (
        <div
          key={acc.id}
          className={`ag-acc-compact-row${acc.isCurrent ? " is-current" : ""}${isSelected ? " is-selected" : ""}`}
        >
          <input
            type="checkbox"
            checked={isSelected}
            onChange={() => toggleSelectOne(acc.id)}
          />
          <div className="ag-acc-compact-email">
            <strong>{maskEmail(acc.email, privacyMode)}</strong>
            {acc.isCurrent ? (
              <span className="ag-badge-current">
                {vi ? "Hiện tại" : "Current"}
              </span>
            ) : null}
            <span className={`ag-badge-tier is-${tier.toLowerCase()}`}>
              {tier}
            </span>
          </div>
          <div className="ag-acc-compact-pills">
            {buckets.map((b) => (
              <span key={b.key} className="ag-acc-mini-pill">
                {b.label}: <strong>{b.percentage}%</strong>
              </span>
            ))}
          </div>
          <div className="ag-acc-card-actions">
            <button
              type="button"
              className={`od-api-chip${inApiPool ? " is-in-pool" : ""}`}
              title={
                inApiPool
                  ? vi
                    ? "Gỡ khỏi API Pool"
                    : "Remove from API Pool"
                  : vi
                    ? "Thêm vào API Pool"
                    : "Add to API Pool"
              }
              disabled={isBusyCard}
              onClick={async () => {
                setBusyKey(`api-${acc.id}`);
                try {
                  await onToggleApiPool(acc.id);
                } finally {
                  setBusyKey(null);
                }
              }}
            >
              {busyKey === `api-${acc.id}` ? (
                <Loader2 size={12} className="spin" />
              ) : (
                <Radio size={12} />
              )}
              <span>{inApiPool ? "API ✓" : "+ API"}</span>
            </button>
            <button
              type="button"
              className="ag-card-icon-btn"
              title={vi ? "Chuyển / Chạy tài khoản" : "Switch / Launch Account"}
              disabled={isBusyCard}
              onClick={() => void openLaunchPopup(acc)}
            >
              <Play size={14} />
            </button>
            <button
              type="button"
              className="ag-card-icon-btn"
              title={vi ? "Làm mới Quota" : "Refresh Quota"}
              disabled={isBusyCard}
              onClick={async () => {
                setBusyKey(`quota-${acc.id}`);
                try {
                  await onFetchQuota(acc.id);
                } finally {
                  setBusyKey(null);
                }
              }}
            >
              <RefreshCw
                size={14}
                className={busyKey === `quota-${acc.id}` ? "spin" : undefined}
              />
            </button>
            <button
              type="button"
              className="ag-card-icon-btn is-danger"
              title={vi ? "Xóa tài khoản" : "Delete Account"}
              onClick={async () => {
                if (
                  !window.confirm(
                    vi ? `Xóa tài khoản ${acc.email}?` : `Delete ${acc.email}?`,
                  )
                )
                  return;
                setBusyKey(`del-${acc.id}`);
                try {
                  await onDeleteAccounts([acc.id]);
                } finally {
                  setBusyKey(null);
                }
              }}
            >
              <Trash2 size={14} />
            </button>
          </div>
        </div>
      );
    }

    return (
      <article
        key={acc.id}
        className={`ag-acc-card${acc.isCurrent ? " is-current" : ""}${isSelected ? " is-selected" : ""}${viewMode === "list" ? " is-list" : ""}`}
      >
        {/* Top Row: Checkbox + Email + Current Badge + Tier Badge */}
        <div className="ag-acc-card__top">
          <label className="ag-acc-card__identity">
            <input
              type="checkbox"
              checked={isSelected}
              onChange={() => toggleSelectOne(acc.id)}
            />
            <span className="ag-acc-card__email" title={acc.email}>
              {maskEmail(acc.email, privacyMode)}
            </span>
          </label>
          <div className="ag-acc-card__badges">
            {acc.isCurrent ? (
              <span className="ag-badge-current">
                {vi ? "Hiện tại" : "Current"}
              </span>
            ) : null}
            <span className={`ag-badge-tier is-${tier.toLowerCase()}`}>
              {tier}
            </span>
            {needsReauth ? (
              <span
                className="ag-badge-warn"
                title={acc.quotaError?.message || "Needs Verification"}
              >
                <AlertTriangle size={11} />
                {vi ? "Xác minh" : "Verify"}
              </span>
            ) : null}
            {isForbidden ? (
              <span className="ag-badge-err">403</span>
            ) : null}
          </div>
        </div>

        {/* Inline Tags + "+ Thêm" button */}
        <div className="ag-acc-card__tags">
          {acc.tags.map((tg) => (
            <span key={tg} className="ag-acc-tag-pill">
              <Tag size={10} />
              {tg}
            </span>
          ))}
          <button
            type="button"
            className="ag-acc-add-tag-btn"
            onClick={() => {
              setTagAccount(acc);
              setTagDraftInput(acc.tags.join(", "));
            }}
          >
            <Tag size={11} />
            <span>{vi ? "Thêm" : "Tag"}</span>
          </button>
          {acc.twoFactorSecret ? (
            <button
              type="button"
              className="ag-acc-tag-pill is-2fa"
              onClick={() => handleOpenVault(acc)}
              title={vi ? "Mở mã 2FA TOTP" : "Open 2FA TOTP"}
            >
              <KeyRound size={10} /> 2FA
            </button>
          ) : null}
        </div>

        {/* Quota Buckets Grid */}
        <AntigravityQuotaSection
          account={acc}
          lang={lang}
          isList={viewMode === "list"}
        />

        {/* Timestamp row */}
        <div className="ag-acc-card__meta">
          <Clock size={12} />
          <span>
            {acc.lastSwitchedAt
              ? `${vi ? "Lần chuyển" : "Switched"}: ${formatDateTimeFull(acc.lastSwitchedAt)}`
              : `${vi ? "Lần làm mới" : "Refreshed"}: ${formatDateTimeFull(acc.quota?.lastUpdated ?? acc.lastUsed)}`}
          </span>
        </div>

        {/* Footer Action Row: API Pool Toggle + Core Controls */}
        <div className="ag-acc-card__footer">
          <button
            type="button"
            className={`od-api-chip${inApiPool ? " is-in-pool" : ""}`}
            title={
              inApiPool
                ? vi
                  ? "Đang trong API Pool (bấm để gỡ)"
                  : "In API Pool (click to remove)"
                : vi
                  ? "Thêm tài khoản này vào API Pool"
                  : "Add account to API Pool"
            }
            disabled={isBusyCard}
            onClick={async () => {
              setBusyKey(`api-${acc.id}`);
              try {
                await onToggleApiPool(acc.id);
              } finally {
                setBusyKey(null);
              }
            }}
          >
            {busyKey === `api-${acc.id}` ? (
              <Loader2 size={12} className="spin" />
            ) : (
              <Radio size={12} />
            )}
            <span>{inApiPool ? (vi ? "API ✓" : "API ✓") : "+ API"}</span>
          </button>

          <div className="od-card-actions-right">
            <button
              type="button"
              className="ag-card-icon-btn"
              title={vi ? "Chi tiết Quota" : "Quota Details"}
              onClick={() => setInfoAccount(acc)}
            >
              <Info size={14} />
            </button>
            <button
              type="button"
              className="ag-card-icon-btn"
              title={vi ? "Ghi chú & 2FA" : "Notes & 2FA"}
              onClick={() => handleOpenVault(acc)}
            >
              <FileText size={14} />
            </button>
            <button
              type="button"
              className={`ag-card-icon-btn${acc.isCurrent ? " is-active-play" : ""}`}
              title={vi ? "Chuyển / Mở tài khoản" : "Switch / Launch Account"}
              disabled={isBusyCard}
              onClick={() => void openLaunchPopup(acc)}
            >
              <Play size={14} />
            </button>
            <button
              type="button"
              className="ag-card-icon-btn"
              title={vi ? "Làm mới Quota" : "Refresh Quota"}
              disabled={isBusyCard}
              onClick={async () => {
                setBusyKey(`quota-${acc.id}`);
                try {
                  await onFetchQuota(acc.id);
                } finally {
                  setBusyKey(null);
                }
              }}
            >
              <RefreshCw
                size={14}
                className={busyKey === `quota-${acc.id}` ? "spin" : undefined}
              />
            </button>
            <button
              type="button"
              className="ag-card-icon-btn"
              title={vi ? "Xuất JSON" : "Export JSON"}
              onClick={() => void handleOpenExport([acc.id], true)}
            >
              <Upload size={14} />
            </button>
            <button
              type="button"
              className="ag-card-icon-btn is-danger"
              title={vi ? "Xóa tài khoản" : "Delete"}
              disabled={isBusyCard}
              onClick={async () => {
                if (
                  !window.confirm(
                    vi ? `Xóa tài khoản ${acc.email}?` : `Delete ${acc.email}?`,
                  )
                )
                  return;
                setBusyKey(`del-${acc.id}`);
                try {
                  await onDeleteAccounts([acc.id]);
                } finally {
                  setBusyKey(null);
                }
              }}
            >
              <Trash2 size={14} />
            </button>
          </div>
        </div>
      </article>
    );
  };

  return (
    <div className="agx-page od-page">
      {/* Hidden JSON file import input */}
      <input
        ref={fileInputRef}
        type="file"
        accept=".json,.txt"
        style={{ display: "none" }}
        onChange={async (e) => {
          const f = e.target.files?.[0];
          if (!f) return;
          const text = await f.text();
          setJsonInput(text);
          setAddTab("json");
          setAddModalOpen(true);
          e.target.value = "";
        }}
      />

      {/* Clean Header */}
      <header className="od-header">
        <div className="od-header__left">
          <h1>{tr(lang, "acc.title")}</h1>
        </div>

        <nav className="od-subtabs" role="tablist">
          <button
            type="button"
            role="tab"
            aria-selected={subTab === "overview"}
            className={`od-subtab${subTab === "overview" ? " is-active" : ""}`}
            onClick={() => setSubTab("overview")}
          >
            <Users size={15} />
            <span>{tr(lang, "acc.tabOverview")}</span>
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={subTab === "usage"}
            className={`od-subtab${subTab === "usage" ? " is-active" : ""}`}
            onClick={() => setSubTab("usage")}
          >
            <BarChart3 size={15} />
            <span>{tr(lang, "acc.tabUsage")}</span>
          </button>
        </nav>

        <div className="od-header__right">
          <button
            type="button"
            className="agx-btn agx-btn--primary agx-btn--sm"
            onClick={() => {
              setAddTab("oauth");
              setAddModalOpen(true);
            }}
          >
            <Plus size={15} />
            <span>{tr(lang, "acc.addAccount")}</span>
          </button>
        </div>
      </header>

      {localMsg ? (
        <div className={`od-banner is-${localMsg.tone}`}>
          <span>{localMsg.text}</span>
          <button
            type="button"
            className="agx-icon-btn"
            onClick={() => setLocalMsg(null)}
          >
            <X size={14} />
          </button>
        </div>
      ) : null}

      {/* ============================================================
          TAB 2: MỨC DÙNG PHIÊN (Antigravity Session Token Usage)
         ============================================================ */}
      {subTab === "usage" ? (
        <AntigravityTokenUsagePanel
          lang={lang}
          range={usageRange}
          onRangeChange={(next) => setUsageRange(next)}
          onBackToOverview={() => setSubTab("overview")}
          onBundleUpdated={(b) => setUsageBundle(b)}
        />
      ) : (
        /* ============================================================
           TAB 1: TỔNG QUAN TÀI KHOẢN (Account Overview)
           ============================================================ */
        <div className="ag-overview-wrap">
          {/* 1. Local Token Usage Summary Strip */}
          <section className="ag-token-strip">
            <div className="ag-token-strip__left">
              <span className="ag-token-strip__badge">
                <BarChart3 size={14} />
                {tr(lang, "acc.localTokens")}
              </span>
              <div className="ag-token-strip__ranges">
                {QUICK_RANGES.map((preset) => (
                  <button
                    key={preset}
                    type="button"
                    className={`ag-token-strip__range${usageRange === preset ? " is-active" : ""}`}
                    onClick={() => setUsageRange(preset)}
                  >
                    {tr(lang, `acc.range.${preset}`)}
                  </button>
                ))}
              </div>
            </div>

            <div className="ag-token-strip__metrics">
              <span title={formatExactNumber(stripExact?.inputTokens ?? 0)}>
                <em>{tr(lang, "acc.metric.input")}</em>
                <strong>
                  {usageBundle
                    ? formatTokensShort(stripExact?.inputTokens ?? 0, 2)
                    : "…"}
                </strong>
              </span>
              <span
                className="is-thinking"
                title={formatExactNumber(stripExact?.thinkingTokens ?? 0)}
              >
                <em>{tr(lang, "acc.metric.thinking")}</em>
                <strong>
                  {usageBundle
                    ? formatTokensShort(stripExact?.thinkingTokens ?? 0, 1)
                    : "…"}
                </strong>
              </span>
              <span title={formatExactNumber(stripExact?.outputTokens ?? 0)}>
                <em>{tr(lang, "acc.metric.output")}</em>
                <strong>
                  {usageBundle
                    ? formatTokensShort(stripExact?.outputTokens ?? 0, 1)
                    : "…"}
                </strong>
              </span>
              <span title={formatExactNumber(stripCache)}>
                <em>{tr(lang, "acc.metric.cache")}</em>
                <strong>
                  {usageBundle ? formatTokensShort(stripCache, 2) : "…"}
                </strong>
              </span>
              <span
                className="is-total"
                title={formatExactNumber(stripExact?.totalTokens ?? 0)}
              >
                <em>{tr(lang, "acc.metric.total")}</em>
                <strong>
                  {usageBundle
                    ? formatTokensShort(stripExact?.totalTokens ?? 0, 2)
                    : "…"}
                </strong>
              </span>
              <span>
                <em>{tr(lang, "acc.metric.turns")}</em>
                <strong>
                  {usageBundle
                    ? formatExactNumber(stripKpi?.totalTurns ?? 0)
                    : "…"}
                </strong>
              </span>
            </div>

            <div className="ag-token-strip__actions">
              <button
                type="button"
                className="ag-card-icon-btn"
                title={tr(lang, "acc.refresh")}
                disabled={usageSyncing}
                onClick={() => void loadTokenSummary(usageRange, true)}
              >
                <RefreshCw
                  size={13}
                  className={usageSyncing ? "spin" : undefined}
                />
              </button>
              <button
                type="button"
                className="ag-token-strip__detail-btn"
                onClick={() => setSubTab("usage")}
              >
                {tr(lang, "acc.details")}
              </button>
            </div>
          </section>

          {/* 2. Main Toolbar */}
          <section className="od-toolbar">
            <div className="od-toolbar__left">
              <div className="od-search">
                <Search size={15} />
                <input
                  type="text"
                  value={searchQuery}
                  onChange={(e) => setSearchQuery(e.target.value)}
                  placeholder={vi ? "Tìm kiếm..." : "Search..."}
                />
                {searchQuery ? (
                  <button
                    type="button"
                    className="ag-sort-th-btn"
                    onClick={() => setSearchQuery("")}
                  >
                    <X size={13} />
                  </button>
                ) : null}
              </div>

              <div className="ag-view-switcher">
                <button
                  type="button"
                  className={viewMode === "compact" ? "is-active" : ""}
                  title={vi ? "Thu gọn" : "Compact"}
                  onClick={() => setViewMode("compact")}
                >
                  <Rows3 size={15} />
                </button>
                <button
                  type="button"
                  className={viewMode === "list" ? "is-active" : ""}
                  title={vi ? "Danh sách" : "List"}
                  onClick={() => setViewMode("list")}
                >
                  <List size={15} />
                </button>
                <button
                  type="button"
                  className={viewMode === "grid" ? "is-active" : ""}
                  title={vi ? "Lưới thẻ" : "Grid"}
                  onClick={() => setViewMode("grid")}
                >
                  <LayoutGrid size={15} />
                </button>
              </div>

              <select
                className="agx-select ag-toolbar-select"
                value={tierFilter}
                onChange={(e) => setTierFilter(e.target.value as TierFilter)}
              >
                <option value="ALL">
                  {vi ? `Tất cả (${tierCounts.all})` : `All (${tierCounts.all})`}
                </option>
                <option value="PRO">PRO ({tierCounts.pro})</option>
                <option value="ProLite">ProLite ({tierCounts.proLite})</option>
                <option value="ULTRA">ULTRA ({tierCounts.ultra})</option>
                <option value="FREE">FREE ({tierCounts.free})</option>
                <option value="VALID">
                  {vi
                    ? `Hợp lệ (${tierCounts.valid})`
                    : `Valid (${tierCounts.valid})`}
                </option>
                <option value="REAUTH">
                  {vi
                    ? `Cần xác minh (${tierCounts.reauth})`
                    : `Needs Verify (${tierCounts.reauth})`}
                </option>
                <option value="FORBIDDEN">
                  {vi
                    ? `Bị cấm 403 (${tierCounts.forbidden})`
                    : `Forbidden (${tierCounts.forbidden})`}
                </option>
              </select>

              <select
                className="agx-select ag-toolbar-select"
                value={tagFilter}
                onChange={(e) => setTagFilter(e.target.value)}
              >
                <option value="ALL">{vi ? "Lọc thẻ" : "Filter tag"}</option>
                {allTags.map((t) => (
                  <option key={t} value={t}>
                    #{t}
                  </option>
                ))}
              </select>

              <button
                type="button"
                className={`agx-btn agx-btn--secondary agx-btn--sm${groupByTag ? " is-active-toggle" : ""}`}
                onClick={() => setGroupByTag((v) => !v)}
                title={vi ? "Nhóm hiển thị theo thẻ" : "Group view by tag"}
              >
                <Tag size={13} />
                <span>{vi ? "Nhóm thẻ" : "By Tag"}</span>
              </button>

              <select
                className="agx-select ag-toolbar-select"
                value={sortBy}
                onChange={(e) => setSortBy(e.target.value as SortField)}
              >
                <option value="overall">
                  {vi ? "Theo tổng hạn mức" : "By overall quota"}
                </option>
                <option value="gemini:5h">Gemini · 5h</option>
                <option value="gemini:weekly">Gemini · Weekly</option>
                <option value="claude:5h">Claude · 5h</option>
                <option value="claude:weekly">Claude · Weekly</option>
                <option value="argon:weekly">Gemini 4 Argon (Weekly)</option>
                <option value="reset_time">
                  {vi ? "Theo thời gian reset" : "By reset time"}
                </option>
                <option value="last_switched_at">
                  {vi ? "Theo lần chuyển gần nhất" : "By last switched"}
                </option>
                <option value="created_at">
                  {vi ? "Theo thời gian thêm" : "By created time"}
                </option>
                <option value="email">Email (A-Z)</option>
                <option value="custom">
                  {vi ? "Thứ tự tùy chỉnh" : "Custom order"}
                </option>
              </select>

              {sortBy === "custom" ? (
                <button
                  type="button"
                  className="ag-toolbar-icon-btn"
                  title={vi ? "Sắp xếp thủ công" : "Configure custom order"}
                  onClick={openCustomSortModal}
                >
                  <GripVertical size={15} />
                </button>
              ) : (
                <button
                  type="button"
                  className="ag-toolbar-icon-btn"
                  title={sortDir === "desc" ? "Giảm dần" : "Tăng dần"}
                  onClick={() =>
                    setSortDir((d) => (d === "desc" ? "asc" : "desc"))
                  }
                >
                  {sortDir === "desc" ? (
                    <ArrowDown size={15} />
                  ) : (
                    <ArrowUp size={15} />
                  )}
                </button>
              )}
            </div>

            <div className="od-toolbar__right">
              <button
                type="button"
                className="ag-toolbar-icon-btn"
                title={vi ? "Làm mới toàn bộ Quota" : "Refresh All Quotas"}
                disabled={loading || busyKey === "refresh-all"}
                onClick={async () => {
                  setBusyKey("refresh-all");
                  try {
                    await onRefreshAllQuotas();
                    await onRefreshAllData();
                  } finally {
                    setBusyKey(null);
                  }
                }}
              >
                <RefreshCw
                  size={15}
                  className={busyKey === "refresh-all" ? "spin" : undefined}
                />
              </button>

              <button
                type="button"
                className={`ag-toolbar-icon-btn${privacyMode ? " is-active" : ""}`}
                title={
                  privacyMode
                    ? vi
                      ? "Đang ẩn Email"
                      : "Privacy ON"
                    : vi
                      ? "Ẩn Email"
                      : "Hide Emails"
                }
                onClick={() => setPrivacyMode((p) => !p)}
              >
                {privacyMode ? <EyeOff size={15} /> : <Eye size={15} />}
              </button>

              <button
                type="button"
                className={`ag-toolbar-icon-btn${autoSwitchCfg.enabled ? " is-active" : ""}`}
                title={
                  vi
                    ? `Tự động chuyển đổi tài khoản (${autoSwitchCfg.enabled ? `BẬT · ${autoSwitchCfg.threshold}%` : "TẮT"})`
                    : `Auto Switch Accounts (${autoSwitchCfg.enabled ? `ON · ${autoSwitchCfg.threshold}%` : "OFF"})`
                }
                onClick={() => setAutoSwitchModalOpen(true)}
              >
                <Zap size={15} />
              </button>

              <button
                type="button"
                className="ag-toolbar-icon-btn"
                title={vi ? "Lịch sử chuyển tài khoản" : "Switch History"}
                onClick={() => setHistoryModalOpen(true)}
              >
                <History size={15} />
              </button>
            </div>
          </section>

          {/* 3. Selection Bar & Account Group Tabs */}
          <section className="ag-selection-group-bar">
            <div className="ag-selection-left">
              <label className="ag-select-all-label">
                <input
                  type="checkbox"
                  checked={allSelected}
                  onChange={toggleSelectAll}
                />
                <span>
                  {selectedIds.size > 0
                    ? vi
                      ? `Đã chọn ${selectedIds.size} tài khoản`
                      : `${selectedIds.size} selected`
                    : vi
                      ? "Chọn tất cả"
                      : "Select all"}
                </span>
              </label>

              {selectedIds.size > 0 ? (
                <div className="ag-batch-actions">
                  <button
                    type="button"
                    className="agx-btn agx-btn--secondary agx-btn--sm"
                    disabled={busyKey === "batch-refresh"}
                    onClick={() => void handleBatchRefreshSelected()}
                  >
                    <RefreshCw
                      size={13}
                      className={busyKey === "batch-refresh" ? "spin" : undefined}
                    />
                    <span>{vi ? "Làm mới" : "Refresh"}</span>
                  </button>
                  <button
                    type="button"
                    className="agx-btn agx-btn--secondary agx-btn--sm"
                    onClick={() => handleOpenCreateGroup()}
                  >
                    <FolderPlus size={13} />
                    <span>{vi ? "Tạo nhóm" : "New Group"}</span>
                  </button>
                  <button
                    type="button"
                    className="agx-btn agx-btn--secondary agx-btn--sm"
                    onClick={() =>
                      void handleOpenExport(Array.from(selectedIds), true)
                    }
                  >
                    <Download size={13} />
                    <span>{vi ? "Xuất JSON" : "Export"}</span>
                  </button>
                  <button
                    type="button"
                    className="agx-btn agx-btn--danger agx-btn--sm"
                    onClick={async () => {
                      if (
                        !window.confirm(
                          vi
                            ? `Xóa ${selectedIds.size} tài khoản đã chọn?`
                            : `Delete ${selectedIds.size} selected accounts?`,
                        )
                      )
                        return;
                      await onDeleteAccounts(Array.from(selectedIds));
                      setSelectedIds(new Set());
                    }}
                  >
                    <Trash2 size={13} />
                    <span>{vi ? "Xóa" : "Delete"}</span>
                  </button>
                </div>
              ) : null}
            </div>

            <div className="ag-group-tabs">
              <button
                type="button"
                className={`ag-group-tab${activeGroupId === "ALL" ? " is-active" : ""}`}
                onClick={() => setActiveGroupId("ALL")}
              >
                <span>{vi ? "Tất cả" : "All"}</span>
                <span className="ag-group-count">{accounts.length}</span>
              </button>
              {groups.map((grp) => (
                <button
                  key={grp.id}
                  type="button"
                  className={`ag-group-tab${activeGroupId === grp.id ? " is-active" : ""}`}
                  onClick={() => setActiveGroupId(grp.id)}
                  onDoubleClick={() => handleOpenCreateGroup(grp)}
                  title={
                    vi
                      ? "Nhấp đúp để chỉnh sửa / xóa nhóm"
                      : "Double-click to edit / delete group"
                  }
                >
                  <span>{grp.name}</span>
                  <span className="ag-group-count">{grp.accountIds.length}</span>
                </button>
              ))}
              <button
                type="button"
                className="ag-group-add-btn"
                title={vi ? "Tạo nhóm tài khoản mới" : "Create account group"}
                onClick={() => handleOpenCreateGroup()}
              >
                <Plus size={14} />
              </button>
            </div>
          </section>

          {/* 4. Accounts Grid / List / Compact View */}
          {filteredAccounts.length === 0 ? (
            <div className="ag-acc-empty-box">
              <Users size={36} />
              <h3>
                {accounts.length === 0
                  ? vi
                    ? "Chưa có tài khoản"
                    : "No accounts"
                  : vi
                    ? "Không tìm thấy tài khoản"
                    : "No matches"}
              </h3>
              <div className="ag-acc-empty-actions">
                <button
                  type="button"
                  className="agx-btn agx-btn--primary"
                  onClick={() => {
                    setAddTab("oauth");
                    setAddModalOpen(true);
                  }}
                >
                  <Plus size={15} />
                  <span>{vi ? "Thêm tài khoản" : "Add Account"}</span>
                </button>
                <button
                  type="button"
                  className="agx-btn agx-btn--secondary"
                  disabled={busyKey === "import-local"}
                  onClick={async () => {
                    setBusyKey("import-local");
                    try {
                      await onImportLocalAccount();
                    } finally {
                      setBusyKey(null);
                    }
                  }}
                >
                  <UserCheck size={15} />
                  <span>{vi ? "Đồng bộ máy" : "Sync Local"}</span>
                </button>
              </div>
            </div>
          ) : groupedByTagSections ? (
            <div className="ag-tag-grouped-list">
              {groupedByTagSections.map(([tagLabel, items]) => (
                <div key={tagLabel} className="ag-tag-group-section">
                  <div className="ag-tag-group-title">
                    <Tag size={14} />
                    <strong>{tagLabel}</strong>
                    <span>({items.length})</span>
                  </div>
                  <div
                    className={
                      viewMode === "compact"
                        ? "ag-acc-compact-list"
                        : viewMode === "list"
                          ? "ag-acc-list-wrap"
                          : "ag-acc-grid"
                    }
                  >
                    {items.map((acc) => renderAccountCard(acc))}
                  </div>
                </div>
              ))}
            </div>
          ) : (
            <div
              className={
                viewMode === "compact"
                  ? "ag-acc-compact-list"
                  : viewMode === "list"
                    ? "ag-acc-list-wrap"
                    : "ag-acc-grid"
              }
            >
              {filteredAccounts.map((acc) => renderAccountCard(acc))}
            </div>
          )}
        </div>
      )}

      {/* ============================================================
          MODALS
         ============================================================ */}

      {/* Add / Import Account Modal — Redesigned Clean Card UI */}
      {addModalOpen ? (
        <div
          className="agx-modal-backdrop"
          onClick={() => {
            if (!oauthWorking) setAddModalOpen(false);
          }}
        >
          <div
            className="agx-modal od-add-modal"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="agx-modal-head">
              <h3>{vi ? "Thêm tài khoản" : "Add Account"}</h3>
              <button
                type="button"
                className="agx-icon-btn"
                onClick={() => setAddModalOpen(false)}
              >
                <X size={16} />
              </button>
            </div>

            <div className="od-add-tabs">
              {(
                [
                  ["oauth", "OAuth", ExternalLink],
                  ["local", vi ? "Trên máy" : "Local", UserCheck],
                  ["token", "Token", KeyRound],
                  ["batch", vi ? "Hàng loạt" : "Batch", List],
                  ["json", "JSON", FileJson],
                ] as const
              ).map(([key, label, IconComp]) => (
                <button
                  key={key}
                  type="button"
                  className={`od-add-tab${addTab === key ? " is-active" : ""}`}
                  onClick={() => setAddTab(key)}
                >
                  <IconComp size={14} />
                  <span>{label}</span>
                </button>
              ))}
            </div>

            <div className="agx-modal-body">
              {addTab === "oauth" ? (
                <div className="od-add-pane">
                  <div className="od-oauth-actions-row">
                    <button
                      type="button"
                      className="agx-btn agx-btn--primary od-add-hero-btn"
                      disabled={oauthWorking || oauthPrepLoading}
                      onClick={() => void handleStartOAuth()}
                    >
                      {oauthWorking ? (
                        <Loader2 size={16} className="spin" />
                      ) : (
                        <ExternalLink size={16} />
                      )}
                      <span>
                        {oauthWorking
                          ? vi
                            ? "Đang chờ Google xác nhận..."
                            : "Waiting for Google..."
                          : vi
                            ? "Đăng nhập Google (Tự động)"
                            : "Sign in with Google"}
                      </span>
                    </button>
                    <button
                      type="button"
                      className="agx-btn agx-btn--secondary"
                      disabled={!oauthPrep || oauthWorking}
                      onClick={() => void handleCompleteOAuth()}
                      title={
                        vi
                          ? "Dùng nếu bạn tự mở link ở tab ẩn danh / trình duyệt khác"
                          : "Use if you opened the link in another browser or incognito"
                      }
                    >
                      <Check size={15} />
                      <span>{vi ? "Tôi đã đăng nhập" : "I've authorized"}</span>
                    </button>
                    <button
                      type="button"
                      className="agx-btn agx-btn--secondary"
                      disabled={oauthPrepLoading}
                      onClick={() => void handleResetOAuth()}
                      title={vi ? "Hủy phiên cũ & tạo link OAuth mới" : "Reset & generate fresh OAuth link"}
                    >
                      <RefreshCw size={14} className={oauthPrepLoading ? "spin" : ""} />
                    </button>
                  </div>

                  {oauthStatusText ? (
                    <div className="od-oauth-status is-info">
                      <Loader2 size={14} className="spin" />
                      <span>{oauthStatusText}</span>
                    </div>
                  ) : null}

                  {oauthError ? (
                    <div className="od-oauth-status is-err">
                      <AlertTriangle size={14} />
                      <span>{oauthError}</span>
                    </div>
                  ) : null}

                  <div className="od-oauth-box">
                    <div className="od-oauth-field-label">
                      {vi
                        ? "Link ủy quyền OAuth (copy nếu muốn mở bằng trình duyệt khác / ẩn danh)"
                        : "OAuth authorization URL (copy to open in another browser / incognito)"}
                    </div>
                    <div className="ag-oauth-url-row">
                      <code>
                        {oauthPrep?.authUrl ||
                          (vi ? "Đang tạo link OAuth..." : "Generating OAuth link...")}
                      </code>
                      <button
                        type="button"
                        className="agx-btn agx-btn--secondary agx-btn--sm"
                        disabled={!oauthPrep?.authUrl}
                        onClick={() =>
                          oauthPrep?.authUrl &&
                          void copyText(oauthPrep.authUrl, "oauth-url")
                        }
                      >
                        {copiedId === "oauth-url" ? <Check size={13} /> : <Copy size={13} />}
                      </button>
                    </div>

                    <div className="od-oauth-field-label">
                      {vi
                        ? "Dán URL callback thủ công (chỉ cần khi trình duyệt không tự quay về localhost)"
                        : "Manual callback URL (only needed if browser cannot reach localhost)"}
                    </div>
                    <div className="od-inline-submit">
                      <input
                        type="text"
                        className="agx-input"
                        value={oauthCallbackUrl}
                        onChange={(e) => setOauthCallbackUrl(e.target.value)}
                        placeholder="http://localhost:.../oauth-callback?code=...&state=..."
                        disabled={!oauthPrep || busyKey === "oauth-submit"}
                      />
                      <button
                        type="button"
                        className="agx-btn agx-btn--secondary"
                        disabled={
                          !oauthPrep ||
                          !oauthCallbackUrl.trim() ||
                          busyKey === "oauth-submit"
                        }
                        onClick={() => void handleSubmitCallback()}
                      >
                        {busyKey === "oauth-submit" ? (
                          <Loader2 size={14} className="spin" />
                        ) : (
                          <Check size={14} />
                        )}
                        <span>{vi ? "Xác nhận" : "Confirm"}</span>
                      </button>
                    </div>
                  </div>
                </div>
              ) : null}

              {addTab === "local" ? (
                <div className="od-add-pane">
                  <button
                    type="button"
                    className="agx-btn agx-btn--primary od-add-hero-btn"
                    disabled={busyKey === "import-local"}
                    onClick={async () => {
                      setBusyKey("import-local");
                      try {
                        await onImportLocalAccount();
                        setAddModalOpen(false);
                      } finally {
                        setBusyKey(null);
                      }
                    }}
                  >
                    {busyKey === "import-local" ? (
                      <Loader2 size={16} className="spin" />
                    ) : (
                      <UserCheck size={16} />
                    )}
                    <span>
                      {vi ? "Đồng bộ tài khoản đang đăng nhập" : "Sync Current Account"}
                    </span>
                  </button>
                </div>
              ) : null}

              {addTab === "token" ? (
                <div className="od-add-pane">
                  <label className="agx-field">
                    <span>Refresh Token</span>
                    <input
                      type="text"
                      className="agx-input"
                      value={tokenInput}
                      onChange={(e) => setTokenInput(e.target.value)}
                      placeholder="1//0g..."
                    />
                  </label>
                  <label className="agx-field">
                    <span>Email ({vi ? "tùy chọn" : "optional"})</span>
                    <input
                      type="text"
                      className="agx-input"
                      value={tokenEmailHint}
                      onChange={(e) => setTokenEmailHint(e.target.value)}
                      placeholder="user@gmail.com"
                    />
                  </label>
                  <button
                    type="button"
                    className="agx-btn agx-btn--primary"
                    disabled={!tokenInput.trim() || busyKey === "add-token"}
                    onClick={async () => {
                      setBusyKey("add-token");
                      try {
                        await onAddByToken(
                          tokenInput.trim(),
                          tokenEmailHint.trim() || undefined,
                        );
                        setTokenInput("");
                        setTokenEmailHint("");
                        setAddModalOpen(false);
                      } finally {
                        setBusyKey(null);
                      }
                    }}
                  >
                    {busyKey === "add-token" ? (
                      <Loader2 size={15} className="spin" />
                    ) : (
                      <Plus size={15} />
                    )}
                    <span>{vi ? "Thêm tài khoản" : "Add Account"}</span>
                  </button>
                </div>
              ) : null}

              {addTab === "batch" ? (
                <div className="od-add-pane">
                  <textarea
                    className="agx-textarea"
                    rows={7}
                    value={batchInput}
                    onChange={(e) => setBatchInput(e.target.value)}
                    placeholder="1//0g...&#10;user@gmail.com----pass----2fa----1//0g..."
                  />
                  <button
                    type="button"
                    className="agx-btn agx-btn--primary"
                    disabled={!batchInput.trim() || busyKey === "import-batch"}
                    onClick={async () => {
                      setBusyKey("import-batch");
                      try {
                        await onImportContent(batchInput);
                        setBatchInput("");
                        setAddModalOpen(false);
                      } finally {
                        setBusyKey(null);
                      }
                    }}
                  >
                    {vi ? "Nhập hàng loạt" : "Import Batch"}
                  </button>
                </div>
              ) : null}

              {addTab === "json" ? (
                <div className="od-add-pane">
                  <div className="od-import-picks">
                    <button
                      type="button"
                      className="agx-btn agx-btn--secondary agx-btn--sm"
                      onClick={() => fileInputRef.current?.click()}
                    >
                      <FileJson size={14} />
                      <span>{vi ? "Chọn tệp .JSON" : "Pick .JSON"}</span>
                    </button>
                    <button
                      type="button"
                      className="agx-btn agx-btn--secondary agx-btn--sm"
                      disabled={busyKey === "import-dir"}
                      onClick={() => void handleImportDirectory()}
                    >
                      {busyKey === "import-dir" ? (
                        <Loader2 size={14} className="spin" />
                      ) : (
                        <FolderOpen size={14} />
                      )}
                      <span>{vi ? "Chọn thư mục JSON" : "Pick Folder"}</span>
                    </button>
                  </div>
                  <textarea
                    className="agx-textarea"
                    rows={7}
                    value={jsonInput}
                    onChange={(e) => setJsonInput(e.target.value)}
                    placeholder='[{"email":"...","refresh_token":"1//..."}]'
                  />
                  <button
                    type="button"
                    className="agx-btn agx-btn--primary"
                    disabled={!jsonInput.trim() || busyKey === "import-json"}
                    onClick={async () => {
                      setBusyKey("import-json");
                      try {
                        await onImportContent(jsonInput);
                        setJsonInput("");
                        setAddModalOpen(false);
                      } finally {
                        setBusyKey(null);
                      }
                    }}
                  >
                    {vi ? "Nhập JSON" : "Import JSON"}
                  </button>
                </div>
              ) : null}
            </div>
          </div>
        </div>
      ) : null}

      {/* Launch / Switch Account Modal — Opens ONLY when clicking Play on an account */}
      {launchAccount ? (
        <div
          className="agx-modal-backdrop"
          onClick={() => {
            if (!busyKey) setLaunchAccount(null);
          }}
        >
          <div
            className="agx-modal od-launch-modal"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="agx-modal-head">
              <div className="od-launch-head-copy">
                <h3>{vi ? "Chạy tài khoản" : "Run Account"}</h3>
              </div>
              <button
                type="button"
                className="agx-icon-btn"
                disabled={Boolean(busyKey)}
                onClick={() => setLaunchAccount(null)}
              >
                <X size={16} />
              </button>
            </div>

            <div className="agx-modal-body">
              <div className="od-launch-account-chip">
                <strong title={launchAccount.email}>
                  {maskEmail(launchAccount.email, privacyMode)}
                </strong>
                <span
                  className={`ag-badge-tier is-${normalizeTierBadge(
                    launchAccount.quota?.subscriptionTier,
                    launchAccount,
                  ).toLowerCase()}`}
                >
                  {normalizeTierBadge(launchAccount.quota?.subscriptionTier, launchAccount)}
                </span>
              </div>

              <div className="od-launch-targets">
                {(launchTargets.length > 0
                  ? launchTargets
                  : [
                      {
                        id: "antigravity_ide",
                        label: "Antigravity IDE",
                        available: false,
                        resolvedPath: null,
                      },
                      {
                        id: "antigravity",
                        label: "Antigravity",
                        available: false,
                        resolvedPath: null,
                      },
                      {
                        id: "cli",
                        label: "Terminal CLI",
                        available: false,
                        resolvedPath: null,
                      },
                    ]
                ).map((t) => {
                  const active = selectedLaunchTarget === t.id;
                  const IconComp =
                    t.id === "cli"
                      ? Terminal
                      : t.id === "antigravity_ide"
                        ? Sparkles
                        : Monitor;
                  return (
                    <button
                      key={t.id}
                      type="button"
                      className={`od-launch-target-card${active ? " is-active" : ""}`}
                      onClick={() => {
                        setSelectedLaunchTarget(t.id);
                        setLaunchError(null);
                      }}
                    >
                      <div className="od-launch-target-icon">
                        <IconComp size={18} />
                      </div>
                      <span className="od-launch-target-name">{t.label}</span>
                      <span
                        className={`od-launch-target-dot${t.available ? " is-found" : ""}`}
                      >
                        {t.available
                          ? vi
                            ? "Sẵn sàng"
                            : "Ready"
                          : vi
                            ? "Chưa thấy"
                            : "Not found"}
                      </span>
                    </button>
                  );
                })}
              </div>

              {(() => {
                const cur = launchTargets.find((t) => t.id === selectedLaunchTarget);
                return (
                  <div className="od-launch-path-row">
                    <label className="agx-field">
                      <span>
                        {selectedLaunchTarget === "cli"
                          ? vi
                            ? "Thư mục làm việc (tùy chọn)"
                            : "Working directory (optional)"
                          : vi
                            ? "Đường dẫn thực thi"
                            : "Executable path"}
                      </span>
                      {selectedLaunchTarget === "cli" ? (
                        <div className="od-inline-submit">
                          <input
                            type="text"
                            className="agx-input"
                            value={launchWorkingDir}
                            onChange={(e) => setLaunchWorkingDir(e.target.value)}
                            placeholder={vi ? "Mặc định: Home" : "Default: Home"}
                          />
                          <button
                            type="button"
                            className="agx-btn agx-btn--secondary"
                            onClick={async () => {
                              const p = await agxInvoke<string | null>("pick_folder_dialog");
                              if (p) setLaunchWorkingDir(p);
                            }}
                          >
                            <FolderOpen size={14} />
                          </button>
                        </div>
                      ) : (
                        <input
                          type="text"
                          className="agx-input"
                          value={launchCustomPath}
                          onChange={(e) => setLaunchCustomPath(e.target.value)}
                          placeholder={cur?.resolvedPath || "C:\\...\\Antigravity.exe"}
                        />
                      )}
                    </label>
                  </div>
                );
              })()}

              {launchError ? (
                <div className="od-banner is-err">
                  <span>{launchError}</span>
                </div>
              ) : null}
            </div>

            <div className="agx-modal-foot">
              <button
                type="button"
                className="agx-btn agx-btn--ghost"
                disabled={Boolean(busyKey)}
                onClick={() => setLaunchAccount(null)}
              >
                {tr(lang, "acc.cancel")}
              </button>
              <button
                type="button"
                className="agx-btn agx-btn--secondary"
                disabled={Boolean(busyKey)}
                onClick={() => void executeLaunchOrSwitch(false)}
              >
                {busyKey === `switch-${launchAccount.id}` ? (
                  <Loader2 size={14} className="spin" />
                ) : null}
                <span>{vi ? "Chỉ chuyển" : "Switch Only"}</span>
              </button>
              <button
                type="button"
                className="agx-btn agx-btn--primary"
                disabled={Boolean(busyKey)}
                onClick={() => void executeLaunchOrSwitch(true)}
              >
                {busyKey === `launch-${launchAccount.id}` ? (
                  <Loader2 size={14} className="spin" />
                ) : (
                  <Play size={14} />
                )}
                <span>{vi ? "Chuyển & Mở" : "Switch & Launch"}</span>
              </button>
            </div>
          </div>
        </div>
      ) : null}

      {/* Account Info & Full Model Quota Modal */}
      {infoAccount ? (
        <div
          className="agx-modal-backdrop"
          onClick={() => setInfoAccount(null)}
        >
          <div
            className="agx-modal agx-modal--wide"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="agx-modal-head">
              <div>
                <h3>{maskEmail(infoAccount.email, privacyMode)}</h3>
                <span className="agx-muted">
                  Tier: {normalizeTierBadge(infoAccount.quota?.subscriptionTier, infoAccount)} · Project:{" "}
                  {infoAccount.quota?.projectId ||
                    infoAccount.token.projectId ||
                    "—"}
                </span>
              </div>
              <button
                type="button"
                className="agx-icon-btn"
                onClick={() => setInfoAccount(null)}
              >
                <X size={16} />
              </button>
            </div>
            <div className="agx-modal-body">
              {infoAccount.quotaError ? (
                <div className="ag-token-usage__error">
                  <strong>
                    {infoAccount.quotaError.reason || "Quota Error"}:
                  </strong>{" "}
                  {infoAccount.quotaError.message}
                  {infoAccount.quotaError.validationUrl ? (
                    <div style={{ marginTop: 6 }}>
                      <a
                        href={infoAccount.quotaError.validationUrl}
                        target="_blank"
                        rel="noreferrer"
                      >
                        {vi ? "Mở link xác minh Google" : "Open Google Verification"}
                      </a>
                    </div>
                  ) : null}
                </div>
              ) : null}

              {(infoAccount.quota?.credits?.length ?? 0) > 0 ? (
                <div className="ag-info-credits">
                  <h4>Credits</h4>
                  <div className="ag-info-credit-pills">
                    {infoAccount.quota!.credits.map((c, idx) => (
                      <span key={idx} className="ag-acc-tag-pill">
                        {c.creditType}: <strong>{c.creditAmount ?? "—"}</strong>
                      </span>
                    ))}
                  </div>
                </div>
              ) : null}

              <div className="ag-token-table-wrap">
                <table className="ag-token-table">
                  <thead>
                    <tr>
                      <th>{vi ? "Mô hình / Bucket" : "Model / Bucket"}</th>
                      <th className="is-num">{vi ? "Còn lại" : "Remaining"}</th>
                      <th className="is-num">{vi ? "Reset sau" : "Reset In"}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {(infoAccount.quota?.models ?? []).map((m) => (
                      <tr key={m.name}>
                        <td>
                          <strong>{m.displayName || m.name}</strong>
                          <div className="ag-token-cell-sub">{m.name}</div>
                        </td>
                        <td className="is-num">
                          <strong>{m.percentage}%</strong>
                        </td>
                        <td className="is-num">
                          {formatResetCountdown(m.resetTime, vi) || "—"}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </div>
          </div>
        </div>
      ) : null}

      {/* Quick Tag Editor Modal */}
      {tagAccount ? (
        <div
          className="agx-modal-backdrop"
          onClick={() => setTagAccount(null)}
        >
          <div className="agx-modal" onClick={(e) => e.stopPropagation()}>
            <div className="agx-modal-head">
              <h3>
                {vi ? "Chỉnh sửa thẻ (Tags)" : "Edit Tags"} —{" "}
                {maskEmail(tagAccount.email, privacyMode)}
              </h3>
              <button
                type="button"
                className="agx-icon-btn"
                onClick={() => setTagAccount(null)}
              >
                <X size={16} />
              </button>
            </div>
            <div className="agx-modal-body">
              <label className="agx-field">
                <span>
                  {vi
                    ? "Danh sách thẻ, phân tách bởi dấu phẩy:"
                    : "Comma-separated tags:"}
                </span>
                <input
                  type="text"
                  className="agx-input"
                  value={tagDraftInput}
                  onChange={(e) => setTagDraftInput(e.target.value)}
                  placeholder="pro, main, team-a"
                />
              </label>
              {allTags.length > 0 ? (
                <div className="ag-existing-tags-pick">
                  {allTags.map((t) => (
                    <button
                      key={t}
                      type="button"
                      className="ag-acc-tag-pill"
                      onClick={() => {
                        const cur = tagDraftInput
                          .split(",")
                          .map((s) => s.trim())
                          .filter(Boolean);
                        if (!cur.includes(t)) {
                          setTagDraftInput([...cur, t].join(", "));
                        }
                      }}
                    >
                      + {t}
                    </button>
                  ))}
                </div>
              ) : null}
            </div>
            <div className="agx-modal-foot">
              <button
                type="button"
                className="agx-btn agx-btn--secondary"
                onClick={() => setTagAccount(null)}
              >
                {vi ? "Hủy" : "Cancel"}
              </button>
              <button
                type="button"
                className="agx-btn agx-btn--primary"
                disabled={busyKey === "save-tags"}
                onClick={() => void handleSaveTags()}
              >
                <Save size={14} />
                <span>{vi ? "Lưu thẻ" : "Save Tags"}</span>
              </button>
            </div>
          </div>
        </div>
      ) : null}

      {/* Vault (Notes / Password / 2FA TOTP) Modal */}
      {vaultAccount ? (
        <div
          className="agx-modal-backdrop"
          onClick={() => setVaultAccount(null)}
        >
          <div
            className="agx-modal agx-modal--wide"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="agx-modal-head">
              <h3>
                {vi ? "Ghi chú & 2FA Vault" : "Notes & 2FA Vault"} —{" "}
                {maskEmail(vaultAccount.email, privacyMode)}
              </h3>
              <button
                type="button"
                className="agx-icon-btn"
                onClick={() => setVaultAccount(null)}
              >
                <X size={16} />
              </button>
            </div>
            <div className="agx-modal-body">
              {totpLive ? (
                <div className="ag-totp-banner">
                  <div>
                    <span>{vi ? "Mã 2FA TOTP trực tiếp" : "Live 2FA TOTP"}</span>
                    <strong>{totpLive.code}</strong>
                  </div>
                  <div className="ag-totp-right">
                    <span>{totpLive.remainingSeconds}s</span>
                    <button
                      type="button"
                      className="agx-btn agx-btn--primary agx-btn--sm"
                      onClick={() => void copyText(totpLive.code, "totp")}
                    >
                      {copiedId === "totp" ? (
                        <Check size={14} />
                      ) : (
                        <Copy size={14} />
                      )}
                      <span>{vi ? "Sao chép" : "Copy"}</span>
                    </button>
                  </div>
                </div>
              ) : null}

              <div className="ag-vault-grid">
                <label className="agx-field">
                  <span>{vi ? "Mật khẩu tài khoản" : "Account Password"}</span>
                  <div className="ag-input-with-btn">
                    <input
                      type={showVaultPassword ? "text" : "password"}
                      className="agx-input"
                      value={vaultForm.accountPassword}
                      onChange={(e) =>
                        setVaultForm((f) => ({
                          ...f,
                          accountPassword: e.target.value,
                        }))
                      }
                    />
                    <button
                      type="button"
                      className="agx-icon-btn"
                      onClick={() => setShowVaultPassword((s) => !s)}
                    >
                      {showVaultPassword ? <EyeOff size={14} /> : <Eye size={14} />}
                    </button>
                  </div>
                </label>

                <label className="agx-field">
                  <span>{vi ? "Khóa bí mật 2FA (Base32 / otpauth)" : "2FA Secret (Base32 / otpauth)"}</span>
                  <input
                    type="text"
                    className="agx-input"
                    value={vaultForm.twoFactorSecret}
                    onChange={(e) =>
                      setVaultForm((f) => ({
                        ...f,
                        twoFactorSecret: e.target.value,
                      }))
                    }
                    placeholder="JBSWY3DPEHPK3PXP"
                  />
                </label>

                <label className="agx-field">
                  <span>{vi ? "Email khôi phục" : "Recovery Email"}</span>
                  <input
                    type="text"
                    className="agx-input"
                    value={vaultForm.auxEmail}
                    onChange={(e) =>
                      setVaultForm((f) => ({ ...f, auxEmail: e.target.value }))
                    }
                  />
                </label>

                <label className="agx-field">
                  <span>{vi ? "Số điện thoại" : "Phone Number"}</span>
                  <input
                    type="text"
                    className="agx-input"
                    value={vaultForm.phoneNumber}
                    onChange={(e) =>
                      setVaultForm((f) => ({ ...f, phoneNumber: e.target.value }))
                    }
                  />
                </label>

                <label className="agx-field">
                  <span>{vi ? "Link hộp thư (Mail URL)" : "Mail URL"}</span>
                  <input
                    type="text"
                    className="agx-input"
                    value={vaultForm.mailUrl}
                    onChange={(e) =>
                      setVaultForm((f) => ({ ...f, mailUrl: e.target.value }))
                    }
                  />
                </label>

                <label className="agx-field">
                  <span>{vi ? "Thẻ (Tags)" : "Tags"}</span>
                  <input
                    type="text"
                    className="agx-input"
                    value={vaultForm.tags}
                    onChange={(e) =>
                      setVaultForm((f) => ({ ...f, tags: e.target.value }))
                    }
                  />
                </label>
              </div>

              <label className="agx-field" style={{ marginTop: 12 }}>
                <span>{vi ? "Ghi chú" : "Notes"}</span>
                <textarea
                  className="agx-textarea"
                  rows={3}
                  value={vaultForm.notes}
                  onChange={(e) =>
                    setVaultForm((f) => ({ ...f, notes: e.target.value }))
                  }
                />
              </label>
            </div>
            <div className="agx-modal-foot">
              <button
                type="button"
                className="agx-btn agx-btn--secondary"
                onClick={() => setVaultAccount(null)}
              >
                {vi ? "Hủy" : "Cancel"}
              </button>
              <button
                type="button"
                className="agx-btn agx-btn--primary"
                disabled={busyKey === "save-vault"}
                onClick={() => void handleSaveVault()}
              >
                <Save size={14} />
                <span>{vi ? "Lưu Vault" : "Save Vault"}</span>
              </button>
            </div>
          </div>
        </div>
      ) : null}

      {/* Account Group Manager Modal */}
      {groupModalOpen ? (
        <div
          className="agx-modal-backdrop"
          onClick={() => setGroupModalOpen(false)}
        >
          <div className="agx-modal" onClick={(e) => e.stopPropagation()}>
            <div className="agx-modal-head">
              <h3>
                {editingGroupId
                  ? vi
                    ? "Chỉnh sửa nhóm tài khoản"
                    : "Edit Account Group"
                  : vi
                    ? "Tạo nhóm tài khoản mới"
                    : "Create Account Group"}
              </h3>
              <button
                type="button"
                className="agx-icon-btn"
                onClick={() => setGroupModalOpen(false)}
              >
                <X size={16} />
              </button>
            </div>
            <div className="agx-modal-body">
              <label className="agx-field">
                <span>{vi ? "Tên nhóm" : "Group Name"}</span>
                <input
                  type="text"
                  className="agx-input"
                  value={groupNameInput}
                  onChange={(e) => setGroupNameInput(e.target.value)}
                  placeholder={vi ? "Ví dụ: PRO chính, Dự phòng..." : "e.g. Main PRO, Backup..."}
                />
              </label>
              <div className="ag-group-member-picker">
                <span className="agx-muted">
                  {vi ? "Chọn tài khoản thuộc nhóm:" : "Select accounts in group:"}
                </span>
                <div className="ag-group-member-list">
                  {accounts.map((a) => (
                    <label key={a.id} className="ag-group-member-item">
                      <input
                        type="checkbox"
                        checked={groupMemberIds.has(a.id)}
                        onChange={() => {
                          setGroupMemberIds((prev) => {
                            const next = new Set(prev);
                            if (next.has(a.id)) next.delete(a.id);
                            else next.add(a.id);
                            return next;
                          });
                        }}
                      />
                      <span>{maskEmail(a.email, privacyMode)}</span>
                    </label>
                  ))}
                </div>
              </div>
            </div>
            <div className="agx-modal-foot">
              {editingGroupId ? (
                <button
                  type="button"
                  className="agx-btn agx-btn--danger"
                  onClick={() => void handleDeleteGroup(editingGroupId)}
                >
                  <Trash2 size={14} />
                  <span>{vi ? "Xóa nhóm" : "Delete Group"}</span>
                </button>
              ) : null}
              <button
                type="button"
                className="agx-btn agx-btn--secondary"
                onClick={() => setGroupModalOpen(false)}
              >
                {vi ? "Hủy" : "Cancel"}
              </button>
              <button
                type="button"
                className="agx-btn agx-btn--primary"
                disabled={!groupNameInput.trim() || busyKey === "save-group"}
                onClick={() => void handleSaveGroup()}
              >
                <Save size={14} />
                <span>{vi ? "Lưu nhóm" : "Save Group"}</span>
              </button>
            </div>
          </div>
        </div>
      ) : null}

      {/* Custom Sort Reorder Modal */}
      {customSortModalOpen ? (
        <div
          className="agx-modal-backdrop"
          onClick={() => setCustomSortModalOpen(false)}
        >
          <div className="agx-modal" onClick={(e) => e.stopPropagation()}>
            <div className="agx-modal-head">
              <h3>{vi ? "Sắp xếp thứ tự tùy chỉnh" : "Custom Account Order"}</h3>
              <button
                type="button"
                className="agx-icon-btn"
                onClick={() => setCustomSortModalOpen(false)}
              >
                <X size={16} />
              </button>
            </div>
            <div className="agx-modal-body">
              <div className="ag-custom-sort-list">
                {sortDraftIds.map((id, idx) => {
                  const acc = accounts.find((a) => a.id === id);
                  if (!acc) return null;
                  return (
                    <div key={id} className="ag-custom-sort-row">
                      <span>
                        {idx + 1}. {maskEmail(acc.email, privacyMode)}
                      </span>
                      <div className="ag-custom-sort-btns">
                        <button
                          type="button"
                          className="ag-card-icon-btn"
                          disabled={idx === 0}
                          onClick={() => moveDraftOrder(idx, -1)}
                        >
                          <ArrowUp size={13} />
                        </button>
                        <button
                          type="button"
                          className="ag-card-icon-btn"
                          disabled={idx === sortDraftIds.length - 1}
                          onClick={() => moveDraftOrder(idx, 1)}
                        >
                          <ArrowDown size={13} />
                        </button>
                      </div>
                    </div>
                  );
                })}
              </div>
            </div>
            <div className="agx-modal-foot">
              <button
                type="button"
                className="agx-btn agx-btn--secondary"
                onClick={() => setCustomSortModalOpen(false)}
              >
                {vi ? "Hủy" : "Cancel"}
              </button>
              <button
                type="button"
                className="agx-btn agx-btn--primary"
                disabled={busyKey === "save-sort"}
                onClick={() => void saveDraftCustomSort()}
              >
                <Save size={14} />
                <span>{vi ? "Lưu thứ tự" : "Save Order"}</span>
              </button>
            </div>
          </div>
        </div>
      ) : null}

      {/* Export JSON Modal */}
      {exportModalOpen ? (
        <div
          className="agx-modal-backdrop"
          onClick={() => setExportModalOpen(false)}
        >
          <div
            className="agx-modal agx-modal--wide"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="agx-modal-head">
              <h3>{vi ? "Xuất tài khoản JSON" : "Export Accounts JSON"}</h3>
              <button
                type="button"
                className="agx-icon-btn"
                onClick={() => setExportModalOpen(false)}
              >
                <X size={16} />
              </button>
            </div>
            <div className="agx-modal-body">
              <label className="ag-select-all-label" style={{ marginBottom: 10 }}>
                <input
                  type="checkbox"
                  checked={exportIncludeSensitive}
                  onChange={(e) => {
                    const checked = e.target.checked;
                    setExportIncludeSensitive(checked);
                    void handleOpenExport(exportTargetIds, checked);
                  }}
                />
                <span>
                  {vi
                    ? "Bao gồm thông tin nhạy cảm (Mật khẩu, 2FA, ghi chú)"
                    : "Include sensitive Vault fields (Password, 2FA, notes)"}
                </span>
              </label>
              <textarea
                className="agx-textarea"
                rows={12}
                readOnly
                value={exportJsonText}
              />
            </div>
            <div className="agx-modal-foot">
              <button
                type="button"
                className="agx-btn agx-btn--secondary"
                onClick={() => void copyText(exportJsonText, "export-json")}
              >
                {copiedId === "export-json" ? (
                  <Check size={14} />
                ) : (
                  <Copy size={14} />
                )}
                <span>{vi ? "Sao chép JSON" : "Copy JSON"}</span>
              </button>
              <button
                type="button"
                className="agx-btn agx-btn--primary"
                onClick={handleDownloadExport}
              >
                <Download size={14} />
                <span>{vi ? "Tải tệp .JSON" : "Download .JSON"}</span>
              </button>
            </div>
          </div>
        </div>
      ) : null}

      {/* Switch History Modal */}
      {historyModalOpen ? (
        <div
          className="agx-modal-backdrop"
          onClick={() => setHistoryModalOpen(false)}
        >
          <div
            className="agx-modal agx-modal--wide"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="agx-modal-head">
              <h3>{vi ? "Lịch sử chuyển tài khoản" : "Account Switch History"}</h3>
              <button
                type="button"
                className="agx-icon-btn"
                onClick={() => setHistoryModalOpen(false)}
              >
                <X size={16} />
              </button>
            </div>
            <div className="agx-modal-body">
              {switchHistory.length === 0 ? (
                <p className="agx-muted">
                  {vi ? "Chưa có lịch sử chuyển tài khoản." : "No switch history yet."}
                </p>
              ) : (
                <div className="ag-token-table-wrap">
                  <table className="ag-token-table">
                    <thead>
                      <tr>
                        <th>{vi ? "Thời gian" : "Time"}</th>
                        <th>{vi ? "Tài khoản" : "Target"}</th>
                        <th>{vi ? "Phạm vi" : "Scope"}</th>
                        <th>{vi ? "Trạng thái" : "Status"}</th>
                        <th className="is-num">ms</th>
                      </tr>
                    </thead>
                    <tbody>
                      {switchHistory.map((h) => (
                        <tr key={h.id}>
                          <td>{formatDateTimeFull(h.timestamp)}</td>
                          <td>{maskEmail(h.targetEmail, privacyMode)}</td>
                          <td>{h.switchTarget.toUpperCase()}</td>
                          <td>
                            {h.success ? (
                              <span className="ag-badge-current">OK</span>
                            ) : (
                              <span className="ag-badge-err">{h.message}</span>
                            )}
                          </td>
                          <td className="is-num">{h.durationMs}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
            </div>
            <div className="agx-modal-foot">
              {switchHistory.length > 0 ? (
                <button
                  type="button"
                  className="agx-btn agx-btn--danger"
                  onClick={() => void onClearSwitchHistory()}
                >
                  <Trash2 size={14} />
                  <span>{vi ? "Xóa lịch sử" : "Clear History"}</span>
                </button>
              ) : null}
              <button
                type="button"
                className="agx-btn agx-btn--secondary"
                onClick={() => setHistoryModalOpen(false)}
              >
                {vi ? "Đóng" : "Close"}
              </button>
            </div>
          </div>
        </div>
      ) : null}

      {autoSwitchModalOpen ? (
        <div
          className="agx-modal-backdrop"
          onClick={() => setAutoSwitchModalOpen(false)}
        >
          <div
            className="agx-modal agx-modal--wide"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="agx-modal-head">
              <h3>
                <Zap size={16} />{" "}
                {vi ? "Tự động chuyển đổi tài khoản" : "Auto Switch Accounts"}
              </h3>
              <button
                type="button"
                className="agx-icon-btn"
                onClick={() => setAutoSwitchModalOpen(false)}
              >
                <X size={16} />
              </button>
            </div>
            <div className="agx-modal-body">
              <div className="ag-autoswitch-list">
                <label className="ag-autoswitch-row">
                  <span>{vi ? "Tự động chuyển đổi" : "Enable Auto Switch"}</span>
                  <input
                    type="checkbox"
                    checked={autoSwitchCfg.enabled}
                    onChange={(e) =>
                      setAutoSwitchCfg({ ...autoSwitchCfg, enabled: e.target.checked })
                    }
                  />
                </label>

                <div className="ag-autoswitch-row">
                  <span>{vi ? "Ngưỡng chuyển đổi" : "Switch Threshold"}</span>
                  <select
                    className="agx-select"
                    value={autoSwitchCfg.threshold}
                    onChange={(e) =>
                      setAutoSwitchCfg({
                        ...autoSwitchCfg,
                        threshold: Number(e.target.value),
                      })
                    }
                  >
                    {[0, 5, 10, 15, 20, 30, 40].map((v) => (
                      <option key={v} value={v}>
                        {v}%
                      </option>
                    ))}
                  </select>
                </div>

                <label className="ag-autoswitch-row">
                  <span>
                    {vi
                      ? "Tách ngưỡng 5 giờ và hằng tuần"
                      : "Split 5-hour and Weekly thresholds"}
                  </span>
                  <input
                    type="checkbox"
                    checked={autoSwitchCfg.splitWindowThresholds}
                    onChange={(e) =>
                      setAutoSwitchCfg({
                        ...autoSwitchCfg,
                        splitWindowThresholds: e.target.checked,
                      })
                    }
                  />
                </label>

                {autoSwitchCfg.splitWindowThresholds ? (
                  <div className="ag-autoswitch-subgrid">
                    <label className="agx-field">
                      <span>{vi ? "Ngưỡng cửa sổ 5 giờ" : "5-Hour Threshold"}</span>
                      <select
                        className="agx-select"
                        value={autoSwitchCfg.fiveHourThreshold}
                        onChange={(e) =>
                          setAutoSwitchCfg({
                            ...autoSwitchCfg,
                            fiveHourThreshold: Number(e.target.value),
                          })
                        }
                      >
                        {[0, 5, 10, 15, 20, 30].map((v) => (
                          <option key={v} value={v}>
                            {v}%
                          </option>
                        ))}
                      </select>
                    </label>
                    <label className="agx-field">
                      <span>{vi ? "Ngưỡng hằng tuần" : "Weekly Threshold"}</span>
                      <select
                        className="agx-select"
                        value={autoSwitchCfg.weeklyThreshold}
                        onChange={(e) =>
                          setAutoSwitchCfg({
                            ...autoSwitchCfg,
                            weeklyThreshold: Number(e.target.value),
                          })
                        }
                      >
                        {[0, 5, 10, 15, 20, 30].map((v) => (
                          <option key={v} value={v}>
                            {v}%
                          </option>
                        ))}
                      </select>
                    </label>
                  </div>
                ) : null}

                <label className="ag-autoswitch-row">
                  <span>{vi ? "Theo dõi credits" : "Monitor Credits"}</span>
                  <input
                    type="checkbox"
                    checked={autoSwitchCfg.creditsEnabled}
                    onChange={(e) =>
                      setAutoSwitchCfg({
                        ...autoSwitchCfg,
                        creditsEnabled: e.target.checked,
                      })
                    }
                  />
                </label>

                {autoSwitchCfg.creditsEnabled ? (
                  <div className="ag-autoswitch-row">
                    <span>{vi ? "Ngưỡng credits tối thiểu" : "Min Credits Threshold"}</span>
                    <input
                      type="number"
                      className="agx-input"
                      style={{ width: 120 }}
                      min={0}
                      value={autoSwitchCfg.creditsThreshold}
                      onChange={(e) =>
                        setAutoSwitchCfg({
                          ...autoSwitchCfg,
                          creditsThreshold: Number(e.target.value) || 0,
                        })
                      }
                    />
                  </div>
                ) : null}

                <div className="ag-autoswitch-row">
                  <span>{vi ? "Mô hình kích hoạt" : "Trigger Model Scope"}</span>
                  <select
                    className="agx-select"
                    value={autoSwitchCfg.scopeMode}
                    onChange={(e) =>
                      setAutoSwitchCfg({
                        ...autoSwitchCfg,
                        scopeMode: e.target.value,
                      })
                    }
                  >
                    <option value="any_group">
                      {vi ? "Bất kỳ nhóm mô hình nào" : "Any model group"}
                    </option>
                    <option value="all_groups">
                      {vi ? "Tất cả nhóm mô hình" : "All model groups"}
                    </option>
                    <option value="selected_groups">
                      {vi ? "Chọn nhóm mô hình cụ thể" : "Selected model groups"}
                    </option>
                  </select>
                </div>

                {autoSwitchCfg.scopeMode === "selected_groups" ? (
                  <div className="ag-autoswitch-pills">
                    {[
                      { id: "gemini-5h", label: "Gemini 5h" },
                      { id: "gemini-weekly", label: "Gemini Weekly" },
                      { id: "claude-5h", label: "Claude 5h" },
                      { id: "claude-weekly", label: "Claude Weekly" },
                    ].map((g) => {
                      const active = autoSwitchCfg.selectedGroupIds.includes(g.id);
                      return (
                        <button
                          key={g.id}
                          type="button"
                          className={`agx-btn agx-btn--sm ${
                            active ? "agx-btn--primary" : "agx-btn--secondary"
                          }`}
                          onClick={() => {
                            const next = active
                              ? autoSwitchCfg.selectedGroupIds.filter((x) => x !== g.id)
                              : [...autoSwitchCfg.selectedGroupIds, g.id];
                            setAutoSwitchCfg({
                              ...autoSwitchCfg,
                              selectedGroupIds: next.length > 0 ? next : [g.id],
                            });
                          }}
                        >
                          {g.label}
                        </button>
                      );
                    })}
                  </div>
                ) : null}

                <div className="ag-autoswitch-row">
                  <div>
                    <div>
                      {vi ? "Phạm vi chuyển tài khoản tự động" : "Auto-Switch Account Scope"}
                    </div>
                    <small className="agx-muted">
                      {autoSwitchCfg.accountScopeMode === "all_accounts"
                        ? vi
                          ? `Phạm vi hiện tại: tất cả tài khoản (tổng ${accounts.length})`
                          : `Current scope: all accounts (${accounts.length} total)`
                        : vi
                          ? `Đã chọn ${autoSwitchCfg.selectedAccountIds.length}/${accounts.length} tài khoản`
                          : `Selected ${autoSwitchCfg.selectedAccountIds.length}/${accounts.length} accounts`}
                    </small>
                  </div>
                  <button
                    type="button"
                    className="agx-btn agx-btn--secondary agx-btn--sm"
                    onClick={() => setAutoSwitchScopeOpen((v) => !v)}
                  >
                    {vi ? "Chọn tài khoản" : "Select Accounts"}
                  </button>
                </div>

                {autoSwitchScopeOpen ? (
                  <div className="ag-autoswitch-scope-box">
                    <div style={{ display: "flex", gap: 8, marginBottom: 8 }}>
                      <button
                        type="button"
                        className={`agx-btn agx-btn--sm ${
                          autoSwitchCfg.accountScopeMode === "all_accounts"
                            ? "agx-btn--primary"
                            : "agx-btn--secondary"
                        }`}
                        onClick={() =>
                          setAutoSwitchCfg({
                            ...autoSwitchCfg,
                            accountScopeMode: "all_accounts",
                          })
                        }
                      >
                        {vi ? "Tất cả tài khoản" : "All Accounts"}
                      </button>
                      <button
                        type="button"
                        className={`agx-btn agx-btn--sm ${
                          autoSwitchCfg.accountScopeMode === "selected_accounts"
                            ? "agx-btn--primary"
                            : "agx-btn--secondary"
                        }`}
                        onClick={() =>
                          setAutoSwitchCfg({
                            ...autoSwitchCfg,
                            accountScopeMode: "selected_accounts",
                          })
                        }
                      >
                        {vi ? "Chỉ tài khoản đã chọn" : "Selected Only"}
                      </button>
                    </div>
                    {autoSwitchCfg.accountScopeMode === "selected_accounts" ? (
                      <div className="ag-autoswitch-acc-list">
                        {accounts.map((acc) => {
                          const checked = autoSwitchCfg.selectedAccountIds.includes(acc.id);
                          return (
                            <label key={acc.id} className="ag-select-all-label">
                              <input
                                type="checkbox"
                                checked={checked}
                                onChange={(e) => {
                                  const next = e.target.checked
                                    ? [...autoSwitchCfg.selectedAccountIds, acc.id]
                                    : autoSwitchCfg.selectedAccountIds.filter(
                                        (id) => id !== acc.id,
                                      );
                                  setAutoSwitchCfg({
                                    ...autoSwitchCfg,
                                    selectedAccountIds: next,
                                  });
                                }}
                              />
                              <span>{maskEmail(acc.email, privacyMode)}</span>
                            </label>
                          );
                        })}
                      </div>
                    ) : null}
                  </div>
                ) : null}

                <div className="ag-autoswitch-subgrid">
                  <label className="agx-field">
                    <span>{vi ? "Đích chuyển tài khoản" : "Switch Target"}</span>
                    <select
                      className="agx-select"
                      value={autoSwitchCfg.switchTarget}
                      onChange={(e) =>
                        setAutoSwitchCfg({
                          ...autoSwitchCfg,
                          switchTarget: e.target.value,
                        })
                      }
                    >
                      <option value="ide">Antigravity IDE</option>
                      <option value="cli">Antigravity CLI</option>
                      <option value="both">{vi ? "Cả hai (IDE + CLI)" : "Both (IDE + CLI)"}</option>
                    </select>
                  </label>
                  <label className="agx-field">
                    <span>{vi ? "Chu kỳ quét nền" : "Background Check Interval"}</span>
                    <select
                      className="agx-select"
                      value={autoSwitchCfg.pollIntervalMinutes}
                      onChange={(e) =>
                        setAutoSwitchCfg({
                          ...autoSwitchCfg,
                          pollIntervalMinutes: Number(e.target.value),
                        })
                      }
                    >
                      {[1, 3, 5, 10, 15].map((m) => (
                        <option key={m} value={m}>
                          {m} {vi ? "phút" : "min"}
                        </option>
                      ))}
                    </select>
                  </label>
                </div>

                <div className="ag-autoswitch-note">
                  {vi
                    ? "Tự động chuyển khi các nhóm mô hình đang theo dõi chạm ngưỡng; nếu bật theo dõi credits, hệ thống cũng chuyển khi credits còn lại thấp hơn ngưỡng."
                    : "Automatically switches when monitored model groups hit the threshold; if credit monitoring is enabled, also triggers when remaining credits drop below the threshold."}
                </div>

                <label className="ag-autoswitch-row">
                  <span>{vi ? "Bật cảnh báo hạn mức" : "Enable Quota Alert"}</span>
                  <input
                    type="checkbox"
                    checked={autoSwitchCfg.quotaAlertEnabled}
                    onChange={(e) =>
                      setAutoSwitchCfg({
                        ...autoSwitchCfg,
                        quotaAlertEnabled: e.target.checked,
                      })
                    }
                  />
                </label>

                {autoSwitchCfg.quotaAlertEnabled ? (
                  <div className="ag-autoswitch-row">
                    <span>{vi ? "Ngưỡng cảnh báo hạn mức" : "Quota Alert Threshold"}</span>
                    <select
                      className="agx-select"
                      value={autoSwitchCfg.quotaAlertThreshold}
                      onChange={(e) =>
                        setAutoSwitchCfg({
                          ...autoSwitchCfg,
                          quotaAlertThreshold: Number(e.target.value),
                        })
                      }
                    >
                      {[5, 10, 15, 20, 30, 50].map((v) => (
                        <option key={v} value={v}>
                          {v}%
                        </option>
                      ))}
                    </select>
                  </div>
                ) : null}
              </div>
            </div>
            <div className="agx-modal-foot">
              <button
                type="button"
                className="agx-btn agx-btn--secondary"
                disabled={autoSwitchChecking}
                onClick={async () => {
                  setAutoSwitchChecking(true);
                  try {
                    const res = await agxInvoke<AutoSwitchEvalResult>("run_auto_switch_check");
                    flashMsg(
                      res.switched ? "ok" : "err",
                      res.switched
                        ? vi
                          ? `Đã chuyển sang ${res.toEmail}: ${res.reason}`
                          : `Switched to ${res.toEmail}: ${res.reason}`
                        : res.reason,
                    );
                    if (res.switched) {
                      await onRefreshAllData();
                    }
                  } catch (e) {
                    flashMsg("err", e instanceof Error ? e.message : String(e));
                  } finally {
                    setAutoSwitchChecking(false);
                  }
                }}
              >
                <RefreshCw size={14} className={autoSwitchChecking ? "spin" : undefined} />
                <span>{vi ? "Kiểm tra ngay" : "Check Now"}</span>
              </button>
              <button
                type="button"
                className="agx-btn agx-btn--primary"
                disabled={autoSwitchSaving}
                onClick={async () => {
                  setAutoSwitchSaving(true);
                  try {
                    const saved = await agxInvoke<AutoSwitchConfig>(
                      "save_auto_switch_config",
                      { config: autoSwitchCfg },
                    );
                    setAutoSwitchCfg(saved);
                    setAutoSwitchModalOpen(false);
                    flashMsg(
                      "ok",
                      vi ? "Đã lưu cấu hình Tự động chuyển đổi" : "Saved Auto Switch settings",
                    );
                  } catch (e) {
                    flashMsg("err", e instanceof Error ? e.message : String(e));
                  } finally {
                    setAutoSwitchSaving(false);
                  }
                }}
              >
                <Save size={14} />
                <span>{vi ? "Lưu cấu hình" : "Save Settings"}</span>
              </button>
            </div>
          </div>
        </div>
      ) : null}
    </div>
  );
}
