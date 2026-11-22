# OnlyDEVS — Antigravity (`OnlyDEVS-Antigravity.exe`)

Control Suite (Tauri 2 + React 19 + Rust) for **Antigravity** — built by **OnlyDEVS**.

- **Website:** https://onlydevs.online/
- **Discord Server:** https://discord.gg/kEdMcjnftp
- **Releases:** https://github.com/Khoahihi1/OnlyDEVS-Antigravity/releases

## Repository Layout

- `build.bat` — One-click Windows build script (auto checks Node/pnpm/Rust, runs typecheck, builds `.exe` + NSIS `-setup.exe`).
- `dev.bat` — Launch Tauri dev mode from repository root.
- `antigravity-x/` — Tauri 2 + React 19 + Rust application source.
- `plugin/` — Source of `antigravity-filter-bypass` plugin (also embedded directly inside the compiled binary).
- `Prompt/` — Curated prompt templates synced into `~/.gemini/config/AGENTS.md`.

## Features

- **Account & Session Token Usage:**
  - **Account Overview:** Google OAuth 2.0 sign-in, local account sync, Refresh Token / Batch / JSON & folder import/export, live Cloud Code quota buckets (`Gemini · 5h`, `Gemini · Weekly`, `Claude · 5h`, `Claude · Weekly`, `Gemini 4 Argon`, `Gemini 4 Carbon`), 1-click `+ API / API ✓` pool toggle per account, Launch/Switch popup (auto-detects installed targets, graceful restart + full credential sync), Account Groups, Custom Sort, Tags, Privacy Mode, Notes & live 2FA TOTP Vault, Switch History.
  - **Session Token Usage:** Read-only local scanner separating Thinking vs Output tokens, interactive SVG trend chart, model/workspace/type cross-filtering, conversation table & per-turn inspector.
- **API Service (Local / LAN Router):**
  - OpenAI (`/v1/chat/completions`, `/v1/models`) & Anthropic (`/v1/messages`) compatible local/LAN router with streaming & non-streaming support.
  - Multi-account pooling (`highest_quota`, `round_robin`, `sticky_session`, `failover`), automatic OAuth refresh on 401, and automatic cooldown recovery on 429/5xx.
  - Multi API Key management (Master Key + scoped keys), custom model alias rules, copy-ready setup snippets, built-in live playground, request logs inspector, and dedicated API usage analytics.
- **Plugin:** Built-in `antigravity-filter-bypass v1.3.1` toggle, self-test runner, `hooks.json` & `config.json` editors, plus Folder / `.ZIP` / Git URL / Custom Manifest installer.
- **Prompt:** Instruction prompt manager synced with `Prompt/`, writing enabled content directly to `~/.gemini/config/AGENTS.md`.
- **Session:** Session scanner, transcript viewer, sync, Markdown export, batch delete.
- **MCP / Skill:** Manage `~/.gemini/config/mcp_config.json` servers and Agent Skills (`SKILL.md`).
- **Auto-Update & Themes:** GitHub Releases auto-updater with live download progress + NSIS installer handoff, 8 languages (`VI`, `EN`, `简体中文`, `繁體中文`, `JA`, `KO`, `RU`, `DE`), and 7 color themes (`OnlyDEVS`, `Nord`, `Tokyo Night`, `Catppuccin`, `Gruvbox`, `Everforest`, `OLED Pure`) in Dark & Light modes.

## Build From Source (Windows)

Double-click `build.bat` at the repository root, or run:

```bat
build.bat
```

Output binaries:
- `antigravity-x\src-tauri\target\release\OnlyDEVS-Antigravity.exe`
- `antigravity-x\src-tauri\target\release\bundle\nsis\OnlyDEVS-Antigravity_<version>_x64-setup.exe`
