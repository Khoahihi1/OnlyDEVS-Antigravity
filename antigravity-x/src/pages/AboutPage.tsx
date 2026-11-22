import { useEffect, useState } from "react";
import {
  Check,
  Copy,
  Download,
  ExternalLink,
  FolderOpen,
  Github,
  Globe,
  Loader2,
  MessageSquare,
  Rocket,
  ShieldCheck,
} from "lucide-react";
import type {
  AppLanguage,
  OverviewState,
  UpdateCheckResult,
  UpdateSettings,
} from "../types";
import { agxInvoke, openExternalUrl } from "../platform/invoke";
import logoUrl from "../assets/onlydevs-logo.png";

const WEBSITE_URL = "https://onlydevs.online/";
const DISCORD_URL = "https://discord.gg/kEdMcjnftp";
const GITHUB_REPO_URL = "https://github.com/Khoahihi1/OnlyDEVS-Antigravity";
const GITHUB_RELEASES_URL =
  "https://github.com/Khoahihi1/OnlyDEVS-Antigravity/releases";

type AboutPageProps = {
  lang: AppLanguage;
  overview: OverviewState | null;
  appVersion: string;
  onRefresh: () => Promise<void>;
  onOpenUpdateModal?: (result: UpdateCheckResult) => void;
};

export function AboutPage({
  lang,
  overview,
  appVersion,
  onOpenUpdateModal,
}: AboutPageProps) {
  const vi = lang === "vi";
  const [copiedKey, setCopiedKey] = useState<string | null>(null);
  const [updateSettings, setUpdateSettings] = useState<UpdateSettings | null>(null);
  const [checkingUpdate, setCheckingUpdate] = useState(false);
  const [updateResult, setUpdateResult] = useState<UpdateCheckResult | null>(null);
  const [updateError, setUpdateError] = useState<string | null>(null);

  useEffect(() => {
    void agxInvoke<UpdateSettings>("get_update_settings")
      .then(setUpdateSettings)
      .catch(() => {});
  }, []);

  const handleCheckUpdate = async () => {
    setCheckingUpdate(true);
    setUpdateError(null);
    try {
      const res = await agxInvoke<UpdateCheckResult>("check_for_updates", {
        force: true,
      });
      setUpdateResult(res);
      if (res.hasUpdate && onOpenUpdateModal) {
        onOpenUpdateModal(res);
      }
    } catch (err) {
      setUpdateError(err instanceof Error ? err.message : String(err));
    } finally {
      setCheckingUpdate(false);
    }
  };

  const handleToggleAutoCheck = async () => {
    const next = !(updateSettings?.autoCheck ?? true);
    try {
      const updated = await agxInvoke<UpdateSettings>("patch_update_settings", {
        autoCheck: next,
      });
      setUpdateSettings(updated);
    } catch {
      // ignore
    }
  };

  const copyText = (key: string, value: string) => {
    void navigator.clipboard?.writeText(value);
    setCopiedKey(key);
    window.setTimeout(() => setCopiedKey(null), 1600);
  };

  const pathItems = [
    { key: "gemini", label: "GEMINI_HOME", value: overview?.geminiHome ?? "~/.gemini" },
    { key: "agents", label: "AGENTS.md", value: overview?.agentsPath ?? "~/.gemini/config/AGENTS.md" },
    { key: "config", label: "config.json", value: overview?.configPath ?? "~/.gemini/config/config.json" },
    { key: "mcp", label: "mcp_config.json", value: overview?.mcpConfigPath ?? "~/.gemini/config/mcp_config.json" },
    ...(overview?.pythonPath
      ? [{ key: "python", label: "Python", value: overview.pythonPath }]
      : []),
  ];

  return (
    <section className="agx-page">
      {/* Compact Brand Hero Banner */}
      <article className="od-about-banner">
        <div className="od-about-banner__left">
          <div className="od-about-logo-ring">
            <img src={logoUrl} alt="OnlyDEVS Logo" className="od-about-logo-img" />
          </div>
          <div className="od-about-banner__meta">
            <div className="od-about-banner__title-row">
              <h2>OnlyDEVS — Antigravity</h2>
              <span className="agx-badge agx-badge--brand">v{appVersion}</span>
            </div>
            <p className="od-about-banner__sub">
              {vi
                ? "Bộ công cụ điều khiển & Local API Router thuần Antigravity"
                : "All-in-One Control Suite & Pure Antigravity Local API Router"}
            </p>
          </div>
        </div>

        <div className="od-about-banner__links">
          <div className="od-about-link-pill">
            <button
              type="button"
              className="od-about-link-pill__main"
              onClick={() => void openExternalUrl(WEBSITE_URL)}
            >
              <Globe size={14} className="od-about-ico--blue" />
              <span>onlydevs.online</span>
              <ExternalLink size={12} />
            </button>
            <button
              type="button"
              className="od-about-link-pill__copy"
              title="Copy"
              onClick={() => copyText("web", WEBSITE_URL)}
            >
              {copiedKey === "web" ? <Check size={12} /> : <Copy size={12} />}
            </button>
          </div>

          <div className="od-about-link-pill">
            <button
              type="button"
              className="od-about-link-pill__main"
              onClick={() => void openExternalUrl(DISCORD_URL)}
            >
              <MessageSquare size={14} className="od-about-ico--red" />
              <span>Discord</span>
              <ExternalLink size={12} />
            </button>
            <button
              type="button"
              className="od-about-link-pill__copy"
              title="Copy"
              onClick={() => copyText("discord", DISCORD_URL)}
            >
              {copiedKey === "discord" ? <Check size={12} /> : <Copy size={12} />}
            </button>
          </div>

          <div className="od-about-link-pill">
            <button
              type="button"
              className="od-about-link-pill__main"
              onClick={() => void openExternalUrl(GITHUB_REPO_URL)}
            >
              <Github size={14} />
              <span>GitHub</span>
              <ExternalLink size={12} />
            </button>
            <button
              type="button"
              className="od-about-link-pill__copy"
              title="Copy"
              onClick={() => copyText("github", GITHUB_REPO_URL)}
            >
              {copiedKey === "github" ? <Check size={12} /> : <Copy size={12} />}
            </button>
          </div>
        </div>
      </article>

      {/* Balanced 2-Column Content Grid */}
      <div className="od-about-grid">
        {/* Left: Auto-Update Card */}
        <article className="od-about-card">
          <div className="od-about-card__head">
            <div className="od-about-card__title">
              <Rocket size={16} className="od-about-ico--red" />
              <h3>{vi ? "Cập nhật tự động" : "Auto Update"}</h3>
            </div>
            {updateResult?.hasUpdate ? (
              <span className="agx-badge agx-badge--brand">
                v{updateResult.latestVersion} {vi ? "mới" : "new"}
              </span>
            ) : (
              <span className="agx-badge agx-badge--success">
                <ShieldCheck size={11} style={{ marginRight: 3 }} />
                v{appVersion}
              </span>
            )}
          </div>

          <div className="od-about-version-strip">
            <div className="od-about-version-box">
              <span>{vi ? "Hiện tại" : "Installed"}</span>
              <strong>v{appVersion}</strong>
            </div>
            <div className="od-about-version-box">
              <span>{vi ? "Phiên bản mới nhất" : "Latest Release"}</span>
              <strong>
                {updateResult
                  ? `v${updateResult.latestVersion}`
                  : vi
                    ? "Chưa kiểm tra"
                    : "Not checked"}
              </strong>
            </div>
          </div>

          {updateError ? (
            <div className="ag-token-usage__error" style={{ marginTop: 10 }}>
              {updateError}
            </div>
          ) : null}

          <div className="od-about-card__actions">
            <button
              type="button"
              className="agx-btn agx-btn--primary"
              disabled={checkingUpdate}
              onClick={() => void handleCheckUpdate()}
            >
              {checkingUpdate ? (
                <Loader2 size={14} className="spin" />
              ) : (
                <Rocket size={14} />
              )}
              <span>
                {checkingUpdate
                  ? vi
                    ? "Đang kiểm tra..."
                    : "Checking..."
                  : vi
                    ? "Kiểm tra cập nhật"
                    : "Check Updates"}
              </span>
            </button>

            {updateResult?.hasUpdate && onOpenUpdateModal ? (
              <button
                type="button"
                className="agx-btn agx-btn--secondary"
                onClick={() => onOpenUpdateModal(updateResult)}
              >
                <Download size={14} />
                <span>
                  {vi
                    ? `Cài v${updateResult.latestVersion}`
                    : `Install v${updateResult.latestVersion}`}
                </span>
              </button>
            ) : null}

            <button
              type="button"
              className="agx-btn agx-btn--secondary"
              onClick={() => void openExternalUrl(GITHUB_RELEASES_URL)}
            >
              <ExternalLink size={14} />
              <span>Releases</span>
            </button>
          </div>

          <label className="od-about-toggle-row">
            <span>
              {vi
                ? "Tự động kiểm tra cập nhật khi mở app"
                : "Auto-check updates on startup"}
            </span>
            <input
              type="checkbox"
              checked={updateSettings?.autoCheck ?? true}
              onChange={() => void handleToggleAutoCheck()}
            />
          </label>
        </article>

        {/* Right: Clean Environment & Paths Card (zero duplicates) */}
        <article className="od-about-card">
          <div className="od-about-card__head">
            <div className="od-about-card__title">
              <FolderOpen size={16} className="od-about-ico--blue" />
              <h3>{vi ? "Đường dẫn cấu hình" : "Config Paths"}</h3>
            </div>
            <span className="agx-badge">
              {overview?.pluginsEnabled ?? 0}/{overview?.pluginCount ?? 0} Plugin
            </span>
          </div>

          <div className="od-about-paths-list">
            {pathItems.map((item) => (
              <div key={item.key} className="od-about-path-row">
                <span className="od-about-path-label">{item.label}</span>
                <code className="od-about-path-code" title={item.value}>
                  {item.value}
                </code>
                <button
                  type="button"
                  className="agx-icon-btn"
                  title="Copy path"
                  onClick={() => copyText(item.key, item.value)}
                >
                  {copiedKey === item.key ? <Check size={13} /> : <Copy size={13} />}
                </button>
              </div>
            ))}
          </div>
        </article>
      </div>
    </section>
  );
}
