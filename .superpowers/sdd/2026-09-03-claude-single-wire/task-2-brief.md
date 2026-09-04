### Task 2: Live #8 — argument-asserting tool turn on Messages HTTP

No source change. This task is a numbered live run; it needs the user's explicit "#8 approved" in this session before step 3.

**Files:**
- Create: `.scratch/claude-spike/logs/write-request-8.md`
- Create: `.scratch/claude-spike/logs/b3-live-args-evidence.md`, `b3-live-args-server.{pid,stdout.log,stderr.log}`, `b3-live-args-chat.{stdout,stderr}.log`, `b3-live-args-timeline.json`, `b3-live-args-toolcalls.json`

**Interfaces:**
- Consumes: Task 1's parser (arguments come from `input_json_delta`).
- Produces: evidence that live `tool_use` arguments reach `AgentToolCall.args`. Task 6's owned-loop test asserts the same thing offline.

- [ ] **Step 1: Write the request file**

`.scratch/claude-spike/logs/write-request-8.md`:

```markdown
# CLAUDE WRITE REQUEST #8 — argument fidelity on Messages HTTP (after C1 fix)

Date: 2026-09-03
Branch: `spike/claude-b3-live-tools` @ <commit from Task 1>
Agent: did:key:z6MkpEq4KsC2oibBmHe6faw1246CU5gmtVJJEYf3wvo1yKiQ:default (model `claude-sonnet-5`, unchanged)
Approval: pending — this file is written before asking

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
```

- [ ] **Step 2: Build and preflight**

```bash
export GENTS_SKIP_LENS_BUILD=1 GENTS_SKIP_CALLBACK_WASM_BUILD=1 TMPDIR="$PWD/.scratch/tmp"
cargo build --bin gents 2>&1 | tail -3
ls -i ./target/debug/gents
lsof -nP -iTCP:9191 -sTCP:LISTEN; lsof -nP -iTCP:8787 -sTCP:LISTEN   # both must print nothing
```

- [ ] **Step 3: Ask for approval #8 and wait**

Tell the user: "Write request #8 is at `.scratch/claude-spike/logs/write-request-8.md`. Bar: `AgentToolCall.args.path == "."`. Approve #8?" Do nothing further until the user replies with an explicit approval naming #8.

- [ ] **Step 4: Start the server**

```bash
L=.scratch/claude-spike/logs
nohup ./target/debug/gents server \
  --home ~/.gents \
  --tool-root "$PWD" \
  --tool-ceiling readwrite \
  --claude-config-dir "$PWD/.scratch/claude-spike/claude-config" \
  --claude-write-approved \
  > $L/b3-live-args-server.stdout.log 2> $L/b3-live-args-server.stderr.log &
echo $! > $L/b3-live-args-server.pid
sleep 8; lsof -nP -iTCP:9191 -sTCP:LISTEN | tail -1
```

- [ ] **Step 5: One chat turn**

```bash
L=.scratch/claude-spike/logs
./target/debug/gents chat --home ~/.gents --timeout-secs 180 \
  'Use the gents list_files tool on the directory "." (not Bash). After the tool returns, reply with exactly: listed' \
  > $L/b3-live-args-chat.stdout.log 2> $L/b3-live-args-chat.stderr.log
grep -o '"request_id": *"[^"]*"' $L/b3-live-args-chat.stdout.log | head -1
```

- [ ] **Step 6: Collect timeline and tool calls**

```bash
L=.scratch/claude-spike/logs; REQ=<request id from step 5>
./target/debug/gents trace timeline --home ~/.gents --request-id "$REQ" --output-file $L/b3-live-args-timeline.json
./target/debug/gents query --home ~/.gents --collection AgentToolCall \
  --field tool_name --field tool_call_id --field args --field lifecycle_state \
  --filter "{\"request_id\":{\"_eq\":\"$REQ\"}}" > $L/b3-live-args-toolcalls.json
jq '.results[] | {tool_name, lifecycle_state, args: (.args | fromjson)}' $L/b3-live-args-toolcalls.json
```
(`gents chat` and `gents query` default to the server's GraphQL endpoint `http://127.0.0.1:9191/api/v0/graphql`; `--http-port` default is 9191.)

Expected: one row, `tool_name: "list_files"`, `lifecycle_state: "completed"`, `args.path == "."`.

- [ ] **Step 7: Stop the server and check hygiene**

```bash
L=.scratch/claude-spike/logs
kill "$(cat $L/b3-live-args-server.pid)"; sleep 2; lsof -nP -iTCP:9191 -sTCP:LISTEN
grep -l 'sk-ant\|Bearer ' $L/b3-live-args-* ; echo "token scan exit=$?"   # must list nothing
```

- [ ] **Step 8: Write the evidence file**

`.scratch/claude-spike/logs/b3-live-args-evidence.md` with the same table shape as `b3-live-identity-evidence.md`: When, write request #8, branch @ commit, binary inode, server PID + flags, session, request id, prompt, response, and a Bar table with rows: HTTP status (no 4xx/429), `AgentToolCall.args.path == "."`, response `listed`, `OAuthCredential` Claude rows 0, `:8787` closed, token scan clean, default behavior model `claude-sonnet-5`. Mark **PASS** or **FAIL**. If FAIL, stop the plan here and report; do not proceed to Task 3 until the user decides.

- [ ] **Step 9: Append the correction to the #7 evidence**

Append to `.scratch/claude-spike/logs/b3-live-identity-evidence.md`:

```markdown
## Correction (2026-09-03, after write request #8)
Argument fidelity was not demonstrated by #7 (`arguments: {}`, defect C1). See `b3-live-args-evidence.md`.
```

No commit: `.scratch/` is untracked evidence.

