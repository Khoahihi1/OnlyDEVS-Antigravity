# OnlyDEVS — Antigravity (`OnlyDEVS-Antigravity.exe`)

Desktop Control Suite (Tauri 2 + React 19 + Rust) for **Antigravity IDE & CLI** — built by **OnlyDEVS**.

- **Website:** https://onlydevs.online/
- **Discord Server:** https://discord.gg/kEdMcjnftp
- **Releases:** https://github.com/Khoahihi1/OnlyDEVS-Antigravity/releases

## Features

- **Account (Cockpit Parity — 2 Tabs):**
  - **Account Overview:** Google OAuth 2.0 sign-in, Local Client auto-import (Windows Credential Manager + `.gemini` + `state.vscdb`), Cockpit Tools import, Refresh Token / Batch / JSON import & export, live Cloud Code Quota buckets (`Gemini · 5h`, `Gemini · Weekly`, `Claude · 5h`, `Claude · Weekly`, `Gemini 4 Argon (Weekly)`, `Gemini 4 Carbon (Weekly)`), 1-click account switcher, Account Groups, Custom Sort, Tags, Privacy Mode, Notes & live 2FA TOTP Vault, Switch History.
  - **Session Token Usage:** Read-only local scanner (`~/.gemini/antigravity` SQLite `steps`/`gen_metadata` with WAL detection, RPC cache, transcript JSONL) separating Thinking vs Output tokens, interactive smooth SVG trend chart, model/workspace/type cross-filtering, conversation table & per-turn inspector.
- **Plugin:** Hardcoded built-in `antigravity-filter-bypass v1.3.1` toggle (zero external source dependency), self-test runner, `hooks.json` & `config.json` editors, plus multi-method installer (Folder / `.ZIP` / Git URL / Custom Manifest).
- **Prompt:** Instruction prompt manager synced with `Prompt/` (`KhoaHihi`, `Gemini-4-Argon`, `Gemini-4-Carbon`, `Gemini-Hydrogen`), writing enabled content directly to `~/.gemini/config/AGENTS.md` without renaming `AGENTS.md`.
- **Session:** IDE ↔ CLI session scanner, transcript viewer, 2-way sync, Markdown export, batch delete.
- **MCP / Skill:** Manage `~/.gemini/config/mcp_config.json` servers (with SQLite stash) and Agent Skills (`SKILL.md`).
- **Auto-Update & Themes:** Cockpit-style automatic GitHub Releases updater (download progress + 1-click NSIS setup launch), 8 languages (`VI`, `EN`, `简体中文`, `繁體中文`, `JA`, `KO`, `RU`, `DE`), and 7 Cockpit theme palettes (`Cockpit Ocean`, `Nord`, `Tokyo Night`, `Catppuccin`, `Gruvbox`, `Everforest`, `OLED Pure`) in Dark & Light modes.

## Build From Source

```bat
cd antigravity-x
scripts\build.bat
```
