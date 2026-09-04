# B3 live: body + streaming on the single wire — **FAIL** (seat auth expired; no request reached the provider)

**When:** 2026-09-04T02:11:47Z (server up) → 02:11:54Z (request dispatched) → 02:12:01Z (response errored); ~7 s request wall
**Write request:** #10 (`write-request-10.md`), approved by user /goal directive
**Branch:** `spike/claude-b3-live-tools` @ `39bacaf1` (no source edits; `.scratch/` untracked)
**Binary:** `./target/debug/gents` inode `44920977` (rebuilt from `39bacaf1`, 1m 27s)
**Server:** PID `4923` `--home ~/.gents --tool-root <repo> --tool-ceiling readwrite --claude-config-dir <repo>/.scratch/claude-spike/claude-config --claude-write-approved`; stopped after the turn (`kill` exit 0, pid gone in 1 s, `:9191` and `:8787` closed)
**Behavior:** `did:key:z6MkpEq4KsC2oibBmHe6faw1246CU5gmtVJJEYf3wvo1yKiQ:default`, model `claude-sonnet-5` (unchanged; not modified)
**Session:** `eef63725-536f-4231-bb42-dc51670f2eee` (fresh)
**Request:** `1c1681fe-c46b-4f87-8f6c-674a26da3611` (doc `bae-45520779-f11a-57e9-9efe-6db455f9394f`)
**Prompt:** `Use the gents list_files tool on the directory "." (not Bash). After the tool returns, reply with exactly: listed`
**Response:** none — `status=error`, request `lifecycle_state=failed`, `gents chat` exit 0 with `[agent error] … Claude Messages seat auth: Claude seat OAuth access token is expired; re-run gents claude-login`

## Change under test

Commit `39bacaf1` — single-wire cut: `system[]` with `cache_control` breakpoints, incremental SSE materialization, process-CLI wire deleted. **Not exercised**: every inference call failed closed at the seat-auth check (`claude_messages` seat auth) before any HTTP request was built or sent, so no `RenderedRequest` row was captured and nothing was billed.

## What happened

The spike seat's OAuth access token (in `.scratch/claude-spike/claude-config`, not read or printed by this run) had expired since #9 (~2.5 h earlier). The runtime refused to send with an expired token — the intended fail-closed behavior — and retried per its budget: title one-off `call_seq` 1–2 (attempts 1–2), then inference `call_seq` 3–4, all `call_state=failed` in ~0.3–0.6 s each; title fell back to `use-gents-list-files-tool`; response errored at 02:12:01.19. `gents chat` was run exactly once (no retry per the stop rule).

## Bar

| Check | Result |
|---|---|
| `inference.1` turn 0: `system[0]` identity | **not testable** — 0 `RenderedRequest` rows (captures count 0) |
| `system[1]` present | not testable (no capture) |
| no `messages[]` block starting `system: ` | not testable (no capture) |
| `cache_control` on last system block and last content block | not testable (no capture) |
| `tools` present on `inference.1`, absent on `title.1` | not testable (no capture) |
| `cached_input_tokens` on second inference call | **null** — all 4 `inference_call` events `failed`, no token counts |
| first assistant delta timestamp vs completion | **no assistant message** (`first` = null); response `status=error` at `2026-09-04T02:12:01.186776Z`, `completed_at` null |
| `AgentToolCall.args.path == "."` | **no rows** (toolcalls count 0) |
| response `listed` | **no** — error, no content |
| HTTP 4xx/429 in server stderr | **0 real** — raw `grep -ciE` = 2, both the substring `429` inside the deadline timestamp `…02:11:54.429977` (`HTTP 4xx` = 0, `rate_limit` = 0); no request was sent to Anthropic |
| Claude `OAuthCredential` rows | **0** — 1 row total, provider `xai-oauth` only (`b3-live-single-wire-oauth.json`) |
| Token scan (`grep -l 'sk-ant\|Bearer '` over `b3-live-single-wire-*`) | **clean** — no files listed, exit 1 |
| Server stopped, `:9191`/`:8787` closed | yes |

**Verdict: FAIL** (bar not met because the run could not exercise the change; not a defect in `39bacaf1` as far as this run shows). Plan stops for a user decision. The obvious remedy is a fresh `gents claude-login` on the spike seat followed by a new write request (#11) — not done here (credentials must not be touched by the agent).

## Inference calls (from `trace timeline`)

| call_seq | attempt | kind | started → ended | state |
|---|---|---|---|---|
| 1 | 1 | title one-off | 02:11:55.25 → 02:11:55.82 | failed (seat auth expired) |
| 2 | 2 | title one-off | 02:11:56.34 → 02:11:56.92 | failed |
| 3 | 1 | inference | 02:11:57.71 → 02:11:58.28 | failed |
| 4 | 1 | inference (retry) | 02:12:00.62 → 02:12:00.90 | failed |

## Anomalies

- The run is a pure environmental failure (expired spike seat token). No source edits, no commits, no behavior/backend/credential documents touched.
- The runtime's fail-closed seat-auth check worked as designed: it never sent a request with an expired token.
- `gents chat` exits 0 on an agent error (prints `[agent error] …` and an `[inspect]` hint); the request id came from the server stderr `router: dispatching request` line as in #8/#9.

Raw: `b3-live-single-wire-captures.json` (0 rows), `b3-live-single-wire-timeline.json`, `b3-live-single-wire-toolcalls.json` (0 rows), `b3-live-single-wire-oauth.json`, `b3-live-single-wire-jq.txt`
Logs: `b3-live-single-wire-server.{stdout,stderr}.log`, `b3-live-single-wire-chat.{stdout,stderr}.log`, `b3-live-single-wire-server.pid`, `b3-live-single-wire-{start,end}.ts`
