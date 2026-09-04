# CLAUDE WRITE REQUEST #8 — argument fidelity on Messages HTTP (after C1 fix)

Date: 2026-09-03
Branch: `spike/claude-b3-live-tools` @ 44908d95
Agent: did:key:z6MkpEq4KsC2oibBmHe6faw1246CU5gmtVJJEYf3wvo1yKiQ:default (model `claude-sonnet-5`, unchanged)
Approval: #8 approved by the user in session 0195Uzn9EziXQPWV4e8WdKHy (2026-09-03, after reading this file)

## Hypothesis
With deltas accumulated (C1), the live `tool_use` for `list_files` carries `{"path":"."}`
and `AgentToolCall.args` records it. Write request #7 proved routing; it recorded `arguments: {}`.

## Change under test
Task 1 commit: `PendingTool { start_input, deltas }`, dup-id and overlap fail closed. Wire, headers,
body keys unchanged from #7 (`system[0]` identity, `anthropic-beta: oauth-2025-04-20`, no sampling).

## Scope (one turn)
1. Server: `--home ~/.gents --tool-root <repo> --tool-ceiling readwrite --claude-config-dir <repo>/.scratch/claude-spike/claude-config --claude-write-approved`
2. One `gents chat` turn, fresh session, prompt:
   `Use the gents list_files tool on the directory "." (not Bash). After the tool returns, reply with exactly: listed`
3. `gents trace timeline --request-id <id>`; GraphQL query of `AgentToolCall { tool_name args lifecycle_state }` for the request
4. Stop server.

## Success bar
- `AgentToolCall` for the request: `tool_name = list_files`, `lifecycle_state = completed`, `args` parses as JSON with `path == "."`
- Response content `listed`, `status=complete`
- Claude `OAuthCredential` rows 0; `:8787` closed; no `sk-ant` / `Bearer` in any `b3-live-args-*` file

## Stop rule
Any 4xx/429: stop after this turn, write evidence, no retry.

## Preflight
- no `gents server` listening on :9191; no listener on :8787
- `./target/debug/gents` rebuilt from the Task 1 commit (inode recorded)

### Preflight results (2026-09-03, prep half of Task 2)
- Build: `cargo build --bin gents` → `Finished dev profile [unoptimized + debuginfo] target(s) in 4m 01s` (1 pre-existing warning in `gents-cli`)
- Binary inode: `44769203 ./target/debug/gents` (built from `44908d95`)
- `lsof -nP -iTCP:9191 -sTCP:LISTEN` → no output (exit 1)
- `lsof -nP -iTCP:8787 -sTCP:LISTEN` → no output (exit 1)
