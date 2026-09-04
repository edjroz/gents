# Task 6 live report — #10b: body + streaming on the single wire (re-run on refreshed seat)

**Status:** DONE (run executed once, collected, server stopped) — **PASS**

## What ran

- Request file `.scratch/claude-spike/logs/write-request-10b.md`; preflight appended (inode, both lsof empty).
- Build: `cargo build --bin gents` at `9a92c586` already up to date (1.10 s); binary inode `45229261`. `:9191`/`:8787` closed before start.
- Server PID `56690`, same flags as #10, listening `127.0.0.1:9191` (started 2026-09-04T07:57:38Z).
- Exactly one `gents chat --home ~/.gents --timeout-secs 180 '<list_files prompt>'`; exit 0; stdout: `[tool] list_files ({"path":"."})`, `[tool done] … returned_count 27`, `listed`. Request `9423d21e-fffd-4114-bf13-4e24b89cad7d`, session `cd8733bf-442b-48a7-bf09-0af3296aa46f`, doc `bae-86c5d024-c51d-51d3-a836-498971be3035` (id from server stderr dispatch line).
- Collected: timeline (3 `inference_call` completed, 5 messages, 1 tool_call, response `status=complete` at 07:58:08.37Z), `RenderedRequest` 3 rows, `AgentToolCall` 1 row, `OAuthCredential` 1 row (`xai-oauth`), jq in `b3-live-single-wire-b-jq.txt`.
- `kill 56690` exit 0, pid gone in 2 s, `:9191`/`:8787` closed. Token scan `grep -l` listed nothing. Seat not touched; `.credentials.json` not opened.
- Evidence: `.scratch/claude-spike/logs/b3-live-single-wire-b-evidence.md`, marked PASS.

No source edits, nothing committed.

## Bar

| Check | Result |
|---|---|
| `system[0]` identity / `system[1]` present | yes / yes (`inference.1` turn 0: identity, then behavior System row; `title.1`: identity, then title preamble) |
| no `system: ` leak in `messages[]` | yes (`leaked: false` x3) |
| `cache_control` last system block / last content block | `{"type":"ephemeral"}` on both, all 3 captures |
| `tools` per scope | `inference.1`: present (9); `title.1`: absent |
| `cached_input_tokens` call_seq 2 | 0 (first inference call); call_seq 3 (second inference call) = 9727 |
| first assistant message vs completion | 07:58:03.416Z < response 07:58:08.366Z (request `completed_at` null in projection; response event used) |
| `AgentToolCall.args.path` | `"."`, `list_files`, completed |
| response | `listed`, `status=complete` |
| 4xx/429 | 0 (`HTTP 4xx|rate_limit` 0; ` 429 ` 0; raw `429` 0) |
| Claude `OAuthCredential` rows | 0 |
| token scan | clean |
| server stopped | yes |
| default model | `claude-sonnet-5`, unchanged |

## Notes

- `prompt_tokens` = 2 on both inference calls with `cached_input_tokens` 0 → 9727; cache-creation tokens are not carried in the timeline, so usage accounting looks under-reported on turn 0. Follow-up candidate, not a bar item.
- The first assistant message event (tool_use) is persisted at `message_stop` flush (after the tool ran), so "first delta" is a proxy; no per-delta timestamps are persisted.
