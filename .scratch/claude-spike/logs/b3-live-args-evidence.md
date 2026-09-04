# B3 live: argument fidelity on Messages HTTP (after C1 fix) — **PASS**

**When:** 2026-09-03T20:53:37Z (server up) → 20:53:54Z (request dispatched) → 20:54:10Z (response complete); 16 s request wall
**Write request:** #8 (`write-request-8.md`), approved in-session
**Branch:** `spike/claude-b3-live-tools` @ `44908d95` (no uncommitted source; `.scratch/` untracked)
**Binary:** `./target/debug/gents` inode `44769203` (built from `44908d95`, not rebuilt for this run)
**Server:** PID `23667` `--home ~/.gents --tool-root <repo> --tool-ceiling readwrite --claude-config-dir <repo>/.scratch/claude-spike/claude-config --claude-write-approved`; stopped after the turn (`kill` exit 0, pid gone, `:9191` closed)
**Behavior:** `did:key:z6MkpEq4KsC2oibBmHe6faw1246CU5gmtVJJEYf3wvo1yKiQ:default`, model **`claude-sonnet-5`** (never changed; backend `…:claude-backend`, `claude_subscription.config_dir` = spike config dir)
**Session:** `5af08591-5eec-4d7d-bd3b-92b0a0922c3e` (fresh; title `list-files-tool-run-listed`)
**Request:** `d584afea-dbe3-4553-b1da-ba315cefb72a` (doc `bae-d7f5610e-9d4b-5f95-8c8c-15eb555ee0eb`)
**Prompt:** `Use the gents list_files tool on the directory "." (not Bash). After the tool returns, reply with exactly: listed`
**Response:** `listed` — `status=complete`, conversation `completed`, request `lifecycle_state=completed`

## Change under test

Task 1 (`44908d95`): `PendingTool { start_input, deltas }` — `tool_use` arguments are accumulated from `input_json_delta` events; duplicate ids and overlapping blocks fail closed. Wire, headers, body keys unchanged from #7 (`system[0]` Claude Code identity, `anthropic-beta: oauth-2025-04-20`, no sampling). #7 recorded `arguments: {}` (defect C1); this run tests that the live arguments now reach `AgentToolCall.args`.

## Bar

| Check | Result |
|---|---|
| HTTP status (no 4xx/429) | **none** — server stderr has no error/warn lines; both Messages turns streamed to completion; `gents chat` exit 0 |
| `AgentToolCall.args.path == "."` | **yes** — exactly one row: `tool_name=list_files`, `lifecycle_state=completed`, `args='{"path":"."}'` (parses; `path == "."`), id `Jd46QAbQR25h9e1nuggUx`, 346 ms, `returned_count: 27` |
| Response `listed` | **yes** — `status=complete` |
| Claude `OAuthCredential` rows | **0** — 1 row total, provider `xai-oauth` only (`b3-live-args-oauth.json`) |
| `:8787` closed | yes (before and after) |
| Token scan (`grep -l 'sk-ant\|Bearer ' b3-live-args-*`) | **clean** — no files listed, exit 1 |
| Default behavior model after run | `claude-sonnet-5` |

## Inference calls (from `trace timeline`)

| Scope | Turn | Wire | prompt / completion tokens | Outcome |
|---|---|---|---|---|
| `title.1` | 0 | process CLI (empty tools) | 2 / 13 | title `list-files-tool-run-listed` |
| `inference.1` | 0 | Messages HTTP | 9513 / 72 | `tool_use list_files {"path":"."}` |
| `inference.1` | 1 | Messages HTTP | 9927 / 4 | `listed` |

`cached_input_tokens` 0 on all calls (no `cache_control` sent — unchanged from #7). All three rendered requests captured (`provenance_status=captured_only`, `model_name=claude-sonnet-5`).

## Comparison with #7

Same prompt, same seat, same headers/body shape. #7: `AgentToolCall.args = {}` (C1). #8: `AgentToolCall.args = {"path":"."}`. Token counts nearly identical (9513/69→72, 9906→9927/4), consistent with only the argument accumulation changing. The `gents chat` client also echoed `[tool] list_files ({"path":"."})` live.

## Anomalies

None. `gents chat` stdout is the human-readable stream (tool lines + `listed`), not JSON, so the request id was taken from the server stderr `router: dispatching request` line rather than the brief's `grep -o '"request_id"'`.

Raw: `b3-live-args-toolcalls.json`, `b3-live-args-timeline.json`, `b3-live-args-oauth.json`
Logs: `b3-live-args-server.{stdout,stderr}.log`, `b3-live-args-chat.{stdout,stderr}.log`, `b3-live-args-server.pid`, `b3-live-args-{start,end}.ts`
