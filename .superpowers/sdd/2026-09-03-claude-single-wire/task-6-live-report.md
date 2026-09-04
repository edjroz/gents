# Task 6 live report — #10: body + streaming on the single wire

**Status:** DONE (run executed once, collected, server stopped) — **FAIL** (environmental: spike seat OAuth access token expired; change not exercised)

## What ran (6.8 Steps 1–2)

- Request file `.scratch/claude-spike/logs/write-request-10.md` written with the verbatim /goal approval line; preflight appended.
- Build: `cargo build --bin gents` from `39bacaf1`, 1m 27s; binary inode `44920977`. `:9191`/`:8787` closed before start.
- Server PID `4923`, same flags as #9, listening on `127.0.0.1:9191` at 2026-09-04T02:11:47Z.
- Exactly one `gents chat --home ~/.gents --timeout-secs 180 '<list_files prompt>'`; exit 0 but stdout is `[agent error] agent stream failed: … Claude Messages seat auth: Claude seat OAuth access token is expired; re-run gents claude-login`. Request `1c1681fe-c46b-4f87-8f6c-674a26da3611`, session `eef63725-536f-4231-bb42-dc51670f2eee` (id from server stderr dispatch line).
- Collected: timeline (4 `inference_call` events, all `failed`, no token counts; no assistant message; response `status=error` at 02:12:01.19), `RenderedRequest` 0 rows, `AgentToolCall` 0 rows, `OAuthCredential` 1 row (`xai-oauth`), jq outputs in `b3-live-single-wire-jq.txt`.
- `kill 4923` exit 0, pid gone in 1 s, `:9191`/`:8787` closed. Token scan `grep -l` listed nothing.
- Evidence: `.scratch/claude-spike/logs/b3-live-single-wire-evidence.md`, marked FAIL.

No source edits, nothing committed, no behavior/backend/credential documents touched; `.credentials.json` not opened.

## Bar

| Check | Result |
|---|---|
| system[0] identity / system[1] / no `system: ` leak / cache_control / tools per scope | not testable — 0 captured `RenderedRequest` rows (no HTTP request was built) |
| `cached_input_tokens` on call_seq 2 | null (call failed) |
| first assistant delta vs completion | no assistant message; response error 2026-09-04T02:12:01.186776Z |
| `AgentToolCall.args.path == "."` | no rows |
| response `listed` | no (error) |
| 4xx/429 in server stderr | 0 real (raw grep 2 = `429` substring of deadline timestamp `02:11:54.429977`; `HTTP 4xx` 0, `rate_limit` 0) |
| Claude `OAuthCredential` rows | 0 |
| Token scan clean | yes |
| Server stopped | yes |

## Assessment

The seat token expired between #9 (23:44Z) and this run (02:11Z); the runtime's fail-closed seat-auth check refused every call before transport, so nothing was sent or billed. This says nothing about `39bacaf1`'s body/streaming behavior. Per the brief, FAIL stops the plan for a user decision. Suggested next step: user re-runs `gents claude-login` for the spike config dir, then a new write request #11 with the same scope.
