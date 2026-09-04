# CLAUDE WRITE REQUEST #7 — B3 identity-prefix experiment (Messages HTTP)

Date: 2026-09-03
Branch: `spike/claude-b3-live-tools` @ `551c7969` + uncommitted `claude_messages.rs` identity prefix
Agent: did:key:z6MkpEq4KsC2oibBmHe6faw1246CU5gmtVJJEYf3wvo1yKiQ:default (model `claude-sonnet-5`, unchanged)
Approval: user said "approved, any further coding changes required must be approved before happening and launched in a subagent fable 5.1 unless specified otherwise." (2026-09-03, this session)

## Hypothesis
Claude Code oat against `POST /v1/messages` 429s (`rate_limit_error` `"Error"`, no `retry-after`)
for every model because the request lacks Claude Code identity. Prepending the system block
`You are Claude Code, Anthropic's official CLI for Claude.` (Messages HTTP only; Gents preamble
kept after it) should clear the 429 and let the first `tool_use` land.

## Change under test (tests green, 54 `claude_` unit tests)
- `crates/gents/src/claude_messages.rs`: `system` = `[identity, preamble?]`; new tests pin
  order, identity-only-without-preamble, and identity absent from the process-CLI prompt/argv.
- Headers unchanged: `anthropic-version: 2023-06-01`, `anthropic-beta: oauth-2025-04-20`.
  Body keys unchanged: `model, max_tokens, stream, system, messages, tools`. No sampling.
- Binary: `./target/debug/gents` inode `44681823` (Sep 3 13:19 local)

## Scope (one slice — isolate identity; no `claude-code-20250219` beta this slice)
1. Start server: `--home ~/.gents --tool-root <repo> --tool-ceiling readwrite
   --claude-config-dir <repo>/.scratch/claude-spike/claude-config --claude-write-approved`
2. One `gents chat` turn, fresh session, prompt: use gents `list_files` on `.`, then reply `listed`
3. Capture chat JSON + server stderr to `b3-live-identity-*`
4. Stop server. Default behavior stays `claude-sonnet-5` (never changed).

## Success bar
- `AgentToolCall >= 1` for the request; response completes
- Claude `OAuthCredential` rows: 0; `:8787` closed; spike workdir empty (no CLI Bash)
- No token in any log (`sk-ant` absent from `b3-live-identity-*`)

## Stop rule
If 429 again after the identity prefix: **stop after this single turn**, file evidence, do not
retry, do not hammer. Second slice (`claude-code-20250219` beta header) is a separate numbered request.

## Preflight (2026-09-03, before approval)
- No `gents server` running; no listener on `:9191` / `:8787`
- No `.credentials.json` in spike config dir (Keychain `Claude Code-credentials-6b2dc9d7` path)
- Spike workdir `.scratch/claude-spike/workdir` empty
