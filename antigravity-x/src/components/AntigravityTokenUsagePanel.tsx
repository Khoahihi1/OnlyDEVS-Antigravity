import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  ArrowDown,
  ArrowDownToLine,
  ArrowUp,
  ArrowUpFromLine,
  Brain,
  Calendar,
  CheckCircle2,
  ChevronLeft,
  ChevronRight,
  Database,
  FolderOpen,
  GitBranch,
  Loader2,
  MessageSquare,
  RefreshCw,
  RotateCcw,
  Search,
  Send,
  X,
  Zap,
} from "lucide-react";
import type {
  AgConversationKind,
  AgConversationSortField,
  AgSortDirection,
  AgTimeGranularity,
  AgUsageRangePreset,
  AppLanguage,
  ConversationDetail,
  ConversationDetailResponse,
  DashboardBundleResponse,
  DashboardQueryRequest,
} from "../types";
import { agxInvoke } from "../platform/invoke";
import { tr } from "../i18n";
import {
  AntigravityUsageTrend,
  formatExactNumber,
  formatTokensShort,
} from "./AntigravityUsageTrend";

type AntigravityTokenUsagePanelProps = {
  lang: AppLanguage;
  range: AgUsageRangePreset;
  onRangeChange: (next: AgUsageRangePreset) => void;
  onBackToOverview: () => void;
  onBundleUpdated?: (bundle: DashboardBundleResponse) => void;
};

function sharePct(part: number, total: number): string {
  if (!total || total <= 0 || !part || part <= 0) return "0%";
  const pct = (part / total) * 100;
  return pct >= 10 ? `${pct.toFixed(1)}%` : `${pct.toFixed(2)}%`;
}

function formatDateTimeShort(iso: string | null | undefined): string {
  if (!iso) return "—";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  const hh = String(d.getHours()).padStart(2, "0");
  const mm = String(d.getMinutes()).padStart(2, "0");
  const dd = String(d.getDate()).padStart(2, "0");
  const mo = String(d.getMonth() + 1).padStart(2, "0");
  return `${hh}:${mm} ${dd}-${mo}`;
}

function KindBadge({ kind, vi }: { kind: AgConversationKind; vi: boolean }) {
  const label =
    kind === "swarm"
      ? "Swarm"
      : kind === "subagent"
        ? "Subagent"
        : vi
          ? "Chính (Main)"
          : "Main";
  return <span className={`ag-kind-badge is-${kind}`}>{label}</span>;
}

export function AntigravityTokenUsagePanel({
  lang,
  range,
  onRangeChange,
  onBackToOverview,
  onBundleUpdated,
}: AntigravityTokenUsagePanelProps) {
  const vi = lang === "vi";

  const [granularity, setGranularity] = useState<AgTimeGranularity>("day");
  const [kindFilter, setKindFilter] = useState<string>("");
  const [modelFilter, setModelFilter] = useState<string>("");
  const [workspaceFilter, setWorkspaceFilter] = useState<string>("");
  const [searchInput, setSearchInput] = useState<string>("");
  const [debouncedSearch, setDebouncedSearch] = useState<string>("");
  const [sortField, setSortField] =
    useState<AgConversationSortField>("lastModifiedTime");
  const [sortOrder, setSortOrder] = useState<AgSortDirection>("desc");
  const [page, setPage] = useState<number>(1);
  const pageSize = 20;

  const [bundle, setBundle] = useState<DashboardBundleResponse | null>(null);
  const [loading, setLoading] = useState<boolean>(true);
  const [syncing, setSyncing] = useState<boolean>(false);
  const [rebuilding, setRebuilding] = useState<boolean>(false);
  const [error, setError] = useState<string>("");

  const [dirModalOpen, setDirModalOpen] = useState<boolean>(false);
  const [dirDraft, setDirDraft] = useState<string>("");

  const [selectedConvId, setSelectedConvId] = useState<string | null>(null);
  const [convDetail, setConvDetail] = useState<ConversationDetail | null>(null);
  const [convDetailLoading, setConvDetailLoading] = useState<boolean>(false);
  const [convDetailError, setConvDetailError] = useState<string>("");

  const queryVersionRef = useRef(0);
  const detailReqRef = useRef(0);

  useEffect(() => {
    const t = window.setTimeout(() => {
      setDebouncedSearch(searchInput.trim());
      setPage(1);
    }, 240);
    return () => window.clearTimeout(t);
  }, [searchInput]);

  const queryPayload = useMemo<DashboardQueryRequest>(
    () => ({
      preset: range,
      granularity,
      kind: kindFilter,
      model: modelFilter,
      workspace: workspaceFilter,
      search: debouncedSearch,
      tzOffsetMinutes: new Date().getTimezoneOffset(),
      sort: sortField,
      order: sortOrder,
      page,
      pageSize,
    }),
    [
      debouncedSearch,
      granularity,
      kindFilter,
      modelFilter,
      page,
      range,
      sortField,
      sortOrder,
      workspaceFilter,
    ],
  );

  const loadQuery = useCallback(
    async (q: DashboardQueryRequest) => {
      const ver = ++queryVersionRef.current;
      setLoading((prev) => prev || bundle === null);
      setError("");
      try {
        const res = await agxInvoke<DashboardBundleResponse>(
          "antigravity_query_token_usage",
          { query: q },
        );
        if (ver === queryVersionRef.current) {
          setBundle(res);
          onBundleUpdated?.(res);
        }
      } catch (e) {
        if (ver === queryVersionRef.current) {
          setError(e instanceof Error ? e.message : String(e));
        }
      } finally {
        if (ver === queryVersionRef.current) {
          setLoading(false);
        }
      }
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [],
  );

  const runSync = useCallback(
    async (rebuild: boolean) => {
      setSyncing(true);
      if (rebuild) setRebuilding(true);
      setError("");
      try {
        const res = await agxInvoke<DashboardBundleResponse>(
          "antigravity_sync_token_usage",
          { rebuild, query: queryPayload },
        );
        setBundle(res);
        setLoading(false);
        onBundleUpdated?.(res);
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        setSyncing(false);
        setRebuilding(false);
      }
    },
    [onBundleUpdated, queryPayload],
  );

  useEffect(() => {
    void loadQuery(queryPayload);
  }, [loadQuery, queryPayload]);

  const handleChooseDataDir = async () => {
    try {
      const picked = await agxInvoke<string | null>("pick_folder_dialog", {
        defaultPath: bundle?.settings.effectiveDataDir ?? null,
      });
      if (!picked) return;
      setSyncing(true);
      setError("");
      const res = await agxInvoke<DashboardBundleResponse>(
        "antigravity_set_token_usage_data_dir",
        { customDir: picked, query: { ...queryPayload, page: 1 } },
      );
      setPage(1);
      setBundle(res);
      onBundleUpdated?.(res);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSyncing(false);
    }
  };

  const handleApplyManualDir = async () => {
    const trimmed = dirDraft.trim();
    setSyncing(true);
    setError("");
    try {
      const res = await agxInvoke<DashboardBundleResponse>(
        "antigravity_set_token_usage_data_dir",
        {
          customDir: trimmed ? trimmed : null,
          query: { ...queryPayload, page: 1 },
        },
      );
      setPage(1);
      setBundle(res);
      setDirModalOpen(false);
      onBundleUpdated?.(res);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSyncing(false);
    }
  };

  const handleResetDataDir = async () => {
    setSyncing(true);
    setError("");
    try {
      const res = await agxInvoke<DashboardBundleResponse>(
        "antigravity_set_token_usage_data_dir",
        { customDir: null, query: { ...queryPayload, page: 1 } },
      );
      setPage(1);
      setBundle(res);
      onBundleUpdated?.(res);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSyncing(false);
    }
  };

  const openConversationDetail = async (conversationId: string) => {
    const reqId = ++detailReqRef.current;
    setSelectedConvId(conversationId);
    setConvDetail(null);
    setConvDetailError("");
    setConvDetailLoading(true);
    try {
      const res = await agxInvoke<ConversationDetailResponse>(
        "antigravity_get_conversation_token_detail",
        { conversationId },
      );
      if (reqId === detailReqRef.current) {
        setConvDetail(res.conversation);
      }
    } catch (e) {
      if (reqId === detailReqRef.current) {
        setConvDetailError(e instanceof Error ? e.message : String(e));
      }
    } finally {
      if (reqId === detailReqRef.current) {
        setConvDetailLoading(false);
      }
    }
  };

  const handleSortColumn = (field: AgConversationSortField) => {
    setPage(1);
    if (sortField === field) {
      setSortOrder((prev) => (prev === "desc" ? "asc" : "desc"));
    } else {
      setSortField(field);
      setSortOrder("desc");
    }
  };

  const renderSortIcon = (field: AgConversationSortField) => {
    if (sortField !== field) return null;
    return sortOrder === "desc" ? <ArrowDown size={12} /> : <ArrowUp size={12} />;
  };

  const summary = bundle?.summary;
  const kpi = summary?.kpi;
  const exact = kpi?.exact;
  const estimated = kpi?.estimated;
  const settings = bundle?.settings;
  const conversations = bundle?.conversations;
  const cacheTotal =
    (exact?.cacheReadTokens ?? 0) + (exact?.cacheWriteTokens ?? 0);
  const hasAnyData =
    (kpi?.totalConversations ?? 0) > 0 ||
    (exact?.totalTokens ?? 0) > 0 ||
    (estimated?.totalTokens ?? 0) > 0;

  return (
    <div className="ag-token-usage">
      {/* Header card */}
      <div className="ag-token-usage__surface">
        <div className="ag-token-usage__head">
          <div>
            <h3>{tr(lang, "usage.title")}</h3>
          </div>
          <button
            type="button"
            className="agx-btn agx-btn--secondary agx-btn--sm"
            onClick={onBackToOverview}
          >
            {tr(lang, "usage.back")}
          </button>
        </div>

        {/* Data Dir Bar */}
        {settings ? (
          <div className="ag-token-usage__dir-bar">
            <div className="ag-token-usage__dir-path">
              <FolderOpen size={14} />
              <span>{tr(lang, "usage.dataDir")}</span>
              <code title={settings.effectiveDataDir}>
                {settings.effectiveDataDir}
              </code>
              {settings.isCustomDataDir ? (
                <span className="ag-token-usage__custom-pill">
                  {vi ? "Tùy chỉnh" : "Custom"}
                </span>
              ) : null}
            </div>
            <div className="ag-token-usage__dir-actions">
              <button
                type="button"
                className="agx-btn agx-btn--secondary agx-btn--sm"
                onClick={() => void handleChooseDataDir()}
                disabled={syncing}
              >
                {tr(lang, "usage.changeDir")}
              </button>
              <button
                type="button"
                className="agx-btn agx-btn--secondary agx-btn--sm"
                onClick={() => {
                  setDirDraft(settings.effectiveDataDir);
                  setDirModalOpen(true);
                }}
                disabled={syncing}
              >
                {tr(lang, "usage.editPath")}
              </button>
              {settings.isCustomDataDir ? (
                <button
                  type="button"
                  className="agx-btn agx-btn--secondary agx-btn--sm"
                  onClick={() => void handleResetDataDir()}
                  disabled={syncing}
                >
                  {tr(lang, "usage.resetDefault")}
                </button>
              ) : null}
            </div>
          </div>
        ) : null}

        {/* Filter Toolbar */}
        <div className="ag-token-usage__toolbar">
          <label className="ag-token-usage__field">
            <span>{tr(lang, "usage.timeRange")}</span>
            <select
              className="agx-select"
              value={range}
              onChange={(e) => {
                setPage(1);
                onRangeChange(e.target.value as AgUsageRangePreset);
              }}
            >
              <option value="today">{tr(lang, "acc.range.today")}</option>
              <option value="7d">{tr(lang, "acc.range.7d")}</option>
              <option value="30d">{tr(lang, "acc.range.30d")}</option>
              <option value="month">{tr(lang, "acc.range.month")}</option>
              <option value="all">{tr(lang, "acc.range.all")}</option>
            </select>
          </label>

          <label className="ag-token-usage__field">
            <span>{tr(lang, "usage.granularity")}</span>
            <select
              className="agx-select"
              value={granularity}
              onChange={(e) =>
                setGranularity(e.target.value as AgTimeGranularity)
              }
            >
              <option value="day">{tr(lang, "usage.daily")}</option>
              <option value="week">{tr(lang, "usage.weekly")}</option>
              <option value="month">{tr(lang, "usage.monthly")}</option>
            </select>
          </label>

          <label className="ag-token-usage__field">
            <span>{tr(lang, "usage.sessionType")}</span>
            <select
              className="agx-select"
              value={kindFilter}
              onChange={(e) => {
                setPage(1);
                setKindFilter(e.target.value);
              }}
            >
              <option value="">{tr(lang, "usage.allTypes")}</option>
              <option value="main">Main</option>
              <option value="swarm">Swarm</option>
              <option value="subagent">Subagent</option>
            </select>
          </label>

          <label className="ag-token-usage__field">
            <span>{tr(lang, "usage.model")}</span>
            <select
              className="agx-select"
              value={modelFilter}
              onChange={(e) => {
                setPage(1);
                setModelFilter(e.target.value);
              }}
            >
              <option value="">{tr(lang, "usage.allModels")}</option>
              {(summary?.catalog.models ?? []).map((m) => (
                <option key={m.modelId} value={m.modelId}>
                  {m.modelDisplayName} ({m.count})
                </option>
              ))}
            </select>
          </label>

          <label className="ag-token-usage__field">
            <span>{tr(lang, "usage.workspace")}</span>
            <select
              className="agx-select"
              value={workspaceFilter}
              onChange={(e) => {
                setPage(1);
                setWorkspaceFilter(e.target.value);
              }}
            >
              <option value="">{tr(lang, "usage.allWorkspaces")}</option>
              {(summary?.catalog.workspaces ?? []).map((w) => (
                <option key={w.workspaceKey} value={w.workspaceKey}>
                  {w.workspaceName} ({w.count})
                </option>
              ))}
            </select>
          </label>

          <div className="ag-token-usage__actions">
            <button
              type="button"
              className="agx-btn agx-btn--secondary"
              disabled={syncing}
              onClick={() => void runSync(false)}
            >
              <RefreshCw
                size={14}
                className={syncing && !rebuilding ? "spin" : undefined}
              />
              <span>{tr(lang, "acc.refresh")}</span>
            </button>
            <button
              type="button"
              className="agx-btn agx-btn--secondary"
              disabled={syncing}
              onClick={() => {
                if (
                  window.confirm(
                    vi
                      ? "Xóa bộ đệm quét cục bộ và đọc lại toàn bộ tệp phiên Antigravity (chỉ đọc)?"
                      : "Clear local scan cache and re-read all Antigravity session files in read-only mode?",
                  )
                ) {
                  void runSync(true);
                }
              }}
            >
              <RotateCcw
                size={14}
                className={rebuilding ? "spin" : undefined}
              />
              <span>{tr(lang, "acc.rescan")}</span>
            </button>
          </div>
        </div>

        {/* Scan Metadata Status Line */}
        <div className="ag-token-usage__status">
          <span>
            {syncing
              ? vi
                ? "Đang quét cơ sở dữ liệu phiên Antigravity…"
                : "Scanning Antigravity session databases…"
              : summary
                ? vi
                  ? `Đã quét ${summary.scan.totalDbFiles} tệp DB · ${kpi?.totalConversations ?? 0} phiên · ${formatExactNumber(kpi?.totalTurns ?? 0)} lượt phản hồi`
                  : `Scanned ${summary.scan.totalDbFiles} DB files · ${kpi?.totalConversations ?? 0} sessions · ${formatExactNumber(kpi?.totalTurns ?? 0)} turns`
                : vi
                  ? "Chưa quét dữ liệu"
                  : "Not scanned yet"}
          </span>
          {summary ? (
            <>
              <span>
                {vi
                  ? `Bộ đệm ${summary.scan.cacheHits} khớp / ${summary.scan.cacheMisses} mới (${summary.scan.scanDurationMs} ms)`
                  : `Cache ${summary.scan.cacheHits} hit / ${summary.scan.cacheMisses} new (${summary.scan.scanDurationMs} ms)`}
              </span>
              <span>
                {vi ? "Đồng bộ lúc" : "Synced"}{" "}
                {formatDateTimeShort(summary.scan.scannedAt)}
              </span>
            </>
          ) : null}
        </div>

        {error ? <div className="ag-token-usage__error">{error}</div> : null}

        {loading && !bundle ? (
          <div className="ag-token-usage__empty">
            <Loader2 size={24} className="spin" />
            <span>{vi ? "Đang tải thống kê Token…" : "Loading token usage…"}</span>
          </div>
        ) : !hasAnyData ? (
          <div className="ag-token-usage__empty">
            <h4>
              {vi
                ? "Không có dữ liệu Token khớp bộ lọc hiện tại"
                : "No token usage found for current filters"}
            </h4>
            <p>
              {summary?.scan.dataDirExists === false
                ? vi
                  ? "Không tìm thấy thư mục dữ liệu Antigravity. Kiểm tra lại đường dẫn thư mục phía trên."
                  : "Antigravity data directory was not found. Check the folder path above."
                : vi
                  ? 'Thử chuyển Khoảng thời gian sang "Tất cả" hoặc nhấn Làm mới.'
                  : 'Try switching Time Range to "All time" or click Refresh.'}
            </p>
          </div>
        ) : (
          <>
            {/* Hero KPI + 4 Breakdown Cards */}
            <section className="ag-token-usage__hero">
              <div className="ag-token-usage__hero-head">
                <div className="ag-token-usage__hero-total">
                  <div className="ag-token-usage__hero-icon">
                    <Zap size={24} />
                  </div>
                  <div className="ag-token-usage__hero-total-meta">
                    <span>{tr(lang, "usage.totalExact")}</span>
                    <strong title={formatExactNumber(exact?.totalTokens ?? 0)}>
                      {formatTokensShort(exact?.totalTokens ?? 0, 2)}
                    </strong>
                    <span className="ag-token-usage__hero-sub">
                      {formatExactNumber(exact?.totalTokens ?? 0)} tokens
                    </span>
                  </div>
                </div>

                <div className="ag-token-usage__hero-side">
                  <span>
                    <MessageSquare size={13} />
                    {tr(lang, "usage.conversations")}
                  </span>
                  <strong>{formatExactNumber(kpi?.totalConversations ?? 0)}</strong>

                  <span>
                    <Send size={13} />
                    {tr(lang, "usage.turns")}
                  </span>
                  <strong>{formatExactNumber(kpi?.totalTurns ?? 0)}</strong>

                  <span>
                    <CheckCircle2 size={13} />
                    {tr(lang, "usage.coverage")}
                  </span>
                  <strong>{`${(kpi?.exactCoveragePercent ?? 0).toFixed(1)}%`}</strong>

                  <span>
                    <Calendar size={13} />
                    {tr(lang, "usage.avgDay")}
                  </span>
                  <strong>
                    {formatTokensShort(kpi?.avgExactTokensPerActiveDay ?? 0, 2)}
                  </strong>
                </div>
              </div>

              <div className="ag-token-usage__cards">
                <article className="ag-token-card is-input">
                  <div className="ag-token-card__head">
                    <ArrowDownToLine size={15} />
                    <span>{tr(lang, "usage.cardInput")}</span>
                  </div>
                  <strong title={formatExactNumber(exact?.inputTokens ?? 0)}>
                    {formatTokensShort(exact?.inputTokens ?? 0, 2)}
                  </strong>
                  <span className="ag-token-card__foot">
                    {sharePct(exact?.inputTokens ?? 0, exact?.totalTokens ?? 0)} ·{" "}
                    {formatExactNumber(exact?.inputTokens ?? 0)}
                  </span>
                </article>

                <article className="ag-token-card is-thinking">
                  <div className="ag-token-card__head">
                    <Brain size={15} />
                    <span>{tr(lang, "usage.cardThinking")}</span>
                  </div>
                  <strong title={formatExactNumber(exact?.thinkingTokens ?? 0)}>
                    {formatTokensShort(exact?.thinkingTokens ?? 0, 1)}
                  </strong>
                  <span className="ag-token-card__foot">
                    {sharePct(exact?.thinkingTokens ?? 0, exact?.totalTokens ?? 0)}{" "}
                    · {formatExactNumber(exact?.thinkingTokens ?? 0)}
                  </span>
                </article>

                <article className="ag-token-card is-output">
                  <div className="ag-token-card__head">
                    <ArrowUpFromLine size={15} />
                    <span>{tr(lang, "usage.cardOutput")}</span>
                  </div>
                  <strong title={formatExactNumber(exact?.outputTokens ?? 0)}>
                    {formatTokensShort(exact?.outputTokens ?? 0, 1)}
                  </strong>
                  <span className="ag-token-card__foot">
                    {sharePct(exact?.outputTokens ?? 0, exact?.totalTokens ?? 0)} ·{" "}
                    {formatExactNumber(exact?.outputTokens ?? 0)}
                  </span>
                </article>

                <article className="ag-token-card is-cache">
                  <div className="ag-token-card__head">
                    <Database size={15} />
                    <span>{tr(lang, "usage.cardCache")}</span>
                  </div>
                  <strong title={formatExactNumber(cacheTotal)}>
                    {formatTokensShort(cacheTotal, 2)}
                  </strong>
                  <span className="ag-token-card__foot">
                    R: {formatTokensShort(exact?.cacheReadTokens ?? 0, 2)} · W:{" "}
                    {formatTokensShort(exact?.cacheWriteTokens ?? 0, 1)}
                  </span>
                </article>
              </div>

              {(estimated?.totalTokens ?? 0) > 0 ? (
                <div className="ag-token-usage__estimated-banner">
                  <span>
                    {vi
                      ? `${estimated?.estimatedTurnCount ?? 0} lượt thiếu dữ liệu API gốc nên được ước tính riêng (không cộng vào Tổng chính xác):`
                      : `${estimated?.estimatedTurnCount ?? 0} turns lacked raw API usage and were estimated separately (never added to Exact Total):`}
                  </span>
                  <strong>
                    ~{formatTokensShort(estimated?.totalTokens ?? 0, 2)} tokens
                  </strong>
                </div>
              ) : null}
            </section>

            {/* Trend Chart */}
            <AntigravityUsageTrend series={summary?.timeSeries ?? []} lang={lang} />

            {/* By Model & By Workspace */}
            <div className="ag-token-usage__grids">
              <section className="ag-token-section">
                <div className="ag-token-section__head">
                  <h4>{vi ? "Theo mô hình" : "By Model"}</h4>
                  {modelFilter ? (
                    <button
                      type="button"
                      className="agx-btn agx-btn--ghost agx-btn--sm"
                      onClick={() => setModelFilter("")}
                    >
                      {vi ? "Bỏ lọc" : "Clear"}
                    </button>
                  ) : null}
                </div>
                <div className="ag-token-table-wrap">
                  <table className="ag-token-table">
                    <thead>
                      <tr>
                        <th>{vi ? "Mô hình" : "Model"}</th>
                        <th className="is-num">{vi ? "Lượt" : "Turns"}</th>
                        <th className="is-num">Input</th>
                        <th className="is-num">Thinking</th>
                        <th className="is-num">Output</th>
                        <th className="is-num">Cache</th>
                        <th className="is-num">{vi ? "Tổng" : "Total"}</th>
                        <th className="is-num">%</th>
                      </tr>
                    </thead>
                    <tbody>
                      {(summary?.byModel ?? []).map((row) => (
                        <tr
                          key={row.modelId}
                          className={`is-clickable${modelFilter === row.modelId ? " is-selected" : ""}`}
                          onClick={() => {
                            setPage(1);
                            setModelFilter(
                              modelFilter === row.modelId ? "" : row.modelId,
                            );
                          }}
                        >
                          <td>
                            <div className="ag-token-cell-primary">
                              <span className="ag-token-cell-title">
                                {row.modelDisplayName}
                              </span>
                              <span className="ag-token-cell-sub">
                                {row.apiProvider} · {row.conversationCount}{" "}
                                {vi ? "phiên" : "sessions"}
                              </span>
                            </div>
                          </td>
                          <td className="is-num">
                            {formatExactNumber(row.turnCount)}
                          </td>
                          <td className="is-num">
                            {formatTokensShort(row.exact.inputTokens, 1)}
                          </td>
                          <td className="is-num">
                            {formatTokensShort(row.exact.thinkingTokens, 1)}
                          </td>
                          <td className="is-num">
                            {formatTokensShort(row.exact.outputTokens, 1)}
                          </td>
                          <td className="is-num">
                            {formatTokensShort(
                              row.exact.cacheReadTokens +
                                row.exact.cacheWriteTokens,
                              1,
                            )}
                          </td>
                          <td className="is-num">
                            <strong>
                              {formatTokensShort(row.exact.totalTokens, 2)}
                            </strong>
                          </td>
                          <td className="is-num">
                            <div className="ag-share-bar">
                              <div className="ag-share-bar__track">
                                <div
                                  className="ag-share-bar__fill"
                                  style={{
                                    width: `${Math.min(100, Math.max(0, row.sharePercent))}%`,
                                  }}
                                />
                              </div>
                              <span>{row.sharePercent.toFixed(1)}%</span>
                            </div>
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              </section>

              <section className="ag-token-section">
                <div className="ag-token-section__head">
                  <h4>{vi ? "Theo thư mục làm việc" : "By Workspace"}</h4>
                  {workspaceFilter ? (
                    <button
                      type="button"
                      className="agx-btn agx-btn--ghost agx-btn--sm"
                      onClick={() => setWorkspaceFilter("")}
                    >
                      {vi ? "Bỏ lọc" : "Clear"}
                    </button>
                  ) : null}
                </div>
                <div className="ag-token-table-wrap">
                  <table className="ag-token-table">
                    <thead>
                      <tr>
                        <th>{vi ? "Thư mục" : "Workspace"}</th>
                        <th className="is-num">{vi ? "Phiên" : "Sessions"}</th>
                        <th className="is-num">{vi ? "Lượt" : "Turns"}</th>
                        <th className="is-num">Input</th>
                        <th className="is-num">Thinking</th>
                        <th className="is-num">Output</th>
                        <th className="is-num">{vi ? "Tổng" : "Total"}</th>
                        <th className="is-num">%</th>
                      </tr>
                    </thead>
                    <tbody>
                      {(summary?.byWorkspace ?? []).map((row) => (
                        <tr
                          key={row.workspaceKey}
                          className={`is-clickable${workspaceFilter === row.workspaceKey ? " is-selected" : ""}`}
                          onClick={() => {
                            setPage(1);
                            setWorkspaceFilter(
                              workspaceFilter === row.workspaceKey
                                ? ""
                                : row.workspaceKey,
                            );
                          }}
                        >
                          <td>
                            <div className="ag-token-cell-primary">
                              <span
                                className="ag-token-cell-title"
                                title={row.workspacePath || row.workspaceName}
                              >
                                {row.workspaceName}
                              </span>
                              {row.workspacePath ? (
                                <span
                                  className="ag-token-cell-sub"
                                  title={row.workspacePath}
                                >
                                  {row.workspacePath}
                                </span>
                              ) : null}
                            </div>
                          </td>
                          <td className="is-num">
                            {formatExactNumber(row.conversationCount)}
                          </td>
                          <td className="is-num">
                            {formatExactNumber(row.turnCount)}
                          </td>
                          <td className="is-num">
                            {formatTokensShort(row.exact.inputTokens, 1)}
                          </td>
                          <td className="is-num">
                            {formatTokensShort(row.exact.thinkingTokens, 1)}
                          </td>
                          <td className="is-num">
                            {formatTokensShort(row.exact.outputTokens, 1)}
                          </td>
                          <td className="is-num">
                            <strong>
                              {formatTokensShort(row.exact.totalTokens, 2)}
                            </strong>
                          </td>
                          <td className="is-num">
                            <div className="ag-share-bar">
                              <div className="ag-share-bar__track">
                                <div
                                  className="ag-share-bar__fill"
                                  style={{
                                    width: `${Math.min(100, Math.max(0, row.sharePercent))}%`,
                                  }}
                                />
                              </div>
                              <span>{row.sharePercent.toFixed(1)}%</span>
                            </div>
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              </section>
            </div>

            {/* By Session Kind */}
            {(summary?.byKind ?? []).length > 0 ? (
              <section className="ag-token-section">
                <div className="ag-token-section__head">
                  <h4>
                    {vi
                      ? "Theo loại phiên (Main / Swarm / Subagent)"
                      : "By Session Type (Main / Swarm / Subagent)"}
                  </h4>
                </div>
                <div className="ag-token-table-wrap">
                  <table className="ag-token-table">
                    <thead>
                      <tr>
                        <th>{vi ? "Loại phiên" : "Session Type"}</th>
                        <th className="is-num">{vi ? "Phiên" : "Sessions"}</th>
                        <th className="is-num">{vi ? "Lượt" : "Turns"}</th>
                        <th className="is-num">Input</th>
                        <th className="is-num">Thinking</th>
                        <th className="is-num">Output</th>
                        <th className="is-num">Cache</th>
                        <th className="is-num">{vi ? "Tổng" : "Total"}</th>
                        <th className="is-num">%</th>
                      </tr>
                    </thead>
                    <tbody>
                      {(summary?.byKind ?? []).map((row) => (
                        <tr
                          key={row.kind}
                          className={`is-clickable${kindFilter === row.kind ? " is-selected" : ""}`}
                          onClick={() => {
                            setPage(1);
                            setKindFilter(kindFilter === row.kind ? "" : row.kind);
                          }}
                        >
                          <td>
                            <KindBadge kind={row.kind} vi={vi} />
                          </td>
                          <td className="is-num">
                            {formatExactNumber(row.conversationCount)}
                          </td>
                          <td className="is-num">
                            {formatExactNumber(row.turnCount)}
                          </td>
                          <td className="is-num">
                            {formatTokensShort(row.exact.inputTokens, 1)}
                          </td>
                          <td className="is-num">
                            {formatTokensShort(row.exact.thinkingTokens, 1)}
                          </td>
                          <td className="is-num">
                            {formatTokensShort(row.exact.outputTokens, 1)}
                          </td>
                          <td className="is-num">
                            {formatTokensShort(
                              row.exact.cacheReadTokens +
                                row.exact.cacheWriteTokens,
                              1,
                            )}
                          </td>
                          <td className="is-num">
                            <strong>
                              {formatTokensShort(row.exact.totalTokens, 2)}
                            </strong>
                          </td>
                          <td className="is-num">
                            {row.sharePercent.toFixed(1)}%
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              </section>
            ) : null}

            {/* Conversations Table */}
            <section className="ag-token-section">
              <div className="ag-token-section__head">
                <h4>
                  {vi ? "Danh sách phiên hội thoại" : "Conversation Sessions"} (
                  {formatExactNumber(conversations?.totalItems ?? 0)})
                </h4>
                <div className="ag-conv-search">
                  <Search size={14} />
                  <input
                    type="text"
                    value={searchInput}
                    onChange={(e) => setSearchInput(e.target.value)}
                    placeholder={
                      vi
                        ? "Tìm tiêu đề, ID phiên, thư mục, mô hình…"
                        : "Search title, session ID, workspace, model…"
                    }
                  />
                  {searchInput ? (
                    <button
                      type="button"
                      className="ag-sort-th-btn"
                      onClick={() => setSearchInput("")}
                    >
                      <X size={13} />
                    </button>
                  ) : null}
                </div>
              </div>

              <div className="ag-token-table-wrap">
                <table className="ag-token-table">
                  <thead>
                    <tr>
                      <th>{vi ? "Phiên hội thoại" : "Conversation"}</th>
                      <th>{vi ? "Loại" : "Type"}</th>
                      <th>{vi ? "Mô hình" : "Model"}</th>
                      <th className="is-num">
                        <button
                          type="button"
                          className="ag-sort-th-btn"
                          onClick={() => handleSortColumn("turnCount")}
                        >
                          {vi ? "Lượt" : "Turns"}
                          {renderSortIcon("turnCount")}
                        </button>
                      </th>
                      <th className="is-num">
                        <button
                          type="button"
                          className="ag-sort-th-btn"
                          onClick={() => handleSortColumn("inputTokens")}
                        >
                          Input
                          {renderSortIcon("inputTokens")}
                        </button>
                      </th>
                      <th className="is-num">
                        <button
                          type="button"
                          className="ag-sort-th-btn"
                          onClick={() => handleSortColumn("thinkingTokens")}
                        >
                          Thinking
                          {renderSortIcon("thinkingTokens")}
                        </button>
                      </th>
                      <th className="is-num">
                        <button
                          type="button"
                          className="ag-sort-th-btn"
                          onClick={() => handleSortColumn("outputTokens")}
                        >
                          Output
                          {renderSortIcon("outputTokens")}
                        </button>
                      </th>
                      <th className="is-num">
                        <button
                          type="button"
                          className="ag-sort-th-btn"
                          onClick={() => handleSortColumn("cacheReadTokens")}
                        >
                          Cache
                          {renderSortIcon("cacheReadTokens")}
                        </button>
                      </th>
                      <th className="is-num">
                        <button
                          type="button"
                          className="ag-sort-th-btn"
                          onClick={() => handleSortColumn("totalTokens")}
                        >
                          {vi ? "Tổng" : "Total"}
                          {renderSortIcon("totalTokens")}
                        </button>
                      </th>
                      <th className="is-num">
                        <button
                          type="button"
                          className="ag-sort-th-btn"
                          onClick={() => handleSortColumn("lastModifiedTime")}
                        >
                          {vi ? "Cập nhật" : "Updated"}
                          {renderSortIcon("lastModifiedTime")}
                        </button>
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    {(conversations?.items ?? []).length === 0 ? (
                      <tr>
                        <td colSpan={10}>
                          {vi ? "Không có phiên phù hợp" : "No matching sessions"}
                        </td>
                      </tr>
                    ) : (
                      (conversations?.items ?? []).map((conv) => (
                        <tr
                          key={conv.conversationId}
                          className="is-clickable"
                          onClick={() =>
                            void openConversationDetail(conv.conversationId)
                          }
                        >
                          <td>
                            <div className="ag-token-cell-primary">
                              <span
                                className="ag-token-cell-title"
                                title={conv.displayTitle}
                              >
                                {conv.displayTitle}
                              </span>
                              <span
                                className="ag-token-cell-sub"
                                title={conv.workspacePath || conv.conversationId}
                              >
                                {conv.workspaceName} ·{" "}
                                {conv.conversationId.slice(0, 8)}
                              </span>
                            </div>
                          </td>
                          <td>
                            <KindBadge kind={conv.kind} vi={vi} />
                          </td>
                          <td title={conv.modelDisplayNames.join(", ")}>
                            {conv.primaryModelDisplayName || "—"}
                          </td>
                          <td className="is-num">
                            {formatExactNumber(conv.turnCount)}
                          </td>
                          <td className="is-num">
                            {formatTokensShort(conv.exact.inputTokens, 1)}
                          </td>
                          <td className="is-num">
                            {formatTokensShort(conv.exact.thinkingTokens, 1)}
                          </td>
                          <td className="is-num">
                            {formatTokensShort(conv.exact.outputTokens, 1)}
                          </td>
                          <td className="is-num">
                            {formatTokensShort(
                              conv.exact.cacheReadTokens +
                                conv.exact.cacheWriteTokens,
                              1,
                            )}
                          </td>
                          <td className="is-num">
                            <strong>
                              {formatTokensShort(conv.exact.totalTokens, 2)}
                            </strong>
                          </td>
                          <td className="is-num">
                            {formatDateTimeShort(conv.lastModifiedTime)}
                          </td>
                        </tr>
                      ))
                    )}
                  </tbody>
                </table>
              </div>

              {(conversations?.totalPages ?? 1) > 1 ? (
                <div className="ag-conv-pagination">
                  <span>
                    {vi
                      ? `Trang ${conversations?.page ?? 1} / ${conversations?.totalPages ?? 1} (${conversations?.totalItems ?? 0} phiên)`
                      : `Page ${conversations?.page ?? 1} / ${conversations?.totalPages ?? 1} (${conversations?.totalItems ?? 0} sessions)`}
                  </span>
                  <div className="ag-conv-pagination__btns">
                    <button
                      type="button"
                      className="agx-btn agx-btn--secondary agx-btn--sm"
                      disabled={(conversations?.page ?? 1) <= 1}
                      onClick={() => setPage((p) => Math.max(1, p - 1))}
                    >
                      <ChevronLeft size={14} />
                    </button>
                    <button
                      type="button"
                      className="agx-btn agx-btn--secondary agx-btn--sm"
                      disabled={
                        (conversations?.page ?? 1) >=
                        (conversations?.totalPages ?? 1)
                      }
                      onClick={() => setPage((p) => p + 1)}
                    >
                      <ChevronRight size={14} />
                    </button>
                  </div>
                </div>
              ) : null}
            </section>
          </>
        )}
      </div>

      {/* Manual Folder Path Modal */}
      {dirModalOpen ? (
        <div
          className="agx-modal-backdrop"
          onClick={() => setDirModalOpen(false)}
        >
          <div className="agx-modal" onClick={(e) => e.stopPropagation()}>
            <div className="agx-modal-head">
              <h3>
                {vi
                  ? "Đổi thư mục dữ liệu Antigravity"
                  : "Change Antigravity Data Directory"}
              </h3>
              <button
                type="button"
                className="agx-icon-btn"
                onClick={() => setDirModalOpen(false)}
              >
                <X size={16} />
              </button>
            </div>
            <div className="agx-modal-body">
              <label className="agx-field">
                <span>
                  {vi
                    ? "Đường dẫn thư mục (để trống để dùng mặc định ~/.gemini/antigravity)"
                    : "Directory path (leave empty for default ~/.gemini/antigravity)"}
                </span>
                <input
                  type="text"
                  className="agx-input"
                  value={dirDraft}
                  onChange={(e) => setDirDraft(e.target.value)}
                  placeholder="C:\Users\...\ .gemini\antigravity"
                />
              </label>
            </div>
            <div className="agx-modal-foot">
              <button
                type="button"
                className="agx-btn agx-btn--secondary"
                onClick={() => setDirModalOpen(false)}
              >
                {vi ? "Hủy" : "Cancel"}
              </button>
              <button
                type="button"
                className="agx-btn agx-btn--primary"
                disabled={syncing}
                onClick={() => void handleApplyManualDir()}
              >
                {vi ? "Áp dụng & Quét" : "Apply & Scan"}
              </button>
            </div>
          </div>
        </div>
      ) : null}

      {/* Conversation Token Detail Modal */}
      {selectedConvId ? (
        <div
          className="agx-modal-backdrop"
          onClick={() => setSelectedConvId(null)}
        >
          <div
            className="agx-modal agx-modal--wide"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="agx-modal-head">
              <div>
                <h3>
                  {convDetail?.displayTitle ||
                    (vi ? "Chi tiết Token phiên" : "Conversation Token Detail")}
                </h3>
                <span className="agx-muted">
                  ID: {selectedConvId}
                  {convDetail?.workspacePath
                    ? ` · ${convDetail.workspacePath}`
                    : ""}
                </span>
              </div>
              <button
                type="button"
                className="agx-icon-btn"
                onClick={() => setSelectedConvId(null)}
              >
                <X size={16} />
              </button>
            </div>

            <div className="agx-modal-body">
              {convDetailLoading ? (
                <div className="ag-token-usage__empty">
                  <Loader2 size={20} className="spin" />
                  <span>{vi ? "Đang phân tích các lượt…" : "Loading turns…"}</span>
                </div>
              ) : convDetailError ? (
                <div className="ag-token-usage__error">{convDetailError}</div>
              ) : convDetail ? (
                <div className="ag-conv-detail">
                  <div className="ag-conv-detail__kpis">
                    <div>
                      <span>{vi ? "Tổng Token" : "Total"}</span>
                      <strong>
                        {formatTokensShort(convDetail.exact.totalTokens, 2)}
                      </strong>
                    </div>
                    <div>
                      <span>Input</span>
                      <strong>
                        {formatTokensShort(convDetail.exact.inputTokens, 2)}
                      </strong>
                    </div>
                    <div>
                      <span>Thinking</span>
                      <strong>
                        {formatTokensShort(convDetail.exact.thinkingTokens, 2)}
                      </strong>
                    </div>
                    <div>
                      <span>Output</span>
                      <strong>
                        {formatTokensShort(convDetail.exact.outputTokens, 2)}
                      </strong>
                    </div>
                    <div>
                      <span>Cache</span>
                      <strong>
                        {formatTokensShort(
                          convDetail.exact.cacheReadTokens +
                            convDetail.exact.cacheWriteTokens,
                          2,
                        )}
                      </strong>
                    </div>
                    <div>
                      <span>{vi ? "Số lượt" : "Turns"}</span>
                      <strong>{formatExactNumber(convDetail.turnCount)}</strong>
                    </div>
                  </div>

                  {convDetail.children.length > 0 ? (
                    <div className="ag-conv-detail__children">
                      <h5>
                        <GitBranch size={14} />{" "}
                        {vi ? "Phiên con / Subagent" : "Child / Subagent Sessions"}
                      </h5>
                      <div className="ag-conv-detail__child-list">
                        {convDetail.children.map((ch) => (
                          <button
                            key={ch.conversationId}
                            type="button"
                            className="ag-conv-child-pill"
                            onClick={() =>
                              void openConversationDetail(ch.conversationId)
                            }
                          >
                            <KindBadge kind={ch.kind} vi={vi} />
                            <span>{ch.title || ch.agentName || ch.conversationId.slice(0, 8)}</span>
                            <strong>{formatTokensShort(ch.exactTotalTokens, 2)}</strong>
                          </button>
                        ))}
                      </div>
                    </div>
                  ) : null}

                  <div className="ag-token-table-wrap ag-conv-detail__turns">
                    <table className="ag-token-table">
                      <thead>
                        <tr>
                          <th>#</th>
                          <th>{vi ? "Thời gian" : "Time"}</th>
                          <th>{vi ? "Mô hình" : "Model"}</th>
                          <th className="is-num">Input</th>
                          <th className="is-num">Thinking</th>
                          <th className="is-num">Output</th>
                          <th className="is-num">Cache</th>
                          <th className="is-num">{vi ? "Tổng" : "Total"}</th>
                        </tr>
                      </thead>
                      <tbody>
                        {convDetail.turns.map((turn, i) => (
                          <tr key={`${turn.stepIndex}-${i}`}>
                            <td>{turn.stepIndex}</td>
                            <td>{formatDateTimeShort(turn.timestamp)}</td>
                            <td>{turn.modelDisplayName}</td>
                            <td className="is-num">
                              {formatExactNumber(turn.exact.inputTokens)}
                            </td>
                            <td className="is-num">
                              {formatExactNumber(turn.exact.thinkingTokens)}
                            </td>
                            <td className="is-num">
                              {formatExactNumber(turn.exact.outputTokens)}
                            </td>
                            <td className="is-num">
                              {formatExactNumber(
                                turn.exact.cacheReadTokens +
                                  turn.exact.cacheWriteTokens,
                              )}
                            </td>
                            <td className="is-num">
                              <strong>
                                {formatExactNumber(turn.exact.totalTokens)}
                              </strong>
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                </div>
              ) : null}
            </div>
          </div>
        </div>
      ) : null}
    </div>
  );
}
