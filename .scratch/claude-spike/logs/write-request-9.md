# CLAUDE WRITE REQUEST #9 — text-only turn and title generation over Messages HTTP (single wire go/no-go)

Date: 2026-09-03
Branch: `spike/claude-b3-live-tools` @ 8ae310d0
Agent: did:key:z6MkpEq4KsC2oibBmHe6faw1246CU5gmtVJJEYf3wvo1yKiQ:default (model `claude-sonnet-5`, unchanged)
Approval: #9 approved — user /goal directive in session 0195Uzn9EziXQPWV4e8WdKHy (2026-09-03): "proceed with them as we've done … If we hit 429s you can stop"

## Hypothesis
With the routing switch, a text-only turn (empty tool surface, no `tools` key) and the
title-generation call both complete over Messages HTTP with the identity block; the
process-CLI wire is not used at all. If this fails, the CLI-wire deletion (Task 6) does not proceed.

## Change under test
Commit `8ae310d0` — `ClaudeSubscriptionModel::stream` routes every turn to `stream_messages`
when no fake completer is installed; `build_messages_body` omits `tools` for an empty surface.
Headers/body otherwise unchanged from #8 (`system[0]` identity, `anthropic-beta: oauth-2025-04-20`, no sampling).

## Scope (one turn)
1. Server: `--home ~/.gents --tool-root <repo> --tool-ceiling readwrite --claude-config-dir <repo>/.scratch/claude-spike/claude-config --claude-write-approved` (exactly as #8)
2. One `gents chat` turn, fresh session, prompt:
   `Reply with exactly: pong`
3. `gents trace timeline --request-id <id>`; then
   `gents query --collection RenderedRequest --field capture_scope --field source --field provenance_json --field request_json --filter {"request_id":{"_eq":"<id>"}}`
4. Stop server.

## Success bar
- Response content `pong`, `status=complete`
- `RenderedRequest` rows for both `inference.1` and `title.1` have `source = claude_cli_subscription` and `capture_seam = transport_body` (no `process_cli` row)
- The `title.1` captured `request_json` has no `tools` key and no `temperature`
- No 4xx/429 in the server stderr (`b3-live-http-text-server.stderr.log`)
- Claude `OAuthCredential` rows 0; token scan clean (no `sk-ant` / `Bearer` in any `b3-live-http-text-*` file); default behavior model unchanged
- Note: `stop_reason` is not persisted by gents; `status=complete` stands in for `end_turn` (deviation from spec §6, recorded here and in the evidence)

## Stop rule
Any 4xx/429: stop after this single turn, write evidence, no retry.
FAIL means the cut stops for a user decision (no Task 5/6 without one).

## Preflight
- no `gents server` listening on :9191; no listener on :8787
- `./target/debug/gents` rebuilt from `8ae310d0` (inode recorded)

### Preflight results (2026-09-03, prep half of Task 4)
- Build: `cargo build --bin gents` → `Finished `dev` profile [unoptimized + debuginfo] target(s) in 1m 00s` (1 pre-existing warning in `gents-cli`)
- Binary inode: `44847441 ./target/debug/gents` (built from `8ae310d0`)
- `lsof -nP -iTCP:9191 -sTCP:LISTEN` → no output (exit 1)
- `lsof -nP -iTCP:8787 -sTCP:LISTEN` → no output (exit 1)
