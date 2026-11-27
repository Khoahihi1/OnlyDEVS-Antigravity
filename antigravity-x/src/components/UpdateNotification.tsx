import { useEffect, useState } from "react";
import {
  AlertTriangle,
  Download,
  ExternalLink,
  Loader2,
  Rocket,
} from "lucide-react";
import type {
  AppLanguage,
  UpdateCheckResult,
  UpdateProgressPayload,
} from "../types";
import { agxInvoke, isTauri, openExternalUrl } from "../platform/invoke";

type UpdateNotificationProps = {
  lang: AppLanguage;
  update: UpdateCheckResult | null;
  required?: boolean;
  onClose: () => void;
  onSkipped: (version: string) => void;
};

function formatBytes(bytes?: number | null): string {
  if (!bytes || bytes <= 0) return "";
  if (bytes >= 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  if (bytes >= 1024) return `${(bytes / 1024).toFixed(0)} KB`;
  return `${bytes} B`;
}

export function UpdateNotification({
  lang,
  update,
  required = true,
  onClose,
}: UpdateNotificationProps) {
  const vi = lang === "vi";
  const [installing, setInstalling] = useState(false);
  const [progress, setProgress] = useState<UpdateProgressPayload | null>(null);
  const [infoMessage, setInfoMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!update || !isTauri()) return;
    let unlisten: (() => void) | null = null;
    void (async () => {
      try {
        const ev = await import("@tauri-apps/api/event");
        unlisten = await ev.listen<UpdateProgressPayload>(
          "updater://progress",
          (event) => {
            setProgress(event.payload);
          },
        );
      } catch {
        // ignore event listener errors in browser preview
      }
    })();
    return () => {
      unlisten?.();
    };
  }, [update]);

  // Block Escape key when mandatory update gate is active
  useEffect(() => {
    if (!update || !required) return;
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
      }
    };
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, [update, required]);

  if (!update) return null;

  const isMandatory = required && update.hasUpdate;

  const handleInstall = async () => {
    if (!update.downloadUrl) {
      await openExternalUrl(update.releaseUrl);
      return;
    }
    setInstalling(true);
    setError(null);
    setInfoMessage(null);
    setProgress({
      downloaded: 0,
      total: update.assetSize,
      percent: 0,
      status: "downloading",
    });
    try {
      const msg = await agxInvoke<string>("download_and_install_update", {
        downloadUrl: update.downloadUrl,
        assetName: update.assetName ?? null,
        silent: false,
      });
      if (msg) {
        setInfoMessage(msg);
      }
      setInstalling(false);
      setProgress(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setInstalling(false);
    }
  };

  return (
    <div
      className="agx-modal-backdrop"
      style={{ zIndex: 9999 }}
      onClick={() => {
        if (!installing && !isMandatory) onClose();
      }}
    >
      <div className="agx-modal agx-modal--wide" onClick={(e) => e.stopPropagation()}>
        <div className="agx-modal-head">
          <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
            {isMandatory ? (
              <AlertTriangle size={20} style={{ color: "#ef2328" }} />
            ) : (
              <Rocket size={18} style={{ color: "var(--agx-primary)" }} />
            )}
            <div>
              <div style={{ display: "flex", alignItems: "center", gap: 8, flexWrap: "wrap" }}>
                <h3>
                  {vi
                    ? `Phiên bản mới v${update.latestVersion}`
                    : `New Version v${update.latestVersion}`}
                </h3>
                {isMandatory ? (
                  <span className="agx-badge agx-badge--brand">
                    {vi ? "Bắt buộc cập nhật" : "Update Required"}
                  </span>
                ) : null}
              </div>
              <span className="agx-muted">
                v{update.currentVersion} → v{update.latestVersion}
                {update.assetName ? ` · ${update.assetName}` : ""}
                {update.assetSize ? ` (${formatBytes(update.assetSize)})` : ""}
              </span>
            </div>
          </div>
        </div>

        <div className="agx-modal-body">
          {isMandatory ? (
            <div
              style={{
                padding: "10px 12px",
                borderRadius: 10,
                border: "1px solid rgba(239, 35, 40, 0.35)",
                background: "rgba(239, 35, 40, 0.08)",
                fontSize: 12.5,
                fontWeight: 600,
                marginBottom: 12,
              }}
            >
              {vi
                ? `Bạn đang dùng bản cũ (v${update.currentVersion}). Vui lòng cập nhật lên v${update.latestVersion} để tiếp tục sử dụng.`
                : `You are running an outdated version (v${update.currentVersion}). Please update to v${update.latestVersion} to continue.`}
            </div>
          ) : null}

          {update.releaseNotes ? (
            <div className="agx-update-notes">
              <pre>{update.releaseNotes}</pre>
            </div>
          ) : (
            <p className="agx-muted">
              {vi
                ? "Bản phát hành mới đã sẵn sàng trên GitHub Releases với các bản vá và tính năng mới nhất."
                : "A new release is ready on GitHub Releases with the latest fixes and improvements."}
            </p>
          )}

          {installing && progress ? (
            <div className="agx-update-progress">
              <div className="agx-update-progress-head">
                <span>
                  {progress.status === "installing"
                    ? vi
                      ? "Đang cài đặt bản cập nhật..."
                      : "Installing update..."
                    : vi
                      ? "Đang tải bản cập nhật..."
                      : "Downloading update..."}
                </span>
                <strong>
                  {progress.total
                    ? `${formatBytes(progress.downloaded)} / ${formatBytes(progress.total)} (${progress.percent.toFixed(0)}%)`
                    : `${formatBytes(progress.downloaded)}`}
                </strong>
              </div>
              <div className="ag-quota-bar-track" style={{ height: 8 }}>
                <div
                  className="ag-quota-bar-fill is-high"
                  style={{ width: `${Math.max(4, Math.min(100, progress.percent))}%` }}
                />
              </div>
            </div>
          ) : null}

          {infoMessage ? (
            <div className="od-banner is-ok" style={{ marginTop: 12 }}>
              <span>{infoMessage}</span>
            </div>
          ) : null}

          {error ? <div className="ag-token-usage__error">{error}</div> : null}
        </div>

        <div className="agx-modal-foot" style={{ justifyContent: "space-between" }}>
          <button
            type="button"
            className="agx-btn agx-btn--secondary"
            disabled={installing}
            onClick={() => void openExternalUrl(update.releaseUrl)}
          >
            <ExternalLink size={14} />
            <span>{vi ? "Tải thủ công (GitHub)" : "Manual Download"}</span>
          </button>

          <button
            type="button"
            className="agx-btn agx-btn--primary"
            disabled={installing}
            onClick={() => void handleInstall()}
          >
            {installing ? (
              <Loader2 size={14} className="spin" />
            ) : (
              <Download size={14} />
            )}
            <span>
              {installing
                ? vi
                  ? "Đang tải & cài đặt..."
                  : "Downloading & Installing..."
                : vi
                  ? `Cập nhật lên v${update.latestVersion} ngay`
                  : `Update to v${update.latestVersion} Now`}
            </span>
          </button>
        </div>
      </div>
    </div>
  );
}
