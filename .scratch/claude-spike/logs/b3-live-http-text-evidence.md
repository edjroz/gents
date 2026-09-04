# B3 live: text-only turn + title generation on Messages HTTP (single wire) — **PASS**

**When:** 2026-09-03T23:44:29Z (server up) → 23:44:57Z (request dispatched) → 23:45:07Z (response complete); ~10 s request wall
**Write request:** #9 (`write-request-9.md`), approved in-session
**Branch:** `spike/claude-b3-live-tools` @ `8ae310d0` (no source edits; `.scratch/` untracked)
**Binary:** `./target/debug/gents` inode `44847441` (built from `8ae310d0`, not rebuilt for this run)
**Server:** PID `60029` `--home ~/.gents --tool-root <repo> --tool-ceiling readwrite --claude-config-dir <repo>/.scratch/claude-spike/claude-config --claude-write-approved`; stopped after the turn (`kill` exit 0, pid gone, `:9191` and `:8787` closed)
**Behavior:** `did:key:z6MkpEq4KsC2oibBmHe6faw1246CU5gmtVJJEYf3wvo1yKiQ:default`, model **`claude-sonnet-5`** (never changed; both rendered requests `model_name=claude-sonnet-5`; backend `…:claude-backend`, `probe_status=healthy`)
**Session:** `c0beabd8-9c66-4fea-9b55-c815e7e4462a` (fresh)
**Request:** `30c42ee4-f579-4ebc-a180-a17ad6e0c8a9` (doc `bae-844dc05b-e9e0-5afd-91cb-5a8e2236cf00`)
**Prompt:** `Reply with exactly: pong`
**Response:** `pong` — `status=complete`, request `lifecycle_state=completed`, `gents chat` exit 0

## Change under test

Commit `8ae310d0`: `ClaudeSubscriptionModel::stream` routes every turn to `stream_messages` when no fake completer is installed (predicate `seat.fake_completer.is_none()` only); `build_messages_body` omits the `tools` key for an empty surface. Headers/body otherwise unchanged from #8 (`system[0]` identity block, `anthropic-beta: oauth-2025-04-20`, no sampling). #8 still sent `title.1` over the process CLI; this run tests that the text-only title call and the chat turn both take Messages HTTP.

## Bar

| Check | Result |
|---|---|
| Response `pong`, `status=complete` | **yes** — chat stdout is exactly `pong`; timeline `response` event `status=complete`; request `completed` |
| `RenderedRequest` rows: both `source=claude_cli_subscription`, `capture_seam=transport_body`, no `process_cli` row | **yes** — exactly 2 rows: `title.1` and `inference.1`, both `claude_cli_subscription` / `transport_body`, `provenance_status=captured_only` |
| `title.1` `request_json` has no `tools` key and no `temperature` | **yes** — keys `["max_tokens","messages","model","stream","system"]`; `max_tokens=24`, `system` length 1 (`type=text`), 2 messages |
| HTTP status (no 4xx/429) | **none** — `grep -ciE 'HTTP (4[0-9][0-9])\|429\|rate_limit'` = 0; server stderr had 11 INFO lines and no WARN/ERROR at the time the turn completed (see Anomalies for 3 later local WARNs) |
| Claude `OAuthCredential` rows | **0** — 1 row total, provider `xai-oauth` only (`b3-live-http-text-oauth.json`) |
| `:8787` closed | yes (before and after) |
| Token scan (`grep -l` for the two secret prefixes over `b3-live-http-text-*`) | **clean** — no files listed, exit 1 |
| Default behavior model after run | `claude-sonnet-5` (from the run's rendered requests / backend fingerprint; Behavior doc not modified; a direct Behavior query after the run was not possible because the server was already stopped per procedure) |

`inference.1` carries `tools` (has_tools=true) — expected, the default behavior has a non-empty tool surface; the bar constrains only `title.1`.

## Inference calls (from `trace timeline`)

| Scope | call_seq | kind | Wire | prompt / completion tokens | started → ended | Outcome |
|---|---|---|---|---|---|---|
| `title.1` | 1 | oneoff | Messages HTTP (`transport_body`) | 249 / 12 | 23:44:58.44 → 23:45:01.69 | completed |
| `inference.1` | 2 | inference | Messages HTTP (`transport_body`) | 9485 / 4 | 23:45:01.99 → 23:45:05.40 | completed → `pong` |

`cached_input_tokens` 0 on both (no `cache_control` sent — unchanged from #7/#8). `title.1` prompt tokens rose from 2 (process CLI, #8) to 249 (Messages HTTP with the identity `system[0]` block), consistent with the wire change.

## Deviation from spec §6

`stop_reason` is not persisted by gents; `status=complete` on the `response` event (and `call_state=completed` on both inference calls) stands in for `end_turn`, as recorded in write request #9.

## Anomalies

- None affecting the bar.
- `gents chat` stdout is the human-readable stream (`pong`), not JSON; the request id was read from the server stderr `router: dispatching request` line (as in #8).
- Three `WARN defra_http::handlers::graphql::query … parse error` lines at 23:45:42 in `b3-live-http-text-server.stderr.log` are from post-turn evidence-gathering `gents query` probes (Session/Request/Behavior with wrong field names) against the local DefraDB, 35 s after the response completed. They are not Claude HTTP responses and are not counted.

Raw: `b3-live-http-text-captures.json`, `b3-live-http-text-timeline.json`, `b3-live-http-text-oauth.json`
Logs: `b3-live-http-text-server.{stdout,stderr}.log`, `b3-live-http-text-chat.{stdout,stderr}.log`, `b3-live-http-text-server.pid`, `b3-live-http-text-{start,end}.ts`
