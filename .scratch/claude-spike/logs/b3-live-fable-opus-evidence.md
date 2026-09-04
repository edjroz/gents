# B3 live: claude-fable-5 then claude-opus-5

**When:** 2026-09-03T16:59Z–17:03Z
**Branch:** `spike/claude-b3-live-tools` @ `551c7969`
**Server:** PID `58345` `--claude-write-approved`
**Default behavior restored to** `claude-sonnet-5` after the turns.

Same 429 on both non-Sonnet IDs. Not a Sonnet-only pool.

| Model | Request | Anthropic req | HTTP | `message` | Body keys | `AgentToolCall` |
|---|---|---|---|---|---|---|
| `claude-fable-5` | `3feab884-0aac-4333-952a-ccf3d5533bc5` | `req_011Cegrye7EhboPnX6pLMJYA` | 429 | `"Error"` | `max_tokens, messages, model, stream, tools` | 0 |
| `claude-opus-5` | `f45c8192-a7c9-472b-bbc8-a73bfe34e2b3` | `req_011CegsAeVcGyN3eeB8bbsrk` | 429 | `"Error"` | same, no `temperature` | 0 |

Claude oat 0. `:8787` closed. No 400. Capture `model` field matched the switched ID.

Together with overnight Sonnet 429 + working process-CLI titles: Messages HTTP + OAuth Bearer is rejected for **every** model we tried, not a 5-hour session window and not Sonnet-specific.

Raw: `b3-live-fable-tool.json`, `b3-live-opus-tool.json`
