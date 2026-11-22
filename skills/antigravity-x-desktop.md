# OnlyDEVS-Antigravity desktop manager

## Identity & Binary

- Product Name: **OnlyDEVS**
- Executable: `OnlyDEVS-Antigravity.exe`
- Stack: Tauri 2 + Rust + React 19 + TypeScript + WebView2 (Windows)
- Official Links:
  - Website: `https://onlydevs.online/`
  - Discord Server: `https://discord.gg/kEdMcjnftp`

## Core Modules (6 Tabs)

0. **Account (`accounts.rs` / `AccountsPage.tsx`)**
   - Full Antigravity account management ported from `cockpit-tools-1.4.2`:
     - Live OAuth 2.0 login flow (local loopback callback listener + manual callback URL submission)
     - Import active local account from Windows Credential Manager (`gemini:antigravity`), `~/.gemini/jetski-standalone-oauth-token`, `~/.gemini/antigravity-cli/antigravity-oauth-token`, and `%APPDATA%/Antigravity*/User/globalStorage/state.vscdb`
     - Import/export via Refresh Token (`1//...`), delimited lines (`email----pass----2fa----token`), JSON array/files, and `~/.antigravity_tools` / `cockpit-tools`
     - Cloud Code quota engine (`loadCodeAssist`, `onboardUser`, `fetchAvailableModels`, `retrieveUserQuotaSummary`) returning subscription tier, AI credits, GCP ToS project, 5h/weekly quota buckets, per-model percentages, reset times, and Google validation URLs
     - 1-click Account Switcher writing to Windows Credential Manager (`CredWriteW`), `~/.gemini/jetski-standalone-oauth-token`, `~/.gemini/antigravity-cli/antigravity-oauth-token`, and IDE `state.vscdb` protobuf keys (`antigravityUnifiedStateSync.oauthToken`, `userStatus`, `enterprisePreferences`)
     - Account Tags, Notes Vault (password, aux email, phone, mail URL, 2FA secret with live 6-digit TOTP code generator), Switch History, Privacy Mode, Grid/List/Compact views, and Local Token Usage dashboard
1. **Plugin (`plugins.rs` / `PluginsPage.tsx`)**
   - Default built-in plugin: `antigravity-filter-bypass` bound to `C:\Users\KhoaHihi\Desktop\KhoaHihi\OnlyDEVS-Antigravity\antigravity-filter-bypass-v1.3.1-source` (with embedded fallback). Its toggle switch is always active: toggling ON auto-installs/repairs hooks and enables it in `~/.gemini/config/config.json`.
   - Multi-method Plugin Installer (`+ Add Plugin` modal):
     - **From Folder**: Copy any local plugin directory into `~/.gemini/config/plugins/<id>`
     - **From ZIP**: Import a `.zip` plugin package (file picker or path) and extract to `~/.gemini/config/plugins/<id>`
     - **From Git URL**: `git clone` repository directly into `~/.gemini/config/plugins/<id>`
     - **Create / Register by ID**: Register a plugin ID in `config.json` or scaffold a custom `plugin.json` + `hooks.json`
   - Run `test_filter_bypass.py` self-test and inspect `hooks.json` / `config.json`
2. **Prompt (`prompts.rs` / `PromptsPage.tsx`)**
   - SQLite-backed prompt library (`~/.antigravity-x/antigravity-x.db`) with category management & seeded templates
   - Activation methods: `Keep existing` (append managed block to `AGENTS.md`) and `Replace existing` (overwrite `AGENTS.md`)
   - `.md` import, card-level toggle switch, and live `AGENTS.md` drawer editor
3. **Session (`sessions.rs` / `SessionsPage.tsx`)**
   - Discover IDE (`~/.gemini/antigravity/brain`) and CLI (`~/.gemini/antigravity-cli/brain`) sessions + `annotations/*.pbtxt`
   - Parse `transcript.jsonl` for title, workspace, model, token/step count, and full conversation preview
   - Sync sessions between IDE and CLI, export selected sessions to Markdown, and delete selected sessions
4. **MCP/Skill (`mcp_skills.rs` / `McpSkillsPage.tsx`)**
   - Manage MCP servers in `~/.gemini/config/mcp_config.json` with enable/disable stash in SQLite, tool count discovery from `mcp/<server>/*.json`, custom notes, and add/edit server modal
   - Manage Skills in `~/.gemini/config/skills/*/SKILL.md` (create, edit, enable/disable via stash, inspect built-in skills)
5. **About (`AboutPage.tsx`)**
   - OnlyDEVS brand identity, `agy.version.json` runtime detection, environment paths, and direct links to `https://onlydevs.online/` and `https://discord.gg/kEdMcjnftp`

## Dev & Build

```powershell
cd antigravity-x
pnpm install
pnpm dev:web      # Vite browser preview
pnpm dev          # Tauri + WebView2 desktop
pnpm tauri build  # Builds OnlyDEVS-Antigravity.exe
```
