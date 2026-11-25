# antigravity-filter-bypass

Antigravity lifecycle plugin: keep the agent working through **Gemini / provider filter cards** until the user task is actually done (or the user cancels).

Fork of [v1ktorrr0x/antigravity-filter-bypass](https://github.com/v1ktorrr0x/antigravity-filter-bypass) with expanded filter detection and a stronger **EXECUTION CONTINUITY** directive.

**Scope (v1.4+):** **Stop hook only** — Antigravity runs the plugin when the agent tries to end a turn; if the last output is a Gemini filter / refusal, return `continue` + directive. Normal turns are untouched (no Pre/Post/Session hooks).

---

## Repository layout (source)

| File | Role |
|------|------|
| `filter_bypass.py` | Hook logic (`stop` / `pre` / `post` argv) |
| `hooks.json` | Portable hook registration (python on PATH) |
| `plugin.json` | Antigravity plugin metadata |
| `test_filter_bypass.py` | Local tests |
| `scripts/install.ps1` | Windows install → `~\.gemini\config\plugins\…` |
| `scripts/install.sh` | macOS/Linux install |

---

## Install (Windows)

```powershell
git clone <this-repo-url> C:\path\to\antigravity-filter-bypass
cd C:\path\to\antigravity-filter-bypass
.\scripts\install.ps1
```

Custom destination or Python:

```powershell
.\scripts\install.ps1 -Dest "$env:USERPROFILE\.gemini\config\plugins\antigravity-filter-bypass" `
  -PythonExe "$env:LOCALAPPDATA\Programs\Python\Python312\python.exe"
```

Then **enable** the plugin in Antigravity (`agy plugin enable antigravity-filter-bypass` if needed) and **Reload Window**.

## Install (macOS / Linux)

```bash
git clone <this-repo-url> ~/antigravity-filter-bypass
cd ~/antigravity-filter-bypass
chmod +x scripts/install.sh
./scripts/install.sh
```

---

## How it works

| Hook | When | Action |
|------|------|--------|
| **Stop** | Agent tries to end turn | If filter/refusal detected → `decision: continue` + directive; else `allow` |

`pre` / `post` / `session` phases still exist in `filter_bypass.py` for tests; they are **not** registered in `hooks.json`.

Detection uses encoded + plain regexes (Gemini filter card, policy URLs, common refuse lines). **Does not** `continue` on `USER_CANCELED`.

---

## Develop / test

```bash
python test_filter_bypass.py
# or
python filter_bypass.py stop   # stdin: JSON hook payload
```

---

## License

Same spirit as upstream — use at your own risk; lab / private tooling only.