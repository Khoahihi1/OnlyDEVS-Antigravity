import React, { type ReactNode } from "react";
import {
  Blocks,
  Cpu,
  Globe,
  History,
  Info,
  Languages,
  MessageSquare,
  Moon,
  Palette,
  Sparkles,
  Sun,
  TerminalSquare,
  Users,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import type { AppLanguage, AppTab, AppTheme, AppThemeColor } from "../types";
import { openExternalUrl } from "../platform/invoke";
import { COCKPIT_THEME_PACKS, SUPPORTED_LANGUAGES, tr } from "../i18n";
import logoUrl from "../assets/onlydevs-logo.png";

type NavItem = {
  id: AppTab;
  icon: LucideIcon;
  labelKey: string;
};

const NAV_ITEMS: readonly NavItem[] = [
  { id: "accounts", icon: Users, labelKey: "nav.accounts" },
  { id: "plugins", icon: Blocks, labelKey: "nav.plugins" },
  { id: "prompts", icon: Sparkles, labelKey: "nav.prompts" },
  { id: "sessions", icon: History, labelKey: "nav.sessions" },
  { id: "mcp-skills", icon: Cpu, labelKey: "nav.mcpSkills" },
  { id: "about", icon: Info, labelKey: "nav.about" },
] as const;

export type AppShellProps = {
  activeTab: AppTab;
  onTabChange: (tab: AppTab) => void;
  lang: AppLanguage;
  onLangChange: (lang: AppLanguage) => void;
  theme: AppTheme;
  onToggleTheme: () => void;
  themeColor: AppThemeColor;
  onThemeColorChange: (color: AppThemeColor) => void;
  appVersion: string;
  runtimeLabel: string;
  badges?: Partial<Record<AppTab, string | number | null>>;
  children: ReactNode;
};

export function AppShell({
  activeTab,
  onTabChange,
  lang,
  onLangChange,
  theme,
  onToggleTheme,
  themeColor,
  onThemeColorChange,
  appVersion,
  runtimeLabel,
  badges,
  children,
}: AppShellProps) {
  const themeLabel = tr(lang, "shell.appearance");
  const themeValue = theme === "dark" ? tr(lang, "shell.dark") : tr(lang, "shell.light");
  const ThemeIcon = theme === "dark" ? Moon : Sun;
  const activePack =
    COCKPIT_THEME_PACKS.find((p) => p.id === themeColor) ?? COCKPIT_THEME_PACKS[0];

  return (
    <div className="agx-app-shell">
      <aside className="agx-sidebar">
        <div className="agx-brand">
          <div className="agx-brand-logo-wrap">
            <img src={logoUrl} alt="OnlyDEVS" className="agx-brand-logo" />
          </div>
          <div className="agx-brand-copy">
            <div className="agx-brand-title-row">
              <h1>OnlyDEVS</h1>
              <span className="agx-app-version">v{appVersion}</span>
            </div>
            <p>Antigravity Control Suite</p>
          </div>
        </div>

        <nav className="agx-sidebar-nav" aria-label="Main navigation">
          {NAV_ITEMS.map((item) => {
            const Icon = item.icon;
            const isActive = activeTab === item.id;
            const badge = badges?.[item.id];
            return (
              <button
                key={item.id}
                type="button"
                className={`agx-nav-item${isActive ? " agx-nav-item--active" : ""}`}
                onClick={() => React.startTransition(() => onTabChange(item.id))}
                aria-current={isActive ? "page" : undefined}
              >
                <span className="agx-nav-active-mark" aria-hidden="true" />
                <Icon size={18} strokeWidth={1.95} aria-hidden="true" />
                <span className="agx-nav-label">{tr(lang, item.labelKey)}</span>
                {badge !== undefined && badge !== null && badge !== "" && (
                  <span className={`agx-nav-badge${isActive ? " agx-nav-badge--active" : ""}`}>
                    {badge}
                  </span>
                )}
              </button>
            );
          })}
        </nav>

        <div className="agx-sidebar-footer">
          <div className="agx-community-strip">
            <button
              type="button"
              className="agx-community-pill"
              onClick={() => void openExternalUrl("https://onlydevs.online/")}
              title="https://onlydevs.online/"
            >
              <Globe size={13} />
              <span>onlydevs.online</span>
            </button>
            <button
              type="button"
              className="agx-community-pill agx-community-pill--discord"
              onClick={() => void openExternalUrl("https://discord.gg/kEdMcjnftp")}
              title="https://discord.gg/kEdMcjnftp"
            >
              <MessageSquare size={13} />
              <span>Discord</span>
            </button>
          </div>

          <div className="agx-codex-version" title={runtimeLabel}>
            <TerminalSquare size={17} strokeWidth={1.85} aria-hidden="true" />
            <span>
              <small>{tr(lang, "shell.runtime")}</small>
              <strong>{runtimeLabel}</strong>
            </span>
          </div>

          {/* Cockpit Theme Color Pack + Language Selectors */}
          <div className="agx-footer-pickers">
            <label className="agx-footer-select-wrap" title={tr(lang, "shell.themeColor")}>
              <Palette size={14} className="agx-footer-select-icon" />
              <span
                className="agx-theme-dot"
                style={{
                  background: `linear-gradient(135deg, ${activePack.primary}, ${activePack.accent})`,
                }}
              />
              <select
                className="agx-footer-select"
                value={themeColor}
                onChange={(e) => onThemeColorChange(e.target.value as AppThemeColor)}
                aria-label={tr(lang, "shell.themeColor")}
              >
                {COCKPIT_THEME_PACKS.map((pack) => (
                  <option key={pack.id} value={pack.id}>
                    {pack.label}
                  </option>
                ))}
              </select>
            </label>

            <label className="agx-footer-select-wrap" title={tr(lang, "shell.language")}>
              <Languages size={14} className="agx-footer-select-icon" />
              <select
                className="agx-footer-select"
                value={lang}
                onChange={(e) => onLangChange(e.target.value as AppLanguage)}
                aria-label={tr(lang, "shell.language")}
              >
                {SUPPORTED_LANGUAGES.map((l) => (
                  <option key={l.code} value={l.code}>
                    {l.nativeLabel}
                  </option>
                ))}
              </select>
            </label>
          </div>

          <div className="agx-footer-controls-row">
            <button
              type="button"
              className="agx-theme-button"
              onClick={onToggleTheme}
              role="switch"
              aria-checked={theme === "dark"}
            >
              <ThemeIcon size={16} strokeWidth={1.9} aria-hidden="true" />
              <span className="agx-theme-copy">
                <span>{themeLabel}</span>
                <strong>{themeValue}</strong>
              </span>
              <span className="agx-theme-switch" aria-hidden="true">
                <span className="agx-theme-switch-thumb" />
              </span>
            </button>
          </div>
        </div>
      </aside>

      <main className="agx-app-main">
        <div className="agx-app-content">
          <div className="agx-app-content-inner">{children}</div>
        </div>
      </main>
    </div>
  );
}
