import { useEffect, useState } from "react";
import {
  Check,
  Copy,
  Download,
  ExternalLink,
  Globe,
  Loader2,
  MessageSquare,
  Rocket,
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
    window.setTimeout(() => setCopiedKey(null), 1800);
  };

  return (
    <section className="agx-page">
      <header className="agx-page-header">
        <div className="agx-page-heading">
          <h2>OnlyDEVS</h2>
        </div>
      </header>

      {/* Brand Hero & Official Links Card */}
      <article className="agx-about-hero-card">
        <div className="agx-about-hero-brand">
          <img src={logoUrl} alt="OnlyDEVS Logo" className="agx-about-hero-logo" />
          <div>
            <div className="agx-about-hero-title-row">
              <h3>OnlyDEVS — Antigravity</h3>
              <span className="agx-badge agx-badge--brand">v{appVersion}</span>
            </div>
          </div>
        </div>

        <div className="agx-about-links-grid">
          <div className="agx-about-link-card">
            <div className="agx-about-link-icon agx-about-link-icon--web">
              <Globe size={20} />
            </div>
            <div className="agx-about-link-copy">
              <small>MY WEBSITE</small>
              <strong>{WEBSITE_URL}</strong>
            </div>
            <div className="agx-about-link-actions">
              <button
                type="button"
                className="agx-icon-btn agx-icon-btn--bordered"
                title="Copy URL"
                onClick={() => copyText("web", WEBSITE_URL)}
              >
                {copiedKey === "web" ? <Check size={14} /> : <Copy size={14} />}
              </button>
              <button
                type="button"
                className="agx-btn agx-btn--primary"
                onClick={() => void openExternalUrl(WEBSITE_URL)}
              >
                <ExternalLink size={14} />
                <span>{vi ? "Mở Website" : "Open Website"}</span>
              </button>
            </div>
          </div>

          <div className="agx-about-link-card">
            <div className="agx-about-link-icon agx-about-link-icon--discord">
              <MessageSquare size={20} />
            </div>
            <div className="agx-about-link-copy">
              <small>MY SERVER (DISCORD)</small>
              <strong>{DISCORD_URL}</strong>
            </div>
            <div className="agx-about-link-actions">
              <button
                type="button"
                className="agx-icon-btn agx-icon-btn--bordered"
                title="Copy Discord invite"
                onClick={() => copyText("discord", DISCORD_URL)}
              >
                {copiedKey === "discord" ? <Check size={14} /> : <Copy size={14} />}
              </button>
              <button
                type="button"
                className="agx-btn agx-btn--primary"
                onClick={() => void openExternalUrl(DISCORD_URL)}
              >
                <ExternalLink size={14} />
                <span>{vi ? "Tham gia Discord" : "Join Discord"}</span>
              </button>
            </div>
          </div>
        </div>
      </article>

      {/* GitHub Auto-Update Card */}
      <article className="agx-panel-card">
        <div className="agx-panel-head">
          <h3>{vi ? "Cập nhật tự động" : "Auto Update"}</h3>
          {updateResult?.hasUpdate ? (
            <span className="agx-badge agx-badge--brand">
              v{updateResult.latestVersion} {vi ? "khả dụng" : "available"}
            </span>
          ) : (
            <span className="agx-badge agx-badge--success">
              {vi ? `Bản hiện tại v${appVersion}` : `Up to date · v${appVersion}`}
            </span>
          )}
        </div>

        <div className="agx-about-kv-list">
          <KvRow label="GitHub Repository" value={GITHUB_REPO_URL} mono />
          <KvRow
            label={vi ? "Phiên bản hiện tại" : "Current Version"}
            value={`v${appVersion}`}
          />
          <KvRow
            label={vi ? "Phiên bản mới nhất" : "Latest Release"}
            value={
              updateResult
                ? `v${updateResult.latestVersion}${updateResult.hasUpdate ? (vi ? " (Có bản cập nhật mới!)" : " (Update available!)") : vi ? " (Mới nhất)" : " (Up to date)"}`
                : vi
                  ? "Nhấn Kiểm tra cập nhật"
                  : "Click Check for updates"
            }
          />
        </div>

        {updateError ? (
          <div className="ag-token-usage__error" style={{ marginTop: 10 }}>
            {updateError}
          </div>
        ) : null}

        <div
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            flexWrap: "wrap",
            gap: 10,
            marginTop: 14,
          }}
        >
          <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
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
                    : "Check for updates"}
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
                    ? `Cài đặt v${updateResult.latestVersion}`
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
              <span>{vi ? "Mở trang Releases" : "Open Releases"}</span>
            </button>
          </div>

          <label className="ag-select-all-label">
            <input
              type="checkbox"
              checked={updateSettings?.autoCheck ?? true}
              onChange={() => void handleToggleAutoCheck()}
            />
            <span>
              {vi
                ? "Tự động kiểm tra bản mới khi mở ứng dụng"
                : "Automatically check for updates on startup"}
            </span>
          </label>
        </div>
      </article>

      {/* System Info Card */}
      <article className="agx-panel-card">
        <div className="agx-panel-head">
          <h3>{vi ? "Hệ thống & Đường dẫn" : "System & Paths"}</h3>
        </div>

        <div className="agx-about-kv-list">
          <KvRow label="Application" value={`OnlyDEVS v${appVersion}`} />
          <KvRow label="Website" value={WEBSITE_URL} mono />
          <KvRow label="Discord" value={DISCORD_URL} mono />
          <KvRow label="GEMINI_HOME" value={overview?.geminiHome ?? "~/.gemini"} mono />
          <KvRow label="AGENTS.md" value={overview?.agentsPath ?? "~/.gemini/config/AGENTS.md"} mono />
          <KvRow label="config.json" value={overview?.configPath ?? "~/.gemini/config/config.json"} mono />
          <KvRow
            label="mcp_config.json"
            value={overview?.mcpConfigPath ?? "~/.gemini/config/mcp_config.json"}
            mono
          />
        </div>
      </article>
    </section>
  );
}

function KvRow({ label, value, mono }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className="agx-about-kv-row">
      <span className="agx-about-kv-label">{label}</span>
      {mono ? (
        <code className="agx-about-kv-code">{value}</code>
      ) : (
        <strong className="agx-about-kv-value">{value}</strong>
      )}
    </div>
  );
}
