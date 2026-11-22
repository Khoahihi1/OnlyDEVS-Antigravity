# onlydevs-filter-bypass

Antigravity lifecycle plugin by **OnlyDEVS**: keep the agent working through provider filter interruptions until the task ships (or the user cancels).

Includes **SessionStart**, **PreInvocation**, **PostInvocation**, and **Stop** hooks with expanded detection and an **EXECUTION CONTINUITY** directive.

---

## Repository layout (source)

| File | Role |
|------|------|
| `filter_bypass.py` | Hook logic (`session` / `stop` / `pre` / `post` argv) |
| `hooks.json` | Portable hook registration (python on PATH) |
| `plugin.json` | Antigravity plugin metadata (`onlydevs-filter-bypass`) |
| `test_filter_bypass.py` | Local tests |
| `scripts/install.ps1` | Windows install -> `~\.gemini\config\plugins\onlydevs-filter-bypass` |
| `scripts/install.sh` | macOS/Linux install |

---

## Install (Windows)

```powershell
cd C:\path\to\OnlyDEVS-Antigravity\plugin
.\scripts\install.ps1
```

Custom destination or Python:

```powershell
.\scripts\install.ps1 -Dest "$env:USERPROFILE\.gemini\config\plugins\onlydevs-filter-bypass" `
  -PythonExe "$env:LOCALAPPDATA\Programs\Python\Python312\python.exe"
```

Or enable it with 1 click directly from **OnlyDEVS — Antigravity** (`Plugins` tab).

## Install (macOS / Linux)

```bash
cd ~/OnlyDEVS-Antigravity/plugin
chmod +x scripts/install.sh
./scripts/install.sh
```

---

## How it works

| Hook | When | Action |
|------|------|--------|
| **PostInvocation** | After model returns | If output matches filter signatures -> `force_continue` + inject directive + tool mandate |
| **PreInvocation** | Before next model call | If filter stall / continuation -> inject compact **TOOLS FIRST** mandate |
| **Stop** | Agent tries to end turn | If filter detected -> `decision: continue` + full directive; else `allow` |

Detection uses encoded + plain regexes. **Does not** `continue` on `USER_CANCELED`.

---

## Develop / test

```bash
python test_filter_bypass.py
# or
python filter_bypass.py stop   # stdin: JSON hook payload
```
