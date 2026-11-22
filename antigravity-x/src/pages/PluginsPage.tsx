import { useRef, useState } from "react";
import {
  Archive,
  CheckCircle2,
  Code2,
  Download,
  FileJson,
  FolderGit2,
  FolderInput,
  GitBranch,
  Loader2,
  Play,
  Plus,
  Save,
  ShieldCheck,
  Terminal,
  Trash2,
  Upload,
  Wrench,
  X,
} from "lucide-react";
import type { AppLanguage, OverviewState, PluginEntry } from "../types";
import { agxInvoke } from "../platform/invoke";

const BUILTIN_HARDCODED_LABEL = "Hardcoded Built-in · antigravity-filter-bypass v1.3.1 (Embedded)";

type AddPluginMethod = "folder" | "zip" | "git" | "custom";

type PluginsPageProps = {
  lang: AppLanguage;
  plugins: PluginEntry[];
  overview: OverviewState | null;
  configRaw: string;
  loading: boolean;
  onRefresh: () => Promise<void>;
  onInstallBundled: (customPath?: string | null) => Promise<void>;
  onInstallFromFolder: (sourcePath: string, pluginId?: string | null) => Promise<void>;
  onInstallFromZip: (payload: {
    zipPath?: string | null;
    zipName?: string | null;
    zipBytes?: number[] | null;
    pluginId?: string | null;
  }) => Promise<void>;
  onInstallFromGit: (repoUrl: string, pluginId?: string | null) => Promise<void>;
  onCreateCustomPlugin: (payload: {
    pluginId: string;
    name: string;
    version: string;
    description: string;
    hooksRaw?: string | null;
    registerOnly: boolean;
  }) => Promise<void>;
  onUninstallPlugin: (pluginId: string) => Promise<void>;
  onToggle: (id: string, enabled: boolean) => Promise<void>;
  onRunSelfTest: () => Promise<string>;
  onGetHooksRaw: (pluginId: string) => Promise<string>;
  onSaveHooksRaw: (pluginId: string, raw: string) => Promise<void>;
  onSaveConfigRaw: (raw: string) => Promise<void>;
};

export function PluginsPage({
  lang,
  plugins,
  configRaw,
  onInstallBundled,
  onInstallFromFolder,
  onInstallFromZip,
  onInstallFromGit,
  onCreateCustomPlugin,
  onUninstallPlugin,
  onToggle,
  onRunSelfTest,
  onGetHooksRaw,
  onSaveHooksRaw,
  onSaveConfigRaw,
}: PluginsPageProps) {
  const vi = lang === "vi";
  const zipInputRef = useRef<HTMLInputElement | null>(null);

  const [busyKey, setBusyKey] = useState<string | null>(null);
  const [testOutput, setTestOutput] = useState<string | null>(null);
  const [hooksModal, setHooksModal] = useState<{ pluginId: string; raw: string } | null>(null);
  const [configModalOpen, setConfigModalOpen] = useState(false);
  const [configDraft, setConfigDraft] = useState(configRaw);

  // Add Plugin Multi-Method Modal
  const [addModalOpen, setAddModalOpen] = useState(false);
  const [addMethod, setAddMethod] = useState<AddPluginMethod>("folder");
  const [folderPath, setFolderPath] = useState("");
  const [folderPluginId, setFolderPluginId] = useState("");
  const [zipPathInput, setZipPathInput] = useState("");
  const [zipFileName, setZipFileName] = useState<string | null>(null);
  const [zipFileBytes, setZipFileBytes] = useState<number[] | null>(null);
  const [zipPluginId, setZipPluginId] = useState("");
  const [gitUrl, setGitUrl] = useState("");
  const [gitPluginId, setGitPluginId] = useState("");
  const [customId, setCustomId] = useState("");
  const [customName, setCustomName] = useState("");
  const [customVersion, setCustomVersion] = useState("1.0.0");
  const [customDesc, setCustomDesc] = useState("");
  const [customRegisterOnly, setCustomRegisterOnly] = useState(false);

  const defaultSourcePath = BUILTIN_HARDCODED_LABEL;

  const filterBypass =
    plugins.find((p) => p.id === "antigravity-filter-bypass") ?? {
      id: "antigravity-filter-bypass",
      name: "antigravity-filter-bypass",
      description:
        "Execution Continuity lifecycle plugin (SessionStart, PreInvocation, PostInvocation, Stop)",
      enabled: false,
      installed: false,
      source: "bundled",
      installPath: null,
      version: "1.3.1",
      hooks: ["SessionStart", "PreInvocation", "PostInvocation", "Stop"],
      hasSelfTest: true,
    };

  const handleRunTest = async () => {
    setBusyKey("test");
    try {
      const res = await onRunSelfTest();
      setTestOutput(res);
    } catch (err) {
      setTestOutput(err instanceof Error ? err.message : String(err));
    } finally {
      setBusyKey(null);
    }
  };

  const handleOpenHooks = async (pluginId: string) => {
    setBusyKey(`hooks-${pluginId}`);
    try {
      const raw = await onGetHooksRaw(pluginId);
      setHooksModal({ pluginId, raw });
    } finally {
      setBusyKey(null);
    }
  };

  const handlePickZipFile = async (file?: File | null) => {
    if (!file) return;
    const buf = await file.arrayBuffer();
    setZipFileName(file.name);
    setZipFileBytes(Array.from(new Uint8Array(buf)));
  };

  const handleSubmitAddPlugin = async () => {
    setBusyKey("add-plugin");
    try {
      if (addMethod === "folder") {
        await onInstallFromFolder(
          folderPath.trim(),
          folderPluginId.trim() || null,
        );
      } else if (addMethod === "zip") {
        await onInstallFromZip({
          zipPath: zipPathInput.trim() || null,
          zipName: zipFileName,
          zipBytes: zipFileBytes,
          pluginId: zipPluginId.trim() || null,
        });
      } else if (addMethod === "git") {
        await onInstallFromGit(gitUrl.trim(), gitPluginId.trim() || null);
      } else {
        await onCreateCustomPlugin({
          pluginId: customId.trim(),
          name: customName.trim() || customId.trim(),
          version: customVersion.trim() || "1.0.0",
          description: customDesc.trim(),
          hooksRaw: null,
          registerOnly: customRegisterOnly,
        });
      }
      setAddModalOpen(false);
    } finally {
      setBusyKey(null);
    }
  };

  const enabledCount = plugins.filter((p) => p.enabled).length;

  return (
    <section className="agx-page">
      <input
        ref={zipInputRef}
        type="file"
        accept=".zip"
        style={{ display: "none" }}
        onChange={(e) => {
          const f = e.target.files?.[0];
          void handlePickZipFile(f);
          e.target.value = "";
        }}
      />

      <header className="agx-page-header">
        <div className="agx-page-heading">
          <h2>{vi ? "Plugin" : "Plugins"}</h2>
        </div>

        <div className="agx-header-actions">
          <button
            type="button"
            className="agx-btn agx-btn--secondary agx-btn--sm"
            onClick={() => {
              setConfigDraft(configRaw);
              setConfigModalOpen(true);
            }}
          >
            <FileJson size={14} />
            <span>config.json</span>
          </button>

          <button
            type="button"
            className="agx-btn agx-btn--primary agx-btn--sm"
            onClick={() => {
              setAddMethod("folder");
              setAddModalOpen(true);
            }}
          >
            <Plus size={14} />
            <span>{vi ? "Thêm Plugin" : "Add Plugin"}</span>
          </button>
        </div>
      </header>

      {/* Default Built-in Plugin Hero Card: antigravity-filter-bypass */}
      <article className="agx-hero-plugin-card">
        <div className="agx-hero-plugin-top">
          <div className="agx-hero-plugin-identity">
            <div className="agx-hero-plugin-icon">
              <ShieldCheck size={24} />
            </div>
            <div>
              <div className="agx-hero-plugin-title-row">
                <h3>antigravity-filter-bypass</h3>
                <span className="agx-badge agx-badge--brand">
                  v{filterBypass.version ?? "1.3.1"}
                </span>
                <span
                  className={`agx-badge ${
                    filterBypass.enabled ? "agx-badge--success" : "agx-badge--warning"
                  }`}
                >
                  {filterBypass.enabled ? (vi ? "Đang bật" : "Active") : vi ? "Đã tắt" : "Off"}
                </span>
              </div>
            </div>
          </div>

          <div className="agx-hero-plugin-toggle-box">
            <span className="agx-toggle-caption">
              {filterBypass.enabled ? (vi ? "Đang bật" : "ON") : vi ? "Đang tắt" : "OFF"}
            </span>
            <button
              type="button"
              role="switch"
              aria-checked={filterBypass.enabled}
              disabled={busyKey === "toggle-default"}
              className={`agx-switch-pill${filterBypass.enabled ? " agx-switch-pill--on" : ""}`}
              onClick={async () => {
                setBusyKey("toggle-default");
                try {
                  await onToggle(filterBypass.id, !filterBypass.enabled);
                } finally {
                  setBusyKey(null);
                }
              }}
              title={
                vi
                  ? "Bật/tắt plugin mặc định antigravity-filter-bypass"
                  : "Toggle default plugin antigravity-filter-bypass"
              }
            >
              <span className="agx-switch-thumb" />
            </button>
          </div>
        </div>

        <div className="agx-hero-plugin-meta-grid">
          <div className="agx-meta-box">
            <small>{vi ? "NGUỒN MẶC ĐỊNH (BUNDLED SOURCE)" : "DEFAULT SOURCE PATH"}</small>
            <code title={defaultSourcePath}>{defaultSourcePath}</code>
          </div>

          <div className="agx-meta-box">
            <small>{vi ? "ĐÍCH CÀI ĐẶT (CONFIG PLUGINS)" : "INSTALLED TARGET"}</small>
            <code title={filterBypass.installPath ?? ""}>
              {filterBypass.installPath ??
                "~/.gemini/config/plugins/antigravity-filter-bypass"}
            </code>
          </div>

          <div className="agx-meta-box">
            <small>LIFECYCLE HOOKS</small>
            <div className="agx-hook-pills">
              {(filterBypass.hooks.length > 0
                ? filterBypass.hooks
                : ["SessionStart", "PreInvocation", "PostInvocation", "Stop"]
              ).map((h) => (
                <span key={h} className="agx-hook-pill">
                  <CheckCircle2 size={12} /> {h}
                </span>
              ))}
            </div>
          </div>
        </div>

        <div className="agx-hero-plugin-actions">
          <button
            type="button"
            className="agx-btn agx-btn--primary"
            disabled={busyKey === "install"}
            onClick={async () => {
              setBusyKey("install");
              try {
                await onInstallBundled(null);
              } finally {
                setBusyKey(null);
              }
            }}
          >
            {busyKey === "install" ? (
              <Loader2 className="spin" size={14} />
            ) : (
              <Wrench size={14} />
            )}
            <span>
              {vi
                ? "Đồng bộ / Sửa Hooks từ Source Mặc định"
                : "Sync / Repair from Default Source"}
            </span>
          </button>

          <button
            type="button"
            className="agx-btn agx-btn--secondary"
            disabled={busyKey === "test"}
            onClick={() => void handleRunTest()}
          >
            {busyKey === "test" ? <Loader2 className="spin" size={14} /> : <Play size={14} />}
            <span>{vi ? "Chạy Self-Test (test_filter_bypass.py)" : "Run Self-Test"}</span>
          </button>

          <button
            type="button"
            className="agx-btn agx-btn--secondary"
            onClick={() => void handleOpenHooks("antigravity-filter-bypass")}
          >
            <Code2 size={14} />
            <span>{vi ? "Xem / Sửa hooks.json" : "Inspect hooks.json"}</span>
          </button>
        </div>

        {testOutput && (
          <div className="agx-terminal-box">
            <div className="agx-terminal-head">
              <span>
                <Terminal size={14} /> test_filter_bypass.py output
              </span>
              <button
                type="button"
                className="agx-icon-btn"
                onClick={() => setTestOutput(null)}
                title="Close"
              >
                <X size={14} />
              </button>
            </div>
            <pre>{testOutput}</pre>
          </div>
        )}
      </article>

      {/* All Plugins List */}
      <div className="agx-panel-card">
        <div className="agx-panel-head">
          <h3>
            {vi
              ? `Danh sách Plugin (${enabledCount}/${plugins.length})`
              : `Plugins (${enabledCount}/${plugins.length})`}
          </h3>
        </div>

        <div className="agx-plugin-list">
          {plugins.map((p) => {
            const isDefault = p.id === "antigravity-filter-bypass";
            return (
              <div key={p.id} className="agx-plugin-row">
                <div className="agx-plugin-row-main">
                  <div className="agx-plugin-row-title">
                    <strong>{p.name || p.id}</strong>
                    <code>{p.id}</code>
                    {p.version && <span className="agx-badge agx-badge--muted">v{p.version}</span>}
                    <span
                      className={`agx-badge ${
                        isDefault
                          ? "agx-badge--brand"
                          : p.source === "builtin"
                            ? "agx-badge--info"
                            : "agx-badge--muted"
                      }`}
                    >
                      {isDefault
                        ? vi
                          ? "Mặc định (Default)"
                          : "Default Tool Plugin"
                        : p.source === "builtin"
                          ? "Built-in"
                          : p.source === "user"
                            ? "User Plugin"
                            : "Config Entry"}
                    </span>
                  </div>
                  {p.description && <p className="agx-plugin-row-desc">{p.description}</p>}
                  <div className="agx-plugin-row-sub">
                    <span>
                      {vi ? "Đường dẫn:" : "Path:"}{" "}
                      <code>
                        {p.installPath ??
                          (isDefault
                            ? defaultSourcePath
                            : vi
                              ? "Chỉ khai báo trong config.json"
                              : "config.json only")}
                      </code>
                    </span>
                    {p.hooks.length > 0 && (
                      <span>
                        Hooks: <strong>{p.hooks.join(", ")}</strong>
                      </span>
                    )}
                  </div>
                </div>

                <div className="agx-plugin-row-controls">
                  {p.hooks.length > 0 && (
                    <button
                      type="button"
                      className="agx-btn agx-btn--ghost agx-btn--sm"
                      onClick={() => void handleOpenHooks(p.id)}
                    >
                      <Code2 size={14} /> hooks.json
                    </button>
                  )}
                  {!isDefault && p.source === "user" && p.installed && (
                    <button
                      type="button"
                      className="agx-icon-btn agx-icon-btn--danger"
                      title={vi ? "Gỡ cài đặt thư mục plugin" : "Uninstall plugin folder"}
                      onClick={() => void onUninstallPlugin(p.id)}
                    >
                      <Trash2 size={15} />
                    </button>
                  )}
                  <button
                    type="button"
                    role="switch"
                    aria-checked={p.enabled}
                    className={`agx-switch-pill${p.enabled ? " agx-switch-pill--on" : ""}`}
                    onClick={() => void onToggle(p.id, !p.enabled)}
                  >
                    <span className="agx-switch-thumb" />
                  </button>
                </div>
              </div>
            );
          })}
        </div>
      </div>

      {/* Multi-Method Add Plugin Modal */}
      {addModalOpen && (
        <div className="agx-modal-backdrop" onClick={() => setAddModalOpen(false)}>
          <div className="agx-modal agx-modal--lg" onClick={(e) => e.stopPropagation()}>
            <div className="agx-modal-header">
              <div>
                <span className="agx-eyebrow">MULTI-SOURCE PLUGIN INSTALLER</span>
                <h3>{vi ? "Thêm Plugin vào Antigravity" : "Add Plugin to Antigravity"}</h3>
              </div>
              <button
                type="button"
                className="agx-icon-btn"
                onClick={() => setAddModalOpen(false)}
              >
                <X size={18} />
              </button>
            </div>

            <div className="agx-modal-body">
              <div className="agx-category-tabs" role="tablist">
                <button
                  type="button"
                  className={`agx-category-tab${addMethod === "folder" ? " agx-category-tab--active" : ""}`}
                  onClick={() => setAddMethod("folder")}
                >
                  <FolderInput size={14} />
                  <span>{vi ? "1. Từ Thư mục Local" : "1. Local Folder"}</span>
                </button>
                <button
                  type="button"
                  className={`agx-category-tab${addMethod === "zip" ? " agx-category-tab--active" : ""}`}
                  onClick={() => setAddMethod("zip")}
                >
                  <Archive size={14} />
                  <span>{vi ? "2. Từ File .ZIP" : "2. ZIP Archive"}</span>
                </button>
                <button
                  type="button"
                  className={`agx-category-tab${addMethod === "git" ? " agx-category-tab--active" : ""}`}
                  onClick={() => setAddMethod("git")}
                >
                  <GitBranch size={14} />
                  <span>{vi ? "3. Từ Git URL" : "3. Git Clone"}</span>
                </button>
                <button
                  type="button"
                  className={`agx-category-tab${addMethod === "custom" ? " agx-category-tab--active" : ""}`}
                  onClick={() => setAddMethod("custom")}
                >
                  <Plus size={14} />
                  <span>{vi ? "4. Theo ID / Tạo mới" : "4. By ID / Scaffold"}</span>
                </button>
              </div>

              {addMethod === "folder" && (
                <>
                  <label className="agx-field">
                    <span>
                      {vi
                        ? "Đường dẫn thư mục chứa plugin (plugin.json / hooks.json / scripts)"
                        : "Local plugin directory path"}
                    </span>
                    <div className="agx-inline-drawer" style={{ marginTop: 0 }}>
                      <input
                        type="text"
                        className="agx-input"
                        value={folderPath}
                        onChange={(e) => setFolderPath(e.target.value)}
                        placeholder="C:\path\to\my-plugin-folder"
                      />
                      <button
                        type="button"
                        className="agx-btn agx-btn--secondary"
                        onClick={async () => {
                          try {
                            const picked = await agxInvoke<string | null>("pick_folder_dialog", {
                              defaultPath: folderPath || null,
                            });
                            if (picked) setFolderPath(picked);
                          } catch {
                            // ignore
                          }
                        }}
                      >
                        <FolderGit2 size={14} />
                        <span>{vi ? "Chọn thư mục" : "Browse Folder"}</span>
                      </button>
                    </div>
                  </label>
                  <label className="agx-field">
                    <span>
                      {vi
                        ? "Plugin ID tùy chọn (để trống sẽ tự lấy theo plugin.json hoặc tên thư mục)"
                        : "Optional Plugin ID override"}
                    </span>
                    <input
                      type="text"
                      className="agx-input"
                      value={folderPluginId}
                      onChange={(e) => setFolderPluginId(e.target.value)}
                      placeholder="antigravity-filter-bypass"
                    />
                  </label>
                </>
              )}

              {addMethod === "zip" && (
                <>
                  <label className="agx-field">
                    <span>{vi ? "Chọn gói .ZIP từ máy" : "Pick a .ZIP package"}</span>
                    <div className="agx-inline-drawer" style={{ marginTop: 0 }}>
                      <button
                        type="button"
                        className="agx-btn agx-btn--secondary"
                        onClick={() => zipInputRef.current?.click()}
                      >
                        <Upload size={14} />
                        <span>
                          {zipFileName
                            ? `${vi ? "Đã chọn:" : "Selected:"} ${zipFileName}`
                            : vi
                              ? "Chọn file .zip..."
                              : "Choose .zip file..."}
                        </span>
                      </button>
                      {zipFileName && (
                        <button
                          type="button"
                          className="agx-btn agx-btn--ghost agx-btn--sm"
                          onClick={() => {
                            setZipFileName(null);
                            setZipFileBytes(null);
                          }}
                        >
                          {vi ? "Xóa chọn" : "Clear"}
                        </button>
                      )}
                    </div>
                  </label>
                  <label className="agx-field">
                    <span>
                      {vi
                        ? "Hoặc nhập trực tiếp đường dẫn file .ZIP trên ổ đĩa"
                        : "Or enter local .ZIP file path"}
                    </span>
                    <input
                      type="text"
                      className="agx-input"
                      value={zipPathInput}
                      onChange={(e) => setZipPathInput(e.target.value)}
                      placeholder="C:\Downloads\my-plugin.zip"
                    />
                  </label>
                  <label className="agx-field">
                    <span>{vi ? "Plugin ID tùy chọn" : "Optional Plugin ID"}</span>
                    <input
                      type="text"
                      className="agx-input"
                      value={zipPluginId}
                      onChange={(e) => setZipPluginId(e.target.value)}
                      placeholder="my-plugin"
                    />
                  </label>
                </>
              )}

              {addMethod === "git" && (
                <>
                  <label className="agx-field">
                    <span>{vi ? "Git Repository URL" : "Git Repository URL"}</span>
                    <input
                      type="text"
                      className="agx-input"
                      value={gitUrl}
                      onChange={(e) => setGitUrl(e.target.value)}
                      placeholder="https://github.com/user/antigravity-plugin.git"
                    />
                  </label>
                  <label className="agx-field">
                    <span>{vi ? "Plugin ID tùy chọn" : "Optional Plugin ID"}</span>
                    <input
                      type="text"
                      className="agx-input"
                      value={gitPluginId}
                      onChange={(e) => setGitPluginId(e.target.value)}
                      placeholder="Tự động lấy từ plugin.json hoặc tên repo"
                    />
                  </label>
                </>
              )}

              {addMethod === "custom" && (
                <>
                  <div className="agx-form-row-2">
                    <label className="agx-field">
                      <span>Plugin ID</span>
                      <input
                        type="text"
                        className="agx-input"
                        value={customId}
                        onChange={(e) => setCustomId(e.target.value)}
                        placeholder="my-custom-plugin"
                      />
                    </label>
                    <label className="agx-field">
                      <span>Version</span>
                      <input
                        type="text"
                        className="agx-input"
                        value={customVersion}
                        onChange={(e) => setCustomVersion(e.target.value)}
                        placeholder="1.0.0"
                      />
                    </label>
                  </div>
                  <label className="agx-field">
                    <span>Display Name</span>
                    <input
                      type="text"
                      className="agx-input"
                      value={customName}
                      onChange={(e) => setCustomName(e.target.value)}
                      placeholder="My Custom Plugin"
                    />
                  </label>
                  <label className="agx-field">
                    <span>Description</span>
                    <input
                      type="text"
                      className="agx-input"
                      value={customDesc}
                      onChange={(e) => setCustomDesc(e.target.value)}
                      placeholder="Mô tả chức năng plugin..."
                    />
                  </label>
                  <label className="agx-checkbox-inline">
                    <input
                      type="checkbox"
                      checked={customRegisterOnly}
                      onChange={(e) => setCustomRegisterOnly(e.target.checked)}
                    />
                    <span>
                      {vi
                        ? "Chỉ đăng ký ID vào config.json (dành cho built-in / marketplace plugin)"
                        : "Only register ID in config.json (for built-in / marketplace plugin)"}
                    </span>
                  </label>
                </>
              )}
            </div>

            <div className="agx-modal-footer">
              <button
                type="button"
                className="agx-btn agx-btn--secondary"
                onClick={() => setAddModalOpen(false)}
              >
                {vi ? "Hủy" : "Cancel"}
              </button>
              <button
                type="button"
                className="agx-btn agx-btn--primary"
                disabled={busyKey === "add-plugin"}
                onClick={() => void handleSubmitAddPlugin()}
              >
                {busyKey === "add-plugin" ? (
                  <Loader2 className="spin" size={15} />
                ) : (
                  <Download size={15} />
                )}
                <span>{vi ? "Cài đặt & Bật Plugin" : "Install & Enable Plugin"}</span>
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Hooks JSON Modal */}
      {hooksModal && (
        <div className="agx-modal-backdrop" onClick={() => setHooksModal(null)}>
          <div className="agx-modal agx-modal--lg" onClick={(e) => e.stopPropagation()}>
            <div className="agx-modal-header">
              <div>
                <span className="agx-eyebrow">LIFECYCLE HOOKS</span>
                <h3>{hooksModal.pluginId} / hooks.json</h3>
              </div>
              <button
                type="button"
                className="agx-icon-btn"
                onClick={() => setHooksModal(null)}
              >
                <X size={18} />
              </button>
            </div>
            <div className="agx-modal-body">
              <textarea
                className="agx-code-editor"
                rows={16}
                value={hooksModal.raw}
                onChange={(e) =>
                  setHooksModal((prev) => (prev ? { ...prev, raw: e.target.value } : null))
                }
                spellCheck={false}
              />
            </div>
            <div className="agx-modal-footer">
              <button
                type="button"
                className="agx-btn agx-btn--secondary"
                onClick={() => setHooksModal(null)}
              >
                {vi ? "Đóng" : "Close"}
              </button>
              <button
                type="button"
                className="agx-btn agx-btn--primary"
                onClick={async () => {
                  await onSaveHooksRaw(hooksModal.pluginId, hooksModal.raw);
                  setHooksModal(null);
                }}
              >
                <Save size={15} />
                <span>{vi ? "Lưu hooks.json" : "Save hooks.json"}</span>
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Raw config.json Modal */}
      {configModalOpen && (
        <div className="agx-modal-backdrop" onClick={() => setConfigModalOpen(false)}>
          <div className="agx-modal agx-modal--lg" onClick={(e) => e.stopPropagation()}>
            <div className="agx-modal-header">
              <div>
                <span className="agx-eyebrow">ANTIGRAVITY CONFIG</span>
                <h3>~/.gemini/config/config.json</h3>
              </div>
              <button
                type="button"
                className="agx-icon-btn"
                onClick={() => setConfigModalOpen(false)}
              >
                <X size={18} />
              </button>
            </div>
            <div className="agx-modal-body">
              <textarea
                className="agx-code-editor"
                rows={18}
                value={configDraft}
                onChange={(e) => setConfigDraft(e.target.value)}
                spellCheck={false}
              />
            </div>
            <div className="agx-modal-footer">
              <button
                type="button"
                className="agx-btn agx-btn--secondary"
                onClick={() => setConfigModalOpen(false)}
              >
                {vi ? "Hủy" : "Cancel"}
              </button>
              <button
                type="button"
                className="agx-btn agx-btn--primary"
                onClick={async () => {
                  await onSaveConfigRaw(configDraft);
                  setConfigModalOpen(false);
                }}
              >
                <Save size={15} />
                <span>{vi ? "Lưu config.json" : "Save config.json"}</span>
              </button>
            </div>
          </div>
        </div>
      )}
    </section>
  );
}
