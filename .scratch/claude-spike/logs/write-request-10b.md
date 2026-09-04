# CLAUDE WRITE REQUEST #10b — body + streaming on the single wire (re-run of #10 after seat refresh)

Date: 2026-09-04
Branch: `spike/claude-b3-live-tools` @ 9a92c586 (tag `spike-final-2026-09-04`)
Agent: did:key:z6MkpEq4KsC2oibBmHe6faw1246CU5gmtVJJEYf3wvo1yKiQ:default (model `claude-sonnet-5`, unchanged)
Approval: #10b approved — user /goal directive in session 0195Uzn9EziXQPWV4e8WdKHy (2026-09-03): "proceed with them as we've done … If we hit 429s you can stop". Seat refreshed by write request #12 on 2026-09-04.

## Why a re-run
#10 (`write-request-10.md`, `b3-live-single-wire-evidence.md`) failed closed before any HTTP send because the spike seat's OAuth token had expired; the single-wire body and streaming were never observed live. Nothing was billed.

## Precondition (user)
`CLAUDE_CONFIG_DIR="$PWD/.scratch/claude-spike/claude-config" claude auth login --claudeai` (or a status check that refreshes), then the user says the seat is fresh. Gents never writes the seat.

## Change under test
Commits 39bacaf1..9a92c586: `system[] = [identity, preamble?, System rows…]` with `cache_control` on the last system block and the last content block; `tools` omitted when empty; incremental SSE (`MessagesSseState`); `message_stop` flushes a pending tool; test-only fixture queue; `set_sensitive` on the auth header.

## Scope (one turn)
Identical to #10: server `--home ~/.gents --tool-root <repo> --tool-ceiling readwrite --claude-config-dir <repo>/.scratch/claude-spike/claude-config --claude-write-approved`; one `gents chat` turn, fresh session, prompt `Use the gents list_files tool on the directory "." (not Bash). After the tool returns, reply with exactly: listed`; trace timeline + RenderedRequest/AgentToolCall/OAuthCredential queries; stop server. Evidence prefix `b3-live-single-wire-b-`.

## Success bar
- `inference.1` turn 0 capture: `system[0]` identity, `system[1]` present, no `messages[]` text starting `system: `, `cache_control` on the last `system` block and on the last content block
- `tools` present on `inference.1`, absent on `title.1`
- second `inference_call` (`call_seq` 2) records `cached_input_tokens` (any value; record it)
- first assistant delta timestamp precedes the response `completed_at`
- `AgentToolCall.args.path == "."`, response `listed`, `status=complete`
- 0 4xx/429; Claude `OAuthCredential` rows 0; token scan clean; server stopped; default model unchanged

## Stop rule
Any 4xx/429: stop after this single turn, write evidence, no retry.

### Preflight results
- Build: `cargo build --bin gents` at `9a92c586` — already up to date (1.10s); binary `./target/debug/gents` inode `45229261`
- `lsof -nP -iTCP:9191 -sTCP:LISTEN`: empty (rc=1)
- `lsof -nP -iTCP:8787 -sTCP:LISTEN`: empty (rc=1)
