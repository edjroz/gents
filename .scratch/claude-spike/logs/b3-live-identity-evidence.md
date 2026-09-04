# B3 live: Claude Code identity prefix on Messages HTTP — **PASS**

**When:** 2026-09-03T17:22:57Z–17:23:14Z (17 s wall)
**Write request:** #7 (`write-request-7.md`), approved in-session
**Branch:** `spike/claude-b3-live-tools` @ `551c7969` + uncommitted `crates/gents/src/claude_messages.rs`
**Binary:** `./target/debug/gents` inode `44681823` (Sep 3 13:19 local)
**Server:** PID `62476` `--home ~/.gents --tool-root <repo> --tool-ceiling readwrite --claude-config-dir <repo>/.scratch/claude-spike/claude-config --claude-write-approved`; stopped after the turn
**Behavior:** `did:key:z6MkpEq4KsC2oibBmHe6faw1246CU5gmtVJJEYf3wvo1yKiQ:default`, model **`claude-sonnet-5`** (never changed)
**Session:** `568e7a73-e825-4c8c-a8dc-0872e5090001` (fresh)
**Request:** `d315fc37-2767-464c-a976-55e65fc99c07`
**Prompt:** `Use the gents list_files tool on the directory "." (not Bash). After the tool returns, reply with exactly: listed`
**Response:** `listed` — `status=complete`, `lifecycle_state=completed`

## Change under test

`system` on Messages HTTP is now `[ {text: "You are Claude Code, Anthropic's official CLI for Claude."}, {text: <Gents preamble>}? ]`.
Nothing else changed: headers `anthropic-version: 2023-06-01`, `anthropic-beta: oauth-2025-04-20` (no `claude-code-20250219`), body keys `max_tokens, messages, model, stream, system, tools`, no sampling. The identity is never sent on the process-CLI wire (test-fenced).

## Bar

| Check | Result |
|---|---|
| HTTP | **no 429, no 400** — both Messages turns streamed to completion |
| `AgentToolCall ≥ 1` | **yes** — `list_files` `completed`, id `CFdjtTt2DuyXaiq5tG2Mw`, 333 ms, 27 entries returned |
| Tool executed by Gents (not Claude Code `Bash`) | yes — `tool_use` mapped onto the gents surface; spike workdir still empty |
| Native `tool_result` turn accepted | yes — turn 1 body carries `assistant.tool_use` + `user.tool_result`, then `listed` |
| Title generation (empty tools → process CLI) | succeeded — `list-files-tool-invoke-listed`; capture `title.1` is CLI-shaped (`temperature`, `tools: []`) |
| Claude `OAuthCredential` rows | **0** (only `xai-oauth`) |
| `:8787` | closed |
| Token in logs / captures | none (`sk-ant`, `Bearer` absent from `b3-live-identity-*`) |
| Default behavior model after run | `claude-sonnet-5` |

## Inference calls (from `trace timeline`)

| Scope | Turn | Wire | `system[0]` | prompt / completion tokens | Outcome |
|---|---|---|---|---|---|
| `inference.1` | 0 | Messages HTTP | Claude Code identity | 9513 / 69 | `tool_use list_files` |
| `inference.1` | 1 | Messages HTTP | Claude Code identity | 9906 / 4 | `listed` |
| `title.1` | 0 | process CLI | n/a | 2 / 15 | title |

`cached_input_tokens` 0 on both turns (no `cache_control` sent — expected).

## Nature (now confirmed)

Same seat, same headers, same body minus one system block: yesterday and this morning every model 429'd with `rate_limit_error "Error"`; with the identity block the first attempt succeeds. The 429 was **identity-based OAuth routing**, not quota, not a session window, not model-specific. The `claude-code-20250219` beta header was **not** needed.

## Observations for later slices (not changed this run — needs approval)

- The Gents system prompt reaches the wire as a `user` block prefixed `system: …` (pre-existing `Message::System` mapping in `anthropic_messages()`), not as `system[1]`; `request.preamble` was empty for this behavior. Consider routing `Message::System` into the `system` array behind the identity block — that also makes it cacheable.
- No `cache_control` on the stable prefix (tools + system + workspace context ≈ 9.5k tokens); a breakpoint after the workspace context would cut per-turn input cost.
- `max_tokens` is 32768 on this behavior (fine for streaming).

Raw: `b3-live-identity-tool.json`, `b3-live-identity-timeline.json`, `b3-live-identity-captures.json`
Logs: `b3-live-identity-server.stderr.log`, `b3-live-identity-chat.stderr.log`

## Correction (2026-09-03, after write request #8)
Argument fidelity was not demonstrated by #7 (`arguments: {}`, defect C1). See `b3-live-args-evidence.md`.
Single-wire follow-up (#10): `b3-live-single-wire-evidence.md` — environmental FAIL (expired seat, nothing sent), pending re-run.
