# OnlyDEVS-Antigravity development rules

- Follow root `Agent.MD`: update `skills/` or `rules/` before action when requirements or workflows change.
- App identity:
  - Display name: **OnlyDEVS** (`OnlyDEVS — Antigravity`)
  - Binary executable name: `OnlyDEVS-Antigravity.exe`
  - Main logo: uploaded OnlyDEVS circle logo (`src/assets/onlydevs-logo.png`, `public/onlydevs-logo.png`, `src-tauri/icons/*`).
- Navigation scope (6 core views):
  1. **Account** — Full Antigravity account manager ported from `cockpit-tools-1.4.2`: Google OAuth login (browser listener + manual callback URL), import current local account (Windows Credential Manager `gemini:antigravity`, `~/.gemini/jetski-standalone-oauth-token`, `~/.gemini/antigravity-cli/antigravity-oauth-token`, `state.vscdb`), import refresh tokens / delimited lines (`email----pass----2fa----token`) / JSON / old tools, export JSON, real Cloud Code quota & subscription tier & credit & model bucket fetching (`loadCodeAssist`, `fetchAvailableModels`, `retrieveUserQuotaSummary`), 1-click switch account across IDE + CLI + Credential Manager + `state.vscdb` protobuf injection, switch history, tags, notes vault with live TOTP 2FA generator, privacy masking, and token usage analytics.
  2. **Plugin** — Default built-in toggleable plugin is `antigravity-filter-bypass` (source: `C:\Users\KhoaHihi\Desktop\KhoaHihi\OnlyDEVS-Antigravity\antigravity-filter-bypass-v1.3.1-source` + embedded fallback; toggling ON auto-installs if needed and enables in `config.json`). Also allow adding plugins via multiple methods: (1) Local Folder path, (2) `.zip` archive upload/path, (3) Git repository URL (`git clone`), and (4) Plugin ID / Custom `plugin.json` + `hooks.json` creator.
  2. **Prompt** — Codex-X-inspired card grid with category tabs, `Keep existing (Append)` vs `Replace existing` activation methods, instant sync to `~/.gemini/config/AGENTS.md`, `.md` file import, template seeding, and direct `AGENTS.md` editor.
  3. **Session** — Scan `~/.gemini/antigravity/brain` (IDE) and `~/.gemini/antigravity-cli/brain` (CLI) + `annotations/*.pbtxt`, search/filter, folder grouping, transcript inspection modal, sync IDE ↔ CLI sessions, export `.md`/`.json`, and batch delete.
  4. **MCP/Skill** — Sub-tabs for MCP Servers (`~/.gemini/config/mcp_config.json` + tool schema count from `~/.gemini/antigravity*/mcp/*` + enable/disable stash + add/edit server) and Skills (`~/.gemini/config/skills/*/SKILL.md` + builtin skills + create/edit/toggle skill).
  5. **About** — OnlyDEVS branding, system diagnostics, and official links:
     - My website: `https://onlydevs.online/`
     - My server: `https://discord.gg/kEdMcjnftp`
- Do not commit secrets or API keys from `~/.gemini`.
- Support Vietnamese (`vi`) and English (`en`) plus Light & Dark luxury themes in the sidebar footer.
