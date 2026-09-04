# CLAUDE WRITE REQUEST #10 — body + streaming on the single wire

Date: 2026-09-03
Branch: `spike/claude-b3-live-tools` @ 39bacaf1
Agent: did:key:z6MkpEq4KsC2oibBmHe6faw1246CU5gmtVJJEYf3wvo1yKiQ:default (model `claude-sonnet-5`, unchanged)
Approval: #10 approved — user /goal directive in session 0195Uzn9EziXQPWV4e8WdKHy (2026-09-03): "proceed with them as we've done … If we hit 429s you can stop"

## Hypothesis
Body + streaming on the single wire: with the CLI wire deleted, a tool-capable turn and its
follow-up and the title call all complete over Messages HTTP with `system[]` (identity block
first, behavior preamble/System row second) carrying cache breakpoints, and the response is
streamed incrementally (assistant deltas persisted before completion).

## Change under test
Commit `39bacaf1` — single-wire cut: `system[]` with `cache_control` breakpoints on the last
system block and the last content block, incremental SSE materialization, process-CLI wire
deleted (`--claude-workdir`, `--claude-log-dir`, `--claude-fake-completer` no longer exist).

## Scope (one turn)
1. Server: `--home ~/.gents --tool-root <repo> --tool-ceiling readwrite --claude-config-dir <repo>/.scratch/claude-spike/claude-config --claude-write-approved` (same flags as #9)
2. One `gents chat` turn, fresh session, prompt:
   `Use the gents list_files tool on the directory "." (not Bash). After the tool returns, reply with exactly: listed`
3. `gents trace timeline --request-id <id>`; then
   `gents query --collection RenderedRequest --field capture_scope --field turn_index --field source --field provenance_json --field request_json --filter {"request_id":{"_eq":"<id>"}}`;
   `gents query --collection AgentToolCall --field tool_name --field args --field lifecycle_state --filter {"request_id":{"_eq":"<id>"}}`
4. Stop server.

## Success bar (spec §6 #10)
- captured `request_json` for `inference.1` turn 0: `system[0]` is the identity, `system[1]` exists (behavior preamble or System row), no `messages[]` block whose text starts with `system: `, last `system` block and last content block carry `cache_control`
- the second `inference_call` event (`call_seq` 2) records `cached_input_tokens` (any value; record it)
- timeline shows at least one assistant `message`/delta event timestamped before the response `completed_at` with a gap consistent with streaming (record the first-delta and completion timestamps)
- `AgentToolCall.args.path == "."`, response `listed`, no 4xx/429, token scan clean
- Claude `OAuthCredential` rows 0; token scan clean (no `sk-ant` / `Bearer` in any `b3-live-single-wire-*` file); server stopped, `:9191`/`:8787` closed after

## Stop rule
Any 4xx/429: stop after this single turn, write evidence, no retry.
FAIL stops the plan for a user decision.

## Preflight
- no `gents server` listening on :9191; no listener on :8787
- `./target/debug/gents` rebuilt from `39bacaf1` (inode recorded)

### Preflight results (2026-09-03, prep half of Task 6.8)
- Build: `cargo build --bin gents` → `Finished `dev` profile [unoptimized + debuginfo] target(s) in 1m 27s` (1 pre-existing warning in `gents-cli`)
- Binary inode: `44920977 ./target/debug/gents` (built from `39bacaf1`)
- `lsof -nP -iTCP:9191 -sTCP:LISTEN` → no output (exit 1)
- `lsof -nP -iTCP:8787 -sTCP:LISTEN` → no output (exit 1)
