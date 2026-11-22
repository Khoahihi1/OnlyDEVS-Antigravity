import { useEffect, useState } from "react";
import {
  Download,
  ExternalLink,
  Loader2,
  Rocket,
  SkipForward,
  X,
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
  onClose,
  onSkipped,
}: UpdateNotificationProps) {
  const vi = lang === "vi";
  const [installing, setInstalling] = useState(false);
  const [progress, setProgress] = useState<UpdateProgressPayload | null>(null);
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

  if (!update) return null;

  const handleInstall = async () => {
    if (!update.downloadUrl) {
      await openExternalUrl(update.releaseUrl);
      return;
    }
    setInstalling(true);
    setError(null);
    setProgress({ downloaded: 0, total: update.assetSize, percent: 0, status: "downloading" });
    try {
      await agxInvoke<string>("download_and_install_update", {
        downloadUrl: update.downloadUrl,
        assetName: update.assetName ?? null,
        silent: false,
      });
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setInstalling(false);
    }
  };

  const handleSkip = async () => {
    try {
      await agxInvoke("patch_update_settings", {
        skippedVersion: update.latestVersion,
      });
      onSkipped(update.latestVersion);
    } finally {
      onClose();
    }
  };

  return (
    <div className="agx-modal-backdrop" onClick={() => !installing && onClose()}>
      <div className="agx-modal agx-modal--wide" onClick={(e) => e.stopPropagation()}>
        <div className="agx-modal-head">
          <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
            <Rocket size={18} style={{ color: "var(--agx-primary)" }} />
            <div>
              <h3>
                {vi
                  ? `Đã có phiên bản mới: v${update.latestVersion}`
                  : `New Version Available: v${update.latestVersion}`}
              </h3>
              <span className="agx-muted">
                v{update.currentVersion} → v{update.latestVersion}
                {update.assetName ? ` · ${update.assetName}` : ""}
                {update.assetSize ? ` (${formatBytes(update.assetSize)})` : ""}
              </span>
            </div>
          </div>
          {!installing ? (
            <button type="button" className="agx-icon-btn" onClick={onClose}>
              <X size={16} />
            </button>
          ) : null}
        </div>

        <div className="agx-modal-body">
          {update.releaseNotes ? (
            <div className="agx-update-notes">
              <pre>{update.releaseNotes}</pre>
            </div>
          ) : (
            <p className="agx-muted">
              {vi
                ? "Bản phát hành mới đã sẵn sàng trên GitHub Releases với các cải tiến và bản vá mới nhất."
                : "A new release is ready on GitHub Releases with latest improvements and fixes."}
            </p>
          )}

          {installing && progress ? (
            <div className="agx-update-progress">
              <div className="agx-update-progress-head">
                <span>
                  {progress.status === "installing"
                    ? vi
                      ? "Đang khởi chạy trình cài đặt Setup..."
                      : "Launching setup installer..."
                    : vi
                      ? "Đang tải gói cài đặt..."
                      : "Downloading installer..."}
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

          {error ? <div className="ag-token-usage__error">{error}</div> : null}
        </div>

        <div className="agx-modal-foot" style={{ justifyContent: "space-between" }}>
          <button
            type="button"
            className="agx-btn agx-btn--ghost"
            disabled={installing}
            onClick={() => void handleSkip()}
          >
            <SkipForward size={14} />
            <span>{vi ? "Bỏ qua bản này" : "Skip this version"}</span>
          </button>

          <div style={{ display: "flex", gap: 8 }}>
            <button
              type="button"
              className="agx-btn agx-btn--secondary"
              disabled={installing}
              onClick={() => void openExternalUrl(update.releaseUrl)}
            >
              <ExternalLink size={14} />
              <span>GitHub Releases</span>
            </button>
            <button
              type="button"
              className="agx-btn agx-btn--secondary"
              disabled={installing}
              onClick={onClose}
            >
              {vi ? "Để sau" : "Later"}
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
                    ? "Đang tải..."
                    : "Downloading..."
                  : vi
                    ? "Cập nhật ngay"
                    : "Update Now"}
              </span>
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
