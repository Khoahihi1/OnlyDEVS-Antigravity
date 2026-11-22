import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  AlertTriangle,
  ArrowDown,
  ArrowUp,
  BarChart3,
  Check,
  Clock,
  Copy,
  Database,
  Download,
  ExternalLink,
  Eye,
  EyeOff,
  FileJson,
  FileText,
  FolderPlus,
  GripVertical,
  History,
  Info,
  KeyRound,
  LayoutGrid,
  List,
  Loader2,
  Play,
  Plus,
  RefreshCw,
  Rows3,
  Save,
  Search,
  Tag,
  Trash2,
  Upload,
  UserCheck,
  Users,
  X,
} from "lucide-react";
import type {
  AccountGroup,
  AgUsageRangePreset,
  AntigravityAccount,
  AppLanguage,
  DashboardBundleResponse,
  OAuthPrepareResult,
  RefreshStats,
  SwitchHistoryItem,
  TokenUsageSummary,
  TotpCodeResult,
} from "../types";
import { agxInvoke, openExternalUrl } from "../platform/invoke";
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
  runtimeLabel?: string;
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
    switchTarget: "all" | "ide" | "cli",
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
  onSubmitOAuthCallback: (callbackUrl: string) => Promise<void>;
  onGenerateTotp: (secret: string) => Promise<TotpCodeResult>;
  onClearSwitchHistory: () => Promise<void>;
};

type AccountSubTab = "overview" | "usage";
type ViewMode = "grid" | "list" | "compact";
type TierFilter =
  | "ALL"
  | "PRO"
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
  runtimeLabel,
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
  onSubmitOAuthCallback,
  onGenerateTotp,
  onClearSwitchHistory,
}: AccountsPageProps) {
  const vi = lang === "vi";
  const fileInputRef = useRef<HTMLInputElement | null>(null);

  // Cockpit 2-tab navigation ("Tổng quan tài khoản" | "Mức dùng phiên")
  const [subTab, setSubTab] = useState<AccountSubTab>("overview");

  // Local Token Usage Summary Strip (powered by real Cockpit antigravity_token_usage backend!)
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
  const [switchTarget, setSwitchTarget] = useState<"all" | "ide" | "cli">("all");
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
    "oauth" | "local" | "cockpit" | "token" | "batch" | "json"
  >("oauth");
  const [oauthPrep, setOauthPrep] = useState<OAuthPrepareResult | null>(null);
  const [oauthCallbackUrl, setOauthCallbackUrl] = useState("");
  const [tokenInput, setTokenInput] = useState("");
  const [tokenEmailHint, setTokenEmailHint] = useState("");
  const [batchInput, setBatchInput] = useState("");
  const [jsonInput, setJsonInput] = useState("");

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
    let ultra = 0;
    let free = 0;
    let reauth = 0;
    let forbidden = 0;
    let valid = 0;
    for (const acc of accounts) {
      const t = normalizeTierBadge(acc.quota?.subscriptionTier);
      if (t === "PRO") pro++;
      else if (t === "ULTRA") ultra++;
      else free++;
      const needs = isAccountNeedsReauth(acc);
      const forb = Boolean(acc.quota?.isForbidden);
      if (needs) reauth++;
      if (forb) forbidden++;
      if (!needs && !forb && !acc.disabled) valid++;
    }
    return { all: accounts.length, pro, ultra, free, valid, reauth, forbidden };
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
        const t = normalizeTierBadge(acc.quota?.subscriptionTier);
        const needs = isAccountNeedsReauth(acc);
        const forb = Boolean(acc.quota?.isForbidden);
        if (tierFilter === "PRO" && t !== "PRO") return false;
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

  const handleImportCockpit = async () => {
    setBusyKey("import-cockpit");
    try {
      const imported = await agxInvoke<AntigravityAccount[]>(
        "import_accounts_from_cockpit",
      );
      flashMsg(
        "ok",
        vi
          ? `Đã nhập ${imported.length} tài khoản từ Cockpit Tools`
          : `Imported ${imported.length} accounts from Cockpit Tools`,
      );
      setAddModalOpen(false);
      await onRefreshAllData();
    } catch (e) {
      flashMsg("err", e instanceof Error ? e.message : String(e));
    } finally {
      setBusyKey(null);
    }
  };

  const handleStartOAuth = async () => {
    setBusyKey("oauth-prep");
    try {
      const prep = await onPrepareOAuth();
      setOauthPrep(prep);
      await openExternalUrl(prep.authUrl);
      flashMsg(
        "ok",
        vi
          ? "Đã mở trình duyệt đăng nhập Google. Hoàn tất rồi dán URL callback nếu trình duyệt không tự quay về."
          : "Opened Google sign-in. Complete login or paste callback URL below.",
      );
    } catch (e) {
      flashMsg("err", e instanceof Error ? e.message : String(e));
    } finally {
      setBusyKey(null);
    }
  };

  const handleSubmitCallback = async () => {
    if (!oauthCallbackUrl.trim()) return;
    setBusyKey("oauth-submit");
    try {
      await onSubmitOAuthCallback(oauthCallbackUrl.trim());
      setOauthCallbackUrl("");
      setOauthPrep(null);
      setAddModalOpen(false);
    } catch (e) {
      flashMsg("err", e instanceof Error ? e.message : String(e));
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
    const tier = normalizeTierBadge(acc.quota?.subscriptionTier);
    const needsReauth = isAccountNeedsReauth(acc);
    const isForbidden = Boolean(acc.quota?.isForbidden);
    const isSelected = selectedIds.has(acc.id);
    const isBusyCard =
      busyKey === `switch-${acc.id}` ||
      busyKey === `quota-${acc.id}` ||
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
              className="ag-card-icon-btn"
              title={vi ? "Chuyển tài khoản" : "Switch Account"}
              disabled={isBusyCard}
              onClick={async () => {
                setBusyKey(`switch-${acc.id}`);
                try {
                  await onSwitchAccount(acc.id, switchTarget);
                } finally {
                  setBusyKey(null);
                }
              }}
            >
              {busyKey === `switch-${acc.id}` ? (
                <Loader2 size={14} className="spin" />
              ) : (
                <Play size={14} />
              )}
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
              title={vi ? "Ghi chú & 2FA Vault" : "Notes & 2FA Vault"}
              onClick={() => handleOpenVault(acc)}
            >
              <FileText size={14} />
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

        {/* Exact Cockpit Quota Buckets Grid */}
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

        {/* Exact 7-icon Cockpit Footer Action Row */}
        <div className="ag-acc-card__footer">
          <button
            type="button"
            className="ag-card-icon-btn"
            title={vi ? "Chi tiết tài khoản & toàn bộ mô hình" : "Account & Model Quota Details"}
            onClick={() => setInfoAccount(acc)}
          >
            <Info size={14} />
          </button>
          <button
            type="button"
            className="ag-card-icon-btn"
            title={vi ? "Chỉnh sửa thẻ (Tags)" : "Edit Tags"}
            onClick={() => {
              setTagAccount(acc);
              setTagDraftInput(acc.tags.join(", "));
            }}
          >
            <Tag size={14} />
          </button>
          <button
            type="button"
            className="ag-card-icon-btn"
            title={vi ? "Ghi chú, Mật khẩu & 2FA Vault" : "Notes, Password & 2FA Vault"}
            onClick={() => handleOpenVault(acc)}
          >
            <FileText size={14} />
          </button>
          <button
            type="button"
            className={`ag-card-icon-btn${acc.isCurrent ? " is-active-play" : ""}`}
            title={
              vi
                ? `Chuyển sang tài khoản này (${switchTarget.toUpperCase()})`
                : `Switch to this account (${switchTarget.toUpperCase()})`
            }
            disabled={isBusyCard}
            onClick={async () => {
              setBusyKey(`switch-${acc.id}`);
              try {
                await onSwitchAccount(acc.id, switchTarget);
              } finally {
                setBusyKey(null);
              }
            }}
          >
            {busyKey === `switch-${acc.id}` ? (
              <Loader2 size={14} className="spin" />
            ) : (
              <Play size={14} />
            )}
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
            title={vi ? "Xuất JSON tài khoản này" : "Export this account JSON"}
            onClick={() => void handleOpenExport([acc.id], true)}
          >
            <Upload size={14} />
          </button>
          <button
            type="button"
            className="ag-card-icon-btn is-danger"
            title={vi ? "Xóa tài khoản" : "Delete Account"}
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
      </article>
    );
  };

  return (
    <div className="agx-page ag-cockpit-page">
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

      {/* Cockpit Top Sub-Tab Header */}
      <header className="ag-cockpit-header">
        <div className="ag-cockpit-header__left">
          <h1>{tr(lang, "acc.title")}</h1>
        </div>

        <nav className="ag-cockpit-subtabs" role="tablist">
          <button
            type="button"
            role="tab"
            aria-selected={subTab === "overview"}
            className={`ag-cockpit-subtab${subTab === "overview" ? " is-active" : ""}`}
            onClick={() => setSubTab("overview")}
          >
            <Users size={15} />
            <span>{tr(lang, "acc.tabOverview")}</span>
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={subTab === "usage"}
            className={`ag-cockpit-subtab${subTab === "usage" ? " is-active" : ""}`}
            onClick={() => setSubTab("usage")}
          >
            <BarChart3 size={15} />
            <span>{tr(lang, "acc.tabUsage")}</span>
          </button>
        </nav>

        <div className="ag-cockpit-header__right">
          <span className="ag-runtime-badge">
            <span className="ag-runtime-dot" />
            {runtimeLabel || "Antigravity IDE"}
          </span>
        </div>
      </header>

      {localMsg ? (
        <div className={`ag-cockpit-banner is-${localMsg.tone}`}>
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

          {/* 2. Cockpit Main Toolbar */}
          <section className="ag-cockpit-toolbar">
            <div className="ag-cockpit-toolbar__left">
              <div className="ag-cockpit-search">
                <Search size={15} />
                <input
                  type="text"
                  value={searchQuery}
                  onChange={(e) => setSearchQuery(e.target.value)}
                  placeholder={vi ? "Tìm kiếm tài khoản..." : "Search accounts..."}
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

            <div className="ag-cockpit-toolbar__right">
              <select
                className="agx-select ag-toolbar-select ag-target-select"
                value={switchTarget}
                onChange={(e) =>
                  setSwitchTarget(e.target.value as "all" | "ide" | "cli")
                }
                title={vi ? "Phạm vi khi chuyển tài khoản" : "Switch target scope"}
              >
                <option value="all">IDE + CLI</option>
                <option value="ide">{vi ? "Chỉ IDE" : "IDE Only"}</option>
                <option value="cli">{vi ? "Chỉ CLI" : "CLI Only"}</option>
              </select>

              <button
                type="button"
                className="ag-toolbar-icon-btn is-primary"
                title={vi ? "Thêm / Nhập tài khoản" : "Add / Import Account"}
                onClick={() => {
                  setAddTab("oauth");
                  setAddModalOpen(true);
                }}
              >
                <Plus size={16} />
              </button>

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
                      ? "Đang ẩn Email (bấm để hiện)"
                      : "Privacy mode ON"
                    : vi
                      ? "Chế độ riêng tư (ẩn Email)"
                      : "Privacy mode"
                }
                onClick={() => setPrivacyMode((p) => !p)}
              >
                {privacyMode ? <EyeOff size={15} /> : <Eye size={15} />}
              </button>

              <button
                type="button"
                className="ag-toolbar-icon-btn"
                title={vi ? "Xuất JSON tài khoản" : "Export Accounts JSON"}
                onClick={() =>
                  void handleOpenExport(
                    Array.from(selectedIds),
                    exportIncludeSensitive,
                  )
                }
              >
                <Upload size={15} />
              </button>

              <button
                type="button"
                className="ag-toolbar-icon-btn"
                title={
                  vi
                    ? "Đồng bộ tài khoản đang đăng nhập trên Antigravity IDE / CLI"
                    : "Import currently logged-in local Antigravity client"
                }
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
                {busyKey === "import-local" ? (
                  <Loader2 size={15} className="spin" />
                ) : (
                  <UserCheck size={15} />
                )}
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

          {/* 3. Selection Bar & Account Group Tabs (exact Cockpit strip) */}
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
                    ? "Chưa có tài khoản Antigravity nào"
                    : "No Antigravity accounts yet"
                  : vi
                    ? "Không có tài khoản khớp bộ lọc"
                    : "No accounts match current filters"}
              </h3>
              <p>
                {vi
                  ? "Đăng nhập Google OAuth, đồng bộ tài khoản đang mở trên máy, nhập từ Cockpit Tools hoặc dán Refresh Token / JSON."
                  : "Sign in via Google OAuth, import local Antigravity client, import from Cockpit Tools, or paste Refresh Token / JSON."}
              </p>
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
                  <span>{vi ? "Đồng bộ TK trên máy" : "Import Local Client"}</span>
                </button>
                <button
                  type="button"
                  className="agx-btn agx-btn--secondary"
                  disabled={busyKey === "import-cockpit"}
                  onClick={() => void handleImportCockpit()}
                >
                  <Database size={15} />
                  <span>{vi ? "Nhập từ Cockpit Tools" : "Import from Cockpit"}</span>
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

      {/* Add / Import Account Modal */}
      {addModalOpen ? (
        <div
          className="agx-modal-backdrop"
          onClick={() => setAddModalOpen(false)}
        >
          <div
            className="agx-modal agx-modal--wide"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="agx-modal-head">
              <h3>{vi ? "Thêm / Nhập tài khoản Antigravity" : "Add / Import Antigravity Account"}</h3>
              <button
                type="button"
                className="agx-icon-btn"
                onClick={() => setAddModalOpen(false)}
              >
                <X size={16} />
              </button>
            </div>

            <div className="ag-add-tabs">
              {(
                [
                  ["oauth", "Google OAuth"],
                  ["local", vi ? "TK trên máy" : "Local Client"],
                  ["cockpit", "Cockpit Tools"],
                  ["token", "Refresh Token"],
                  ["batch", vi ? "Nhập hàng loạt" : "Batch Lines"],
                  ["json", "JSON"],
                ] as const
              ).map(([key, label]) => (
                <button
                  key={key}
                  type="button"
                  className={addTab === key ? "is-active" : ""}
                  onClick={() => setAddTab(key)}
                >
                  {label}
                </button>
              ))}
            </div>

            <div className="agx-modal-body">
              {addTab === "oauth" ? (
                <div className="ag-modal-stack">
                  <p className="agx-muted">
                    {vi
                      ? "Đăng nhập bằng tài khoản Google Antigravity qua OAuth 2.0 loopback chính thức (giống hệt Cockpit Tools)."
                      : "Sign in with Google Antigravity via official OAuth 2.0 loopback listener."}
                  </p>
                  <button
                    type="button"
                    className="agx-btn agx-btn--primary"
                    disabled={busyKey === "oauth-prep"}
                    onClick={() => void handleStartOAuth()}
                  >
                    {busyKey === "oauth-prep" ? (
                      <Loader2 size={15} className="spin" />
                    ) : (
                      <ExternalLink size={15} />
                    )}
                    <span>
                      {vi ? "Mở trình duyệt đăng nhập Google" : "Open Google Sign-In"}
                    </span>
                  </button>

                  {oauthPrep ? (
                    <div className="ag-oauth-box">
                      <div className="ag-oauth-url-row">
                        <code>{oauthPrep.authUrl}</code>
                        <button
                          type="button"
                          className="agx-btn agx-btn--secondary agx-btn--sm"
                          onClick={() => void copyText(oauthPrep.authUrl, "oauth-url")}
                        >
                          {copiedId === "oauth-url" ? <Check size={13} /> : <Copy size={13} />}
                        </button>
                      </div>
                      <label className="agx-field">
                        <span>
                          {vi
                            ? "Dán URL callback (nếu trình duyệt không tự redirect về localhost):"
                            : "Paste callback URL (if browser didn't auto-redirect):"}
                        </span>
                        <input
                          type="text"
                          className="agx-input"
                          value={oauthCallbackUrl}
                          onChange={(e) => setOauthCallbackUrl(e.target.value)}
                          placeholder="http://localhost:.../oauth-callback?code=...&state=..."
                        />
                      </label>
                      <button
                        type="button"
                        className="agx-btn agx-btn--primary"
                        disabled={
                          !oauthCallbackUrl.trim() || busyKey === "oauth-submit"
                        }
                        onClick={() => void handleSubmitCallback()}
                      >
                        {vi ? "Xác nhận Callback URL" : "Submit Callback URL"}
                      </button>
                    </div>
                  ) : null}
                </div>
              ) : null}

              {addTab === "local" ? (
                <div className="ag-modal-stack">
                  <p className="agx-muted">
                    {vi
                      ? "Tự động đọc token đang đăng nhập từ Windows Credential Manager (gemini:antigravity), ~/.gemini/jetski-standalone-oauth-token, ~/.gemini/antigravity-cli và state.vscdb."
                      : "Reads currently signed-in token from Windows Credential Manager, ~/.gemini fallback, CLI session, and Antigravity state.vscdb."}
                  </p>
                  <button
                    type="button"
                    className="agx-btn agx-btn--primary"
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
                    <UserCheck size={15} />
                    <span>
                      {vi ? "Đọc & đồng bộ tài khoản đang dùng" : "Import Local Account"}
                    </span>
                  </button>
                </div>
              ) : null}

              {addTab === "cockpit" ? (
                <div className="ag-modal-stack">
                  <p className="agx-muted">
                    {vi
                      ? "Tự động quét thư mục dữ liệu Cockpit Tools trên máy để nhập danh sách tài khoản Antigravity. Nếu Cockpit Tools đang bật mã hóa lưu trữ, hãy bấm 'Xuất JSON' trong Cockpit Tools rồi nhập ở tab JSON."
                      : "Scans local Cockpit Tools account folders. If Cockpit Tools encrypted account storage, export JSON from Cockpit Tools and import via the JSON tab."}
                  </p>
                  <div className="ag-modal-btn-row">
                    <button
                      type="button"
                      className="agx-btn agx-btn--primary"
                      disabled={busyKey === "import-cockpit"}
                      onClick={() => void handleImportCockpit()}
                    >
                      <Database size={15} />
                      <span>
                        {vi ? "Quét & nhập từ Cockpit Tools" : "Scan & Import from Cockpit"}
                      </span>
                    </button>
                    <button
                      type="button"
                      className="agx-btn agx-btn--secondary"
                      onClick={() => fileInputRef.current?.click()}
                    >
                      <FileJson size={15} />
                      <span>{vi ? "Chọn tệp JSON từ Cockpit" : "Pick Cockpit JSON File"}</span>
                    </button>
                  </div>
                </div>
              ) : null}

              {addTab === "token" ? (
                <div className="ag-modal-stack">
                  <label className="agx-field">
                    <span>Refresh Token (1//...)</span>
                    <input
                      type="text"
                      className="agx-input"
                      value={tokenInput}
                      onChange={(e) => setTokenInput(e.target.value)}
                      placeholder="1//0g..."
                    />
                  </label>
                  <label className="agx-field">
                    <span>{vi ? "Gợi ý Email (tùy chọn)" : "Email hint (optional)"}</span>
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
                    {vi ? "Thêm tài khoản" : "Add Account"}
                  </button>
                </div>
              ) : null}

              {addTab === "batch" ? (
                <div className="ag-modal-stack">
                  <p className="agx-muted">
                    {vi
                      ? "Mỗi dòng 1 tài khoản: refresh_token hoặc email----password----2fa----refresh_token"
                      : "One account per line: refresh_token or email----password----2fa----refresh_token"}
                  </p>
                  <textarea
                    className="agx-textarea"
                    rows={8}
                    value={batchInput}
                    onChange={(e) => setBatchInput(e.target.value)}
                    placeholder="user@gmail.com----pass123----JBSWY3DPEHPK3PXP----1//0g..."
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
                <div className="ag-modal-stack">
                  <div className="ag-modal-btn-row">
                    <button
                      type="button"
                      className="agx-btn agx-btn--secondary agx-btn--sm"
                      onClick={() => fileInputRef.current?.click()}
                    >
                      <FileJson size={14} />
                      <span>{vi ? "Mở tệp .JSON" : "Load .JSON file"}</span>
                    </button>
                  </div>
                  <textarea
                    className="agx-textarea"
                    rows={8}
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
                    {vi ? "Nhập từ JSON" : "Import JSON"}
                  </button>
                </div>
              ) : null}
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
                  Tier: {infoAccount.quota?.subscriptionTier || "FREE"} · Project:{" "}
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
    </div>
  );
}
