import type { AntigravityAccount, AppLanguage, ModelQuota } from "../types";

export type AgQuotaDisplayItem = {
  key: string;
  label: string;
  percentage: number;
  resetTime: string;
  stale?: boolean;
  blockedByWeekly?: boolean;
};

const PAID_BUCKET_WINDOWS: ReadonlyArray<{ key: string; label: string }> = [
  { key: "gemini:5h", label: "Gemini · 5h" },
  { key: "gemini:weekly", label: "Gemini · Weekly" },
  { key: "claude:5h", label: "Claude · 5h" },
  { key: "claude:weekly", label: "Claude · Weekly" },
];

const FREE_BUCKET_WINDOWS: ReadonlyArray<{ key: string; label: string }> = [
  { key: "gemini:weekly", label: "Gemini · Weekly" },
  { key: "claude:weekly", label: "Claude · Weekly" },
];

const BUCKET_KEYS = new Set(PAID_BUCKET_WINDOWS.map((w) => w.key));

export function normalizeTierBadge(tier?: string | null): "ULTRA" | "PRO" | "FREE" {
  const t = (tier || "").toLowerCase();
  if (t.includes("ultra")) return "ULTRA";
  if (t.includes("pro") || t.includes("g1") || t.includes("paid") || t.includes("plus")) {
    return "PRO";
  }
  return "FREE";
}

export function getQuotaTone(pct: number): "high" | "medium" | "low" {
  if (pct >= 60) return "high";
  if (pct >= 20) return "medium";
  return "low";
}

export function formatResetCountdown(resetTime: string, vi: boolean): string {
  if (!resetTime) return "";
  const ts = Date.parse(resetTime);
  if (Number.isNaN(ts)) return "";
  const diffSec = Math.floor((ts - Date.now()) / 1000);
  if (diffSec <= 0) return vi ? "Đã reset" : "Reset";
  const mins = Math.floor(diffSec / 60);
  if (mins < 60) return `${mins}m`;
  const hrs = Math.floor(mins / 60);
  const remM = mins % 60;
  if (hrs < 24) return `${hrs}h ${remM}m`;
  const days = Math.floor(hrs / 24);
  const remH = hrs % 24;
  if (remM > 0) return `${days}d ${remH}h ${remM}m`;
  return `${days}d ${remH}h`;
}

export function isAccountNeedsReauth(account: AntigravityAccount): boolean {
  const reason = account.disabledReason || "";
  if (reason === "verification_required" || reason.startsWith("invalid_grant")) {
    return true;
  }
  const qErr = account.quotaError;
  if (qErr?.reason === "VALIDATION_REQUIRED" || Boolean(qErr?.validationUrl)) {
    return true;
  }
  if (
    qErr?.message &&
    (qErr.message.includes("Verify your account") ||
      qErr.message.includes("VALIDATION_REQUIRED") ||
      qErr.message.includes("invalid_grant"))
  ) {
    return true;
  }
  return false;
}

export function getAntigravityQuotaDisplayItems(
  account: AntigravityAccount,
): AgQuotaDisplayItem[] {
  const models: ModelQuota[] = account.quota?.models || [];
  const tier = normalizeTierBadge(account.quota?.subscriptionTier);
  const isFree = tier === "FREE";
  const stale = account.quota?.quotaSummaryStale || undefined;
  const legacyModels =
    !stale && account.quota?.quotaSummaryUpdatedAt == null ? models : [];
  const result: AgQuotaDisplayItem[] = [];

  const isSharedGemini = (rawName: string) => {
    const name = rawName.toLowerCase();
    return (
      name.includes("gemini") && !name.includes("argon") && !name.includes("carbon")
    );
  };

  let claude5h: ModelQuota | undefined;
  let claudeWeekly: ModelQuota | undefined;
  let gemini5h: ModelQuota | undefined;
  let geminiWeekly: ModelQuota | undefined;

  const argonWeekly = models.find(
    ({ name }) =>
      name === "argon:weekly" ||
      (name.startsWith("gemini-argon-") && name.endsWith("-weekly")),
  );
  const carbonWeekly = models.find(
    ({ name }) => name === "carbon:weekly" || name === "gemini-carbon-wtu-weekly",
  );

  if (isFree) {
    claudeWeekly =
      models.find((m) => m.name === "3p-weekly" || m.name === "claude:weekly") ||
      legacyModels.find((m) => m.name.toLowerCase().includes("claude"));
    geminiWeekly =
      models.find((m) => m.name === "gemini-weekly" || m.name === "gemini:weekly") ||
      legacyModels.find((m) => isSharedGemini(m.name));
  } else {
    claude5h =
      models.find((m) => m.name === "3p-5h" || m.name === "claude:5h") ||
      legacyModels.find((m) => {
        const n = m.name.toLowerCase();
        return n.includes("claude") && (n.includes("high") || !n.includes("low"));
      }) ||
      legacyModels.find((m) => m.name.toLowerCase().includes("claude"));

    claudeWeekly =
      models.find((m) => m.name === "3p-weekly" || m.name === "claude:weekly") ||
      legacyModels.find((m) => {
        const n = m.name.toLowerCase();
        return n.includes("claude") && n.includes("low");
      });

    gemini5h =
      models.find((m) => m.name === "gemini-5h" || m.name === "gemini:5h") ||
      legacyModels.find((m) => {
        const n = m.name.toLowerCase();
        return isSharedGemini(n) && n.includes("pro") && n.includes("high");
      }) ||
      legacyModels.find((m) => {
        const n = m.name.toLowerCase();
        return isSharedGemini(n) && n.includes("high");
      }) ||
      legacyModels.find((m) => {
        const n = m.name.toLowerCase();
        return isSharedGemini(n) && n.includes("flash");
      }) ||
      legacyModels.find((m) => {
        const n = m.name.toLowerCase();
        return isSharedGemini(n) && !n.includes("low");
      }) ||
      legacyModels.find((m) => isSharedGemini(m.name));

    geminiWeekly =
      models.find((m) => m.name === "gemini-weekly" || m.name === "gemini:weekly") ||
      legacyModels.find((m) => {
        const n = m.name.toLowerCase();
        return isSharedGemini(n) && n.includes("pro") && n.includes("low");
      }) ||
      legacyModels.find((m) => {
        const n = m.name.toLowerCase();
        return isSharedGemini(n) && n.includes("low");
      });
  }

  const claudeWeeklyResetAt = Date.parse(claudeWeekly?.resetTime ?? "");
  const claude5hBlocked =
    !isFree &&
    !stale &&
    claudeWeekly?.summaryExhausted === true &&
    (Number.isNaN(claudeWeeklyResetAt) || claudeWeeklyResetAt > Date.now());

  if (gemini5h) {
    result.push({
      key: "gemini:5h",
      label: "Gemini · 5h",
      percentage: gemini5h.percentage,
      resetTime: gemini5h.resetTime,
      stale,
    });
  }
  if (geminiWeekly) {
    result.push({
      key: "gemini:weekly",
      label: "Gemini · Weekly",
      percentage: geminiWeekly.percentage,
      resetTime: geminiWeekly.resetTime,
      stale,
    });
  }
  if (claude5h || claude5hBlocked) {
    result.push({
      key: "claude:5h",
      label: "Claude · 5h",
      percentage: claude5h?.percentage ?? 0,
      resetTime: claude5h?.resetTime ?? "",
      stale,
      blockedByWeekly: claude5hBlocked,
    });
  }
  if (claudeWeekly) {
    result.push({
      key: "claude:weekly",
      label: "Claude · Weekly",
      percentage: claudeWeekly.percentage,
      resetTime: claudeWeekly.resetTime,
      stale,
    });
  }
  if (argonWeekly) {
    result.push({
      key: "argon:weekly",
      label: "Gemini 4 Argon (Weekly)",
      percentage: argonWeekly.percentage,
      resetTime: argonWeekly.resetTime,
      stale,
    });
  }
  if (carbonWeekly) {
    result.push({
      key: "carbon:weekly",
      label: "Gemini 4 Carbon (Weekly)",
      percentage: carbonWeekly.percentage,
      resetTime: carbonWeekly.resetTime,
      stale,
    });
  }

  return result;
}

export function getAntigravityOverallScore(account: AntigravityAccount): number {
  const items = getAntigravityQuotaDisplayItems(account);
  if (items.length === 0) return -1;
  const total = items.reduce(
    (sum, item) => sum + (item.blockedByWeekly ? 0 : item.percentage),
    0,
  );
  return Math.round(total / items.length);
}

export function getQuotaBucketPercentage(
  account: AntigravityAccount,
  bucketKey: string,
): number {
  const items = getAntigravityQuotaDisplayItems(account);
  const found = items.find((i) => i.key === bucketKey);
  if (!found || found.blockedByWeekly) return -1;
  return found.percentage;
}

export function getEarliestResetMs(account: AntigravityAccount): number {
  const items = getAntigravityQuotaDisplayItems(account);
  let earliest = Number.MAX_SAFE_INTEGER;
  for (const item of items) {
    const ms = Date.parse(item.resetTime);
    if (!Number.isNaN(ms) && ms < earliest) {
      earliest = ms;
    }
  }
  return earliest;
}

type AntigravityQuotaSectionProps = {
  account: AntigravityAccount;
  lang: AppLanguage;
  isList?: boolean;
};

export function AntigravityQuotaSection({
  account,
  lang,
  isList = false,
}: AntigravityQuotaSectionProps) {
  const vi = lang === "vi";
  const items = getAntigravityQuotaDisplayItems(account);
  const needsReauth = isAccountNeedsReauth(account);

  if (items.length === 0) {
    return (
      <div className="ag-quota-empty">
        {vi
          ? "Chưa có dữ liệu Quota — bấm làm mới để kiểm tra"
          : "No quota data — click refresh to fetch"}
      </div>
    );
  }

  const hasBuckets = items.some((item) => BUCKET_KEYS.has(item.key));
  const hasAny5h = items.some((item) => item.key.endsWith(":5h"));
  const targetWindows = hasAny5h ? PAID_BUCKET_WINDOWS : FREE_BUCKET_WINDOWS;
  const extraItems = items.filter((item) => !BUCKET_KEYS.has(item.key));

  const renderCell = (
    key: string,
    label: string,
    item?: AgQuotaDisplayItem,
    fullWidth = false,
  ) => {
    const blockedReason = item?.blockedByWeekly
      ? vi
        ? "Đã chạm giới hạn tuần"
        : "Weekly limit reached"
      : "";
    const percentage = item?.blockedByWeekly ? undefined : item?.percentage;
    const tone = percentage == null ? "neutral" : getQuotaTone(percentage);
    const resetText =
      blockedReason ||
      (item?.resetTime ? formatResetCountdown(item.resetTime, vi) : "");

    return (
      <div
        key={key}
        className={`ag-quota-cell is-${tone}${fullWidth ? " ag-quota-cell--full" : ""}`}
      >
        <div className="ag-quota-cell-head">
          <span className="ag-quota-cell-label" title={label}>
            {label}
          </span>
          <span
            className={`ag-quota-cell-pct is-${tone}`}
            title={blockedReason || undefined}
          >
            {percentage == null ? "—" : `${percentage}%`}
          </span>
        </div>
        <div className="ag-quota-bar-track" aria-hidden="true">
          {percentage != null ? (
            <div
              className={`ag-quota-bar-fill is-${tone}`}
              style={{ width: `${Math.max(0, Math.min(100, percentage))}%` }}
            />
          ) : null}
        </div>
        {resetText ? (
          <div className="ag-quota-cell-reset" title={item?.resetTime || resetText}>
            {resetText}
          </div>
        ) : null}
      </div>
    );
  };

  return (
    <div className="ag-quota-section">
      {items.some((item) => item.stale) ? (
        <div
          className={`ag-quota-stale-note${needsReauth ? " is-reauth" : ""}`}
        >
          {needsReauth
            ? vi
              ? "Tài khoản cần xác minh web (đang hiển thị dữ liệu lần cuối)"
              : "Account requires web verification (showing last cached quota)"
            : vi
              ? "Đang hiển thị dữ liệu hạn mức lưu đệm"
              : "Showing cached quota snapshot"}
        </div>
      ) : null}
      <div className={`ag-quota-grid${isList ? " ag-quota-grid--list" : ""}`}>
        {hasBuckets
          ? targetWindows.map((w) =>
              renderCell(
                w.key,
                w.label,
                items.find((i) => i.key === w.key),
              ),
            )
          : null}
        {extraItems.map((item, idx) =>
          renderCell(
            item.key,
            item.label,
            item,
            extraItems.length % 2 === 1 && idx === extraItems.length - 1,
          ),
        )}
      </div>
    </div>
  );
}
