# B3 gated live after overnight wait (C2 Messages HTTP)

**When:** 2026-09-03T16:55:19Z–16:56:19Z (~17h after previous 429 at 21:59:44Z / retry 23:34:40Z)
**Branch:** `spike/claude-b3-live-tools` @ `551c7969`
**Binary:** `./target/debug/gents` inode `44644360`
**Server:** PID `57861` `--claude-write-approved` (had been idle since 15:07Z)
**Session:** `f64f2200-4c2b-4941-ab63-0cb77c065193` (new)
**Request:** `e042a396-74c2-4306-a020-fc1b6954609d`
**Anthropic:** `req_011CegrfhAHngeh9o6RsKuh7`

## Bar

| Check | Result |
|---|---|
| HTTP status | **429** `rate_limit_error` |
| `temperature` 400 | **none** |
| Inference Messages body keys | `max_tokens, messages, model, stream, tools` — no `temperature`/`top_p`/`top_k` |
| Title generation (empty tools → process CLI) | **succeeded** |
| `AgentToolCall` | **0** |
| Claude oat | **0** (only `xai-oauth`) |
| `:8787` / credentials file / token logs | clean |

Gents maps this to `rate limited, retry after 60s` (default when no `retry-after` header). Anthropic body: `{"type":"error","error":{"type":"rate_limit_error","message":"Error"}}`.

## Nature (not a 5-hour session reset)

This is **not** the 400 body bug and **not** a short RPM window.

1. Official API quota 429s describe *which* limit and often include `retry-after` or `details.error_code` (e.g. `enforced_spend_limit_reached` + a UTC resume time). We got **`message: "Error"`** and Gents defaulted 60s — no reset timestamp.
2. **Process CLI still works on the same seat:** title capture is the CLI-shaped JSON (`endpoint`, `provider`, `temperature`, empty `tools`) and the title was generated. The 429 is only on `POST /v1/messages` with the OAuth Bearer.
3. Same 429 after **~17 hours**. A 5-hour Claude session window would have cleared.

That matches the undocumented OAuth routing: Claude Code oat against Messages HTTP without Claude Code identity is classified into a starved API pool and 429s with a generic `Error`. It is **not** proven until a numbered experiment adds the identity system prefix; do not invent it on this turn.

Raw: `b3-live-reset-tool.json`
Logs: `b3-live-reset-server.stderr.log`
