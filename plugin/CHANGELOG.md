# Changelog

## 1.4.0

- **Stop-only hooks** (`hooks.json`): removed SessionStart, PreInvocation, PostInvocation so Antigravity does not run hooks every model turn.
- Recovery inject + `continue` only when Stop sees filter/refusal (or `MAX_FORCED_INVOCATIONS` still blocked).

## 1.3.2

- **Stop glue loop fix**: `force_continue` / Pre inject only on filter, refusal, or real stall — not every open work order.
- **PreInvocation**: no longer injects the full directive on every turn while a task is pending.
- **Anti-loop** hint when identical tool output repeats 3+ times (stops IDA/MCP discovery spam).

## 1.3.0

- **Death-march**: while user order is open → Stop/Post/Pre keep forcing work (no retry cap in plugin).
- **Done** only when tools ran + deliverable markers in reply (not “done” words alone).
- Refusal/filter + idle text after user message → always `continue`.
- Pre injects full directive whenever order still open.

## 1.2.1

- **Resume target** block from last user message (paths like `ac_bypass.cpp`).
- Detect **soft refusal** (not only Gemini filter card) when task still pending.
- **Task still pending** heuristic: continue if no tool progress after user order.
- Mandate names **first file** from user message on stall.
- Allow stop when deliverable-style completion or explicit done/complete reply.

## 1.2.0

- **PreInvocation** / **PostInvocation** hooks (in addition to **Stop**).
- **PostInvocation**: `force_continue` when the latest model step is a provider filter card.
- **PreInvocation**: escalating **TOOL MANDATE** when filter-only replies repeat (no max retry cap in the plugin).
- Expanded filter signatures (policy URLs, rephrase prompts).
- Stop continues on filter unless `USER_CANCELED`.
- Portable `hooks.json` (`python` / `python3` on PATH).
- `scripts/install.ps1` / `scripts/install.sh` for installing into `~/.gemini/config/plugins/`.

## 1.1.x (fork baseline)

- Stop hook + multiline filter detection (upstream v1ktorrr0x).