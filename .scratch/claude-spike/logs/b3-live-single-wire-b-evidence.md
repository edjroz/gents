# B3 live #10b: body + streaming on the single wire — **PASS**

**When:** 2026-09-04T07:57:38Z (server started) → 2026-09-04T07:57:54Z (request dispatched) → 2026-09-04T07:58:08Z (response complete); chat returned by 2026-09-04T07:58:09Z; ~14 s request wall
**Write request:** #10b (`write-request-10b.md`), approved by user /goal directive (re-run of #10 on the seat refreshed by #12)
**Branch:** `spike/claude-b3-live-tools` @ `9a92c586` (no source edits; `.scratch/` untracked)
**Binary:** `./target/debug/gents` inode `45229261` (`cargo build --bin gents` already up to date at `9a92c586`, 1.10 s)
**Server:** PID `56690` `--home ~/.gents --tool-root <repo> --tool-ceiling readwrite --claude-config-dir <repo>/.scratch/claude-spike/claude-config --claude-write-approved`; listening `127.0.0.1:9191`; stopped after the turn (`kill` exit 0, pid gone in 2 s, `:9191` and `:8787` closed)
**Behavior:** `did:key:z6MkpEq4KsC2oibBmHe6faw1246CU5gmtVJJEYf3wvo1yKiQ:default`, model `claude-sonnet-5` (unchanged; every capture's `model` is `claude-sonnet-5`)
**Session:** `cd8733bf-442b-48a7-bf09-0af3296aa46f` (fresh)
**Request:** `9423d21e-fffd-4114-bf13-4e24b89cad7d` (doc `bae-86c5d024-c51d-51d3-a836-498971be3035`)
**Prompt:** `Use the gents list_files tool on the directory "." (not Bash). After the tool returns, reply with exactly: listed`
**Response:** `listed` — response event `status=complete`, request `lifecycle_state=completed`; `gents chat` exit 0, stdout shows `[tool] list_files ({"path":"."})`, `[tool done] … returned_count 27`, then `listed`

## Change under test

Commits `39bacaf1..9a92c586` — single-wire cut: `system[] = [identity, preamble?, System rows…]` with `cache_control` on the last system block and the last content block; `tools` omitted when empty; incremental SSE (`MessagesSseState`); `message_stop` flushes a pending tool; `set_sensitive` on the auth header. **Exercised live**: 3 `RenderedRequest` captures at seam `transport_body`, endpoint `https://api.anthropic.com`, all with `stream: true`; 3 `inference_call` events, all `completed`.

## Bar

| Check | Result |
|---|---|
| `inference.1` turn 0: `system[0]` identity | **yes** — `"You are Claude Code, Anthropic's officia…"` |
| `system[1]` present | **yes** — `"You are a terminal-native engineering an…"` (behavior System row); `title.1` has `system[1]` = title preamble |
| no `messages[]` block starting `system: ` | **yes** — `leaked: false` on all 3 captures |
| `cache_control` on last system block and last content block | **yes** — `{"type":"ephemeral"}` on both, all 3 captures |
| `tools` present on `inference.1`, absent on `title.1` | **yes** — `inference.1` turns 0 and 1: `has_tools: true` (9 tools); `title.1`: `has_tools: false` |
| `cached_input_tokens` on `call_seq` 2 | **recorded: 0** (`call_seq` 2 = first `inference`-kind call, `prompt_tokens` 2, `completion_tokens` 60). The second `inference`-kind call, `call_seq` 3, records **9727** cached input tokens (`prompt_tokens` 2, `completion_tokens` 4) — the cache breakpoints hit on the follow-up |
| first assistant delta timestamp precedes response `completed_at` | **yes** — first assistant message `2026-09-04T07:58:03.415955Z` (tool_use block, before the tool ran at 07:58:01.9→02.27 … see note) < response event `2026-09-04T07:58:08.365945Z`. Request doc `completed_at` is `null`; the response event timestamp is used as completion, as in #10 |
| `AgentToolCall.args.path == "."` | **yes** — 1 row: `list_files`, `args {"path":"."}`, `lifecycle_state=completed` |
| response `listed`, `status=complete` | **yes** |
| HTTP 4xx/429 in server stderr | **0** — `grep -ciE 'HTTP (4[0-9][0-9])|rate_limit'` = 0; `grep -c ' 429 '` = 0; raw `grep -c 429` = 0 (no timestamp collision this run); 0 ERROR / 0 WARN lines in 16 stderr lines |
| Claude `OAuthCredential` rows | **0** — 1 row total, provider `xai-oauth` only (`b3-live-single-wire-b-oauth.json`) |
| Token scan (`grep -l 'sk-ant\|Bearer '` over `b3-live-single-wire-b-*`) | **clean** — no files listed, exit 1 |
| Server stopped, `:9191`/`:8787` closed | yes |
| Default model unchanged | yes (`claude-sonnet-5`) |

**Verdict: PASS.**

## Inference calls (from `trace timeline`)

| call_seq | attempt | kind | started → ended | state | cached_in | prompt | completion |
|---|---|---|---|---|---|---|---|
| 1 | 1 | title one-off | 07:57:55.03 → 07:57:58.16 | completed | 0 | 272 | 14 |
| 2 | 1 | inference (turn 0, tool_use) | 07:57:58.45 → 07:58:02.58 | completed | 0 | 2 | 60 |
| 3 | 1 | inference (turn 1, follow-up) | 07:58:04.35 → 07:58:06.71 | completed | 9727 | 2 | 4 |

Captures: `title.1` turn 0 (1 message, no tools), `inference.1` turn 0 (2 messages, 9 tools), `inference.1` turn 1 (4 messages, 9 tools); all `source=claude_cli_subscription`, `seam=transport_body`, `stream=true`.

## Anomalies / notes

- `prompt_tokens` is 2 on both inference calls while `cached_input_tokens` is 0 then 9727: the uncached input on the turn-0 call is presumably counted as cache *creation* tokens, which the timeline does not carry. Worth a follow-up on usage accounting, not a bar item.
- The "first assistant delta" is measured from the first persisted assistant `message` event (07:58:03.42), as in #10; it lands *after* the tool_call started/completed (07:58:01.91 → 07:58:02.27) and after `call_seq` 2 ended (07:58:02.58) — i.e. the assistant tool_use message is persisted once the tool is flushed at `message_stop`, not at first SSE delta. It still precedes completion by ~5 s. No per-delta timestamp is persisted, so this is the closest available proxy.
- Title generation succeeded (`generated conversation title` in server stderr); request doc `title` in the timeline is `null` (the title lives on the session).
- `claimed_at` / `started_at` / `completed_at` on the request doc are `null` in the timeline projection; lifecycle_state is `completed`.
- No source edits, no commits, no behavior/backend/credential documents touched; `.credentials.json` not opened; seat not touched.

Raw: `b3-live-single-wire-b-captures.json` (3 rows), `b3-live-single-wire-b-timeline.json`, `b3-live-single-wire-b-toolcalls.json` (1 row), `b3-live-single-wire-b-oauth.json`, `b3-live-single-wire-b-jq.txt`
Logs: `b3-live-single-wire-b-server.{stdout,stderr}.log`, `b3-live-single-wire-b-chat.{stdout,stderr}.log`, `b3-live-single-wire-b-server.pid`, `b3-live-single-wire-b-{start,end}.ts`
