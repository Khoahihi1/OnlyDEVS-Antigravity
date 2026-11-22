import { useMemo, useState } from "react";
import {
  AlertCircle,
  CheckCircle2,
  Download,
  Eye,
  FolderTree,
  History,
  Loader2,
  Search,
  Trash2,
  X,
  Zap,
} from "lucide-react";
import type { AppLanguage, SessionItem, SessionScanResult, SessionStepItem } from "../types";

type SessionsPageProps = {
  lang: AppLanguage;
  scan: SessionScanResult | null;
  loading: boolean;
  onRefresh: () => Promise<void>;
  onSyncSessions: () => Promise<void>;
  onDeleteSessions: (ids: string[]) => Promise<void>;
  onExportSessions: (ids: string[]) => Promise<void>;
  onGetTranscript: (sessionId: string) => Promise<SessionStepItem[]>;
};

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
}

function formatTime(ms: number): string {
  if (!ms) return "—";
  const d = new Date(ms);
  if (Number.isNaN(d.getTime())) return "—";
  return d.toLocaleString(undefined, {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function shortId(id: string): string {
  return id.length > 14 ? `${id.slice(0, 8)}…${id.slice(-4)}` : id;
}

export function SessionsPage({
  lang,
  scan,
  onSyncSessions,
  onDeleteSessions,
  onExportSessions,
  onGetTranscript,
}: SessionsPageProps) {
  const vi = lang === "vi";
  const [query, setQuery] = useState("");
  const [folderView, setFolderView] = useState(false);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [busyAction, setBusyAction] = useState<string | null>(null);
  const [inspectSession, setInspectSession] = useState<{
    session: SessionItem;
    steps: SessionStepItem[];
  } | null>(null);

  const sessions = scan?.sessions ?? [];

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return sessions.filter((s) => {
      if (!q) return true;
      return (
        s.title.toLowerCase().includes(q) ||
        s.workspace.toLowerCase().includes(q) ||
        s.id.toLowerCase().includes(q) ||
        (s.model ?? "").toLowerCase().includes(q) ||
        s.firstUserMessage.toLowerCase().includes(q)
      );
    });
  }, [sessions, query]);

  const groupedByWorkspace = useMemo(() => {
    const map = new Map<string, SessionItem[]>();
    for (const s of filtered) {
      const key = s.workspace || "Unknown Workspace";
      const list = map.get(key) ?? [];
      list.push(s);
      map.set(key, list);
    }
    return Array.from(map.entries());
  }, [filtered]);

  const toggleSelect = (id: string) => {
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const toggleSelectAll = () => {
    if (selectedIds.size === filtered.length && filtered.length > 0) {
      setSelectedIds(new Set());
    } else {
      setSelectedIds(new Set(filtered.map((s) => s.id)));
    }
  };

  const handleInspect = async (s: SessionItem) => {
    setBusyAction(`inspect-${s.id}`);
    try {
      const steps = await onGetTranscript(s.id);
      setInspectSession({ session: s, steps });
    } finally {
      setBusyAction(null);
    }
  };

  const unsyncedCount = scan?.unsyncedCount ?? 0;

  return (
    <section className="agx-page">
      {/* Header matching Codex-X Screenshot 3 */}
      <header className="agx-page-header">
        <div className="agx-page-heading">
          <h2>{vi ? "Phiên làm việc" : "Sessions"}</h2>
        </div>

        <div className="agx-header-actions">
          <button
            type="button"
            className="agx-btn agx-btn--primary agx-btn--sm"
            disabled={busyAction === "sync"}
            onClick={async () => {
              setBusyAction("sync");
              try {
                await onSyncSessions();
              } finally {
                setBusyAction(null);
              }
            }}
          >
            {busyAction === "sync" ? <Loader2 className="spin" size={14} /> : <Zap size={14} />}
            <span>{vi ? "Đồng bộ" : "Sync"}</span>
          </button>
        </div>
      </header>

      {/* Sync Status Banner */}
      <div
        className={`agx-session-banner ${
          unsyncedCount === 0 ? "agx-session-banner--synced" : "agx-session-banner--pending"
        }`}
      >
        <div className="agx-session-banner-left">
          {unsyncedCount === 0 ? <CheckCircle2 size={15} /> : <AlertCircle size={15} />}
          <strong>
            {unsyncedCount === 0
              ? vi
                ? "Đã đồng bộ"
                : "Synced"
              : vi
                ? `${unsyncedCount} phiên chưa đồng bộ`
                : `${unsyncedCount} unsynced`}
          </strong>
        </div>
        <div className="agx-session-banner-right">
          <span>
            {sessions.length} sessions · {formatBytes(scan?.totalSizeBytes ?? 0)}
          </span>
        </div>
      </div>

      {/* Main Local Sessions Panel */}
      <div className="agx-panel-card agx-session-panel">
        <div className="agx-panel-head">
          <h3>{vi ? "Danh sách phiên" : "Sessions"}</h3>
          <span className="agx-count-pill">
            {filtered.length} / {sessions.length}
          </span>
        </div>

        <div className="agx-session-toolbar">
          <div className="agx-search-box">
            <Search size={15} />
            <input
              type="text"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder={
                vi
                  ? "Search title / project / model / ID..."
                  : "Search title / project / provider / ID"
              }
            />
          </div>

          <label className="agx-folder-toggle-btn">
            <input
              type="checkbox"
              checked={folderView}
              onChange={(e) => setFolderView(e.target.checked)}
            />
            <FolderTree size={15} />
            <span>{vi ? "Folder view" : "Folder view"}</span>
          </label>

          <button
            type="button"
            className="agx-btn agx-btn--secondary"
            disabled={selectedIds.size === 0 || busyAction === "export"}
            onClick={async () => {
              setBusyAction("export");
              try {
                await onExportSessions(Array.from(selectedIds));
              } finally {
                setBusyAction(null);
              }
            }}
          >
            <Download size={14} />
            <span>
              {vi ? "Export selected" : "Export selected"}
              {selectedIds.size > 0 ? ` (${selectedIds.size})` : ""}
            </span>
          </button>

          <button
            type="button"
            className="agx-btn agx-btn--danger"
            disabled={selectedIds.size === 0 || busyAction === "delete"}
            onClick={async () => {
              setBusyAction("delete");
              try {
                await onDeleteSessions(Array.from(selectedIds));
                setSelectedIds(new Set());
              } finally {
                setBusyAction(null);
              }
            }}
          >
            <Trash2 size={14} />
            <span>
              {vi ? "Delete selected" : "Delete selected"}
              {selectedIds.size > 0 ? ` (${selectedIds.size})` : ""}
            </span>
          </button>
        </div>

        {filtered.length > 0 && (
          <div className="agx-session-table-head">
            <label className="agx-checkbox-inline">
              <input
                type="checkbox"
                checked={selectedIds.size === filtered.length && filtered.length > 0}
                onChange={toggleSelectAll}
              />
              <span>{vi ? "Chọn tất cả" : "Select all"}</span>
            </label>
            <span>{vi ? "Runtime / Model / Steps / Thời gian" : "Runtime / Model / Steps / Updated"}</span>
          </div>
        )}

        <div className="agx-session-list">
          {!folderView &&
            filtered.map((s) => (
              <SessionRow
                key={s.id}
                session={s}
                selected={selectedIds.has(s.id)}
                busy={busyAction === `inspect-${s.id}`}
                onToggleSelect={() => toggleSelect(s.id)}
                onInspect={() => void handleInspect(s)}
                onExport={() => void onExportSessions([s.id])}
                onDelete={() => void onDeleteSessions([s.id])}
              />
            ))}

          {folderView &&
            groupedByWorkspace.map(([ws, group]) => (
              <div key={ws} className="agx-session-folder-group">
                <div className="agx-session-folder-header">
                  <FolderTree size={15} />
                  <strong>{ws}</strong>
                  <span className="agx-count-pill">{group.length}</span>
                </div>
                {group.map((s) => (
                  <SessionRow
                    key={s.id}
                    session={s}
                    selected={selectedIds.has(s.id)}
                    busy={busyAction === `inspect-${s.id}`}
                    onToggleSelect={() => toggleSelect(s.id)}
                    onInspect={() => void handleInspect(s)}
                    onExport={() => void onExportSessions([s.id])}
                    onDelete={() => void onDeleteSessions([s.id])}
                  />
                ))}
              </div>
            ))}

          {filtered.length === 0 && (
            <div className="agx-session-empty">
              <History size={32} />
              <p>
                {vi
                  ? "Không tìm thấy session nào. Nhấn Check sessions để quét lại."
                  : "No sessions loaded. Click Check sessions to refresh."}
              </p>
            </div>
          )}
        </div>
      </div>

      {/* Transcript Inspector Modal */}
      {inspectSession && (
        <div className="agx-modal-backdrop" onClick={() => setInspectSession(null)}>
          <div className="agx-modal agx-modal--lg" onClick={(e) => e.stopPropagation()}>
            <div className="agx-modal-header">
              <div>
                <span className="agx-eyebrow">
                  SESSION TRANSCRIPT · {inspectSession.session.source}
                </span>
                <h3>{inspectSession.session.title}</h3>
              </div>
              <button
                type="button"
                className="agx-icon-btn"
                onClick={() => setInspectSession(null)}
              >
                <X size={18} />
              </button>
            </div>
            <div className="agx-modal-body agx-transcript-list">
              {inspectSession.steps.map((st, idx) => (
                <div
                  key={`${st.stepIndex}-${idx}`}
                  className={`agx-transcript-step ${
                    st.stepType === "USER_INPUT"
                      ? "agx-transcript-step--user"
                      : "agx-transcript-step--model"
                  }`}
                >
                  <div className="agx-transcript-step-head">
                    <strong>
                      #{st.stepIndex} · {st.stepType} ({st.source})
                    </strong>
                    <div className="agx-transcript-step-badges">
                      {st.model && <span className="agx-badge agx-badge--info">{st.model}</span>}
                      {st.toolCount > 0 && (
                        <span className="agx-badge agx-badge--brand">
                          {st.toolCount} tool call(s)
                        </span>
                      )}
                    </div>
                  </div>
                  {st.content && <pre className="agx-transcript-content">{st.content}</pre>}
                </div>
              ))}
              {inspectSession.steps.length === 0 && (
                <p className="agx-muted-note">Transcript trống hoặc không đọc được.</p>
              )}
            </div>
            <div className="agx-modal-footer">
              <code className="agx-truncate">{inspectSession.session.transcriptPath}</code>
              <div className="agx-modal-footer-btns">
                <button
                  type="button"
                  className="agx-btn agx-btn--secondary"
                  onClick={() => void onExportSessions([inspectSession.session.id])}
                >
                  <Download size={14} /> Export .md
                </button>
                <button
                  type="button"
                  className="agx-btn agx-btn--primary"
                  onClick={() => setInspectSession(null)}
                >
                  Close
                </button>
              </div>
            </div>
          </div>
        </div>
      )}
    </section>
  );
}

function SessionRow({
  session,
  selected,
  busy,
  onToggleSelect,
  onInspect,
  onExport,
  onDelete,
}: {
  session: SessionItem;
  selected: boolean;
  busy: boolean;
  onToggleSelect: () => void;
  onInspect: () => void;
  onExport: () => void;
  onDelete: () => void;
}) {
  return (
    <div className={`agx-session-row${selected ? " agx-session-row--selected" : ""}`}>
      <div className="agx-session-row-left">
        <input type="checkbox" checked={selected} onChange={onToggleSelect} />
        <div className="agx-session-row-copy">
          <div className="agx-session-row-title-line">
            <strong title={session.firstUserMessage || session.title}>{session.title}</strong>
            {session.model && <span className="agx-badge agx-badge--muted">{session.model}</span>}
          </div>
          <div className="agx-session-row-sub">
            <code>{shortId(session.id)}</code>
            <span>·</span>
            <span className="agx-truncate" title={session.workspace}>
              {session.workspace}
            </span>
          </div>
        </div>
      </div>

      <div className="agx-session-row-right">
        <div className="agx-session-metrics">
          <span>
            <strong>{session.userTurnCount}</strong> turns · <strong>{session.stepCount}</strong> steps
          </span>
          <small>
            {formatBytes(session.sizeBytes)} · {formatTime(session.updatedAtMs)}
          </small>
        </div>

        <div className="agx-session-actions">
          <button
            type="button"
            className="agx-icon-btn agx-icon-btn--bordered"
            title="Inspect conversation transcript"
            onClick={onInspect}
          >
            {busy ? <Loader2 className="spin" size={14} /> : <Eye size={14} />}
          </button>
          <button
            type="button"
            className="agx-icon-btn agx-icon-btn--bordered"
            title="Export to Markdown"
            onClick={onExport}
          >
            <Download size={14} />
          </button>
          <button
            type="button"
            className="agx-icon-btn agx-icon-btn--bordered agx-icon-btn--danger-hover"
            title="Delete session"
            onClick={onDelete}
          >
            <Trash2 size={14} />
          </button>
        </div>
      </div>
    </div>
  );
}
