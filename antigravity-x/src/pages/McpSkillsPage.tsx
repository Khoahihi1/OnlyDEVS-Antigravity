import { useMemo, useState } from "react";
import {
  Cpu,
  FileCode2,
  FileJson,
  Loader2,
  PencilLine,
  PlugZap,
  Plus,
  RefreshCw,
  Save,
  Search,
  Sparkles,
  Trash2,
  Wrench,
  X,
} from "lucide-react";
import type {
  AppLanguage,
  ManagedMcpServer,
  ManagedSkill,
  McpSkillsState,
} from "../types";

type McpSkillsPageProps = {
  lang: AppLanguage;
  state: McpSkillsState | null;
  loading: boolean;
  onRefresh: () => Promise<void>;
  onToggleMcp: (id: string, enabled: boolean) => Promise<void>;
  onUpsertMcp: (payload: {
    id: string;
    transport: string;
    url?: string | null;
    command?: string | null;
    args: string[];
    env: Record<string, string>;
    enabled: boolean;
  }) => Promise<void>;
  onDeleteMcp: (id: string) => Promise<void>;
  onGetMcpConfigRaw: () => Promise<string>;
  onSaveMcpConfigRaw: (raw: string) => Promise<void>;
  onToggleSkill: (id: string, enabled: boolean) => Promise<void>;
  onGetSkillContent: (id: string, source: string) => Promise<string>;
  onUpsertSkill: (payload: {
    id: string;
    name: string;
    description: string;
    body: string;
  }) => Promise<void>;
  onDeleteSkill: (id: string) => Promise<void>;
  onSaveNote: (kind: "mcp" | "skill", id: string, note: string) => Promise<void>;
};

export function McpSkillsPage({
  lang,
  state,
  loading,
  onRefresh,
  onToggleMcp,
  onUpsertMcp,
  onDeleteMcp,
  onGetMcpConfigRaw,
  onSaveMcpConfigRaw,
  onToggleSkill,
  onGetSkillContent,
  onUpsertSkill,
  onDeleteSkill,
  onSaveNote,
}: McpSkillsPageProps) {
  const vi = lang === "vi";
  const [subTab, setSubTab] = useState<"mcp" | "skills">("mcp");
  const [searchQuery, setSearchQuery] = useState("");
  const [mcpRawOpen, setMcpRawOpen] = useState(false);
  const [mcpRawDraft, setMcpRawDraft] = useState("");

  // Add / Edit MCP Modal
  const [mcpFormOpen, setMcpFormOpen] = useState(false);
  const [mcpDraft, setMcpDraft] = useState({
    id: "",
    transport: "http",
    url: "http://127.0.0.1:13337/mcp",
    command: "",
    argsText: "",
    enabled: true,
  });

  // Tools Preview Modal
  const [toolsModal, setToolsModal] = useState<ManagedMcpServer | null>(null);

  // Add / Edit Skill Modal
  const [skillFormOpen, setSkillFormOpen] = useState(false);
  const [skillDraft, setSkillDraft] = useState({
    id: "",
    name: "",
    description: "",
    body: "",
    readOnly: false,
  });

  // Note Modal
  const [noteModal, setNoteModal] = useState<{
    kind: "mcp" | "skill";
    id: string;
    note: string;
  } | null>(null);

  const mcpServers = state?.mcpServers ?? [];
  const skills = state?.skills ?? [];

  const filteredMcp = useMemo(() => {
    const q = searchQuery.trim().toLowerCase();
    if (!q) return mcpServers;
    return mcpServers.filter(
      (m) =>
        m.id.toLowerCase().includes(q) ||
        m.summary.toLowerCase().includes(q) ||
        (m.note ?? "").toLowerCase().includes(q),
    );
  }, [mcpServers, searchQuery]);

  const filteredSkills = useMemo(() => {
    const q = searchQuery.trim().toLowerCase();
    if (!q) return skills;
    return skills.filter(
      (s) =>
        s.id.toLowerCase().includes(q) ||
        s.name.toLowerCase().includes(q) ||
        s.description.toLowerCase().includes(q) ||
        (s.note ?? "").toLowerCase().includes(q),
    );
  }, [skills, searchQuery]);

  const openEditMcp = (m: ManagedMcpServer) => {
    setMcpDraft({
      id: m.id,
      transport: m.transport,
      url: m.url ?? "",
      command: m.command ?? "",
      argsText: m.args.join("\n"),
      enabled: m.enabled,
    });
    setMcpFormOpen(true);
  };

  const openEditSkill = async (s: ManagedSkill) => {
    const raw = await onGetSkillContent(s.id, s.source);
    setSkillDraft({
      id: s.id,
      name: s.name,
      description: s.description,
      body: raw,
      readOnly: s.source === "builtin",
    });
    setSkillFormOpen(true);
  };

  return (
    <section className="agx-page">
      <header className="agx-page-header">
        <div className="agx-page-heading">
          <p className="agx-eyebrow">
            <Cpu size={13} />
            SKILLS & MCP SERVERS
          </p>
          <h2>{vi ? "MCP & Skills" : "Skills & MCP"}</h2>
          <span>
            {vi
              ? "Quản lý các MCP Server trong ~/.gemini/config/mcp_config.json và kho Agent Skills cục bộ."
              : "Manage MCP servers in ~/.gemini/config/mcp_config.json and local Agent Skills."}
          </span>
        </div>

        <div className="agx-header-actions">
          <button
            type="button"
            className="agx-btn agx-btn--secondary"
            onClick={async () => {
              const raw = await onGetMcpConfigRaw();
              setMcpRawDraft(raw);
              setMcpRawOpen(true);
            }}
          >
            <FileJson size={15} />
            <span>mcp_config.json</span>
          </button>

          <button
            type="button"
            className="agx-btn agx-btn--secondary"
            onClick={() => void onRefresh()}
            disabled={loading}
          >
            {loading ? <Loader2 className="spin" size={15} /> : <RefreshCw size={15} />}
            <span>{vi ? "Làm mới" : "Refresh"}</span>
          </button>

          {subTab === "mcp" ? (
            <button
              type="button"
              className="agx-btn agx-btn--primary"
              onClick={() => {
                setMcpDraft({
                  id: "",
                  transport: "http",
                  url: "http://127.0.0.1:13337/mcp",
                  command: "",
                  argsText: "",
                  enabled: true,
                });
                setMcpFormOpen(true);
              }}
            >
              <Plus size={15} />
              <span>{vi ? "Thêm MCP Server" : "Add MCP Server"}</span>
            </button>
          ) : (
            <button
              type="button"
              className="agx-btn agx-btn--primary"
              onClick={() => {
                setSkillDraft({
                  id: "",
                  name: "",
                  description: "",
                  body: "",
                  readOnly: false,
                });
                setSkillFormOpen(true);
              }}
            >
              <Plus size={15} />
              <span>{vi ? "Tạo Skill mới" : "Create Skill"}</span>
            </button>
          )}
        </div>
      </header>

      {/* Sub-Tabs + Search Toolbar */}
      <div className="agx-prompts-toolbar">
        <div className="agx-category-tabs" role="tablist">
          <button
            type="button"
            role="tab"
            aria-selected={subTab === "mcp"}
            className={`agx-category-tab${subTab === "mcp" ? " agx-category-tab--active" : ""}`}
            onClick={() => setSubTab("mcp")}
          >
            <PlugZap size={14} />
            <span>MCP Servers</span>
            <small>{mcpServers.length}</small>
          </button>

          <button
            type="button"
            role="tab"
            aria-selected={subTab === "skills"}
            className={`agx-category-tab${subTab === "skills" ? " agx-category-tab--active" : ""}`}
            onClick={() => setSubTab("skills")}
          >
            <Sparkles size={14} />
            <span>Skills</span>
            <small>{skills.length}</small>
          </button>
        </div>

        <div className="agx-search-box agx-search-box--compact">
          <Search size={14} />
          <input
            type="text"
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            placeholder={
              subTab === "mcp"
                ? vi
                  ? "Tìm MCP server (ida, rea, windbg...)"
                  : "Search MCP servers..."
                : vi
                  ? "Tìm Skill..."
                  : "Search Skills..."
            }
          />
        </div>
      </div>

      {/* Sub-Tab 1: MCP Servers */}
      {subTab === "mcp" && (
        <div className="agx-panel-card">
          <div className="agx-panel-head">
            <div>
              <span className="agx-section-tag">MCP REGISTRY</span>
              <h3>
                {vi
                  ? `${mcpServers.filter((m) => m.enabled).length}/${mcpServers.length} MCP Server đang bật`
                  : `${mcpServers.filter((m) => m.enabled).length}/${mcpServers.length} MCP Servers enabled`}
              </h3>
            </div>
            <code className="agx-muted-note">{state?.mcpConfigPath}</code>
          </div>

          <div className="agx-plugin-list">
            {filteredMcp.map((m) => (
              <div key={m.id} className="agx-plugin-row">
                <div className="agx-plugin-row-main">
                  <div className="agx-plugin-row-title">
                    <strong>{m.id}</strong>
                    <span
                      className={`agx-badge ${
                        m.transport === "http" ? "agx-badge--info" : "agx-badge--brand"
                      }`}
                    >
                      {m.transport.toUpperCase()}
                    </span>
                    {m.toolCount > 0 && (
                      <button
                        type="button"
                        className="agx-badge agx-badge--success agx-badge--clickable"
                        onClick={() => setToolsModal(m)}
                      >
                        <Wrench size={11} /> {m.toolCount} tools
                      </button>
                    )}
                    {m.note && <span className="agx-note-chip">{m.note}</span>}
                  </div>
                  <div className="agx-plugin-row-sub">
                    <code>{m.summary}</code>
                  </div>
                </div>

                <div className="agx-plugin-row-controls">
                  <button
                    type="button"
                    className="agx-icon-btn agx-icon-btn--bordered"
                    title={vi ? "Ghi chú" : "Edit note"}
                    onClick={() =>
                      setNoteModal({ kind: "mcp", id: m.id, note: m.note ?? "" })
                    }
                  >
                    <PencilLine size={14} />
                  </button>
                  <button
                    type="button"
                    className="agx-icon-btn agx-icon-btn--bordered"
                    title={vi ? "Sửa cấu hình MCP" : "Edit MCP config"}
                    onClick={() => openEditMcp(m)}
                  >
                    <FileCode2 size={14} />
                  </button>
                  <button
                    type="button"
                    className="agx-icon-btn agx-icon-btn--bordered agx-icon-btn--danger-hover"
                    title={vi ? "Xóa MCP server" : "Delete MCP server"}
                    onClick={() => void onDeleteMcp(m.id)}
                  >
                    <Trash2 size={14} />
                  </button>
                  <button
                    type="button"
                    role="switch"
                    aria-checked={m.enabled}
                    className={`agx-switch-pill${m.enabled ? " agx-switch-pill--on" : ""}`}
                    onClick={() => void onToggleMcp(m.id, !m.enabled)}
                  >
                    <span className="agx-switch-thumb" />
                  </button>
                </div>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Sub-Tab 2: Skills */}
      {subTab === "skills" && (
        <div className="agx-panel-card">
          <div className="agx-panel-head">
            <div>
              <span className="agx-section-tag">AGENT SKILLS</span>
              <h3>
                {vi
                  ? `${skills.filter((s) => s.enabled).length}/${skills.length} Skill khả dụng`
                  : `${skills.filter((s) => s.enabled).length}/${skills.length} Skills active`}
              </h3>
            </div>
            <code className="agx-muted-note">{state?.skillsDir}</code>
          </div>

          <div className="agx-plugin-list">
            {filteredSkills.map((s) => (
              <div key={s.id} className="agx-plugin-row">
                <div className="agx-plugin-row-main">
                  <div className="agx-plugin-row-title">
                    <strong>{s.name}</strong>
                    <code>/{s.id}</code>
                    <span
                      className={`agx-badge ${
                        s.source === "user" ? "agx-badge--brand" : "agx-badge--info"
                      }`}
                    >
                      {s.source === "user" ? "User Skill" : "Built-in"}
                    </span>
                    {s.note && <span className="agx-note-chip">{s.note}</span>}
                  </div>
                  <p className="agx-plugin-row-desc">{s.description}</p>
                  <div className="agx-plugin-row-sub">
                    <code>{s.path}</code>
                  </div>
                </div>

                <div className="agx-plugin-row-controls">
                  <button
                    type="button"
                    className="agx-icon-btn agx-icon-btn--bordered"
                    title={vi ? "Ghi chú" : "Edit note"}
                    onClick={() =>
                      setNoteModal({ kind: "skill", id: s.id, note: s.note ?? "" })
                    }
                  >
                    <PencilLine size={14} />
                  </button>
                  <button
                    type="button"
                    className="agx-icon-btn agx-icon-btn--bordered"
                    title={vi ? "Xem / Sửa SKILL.md" : "View / Edit SKILL.md"}
                    onClick={() => void openEditSkill(s)}
                  >
                    <FileCode2 size={14} />
                  </button>
                  {s.source === "user" && (
                    <button
                      type="button"
                      className="agx-icon-btn agx-icon-btn--bordered agx-icon-btn--danger-hover"
                      title={vi ? "Xóa Skill" : "Delete Skill"}
                      onClick={() => void onDeleteSkill(s.id)}
                    >
                      <Trash2 size={14} />
                    </button>
                  )}
                  <button
                    type="button"
                    role="switch"
                    aria-checked={s.enabled}
                    disabled={s.source === "builtin"}
                    className={`agx-switch-pill${s.enabled ? " agx-switch-pill--on" : ""}`}
                    onClick={() => void onToggleSkill(s.id, !s.enabled)}
                  >
                    <span className="agx-switch-thumb" />
                  </button>
                </div>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Raw mcp_config.json Modal */}
      {mcpRawOpen && (
        <div className="agx-modal-backdrop" onClick={() => setMcpRawOpen(false)}>
          <div className="agx-modal agx-modal--lg" onClick={(e) => e.stopPropagation()}>
            <div className="agx-modal-header">
              <div>
                <span className="agx-eyebrow">MCP CONFIGURATION</span>
                <h3>~/.gemini/config/mcp_config.json</h3>
              </div>
              <button
                type="button"
                className="agx-icon-btn"
                onClick={() => setMcpRawOpen(false)}
              >
                <X size={18} />
              </button>
            </div>
            <div className="agx-modal-body">
              <textarea
                className="agx-code-editor"
                rows={18}
                value={mcpRawDraft}
                onChange={(e) => setMcpRawDraft(e.target.value)}
                spellCheck={false}
              />
            </div>
            <div className="agx-modal-footer">
              <button
                type="button"
                className="agx-btn agx-btn--secondary"
                onClick={() => setMcpRawOpen(false)}
              >
                {vi ? "Đóng" : "Close"}
              </button>
              <button
                type="button"
                className="agx-btn agx-btn--primary"
                onClick={async () => {
                  await onSaveMcpConfigRaw(mcpRawDraft);
                  setMcpRawOpen(false);
                }}
              >
                <Save size={15} /> {vi ? "Lưu mcp_config.json" : "Save mcp_config.json"}
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Add / Edit MCP Server Modal */}
      {mcpFormOpen && (
        <div className="agx-modal-backdrop" onClick={() => setMcpFormOpen(false)}>
          <div className="agx-modal" onClick={(e) => e.stopPropagation()}>
            <div className="agx-modal-header">
              <div>
                <span className="agx-eyebrow">MCP SERVER</span>
                <h3>{vi ? "Cấu hình MCP Server" : "Configure MCP Server"}</h3>
              </div>
              <button
                type="button"
                className="agx-icon-btn"
                onClick={() => setMcpFormOpen(false)}
              >
                <X size={18} />
              </button>
            </div>
            <div className="agx-modal-body">
              <div className="agx-form-row-2">
                <label className="agx-field">
                  <span>Server ID</span>
                  <input
                    type="text"
                    className="agx-input"
                    value={mcpDraft.id}
                    onChange={(e) => setMcpDraft((d) => ({ ...d, id: e.target.value }))}
                    placeholder="ida-13337 / custom-mcp"
                  />
                </label>
                <label className="agx-field">
                  <span>Transport</span>
                  <select
                    className="agx-input"
                    value={mcpDraft.transport}
                    onChange={(e) => setMcpDraft((d) => ({ ...d, transport: e.target.value }))}
                  >
                    <option value="http">HTTP (URL)</option>
                    <option value="stdio">STDIO (Command + Args)</option>
                  </select>
                </label>
              </div>

              {mcpDraft.transport === "http" ? (
                <label className="agx-field">
                  <span>Endpoint URL</span>
                  <input
                    type="text"
                    className="agx-input"
                    value={mcpDraft.url}
                    onChange={(e) => setMcpDraft((d) => ({ ...d, url: e.target.value }))}
                    placeholder="http://127.0.0.1:13337/mcp"
                  />
                </label>
              ) : (
                <>
                  <label className="agx-field">
                    <span>Command Executable</span>
                    <input
                      type="text"
                      className="agx-input"
                      value={mcpDraft.command}
                      onChange={(e) => setMcpDraft((d) => ({ ...d, command: e.target.value }))}
                      placeholder="C:\Program Files\nodejs\node.exe"
                    />
                  </label>
                  <label className="agx-field">
                    <span>Arguments (mỗi dòng 1 arg)</span>
                    <textarea
                      className="agx-code-editor"
                      rows={5}
                      value={mcpDraft.argsText}
                      onChange={(e) => setMcpDraft((d) => ({ ...d, argsText: e.target.value }))}
                      placeholder="-m&#10;mcp_windbg"
                    />
                  </label>
                </>
              )}
            </div>
            <div className="agx-modal-footer">
              <button
                type="button"
                className="agx-btn agx-btn--secondary"
                onClick={() => setMcpFormOpen(false)}
              >
                {vi ? "Hủy" : "Cancel"}
              </button>
              <button
                type="button"
                className="agx-btn agx-btn--primary"
                disabled={!mcpDraft.id.trim()}
                onClick={async () => {
                  await onUpsertMcp({
                    id: mcpDraft.id.trim(),
                    transport: mcpDraft.transport,
                    url: mcpDraft.transport === "http" ? mcpDraft.url : null,
                    command: mcpDraft.transport === "stdio" ? mcpDraft.command : null,
                    args: mcpDraft.argsText
                      .split("\n")
                      .map((s) => s.trim())
                      .filter(Boolean),
                    env: {},
                    enabled: mcpDraft.enabled,
                  });
                  setMcpFormOpen(false);
                }}
              >
                <Save size={15} /> {vi ? "Lưu MCP Server" : "Save MCP Server"}
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Add / Edit Skill Modal */}
      {skillFormOpen && (
        <div className="agx-modal-backdrop" onClick={() => setSkillFormOpen(false)}>
          <div className="agx-modal agx-modal--lg" onClick={(e) => e.stopPropagation()}>
            <div className="agx-modal-header">
              <div>
                <span className="agx-eyebrow">AGENT SKILL</span>
                <h3>
                  {skillDraft.readOnly
                    ? `Built-in Skill: ${skillDraft.name}`
                    : vi
                      ? "Tạo / Chỉnh sửa Skill (SKILL.md)"
                      : "Create / Edit Skill (SKILL.md)"}
                </h3>
              </div>
              <button
                type="button"
                className="agx-icon-btn"
                onClick={() => setSkillFormOpen(false)}
              >
                <X size={18} />
              </button>
            </div>
            <div className="agx-modal-body">
              {!skillDraft.readOnly && (
                <div className="agx-form-row-2">
                  <label className="agx-field">
                    <span>Skill Folder ID</span>
                    <input
                      type="text"
                      className="agx-input"
                      value={skillDraft.id}
                      onChange={(e) =>
                        setSkillDraft((d) => ({
                          ...d,
                          id: e.target.value,
                          name: d.name || e.target.value,
                        }))
                      }
                      placeholder="my-custom-skill"
                    />
                  </label>
                  <label className="agx-field">
                    <span>Description</span>
                    <input
                      type="text"
                      className="agx-input"
                      value={skillDraft.description}
                      onChange={(e) =>
                        setSkillDraft((d) => ({ ...d, description: e.target.value }))
                      }
                      placeholder="When and how the agent should activate this skill..."
                    />
                  </label>
                </div>
              )}

              <label className="agx-field">
                <span>SKILL.md Content</span>
                <textarea
                  className="agx-code-editor"
                  rows={16}
                  readOnly={skillDraft.readOnly}
                  value={skillDraft.body}
                  onChange={(e) => setSkillDraft((d) => ({ ...d, body: e.target.value }))}
                  placeholder="# Skill Instructions..."
                />
              </label>
            </div>
            <div className="agx-modal-footer">
              <button
                type="button"
                className="agx-btn agx-btn--secondary"
                onClick={() => setSkillFormOpen(false)}
              >
                {vi ? "Đóng" : "Close"}
              </button>
              {!skillDraft.readOnly && (
                <button
                  type="button"
                  className="agx-btn agx-btn--primary"
                  disabled={!skillDraft.id.trim()}
                  onClick={async () => {
                    await onUpsertSkill({
                      id: skillDraft.id.trim(),
                      name: skillDraft.name.trim() || skillDraft.id.trim(),
                      description: skillDraft.description.trim(),
                      body: skillDraft.body,
                    });
                    setSkillFormOpen(false);
                  }}
                >
                  <Save size={15} /> {vi ? "Lưu SKILL.md" : "Save SKILL.md"}
                </button>
              )}
            </div>
          </div>
        </div>
      )}

      {/* Tools Preview Modal */}
      {toolsModal && (
        <div className="agx-modal-backdrop" onClick={() => setToolsModal(null)}>
          <div className="agx-modal" onClick={(e) => e.stopPropagation()}>
            <div className="agx-modal-header">
              <div>
                <span className="agx-eyebrow">DISCOVERED MCP TOOLS</span>
                <h3>
                  {toolsModal.id} ({toolsModal.toolCount} tools)
                </h3>
              </div>
              <button
                type="button"
                className="agx-icon-btn"
                onClick={() => setToolsModal(null)}
              >
                <X size={18} />
              </button>
            </div>
            <div className="agx-modal-body">
              <div className="agx-hook-pills">
                {toolsModal.toolsPreview.map((t) => (
                  <code key={t} className="agx-tool-chip">
                    {t}
                  </code>
                ))}
                {toolsModal.toolCount > toolsModal.toolsPreview.length && (
                  <span className="agx-badge agx-badge--muted">
                    +{toolsModal.toolCount - toolsModal.toolsPreview.length} more
                  </span>
                )}
              </div>
            </div>
          </div>
        </div>
      )}

      {/* Note Modal */}
      {noteModal && (
        <div className="agx-modal-backdrop" onClick={() => setNoteModal(null)}>
          <div className="agx-modal" onClick={(e) => e.stopPropagation()}>
            <div className="agx-modal-header">
              <div>
                <span className="agx-eyebrow">CUSTOM NOTE</span>
                <h3>{noteModal.id}</h3>
              </div>
              <button
                type="button"
                className="agx-icon-btn"
                onClick={() => setNoteModal(null)}
              >
                <X size={18} />
              </button>
            </div>
            <div className="agx-modal-body">
              <input
                type="text"
                className="agx-input"
                value={noteModal.note}
                onChange={(e) =>
                  setNoteModal((prev) => (prev ? { ...prev, note: e.target.value } : null))
                }
                placeholder={vi ? "Nhập ghi chú (để trống để xóa)..." : "Enter note..."}
              />
            </div>
            <div className="agx-modal-footer">
              <button
                type="button"
                className="agx-btn agx-btn--secondary"
                onClick={() => setNoteModal(null)}
              >
                {vi ? "Hủy" : "Cancel"}
              </button>
              <button
                type="button"
                className="agx-btn agx-btn--primary"
                onClick={async () => {
                  await onSaveNote(noteModal.kind, noteModal.id, noteModal.note);
                  setNoteModal(null);
                }}
              >
                <Save size={15} /> {vi ? "Lưu ghi chú" : "Save note"}
              </button>
            </div>
          </div>
        </div>
      )}
    </section>
  );
}
