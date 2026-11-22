import { useEffect, useId, useMemo, useRef, useState } from "react";
import {
  ArrowLeft,
  ArrowLeftRight,
  CircleHelp,
  CirclePlus,
  FileText,
  Loader2,
  PencilLine,
  Plus,
  RefreshCw,
  Save,
  Settings2,
  Sparkles,
  Trash2,
  Upload,
  X,
} from "lucide-react";
import type { AppLanguage, InstructionMode, PromptCategory, SavedPrompt } from "../types";
import { stripMdSuffix } from "../platform/invoke";

export type PromptDraft = {
  id?: string;
  category: string;
  title: string;
  filename: string;
  description: string;
  content: string;
  enabled: boolean;
};

type PromptsPageProps = {
  lang: AppLanguage;
  prompts: SavedPrompt[];
  categories: PromptCategory[];
  instructionMode: InstructionMode;
  agentsPreview: string;
  loading: boolean;
  onRefresh: () => Promise<void>;
  onSyncSeedPrompts: () => Promise<void>;
  onSavePrompt: (draft: PromptDraft) => Promise<void>;
  onDelete: (id: string) => Promise<void>;
  onToggle: (id: string, enabled: boolean) => Promise<void>;
  onSaveAgents: (content: string) => Promise<void>;
  onModeChange: (mode: InstructionMode) => Promise<void>;
  onCreateCategory: (name: string) => Promise<void>;
  onDeleteCategory: (name: string) => Promise<void>;
};

const emptyDraft = (category = "Security / reverse"): PromptDraft => ({
  category,
  title: "",
  filename: "",
  description: "",
  content: "",
  enabled: false,
});

export function PromptsPage({
  lang,
  prompts,
  categories,
  instructionMode,
  loading,
  onRefresh,
  onSyncSeedPrompts,
  onSavePrompt,
  onDelete,
  onToggle,
  onModeChange,
  onCreateCategory,
  onDeleteCategory,
}: PromptsPageProps) {
  const vi = lang === "vi";
  const helpId = useId();
  const fileInputRef = useRef<HTMLInputElement | null>(null);

  const [activeCategory, setActiveCategory] = useState<string>("Security / reverse");
  const [viewMode, setViewMode] = useState<"list" | "form">("list");
  const [draft, setDraft] = useState<PromptDraft>(emptyDraft());
  const [helpOpen, setHelpOpen] = useState(false);
  const [catModalOpen, setCatModalOpen] = useState(false);
  const [newCategoryName, setNewCategoryName] = useState("");
  const [syncing, setSyncing] = useState(false);

  const categoryTabs = useMemo(() => {
    const names = new Set<string>(["Security / reverse", "Software development", "Writing"]);
    for (const c of categories) names.add(c.name);
    for (const p of prompts) names.add(p.category);
    return Array.from(names);
  }, [categories, prompts]);

  useEffect(() => {
    if (!categoryTabs.includes(activeCategory)) {
      setActiveCategory(categoryTabs[0] ?? "Security / reverse");
    }
  }, [activeCategory, categoryTabs]);

  const enabledPrompts = useMemo(() => prompts.filter((p) => p.enabled), [prompts]);

  const visiblePrompts = useMemo(
    () => prompts.filter((p) => p.category === activeCategory),
    [prompts, activeCategory],
  );

  const handleImportMdFile = async (file?: File | null) => {
    if (!file) return;
    const text = await file.text();
    const displayTitle = stripMdSuffix(file.name);
    await onSavePrompt({
      category: activeCategory,
      title: displayTitle,
      filename: file.name.endsWith(".md") ? file.name : `${displayTitle}.md`,
      description: "",
      content: text,
      enabled: false,
    });
  };

  const openNewPrompt = () => {
    setDraft(emptyDraft(activeCategory));
    setViewMode("form");
  };

  const openEditPrompt = (p: SavedPrompt) => {
    setDraft({
      id: p.id,
      category: p.category,
      title: stripMdSuffix(p.title),
      filename: p.filename,
      description: p.description,
      content: p.content,
      enabled: p.enabled,
    });
    setViewMode("form");
  };

  // Full-page Form View (Codex-X PromptFormView style)
  if (viewMode === "form") {
    return (
      <section className="cx-prompts-page cx-prompts-page--form">
        <header className="cx-prompts-header cx-prompts-form-header">
          <div className="cx-prompts-heading">
            <p>
              <PencilLine size={14} aria-hidden="true" />
              {draft.id ? "EDIT PROMPT TEMPLATE" : "CUSTOM PROMPT"}
            </p>
            <h2>
              {draft.id
                ? `${vi ? "Chỉnh sửa" : "Edit"}: ${stripMdSuffix(draft.title)}`
                : vi
                  ? "Add prompt"
                  : "Add prompt"}
            </h2>
            <span>
              {vi
                ? "Tên hiển thị không có đuôi .md. Khi bật sẽ ghi trực tiếp vào ~/.gemini/config/AGENTS.md (giữ nguyên tên file AGENTS.md)."
                : "Display title omits .md. When enabled, writes directly to ~/.gemini/config/AGENTS.md without renaming AGENTS.md."}
            </span>
          </div>
          <button
            type="button"
            className="cx-prompts-btn cx-prompts-btn--secondary"
            onClick={() => setViewMode("list")}
            disabled={loading}
          >
            <ArrowLeft size={15} />
            <span>{vi ? "Back" : "Back"}</span>
          </button>
        </header>

        <section className="cx-prompts-form-panel">
          <div className="cx-prompts-form-panel-head">
            <FileText size={17} aria-hidden="true" />
            <h3>{vi ? "Prompt details" : "Prompt details"}</h3>
          </div>

          <div className="cx-prompts-form-grid">
            <label className="cx-prompts-field">
              <span>{vi ? "Prompt name" : "Prompt name"}</span>
              <input
                type="text"
                value={draft.title}
                onChange={(e) => {
                  const clean = stripMdSuffix(e.target.value);
                  setDraft((d) => ({
                    ...d,
                    title: clean,
                    filename: `${clean || "prompt"}.md`,
                  }));
                }}
                placeholder={vi ? "Ví dụ: KhoaHihi" : "For example: KhoaHihi"}
                autoComplete="off"
              />
            </label>

            <label className="cx-prompts-field">
              <span>{vi ? "Category" : "Category"}</span>
              <select
                value={draft.category}
                onChange={(e) => setDraft((d) => ({ ...d, category: e.target.value }))}
              >
                {categoryTabs.map((c) => (
                  <option key={c} value={c}>
                    {c}
                  </option>
                ))}
              </select>
            </label>

            <label className="cx-prompts-field cx-prompts-field--full">
              <span>{vi ? "Subtitle / Short description" : "Subtitle / Short description"}</span>
              <input
                type="text"
                value={draft.description}
                onChange={(e) => setDraft((d) => ({ ...d, description: e.target.value }))}
                placeholder={
                  vi
                    ? "Mô tả ngắn cho prompt..."
                    : "Short description for this prompt..."
                }
                autoComplete="off"
              />
            </label>

            <label className="cx-prompts-field cx-prompts-field--full">
              <span>
                {vi ? "Prompt content" : "Prompt content"}
                <small>Markdown → ~/.gemini/config/AGENTS.md</small>
              </span>
              <textarea
                value={draft.content}
                onChange={(e) => setDraft((d) => ({ ...d, content: e.target.value }))}
                placeholder="Enter prompt content here..."
                spellCheck={false}
              />
            </label>
          </div>
        </section>

        <div className="cx-prompts-form-actions">
          {draft.id && (
            <button
              type="button"
              className="cx-prompts-btn cx-prompts-btn--danger"
              onClick={async () => {
                await onDelete(draft.id!);
                setViewMode("list");
              }}
            >
              <Trash2 size={15} />
              <span>{vi ? "Delete" : "Delete"}</span>
            </button>
          )}
          <button
            type="button"
            className="cx-prompts-btn cx-prompts-btn--primary"
            disabled={loading || !draft.title.trim()}
            onClick={async () => {
              const cleanTitle = stripMdSuffix(draft.title);
              await onSavePrompt({
                ...draft,
                title: cleanTitle,
                filename: `${cleanTitle}.md`,
              });
              setViewMode("list");
            }}
          >
            <Save size={15} />
            <span>{vi ? "Save" : "Save"}</span>
          </button>
        </div>
      </section>
    );
  }

  return (
    <section className="cx-prompts-page cx-prompts-page--list">
      <input
        ref={fileInputRef}
        type="file"
        accept=".md,.markdown,.txt"
        style={{ display: "none" }}
        onChange={(e) => {
          const f = e.target.files?.[0];
          void handleImportMdFile(f);
          e.target.value = "";
        }}
      />

      {/* 1. Header — exact Codex-X layout */}
      <header className="cx-prompts-header">
        <div className="cx-prompts-heading">
          <p>
            <Sparkles size={14} aria-hidden="true" />
            PROMPT INJECTION
          </p>
          <h2>Manage instruction prompts</h2>
        </div>

        <div className="cx-prompts-header-actions">
          <button
            type="button"
            className="cx-prompts-btn cx-prompts-btn--secondary"
            disabled={syncing || loading}
            onClick={async () => {
              setSyncing(true);
              try {
                await onSyncSeedPrompts();
                await onRefresh();
              } finally {
                setSyncing(false);
              }
            }}
          >
            {syncing ? (
              <Loader2 size={15} className="spin" />
            ) : (
              <RefreshCw size={15} />
            )}
            <span>{syncing ? "Syncing..." : "Sync GitHub templates"}</span>
          </button>

          <button
            type="button"
            className="cx-prompts-btn cx-prompts-btn--secondary"
            onClick={() => fileInputRef.current?.click()}
            disabled={loading}
          >
            <Upload size={15} />
            <span>Import md</span>
          </button>

          <button
            type="button"
            className="cx-prompts-btn cx-prompts-btn--primary"
            onClick={openNewPrompt}
            disabled={loading}
          >
            <Plus size={15} />
            <span>Add prompt</span>
          </button>
        </div>
      </header>

      {/* 2. Mode Panel — exact Codex-X 2-column split card */}
      <section className="cx-prompts-mode-panel">
        <div className="cx-prompts-active-summary">
          <p>Current status</p>
          <div className="cx-prompts-active-title">
            <span
              className={`cx-prompts-state-dot${
                enabledPrompts.length > 0 ? " cx-prompts-state-dot--active" : ""
              }`}
              aria-hidden="true"
            />
            <strong>
              {enabledPrompts.length > 0
                ? enabledPrompts.map((p) => stripMdSuffix(p.title)).join(", ")
                : "No prompt enabled"}
            </strong>
            {enabledPrompts.length > 0 && (
              <span className="cx-prompts-status-badge">
                {instructionMode === "append"
                  ? "Append to AGENTS.md"
                  : "Replace instruction file"}
              </span>
            )}
          </div>
          <span>
            {enabledPrompts.length > 0
              ? instructionMode === "append"
                ? "The current template is written to ~/.gemini/config/AGENTS.md while preserving existing rules."
                : "The current template replaces the content of ~/.gemini/config/AGENTS.md (filename preserved)."
              : "Choose an activation method, then turn on a template below."}
          </span>
        </div>

        <div className="cx-prompts-mode-choice">
          <div className="cx-prompts-mode-copy">
            <div className="cx-prompts-mode-title">
              <strong>Enable method</strong>
              <button
                type="button"
                className="cx-prompts-help-btn"
                aria-expanded={helpOpen}
                aria-controls={helpId}
                onClick={() => setHelpOpen((v) => !v)}
                title="Show activation method help"
              >
                <CircleHelp size={15} />
              </button>
            </div>
            {helpOpen && (
              <div id={helpId} className="cx-prompts-mode-help" role="dialog">
                <div>
                  <strong>Keep existing</strong>
                  <span>
                    Appends a managed block to ~/.gemini/config/AGENTS.md while preserving existing content.
                  </span>
                </div>
                <div>
                  <strong>Replace existing</strong>
                  <span>
                    Writes the enabled template directly into ~/.gemini/config/AGENTS.md without changing the filename.
                  </span>
                </div>
              </div>
            )}
            <span>This method is used when a template is turned on.</span>
          </div>

          <div className="cx-prompts-mode-segments" role="radiogroup" aria-label="Enable method">
            <button
              type="button"
              role="radio"
              aria-checked={instructionMode === "append"}
              className={`cx-prompts-mode-button${
                instructionMode === "append" ? " cx-prompts-mode-button--active" : ""
              }`}
              onClick={() => void onModeChange("append")}
            >
              <CirclePlus size={16} aria-hidden="true" />
              <span>Keep existing</span>
            </button>

            <button
              type="button"
              role="radio"
              aria-checked={instructionMode === "replace"}
              className={`cx-prompts-mode-button${
                instructionMode === "replace" ? " cx-prompts-mode-button--active" : ""
              }`}
              onClick={() => void onModeChange("replace")}
            >
              <ArrowLeftRight size={16} aria-hidden="true" />
              <span>Replace existing</span>
            </button>
          </div>
        </div>
      </section>

      {/* 3. Category Toolbar — exact Codex-X pill tabs + Manage categories */}
      <div className="cx-prompt-category-toolbar">
        <div className="cx-prompt-category-tabs" role="group" aria-label="Prompt categories">
          {categoryTabs.map((cat) => {
            const active = activeCategory === cat;
            return (
              <button
                key={cat}
                type="button"
                aria-pressed={active}
                className={`cx-prompt-category-tab${
                  active ? " cx-prompt-category-tab--active" : ""
                }`}
                onClick={() => setActiveCategory(cat)}
              >
                {cat}
              </button>
            );
          })}
        </div>

        <button
          type="button"
          className="cx-prompt-category-manage"
          onClick={() => setCatModalOpen(true)}
        >
          <Settings2 size={15} aria-hidden="true" />
          <span>Manage categories</span>
        </button>
      </div>

      {/* 4. 3-Column Cards Grid — exact Codex-X PromptRow geometry */}
      <section className="cx-prompts-list-panel">
        <div className="cx-prompts-list">
          {visiblePrompts.length === 0 && (
            <div className="cx-prompt-category-list-empty">
              <FileText size={20} aria-hidden="true" />
              <span>No prompts in this category</span>
            </div>
          )}

          {visiblePrompts.map((p) => {
            const displayTitle = stripMdSuffix(p.title);
            return (
              <article key={p.id} className="cx-prompts-row">
                <div className="cx-prompts-row-head">
                  <div className="cx-prompts-row-heading">
                    <div className="cx-prompts-row-icon" aria-hidden="true">
                      <FileText size={16} strokeWidth={1.9} />
                    </div>
                    <div className="cx-prompts-row-title" title={displayTitle}>
                      <strong>{displayTitle}</strong>
                    </div>
                  </div>

                  <div className="cx-prompts-row-actions">
                    <button
                      type="button"
                      role="switch"
                      aria-checked={p.enabled}
                      aria-label={`Toggle ${displayTitle}`}
                      disabled={loading}
                      className={`cx-toggle${p.enabled ? " cx-toggle--checked" : ""}`}
                      onClick={() => void onToggle(p.id, !p.enabled)}
                    >
                      <span className="cx-toggle-thumb" />
                    </button>
                  </div>
                </div>

                <div className="cx-prompts-row-copy">
                  <p title={p.description}>{p.description}</p>
                </div>

                <div className="cx-prompts-row-footer">
                  <div className="cx-prompts-row-details" />
                  <div className="cx-prompts-icon-actions">
                    <button
                      type="button"
                      className="cx-edit-icon-btn"
                      title={vi ? "Xem / Chỉnh sửa prompt" : "View or edit local template"}
                      onClick={() => openEditPrompt(p)}
                    >
                      <PencilLine size={15} strokeWidth={1.9} />
                    </button>
                  </div>
                </div>
              </article>
            );
          })}
        </div>
      </section>

      {/* Manage Categories Modal */}
      {catModalOpen && (
        <div className="agx-modal-backdrop" onClick={() => setCatModalOpen(false)}>
          <div className="agx-modal" onClick={(e) => e.stopPropagation()}>
            <div className="agx-modal-header">
              <div>
                <span className="agx-eyebrow">CATEGORIES</span>
                <h3>Manage categories</h3>
              </div>
              <button
                type="button"
                className="agx-icon-btn"
                onClick={() => setCatModalOpen(false)}
              >
                <X size={18} />
              </button>
            </div>
            <div className="agx-modal-body">
              <div className="agx-inline-drawer" style={{ marginTop: 0 }}>
                <input
                  type="text"
                  className="agx-input"
                  value={newCategoryName}
                  onChange={(e) => setNewCategoryName(e.target.value)}
                  placeholder="New category name..."
                />
                <button
                  type="button"
                  className="cx-prompts-btn cx-prompts-btn--primary"
                  disabled={!newCategoryName.trim()}
                  onClick={async () => {
                    await onCreateCategory(newCategoryName.trim());
                    setNewCategoryName("");
                  }}
                >
                  <Plus size={15} /> Add
                </button>
              </div>

              <div className="agx-cat-manage-list">
                {categoryTabs.map((c) => {
                  const isCore =
                    c === "Security / reverse" ||
                    c === "Software development" ||
                    c === "Writing";
                  return (
                    <div key={c} className="agx-cat-manage-row">
                      <strong>{c}</strong>
                      {isCore ? (
                        <span className="agx-badge agx-badge--muted">Default</span>
                      ) : (
                        <button
                          type="button"
                          className="agx-icon-btn agx-icon-btn--danger"
                          onClick={() => void onDeleteCategory(c)}
                        >
                          <Trash2 size={14} />
                        </button>
                      )}
                    </div>
                  );
                })}
              </div>
            </div>
          </div>
        </div>
      )}
    </section>
  );
}
