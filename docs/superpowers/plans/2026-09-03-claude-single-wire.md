# Claude Single-Wire Messages HTTP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Collapse the Claude subscription backend to one wire (`POST /v1/messages` over the seat's OAuth token), fix the confirmed defects on that wire (C1, C8, C9, C10, C6), fence the parser and body assembly from Lean, and delete the process-CLI wire and everything that only existed to serve it.

**Architecture:** `ClaudeSubscriptionModel::stream` always calls `claude_messages::stream_messages`, which builds the body (`system[] = [identity, preamble?, System rows…]`, `tools` absent when empty, two `cache_control` breakpoints), reads the seat token per request, POSTs through `RenderedRequestCapturingHttpClient`, and parses the SSE body incrementally through one `MessagesSseState`. Health probes read the same token the wire uses and promote the backend document like every other HTTP backend. The `claude` binary remains only as the `gents claude-login` dependency. Lean `ClaudeMap.lean` gains a system-assembly model and a tool-block accumulation model; witnesses drive the production parser and body builder.

**Tech Stack:** Rust (`crates/gents`, `crates/gents-cli`, `crates/gents-protocol`), rig-core 0.35 (`sourcenetwork/rig` fork; `CompletionModel`, `RawStreamingChoice`, `HttpClientExt`), reqwest, `async-stream`, serde_json, Lean 4 + Mathlib (`crates/gents/proofs`), `gents-lean-contract` snapshot loader, DefraDB (GraphQL).

**Spec:** `docs/superpowers/specs/2026-09-03-claude-single-wire-design.md` (approved 2026-09-03, not committed). Review that drove it: `.scratch/claude-spike/logs/review-track-ab-2026-09-03.md`.

**Branch:** `spike/claude-b3-live-tools`. All slices land here as commits; the PR carve (spec §8) is the final task.

## Global Constraints

Copied from spec §9 ("Constraints carried through"):

- No token in any log, capture, evidence file, or chat output (`sk-ant`, `Bearer` values).
- Gents never writes the seat; Claude `OAuthCredential` rows stay 0.
- No rustfmt of `crates/gents/src/lib.rs`; import-order churn kept out of the PRs.
- Leftover `SPEC-claude-a2b-in-process.md`, `TODO.md`, `tasks/a2b2-interjection-plan.md` untouched.
- Grok is not restored as the default backend; default behavior model stays `claude-sonnet-5`.
- Any code change: approved first, executed in a Fable 5.1 subagent.

Session rules that bind every task in this plan:

- **Approval gate.** Each task is proposed to the user in this session; on an explicit "yes" it is executed by an `Agent` call with `model: "fable"`. The main session never edits source.
- **Live runs** (Tasks 2, 4, 8, 11) need a numbered write approval in the current session. Next number is **#8**. Write `.scratch/claude-spike/logs/write-request-N.md` before asking. On any 4xx/429: stop after the single turn, write the evidence file, do not retry.
- **Spec and plan are not committed.** `docs/superpowers/specs/2026-09-03-claude-single-wire-design.md` and this file stay untracked. Never `git add` them.
- **Build environment** for every cargo command:
  ```bash
  export GENTS_SKIP_LENS_BUILD=1 GENTS_SKIP_CALLBACK_WASM_BUILD=1 TMPDIR="$PWD/.scratch/tmp"
  ```
- **Gates for every code task:** `cargo test -p gents` (the full package, never `--lib`), `cargo check --workspace --all-targets`, and when Lean is touched `cd crates/gents/proofs && lake build` with zero `sorry`. `tracing`, never `println`. `graphql::escape_graphql_string()` for anything interpolated into GraphQL. Never emit `[]` in a DefraDB mutation.
- **Seam scan.** `tests/conformance/prompt_assembly.rs::provider_invocations_are_confined_to_the_owned_loop_seam` fails if production code outside `agent/loop_stream.rs` / `admission/client.rs` contains `.stream(` or `.completion(`. New production code in this plan must not introduce those call shapes.
- **Binary** for live runs is `./target/debug/gents` (`cargo build --bin gents`). Record its inode (`ls -i`) in every evidence file.

## Naming deviations from the spec (decided during planning)

- Spec §3 names four ledger entries (`systemAssembly`, `accumulate`, `overlappingBlock`, `emptyTools`). The plan emits **two witness sets** instead, each covering two of those: `PromptAssemblyClaudeBodyCases` (`systemAssembly` + `emptyTools`, drives `build_messages_body`) and `PromptAssemblyClaudeStreamCases` (`accumulate` + `overlappingBlock`, drives `parse_messages_sse`). The ledger entries keep the spec's four names as case-set tags inside those two sets.
- The existing driver `generated_claude_map_cases_drive_the_completer_parser` is renamed `generated_claude_map_cases_drive_the_messages_parser` when it is re-pointed.
- Spec §4 says `try_unfold`; the plan uses `async_stream::stream!` (already a dependency, used by the existing `stream_child_stdout`). Same behaviour: bytes are line-split and events yielded as they arrive.
- Spec §3b says "`Idle | InTool`"; the Lean model represents that as `StreamState.pending : Option Pending`.
- The fail-closed error enum moves from `claude_completer::CompleterParseError` to `claude_messages::MessagesParseError` in Task 6 (the JSONL parser it belonged to is deleted). Display strings are kept byte-identical so the Task 5 conformance driver stays green across the move.
- Spec §5 says the serve status JSON keeps `claude_seat`; the pre-existing key is `claude_subscription` and it stays.
- 2026-09-04: `--claude-write-approved` on `gents server` retired after the plan; `--claude-config-dir` is the opt-in.

## File map

| File | Responsibility after this plan |
|---|---|
| `crates/gents/src/claude_messages.rs` | The only Claude wire: body assembly, SSE parser state machine, `stream_messages`, fixture queue, `MessagesParseError` |
| `crates/gents/src/claude_subscription.rs` | `ClaudeSeatConfig` (config_dir, write_approved, http), seat slot, `ClaudeSubscriptionModel` shell delegating to `stream_messages`, `ClaudeStreamResponse` |
| `crates/gents/src/claude_seat_auth.rs` | Token read: `.credentials.json` → `security(1)` Keychain; error enum; no Security.framework |
| `crates/gents/src/claude_completer/mod.rs` | Only `DEFAULT_MODEL_ID`, `STRIPPED_ENV_VARS`, `sanitize_child_env` (login-time child env) |
| `crates/gents/src/backend_health.rs` | Claude probe = token read; shared recording + promotion |
| `crates/gents/proofs/Proofs/PromptAssembly/ClaudeMap.lean` | Existing map model + system assembly + stream accumulation |
| `crates/gents/proofs/Conformance/ContractCases/PromptAssembly.lean` | Witness generators for body and stream cases |
| `crates/gents/tests/conformance/prompt_assembly.rs` | Drivers: map cases → `parse_messages_sse`; stream cases → `parse_messages_sse`; body cases → `build_messages_body` |
| `crates/gents/src/agent/loop_stream/tests/claude.rs` | Owned-loop round trip on two SSE fixtures |
| `crates/gents-cli/src/cli/args.rs`, `commands/serve.rs`, `commands/claude_login.rs` | Trimmed CLI surface |

---

### Task 1: C1 fix — tool_use input accumulation, duplicate ids, overlapping blocks

**Files:**
- Modify: `crates/gents/src/claude_messages.rs:198-322` (`parse_messages_sse`, `PendingTool`, `mapped_tool_call`)
- Modify: `crates/gents/src/claude_completer/mod.rs:70-83` (`CompleterParseError` gains `OverlappingToolUse`)
- Test: `crates/gents/src/claude_messages.rs` (`mod tests`, after `sse_maps_echo_and_rejects_bash`)

**Interfaces:**
- Consumes: `CompleterParseError::{ToolUse, DuplicateToolUseId, MalformedToolUse}` (existing), `RawStreamingChoice`, `RawStreamingToolCall::new(id, name, arguments)`.
- Produces: `parse_messages_sse(sse: &str, surface: &HashSet<String>) -> Result<Vec<RawStreamingChoice<ClaudeStreamResponse>>, CompletionError>` with these guarantees, which Task 5's witnesses and Task 6's state machine rely on:
  - arguments = parsed `input_json_delta` concatenation when any delta arrived, else parsed `content_block_start.input`, else `{}`;
  - unparseable arguments → `Err` whose Display contains `fail-closed: malformed tool_use`;
  - a second `tool_use` block with an id already flushed → `Err` containing `fail-closed: duplicate tool_use id <id>`;
  - a `content_block_start` (tool_use) while a block is pending → `Err` containing `fail-closed: overlapping tool_use block <id>`.
- Error Display strings (frozen from here on; Task 5 matches on them, Task 6 moves the enum without changing them):
  - `fail-closed: tool_use observed ({names})`
  - `fail-closed: duplicate tool_use id {id}`
  - `fail-closed: malformed tool_use at line {line}: {message}` (Task 6 drops `at line {line}`; Task 5's driver matches only the `fail-closed: malformed tool_use` prefix)
  - `fail-closed: overlapping tool_use block {id}`

- [ ] **Step 1: Write the failing tests**

Append inside `mod tests` in `crates/gents/src/claude_messages.rs`:

```rust
    fn sse_tool_use_block(id: &str, name: &str, start_input: &str, deltas: &[&str]) -> String {
        let mut sse = format!(
            "event: content_block_start\ndata: {{\"type\":\"content_block_start\",\"index\":0,\"content_block\":{{\"type\":\"tool_use\",\"id\":\"{id}\",\"name\":\"{name}\",\"input\":{start_input}}}}}\n\n"
        );
        for partial in deltas {
            let escaped = serde_json::to_string(partial).expect("escape partial_json");
            sse.push_str(&format!(
                "event: content_block_delta\ndata: {{\"type\":\"content_block_delta\",\"index\":0,\"delta\":{{\"type\":\"input_json_delta\",\"partial_json\":{escaped}}}}}\n\n"
            ));
        }
        sse.push_str("event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n");
        sse
    }

    fn tool_call_arguments(events: &[RawStreamingChoice<ClaudeStreamResponse>]) -> Value {
        match &events[0] {
            RawStreamingChoice::ToolCall(call) => call.arguments.clone(),
            other => panic!("expected ToolCall first, got {other:?}"),
        }
    }

    /// C1: Anthropic sends `input: {}` on `content_block_start` and streams the
    /// real arguments as `input_json_delta` fragments. The deltas are the
    /// arguments; the start input is ignored once any delta arrives.
    #[test]
    fn sse_tool_use_deltas_yield_exact_arguments() {
        let sse = sse_tool_use_block("toolu_1", "echo", "{}", &["{\"text\":", " \"hi\"}"]);
        let surface = HashSet::from(["echo".to_string()]);
        let events = parse_messages_sse(&sse, &surface).expect("parse");
        assert_eq!(tool_call_arguments(&events), json!({"text": "hi"}));
    }

    #[test]
    fn sse_tool_use_without_deltas_uses_start_input() {
        let sse = sse_tool_use_block("toolu_1", "echo", "{\"text\":\"hi\"}", &[]);
        let surface = HashSet::from(["echo".to_string()]);
        let events = parse_messages_sse(&sse, &surface).expect("parse");
        assert_eq!(tool_call_arguments(&events), json!({"text": "hi"}));
    }

    #[test]
    fn sse_tool_use_with_no_input_at_all_is_empty_object() {
        let sse = sse_tool_use_block("toolu_1", "echo", "{}", &[]);
        let surface = HashSet::from(["echo".to_string()]);
        let events = parse_messages_sse(&sse, &surface).expect("parse");
        assert_eq!(tool_call_arguments(&events), json!({}));
    }

    #[test]
    fn sse_tool_use_with_unparseable_input_fails_closed() {
        let sse = sse_tool_use_block("toolu_1", "echo", "{}", &["{\"text\":"]);
        let surface = HashSet::from(["echo".to_string()]);
        let err = parse_messages_sse(&sse, &surface).expect_err("truncated json");
        assert!(err.to_string().contains("fail-closed: malformed tool_use"), "{err}");
    }

    #[test]
    fn sse_duplicate_tool_use_id_fails_closed() {
        let mut sse = sse_tool_use_block("toolu_1", "echo", "{}", &["{}"]);
        sse.push_str(&sse_tool_use_block("toolu_1", "echo", "{}", &["{}"]));
        let surface = HashSet::from(["echo".to_string()]);
        let err = parse_messages_sse(&sse, &surface).expect_err("duplicate id");
        assert!(err.to_string().contains("fail-closed: duplicate tool_use id toolu_1"), "{err}");
    }

    #[test]
    fn sse_overlapping_tool_use_block_fails_closed() {
        let first = sse_tool_use_block("toolu_1", "echo", "{}", &[]);
        let (start, _stop) = first.split_once("event: content_block_stop").expect("stop");
        let mut sse = start.to_string();
        sse.push_str(&sse_tool_use_block("toolu_2", "echo", "{}", &[]));
        let surface = HashSet::from(["echo".to_string()]);
        let err = parse_messages_sse(&sse, &surface).expect_err("overlap");
        assert!(err.to_string().contains("fail-closed: overlapping tool_use block toolu_2"), "{err}");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run:
```bash
cargo test -p gents --lib claude_messages::tests::sse_ -- --nocapture
```
Expected: `sse_tool_use_deltas_yield_exact_arguments` FAILS (arguments are `{}` because `{}` + deltas is unparseable and `mapped_tool_call` silently substitutes `json!({})`), `sse_tool_use_with_unparseable_input_fails_closed` FAILS (returns Ok), `sse_duplicate_tool_use_id_fails_closed` FAILS (returns Ok), `sse_overlapping_tool_use_block_fails_closed` FAILS to compile-or-run until the variant exists; `sse_maps_echo_and_rejects_bash` still passes.

- [ ] **Step 3: Add the error variant**

In `crates/gents/src/claude_completer/mod.rs`, inside `pub enum CompleterParseError` after `DuplicateToolUseId`:

```rust
    #[error("fail-closed: overlapping tool_use block {id}")]
    OverlappingToolUse { id: String },
```

- [ ] **Step 4: Rewrite `PendingTool`, the start/delta/stop arms, and `mapped_tool_call`**

Replace the `struct PendingTool` and `fn mapped_tool_call` in `crates/gents/src/claude_messages.rs` with:

```rust
struct PendingTool {
    id: String,
    name: String,
    /// `content_block.input` from `content_block_start`, serialized. Anthropic
    /// sends `{}` here and streams the real arguments as deltas.
    start_input: Option<String>,
    /// Concatenated `input_json_delta.partial_json` fragments, in order.
    deltas: String,
}

impl PendingTool {
    /// Lean `ClaudeMap.accumulate`: deltas win when any arrived; otherwise the
    /// start input; otherwise `{}`.
    fn arguments_json(&self) -> String {
        if !self.deltas.is_empty() {
            self.deltas.clone()
        } else {
            self.start_input.clone().unwrap_or_else(|| "{}".to_string())
        }
    }
}

fn mapped_tool_call(
    tool: PendingTool,
    surface: &HashSet<String>,
    seen_ids: &mut HashSet<String>,
) -> Result<RawStreamingChoice<ClaudeStreamResponse>, CompletionError> {
    if tool.id.trim().is_empty() || tool.name.trim().is_empty() {
        return Err(CompletionError::ProviderError(
            CompleterParseError::MalformedToolUse {
                line: 0,
                message: "missing id or name".to_string(),
            }
            .to_string(),
        ));
    }
    if !seen_ids.insert(tool.id.clone()) {
        return Err(CompletionError::ProviderError(
            CompleterParseError::DuplicateToolUseId { id: tool.id }.to_string(),
        ));
    }
    if surface.is_empty() || !surface.contains(&tool.name) {
        return Err(CompletionError::ProviderError(
            CompleterParseError::ToolUse { names: tool.name }.to_string(),
        ));
    }
    let raw = tool.arguments_json();
    let input: Value = serde_json::from_str(&raw).map_err(|error| {
        CompletionError::ProviderError(
            CompleterParseError::MalformedToolUse {
                line: 0,
                message: format!("tool_use {} input is not JSON: {error}", tool.id),
            }
            .to_string(),
        )
    })?;
    Ok(RawStreamingChoice::ToolCall(RawStreamingToolCall::new(
        tool.id, tool.name, input,
    )))
}
```

In `parse_messages_sse`, add `let mut seen_ids: HashSet<String> = HashSet::new();` next to `let mut pending`, and replace the three arms:

```rust
            "content_block_start" => {
                if let Some(block) = payload.get("content_block") {
                    if block.get("type").and_then(Value::as_str) == Some("tool_use") {
                        let id = block
                            .get("id")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string();
                        if let Some(open) = pending.as_ref() {
                            let _ = open;
                            return Err(CompletionError::ProviderError(
                                CompleterParseError::OverlappingToolUse { id }.to_string(),
                            ));
                        }
                        let name = block
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string();
                        let start_input = match block.get("input") {
                            Some(Value::Object(_)) => Some(block["input"].to_string()),
                            _ => None,
                        };
                        pending = Some(PendingTool {
                            id,
                            name,
                            start_input,
                            deltas: String::new(),
                        });
                    }
                }
            }
            "content_block_delta" => {
                if let Some(delta) = payload.get("delta") {
                    match delta.get("type").and_then(Value::as_str) {
                        Some("text_delta") => {
                            if let Some(text) = delta.get("text").and_then(Value::as_str) {
                                if !text.is_empty() {
                                    events.push(RawStreamingChoice::Message(text.to_string()));
                                }
                            }
                        }
                        Some("input_json_delta") => {
                            if let Some(tool) = pending.as_mut() {
                                if let Some(partial) =
                                    delta.get("partial_json").and_then(Value::as_str)
                                {
                                    tool.deltas.push_str(partial);
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            "content_block_stop" => {
                if let Some(tool) = pending.take() {
                    events.push(mapped_tool_call(tool, surface, &mut seen_ids)?);
                }
            }
```

and the trailing flush becomes `events.push(mapped_tool_call(tool, surface, &mut seen_ids)?);`. Remove the `let _ = open;` line if clippy is happy with `if pending.is_some()` — use `if pending.is_some() { return Err(...) }` in the final code.

- [ ] **Step 5: Run the tests to verify they pass**

Run:
```bash
cargo test -p gents --lib claude_messages -- --nocapture
```
Expected: all `claude_messages::tests::*` PASS, including the six new ones and `sse_maps_echo_and_rejects_bash`.

- [ ] **Step 6: Full gates**

Run:
```bash
cargo test -p gents 2>&1 | tail -30
cargo check --workspace --all-targets 2>&1 | tail -5
```
Expected: all green; no warnings introduced in the two touched files.

- [ ] **Step 7: Commit**

The identity block + three tests already in the working tree (uncommitted from the previous session) are part of this commit; they are the routing fix that Task 2 verifies live.

```bash
git add crates/gents/src/claude_messages.rs crates/gents/src/claude_completer/mod.rs
git commit -m "fix(claude): accumulate tool_use input from deltas, fail closed on dup and overlap

C1: content_block_start.input ({}) was seeded and deltas appended, so every
streamed tool_use reached the loop with empty arguments. Deltas now win when
present, start input otherwise, {} when neither; unparseable input is an
error instead of a silent {}. C8: duplicate tool_use ids and a second
content_block_start before content_block_stop fail closed.

Also lands the Claude Code identity block as system[0] (write request #7)."
```

Do not `git add` `docs/superpowers/**`, `TODO.md`, `tasks/`, or `docs/design-notes/SPEC-claude-a2b-in-process.md`.

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

### Task 3: Split scope creep onto their own branches and revert on the spike

Git-only task, no source edits beyond one conflict resolution. Still runs in a Fable 5.1 subagent after approval, because the revert touches tracked source.

**Files:**
- Branches created: `fix/session-fork-retry` (← `0b89e688`, 7 files: `gents-cli/src/cli/args.rs`, `commands/codex_shim/thread_routes.rs`, `commands/session.rs`, `gents-cli/src/lib.rs`, `gents/src/session.rs`, `gents/src/session/fork.rs`, `gents/tests/e2e_runtime/fork_invariants.rs`), `fix/strip-idless-reasoning` (← `63ff2ff3`, 5 files: `proofs/Proofs/PromptAssembly/Content.lean`, `compaction.rs`, `compaction/history.rs`, `compaction/tests.rs`, `tests/conformance/prompt_assembly.rs`)
- Modify on spike (via `git revert`): the same 12 files; manual resolution in `crates/gents/tests/conformance/prompt_assembly.rs` (~L100-106)

**Interfaces:**
- Produces: the spike no longer contains `strip_idless_reasoning` (C3/C4) or the fork-retry flag; Task 5's driver edits in `tests/conformance/prompt_assembly.rs` start from the reverted `"other"` arm.

- [ ] **Step 1: Confirm the tree is clean apart from the known leftovers**

```bash
git status --porcelain
```
Expected: only ` M docs/design-notes/SPEC-claude-a2b-in-process.md`, `?? TODO.md`, `?? docs/superpowers/specs/…`, `?? docs/superpowers/plans/…`, `?? tasks/a2b2-interjection-plan.md`. Anything else: stop and report.

- [ ] **Step 2: Create the side branches in a throwaway git worktree**

A plain `git worktree add` is acceptable here because nothing is compiled in it (the `make worktree` rule exists to warm `target/`; these branches are validated by CI when their PRs open).

```bash
git worktree add .scratch/wt-split main
git -C .scratch/wt-split checkout -b fix/session-fork-retry
git -C .scratch/wt-split cherry-pick 0b89e688
git -C .scratch/wt-split checkout -b fix/strip-idless-reasoning main
git -C .scratch/wt-split cherry-pick 63ff2ff3
git -C .scratch/wt-split log --oneline main..fix/session-fork-retry main..fix/strip-idless-reasoning
git worktree remove .scratch/wt-split
```
Expected: two branches, one commit each, no conflicts (neither commit overlaps the other or anything on `main`).

- [ ] **Step 3: Record C3/C4 on the reasoning branch's commit trailer**

```bash
git branch --edit-description fix/strip-idless-reasoning
```
is interactive and unavailable; instead write `.scratch/claude-spike/logs/creep-split-notes.md`:

```markdown
# Creep split — 2026-09-03
- fix/session-fork-retry ← 0b89e688 (fork retry at last human user turn). Needs a Lean model of the cut point before review.
- fix/strip-idless-reasoning ← 63ff2ff3. Review findings C3 (unconditional 4th sanitize stage, Provider.lean still says three stages; conformance driver mints rs-lean-* ids so the stage is never exercised by spec) and C4 (provider_view over-drains legacy sessions with null compacted_through_sequence) must be addressed there, Lean-first.
```

- [ ] **Step 4: Revert both commits on the spike**

```bash
git revert --no-edit 0b89e688
git revert --no-edit 63ff2ff3
```
Expected: the first revert applies cleanly. The second stops with a conflict in `crates/gents/tests/conformance/prompt_assembly.rs` (the file was later touched by `fac2a94d`, which added the ClaudeMap driver).

- [ ] **Step 5: Resolve the conflict**

Keep everything `fac2a94d` added (the `generated_claude_map_cases_drive_the_completer_parser` test, `claude_map_assistant_line`, the `claude_completer` import). In the witness-to-Rust conversion (the `"other"` arm around L100-106), drop the id minting so the arm reads:

```rust
            "other" => AssistantContent::Reasoning(Reasoning::new(&reasoning_body(item.value))),
```

instead of

```rust
            "other" => AssistantContent::Reasoning(
                Reasoning::new(&reasoning_body(item.value))
                    .with_id(format!("rs-lean-{}", item.value)),
            ),
```

Then:
```bash
git add crates/gents/tests/conformance/prompt_assembly.rs
git -c core.editor=true revert --continue
```

- [ ] **Step 6: Gates**

```bash
export GENTS_SKIP_LENS_BUILD=1 GENTS_SKIP_CALLBACK_WASM_BUILD=1 TMPDIR="$PWD/.scratch/tmp"
(cd crates/gents/proofs && lake build 2>&1 | tail -3)
cargo test -p gents 2>&1 | tail -30
cargo check --workspace --all-targets 2>&1 | tail -5
```
Expected: `lake build` green (Content.lean is back to `main`'s text). All Rust tests green: the conformance sanitize cases no longer need minted ids because the stripping stage is gone.

- [ ] **Step 7: Verify the branch shape**

```bash
git log --oneline -4
git diff main --stat -- crates/gents/src/session/fork.rs crates/gents/src/compaction/history.rs
```
Expected: two revert commits on top; both diffs empty against `main`.

No further commit: the reverts are the commits.

### Task 4: Route every turn over Messages HTTP, omit `tools` when empty → live #9

**Files:**
- Modify: `crates/gents/src/claude_subscription.rs:253-269` (`ClaudeSubscriptionModel::stream`)
- Modify: `crates/gents/src/claude_messages.rs:74-111` (`build_messages_body`)
- Test: `crates/gents/src/claude_messages.rs` (`mod tests`)
- Create: `.scratch/claude-spike/logs/write-request-9.md`, `b3-live-http-text-*` evidence

**Interfaces:**
- Consumes: `stream_messages(model, &request, surface)` (existing).
- Produces: `build_messages_body` never emits a `tools` key for an empty surface (Lean `toolsField`, Task 5 witness `emptyTools`). Routing predicate is `seat.fake_completer.is_none()` only; the fake-completer branch survives until Task 6 deletes it.

- [ ] **Step 1: Write the failing test**

Append inside `mod tests` in `crates/gents/src/claude_messages.rs`:

```rust
    /// Lean `ClaudeMap.toolsField`: the wire never carries `tools: []`.
    #[test]
    fn messages_body_omits_tools_key_when_surface_is_empty() {
        let mut request = echo_request();
        request.tools.clear();
        let body = build_messages_body("claude-sonnet-5", &request);
        assert!(body.get("tools").is_none(), "{body}");
        let with_tools = build_messages_body("claude-sonnet-5", &echo_request());
        assert_eq!(with_tools["tools"][0]["name"], "echo");
    }
```

- [ ] **Step 2: Run it to verify it fails**

```bash
cargo test -p gents --lib claude_messages::tests::messages_body_omits_tools_key_when_surface_is_empty
```
Expected: FAIL (`tools` is `[]`).

- [ ] **Step 3: Implement**

In `build_messages_body`, replace the `let body = json!({ ... })` block with:

```rust
    let mut body = json!({
        "model": model,
        "max_tokens": request.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
        "stream": true,
        "system": system,
        "messages": anthropic_messages(request),
    });
    if !tools.is_empty() {
        body["tools"] = Value::Array(tools);
    }
```

In `ClaudeSubscriptionModel::stream` (`claude_subscription.rs:259`), change

```rust
        if !request.tools.is_empty() && seat.fake_completer.is_none() {
```
to
```rust
        if seat.fake_completer.is_none() {
```

- [ ] **Step 4: Run the tests**

```bash
cargo test -p gents --lib claude_ 2>&1 | tail -15
```
Expected: all green. `live_path_refuses_without_write_approval` now exercises the Messages refusal (`live Claude path refused: pass --claude-write-approved …`), which still contains `--claude-write-approved`.

- [ ] **Step 5: Gates and commit**

```bash
cargo test -p gents 2>&1 | tail -20
cargo check --workspace --all-targets 2>&1 | tail -5
git add crates/gents/src/claude_subscription.rs crates/gents/src/claude_messages.rs
git commit -m "feat(claude): route every turn over Messages HTTP; omit tools when empty

Write request #7 showed the OAuth 429 was identity routing, not a tool-turn
property. Text-only turns (and title generation) now take the same wire as
tool turns. tools is omitted rather than sent as []."
```

- [ ] **Step 6: Write request #9 and wait for approval**

`.scratch/claude-spike/logs/write-request-9.md`, same shape as #8. Hypothesis: a text-only turn (empty surface, no `tools` key) and the title-generation call both succeed over Messages HTTP with the identity block. Scope: server flags as in Task 2, one `gents chat` turn, fresh session, prompt `Reply with exactly: pong`. Success bar:
- response `pong`, `status=complete`;
- `RenderedRequest` rows for both `inference.1` and `title.1` have `source = claude_cli_subscription` and `capture_seam = transport_body` (no `process_cli` row);
- the `title.1` captured `request_json` has no `tools` key and no `temperature`;
- no 4xx/429 in `b3-live-http-text-server.stderr.log`;
- `OAuthCredential` Claude rows 0, token scan clean, default model unchanged.
`stop_reason` is not persisted by gents; `status=complete` stands in for `end_turn` (deviation from spec §6, recorded in the evidence).

Ask: "Write request #9 is ready. Approve #9?" Wait for an explicit approval naming #9.

- [ ] **Step 7: Run it**

Same commands as Task 2 steps 2, 4, 5 with the `b3-live-http-text-` prefix and the `pong` prompt. Then:

```bash
L=.scratch/claude-spike/logs; REQ=<request id>
./target/debug/gents query --home ~/.gents --collection RenderedRequest \
  --field capture_scope --field source --field provenance_json --field request_json \
  --filter "{\"request_id\":{\"_eq\":\"$REQ\"}}" > $L/b3-live-http-text-captures.json
jq '.results[] | {capture_scope, source, seam: (.provenance_json | fromjson | .capture_seam), has_tools: ((.request_json | fromjson) | has("tools"))}' $L/b3-live-http-text-captures.json
kill "$(cat $L/b3-live-http-text-server.pid)"
grep -l 'sk-ant\|Bearer ' $L/b3-live-http-text-* ; echo "token scan exit=$?"
```
Expected: two rows, both `transport_body`, `has_tools: false`.

- [ ] **Step 8: Evidence**

Write `.scratch/claude-spike/logs/b3-live-http-text-evidence.md` (PASS/FAIL table as in Task 2). **If FAIL, the cut stops here** (spec §6): report to the user and do not start Task 5 without a decision.

### Task 5: Lean slice — system assembly, stream accumulation, witnesses, re-pointed fence

This task is deliberately allowed to leave one Rust test red: `generated_claude_body_cases_drive_the_body_builder` expects `Message::System` rows inside `system[]`, which Task 6 implements. Every other test is green at the end of this task. The Lean build is zero-`sorry`.

**Files:**
- Modify: `crates/gents/proofs/Proofs/PromptAssembly/ClaudeMap.lean` (append after `mappedKind_calls`, before `end PromptAssembly.ClaudeMap`; extend `MapError`/`errorName`)
- Modify: `crates/gents/proofs/Proofs/Conformance/ContractCases/PromptAssembly.lean:524-554` (`surfaceOf`, two new witness structures + case lists before `end Conformance.ContractCases`)
- Modify: `crates/gents/proofs/Proofs/Conformance/Contracts/Json/PromptAssembly.lean:110-123` (two renderers)
- Modify: `crates/gents/proofs/Proofs/Conformance/Contracts/Json/Snapshot.lean:247-248` (two keys)
- Modify: `crates/gents/proofs/Proofs/Conformance/CoverageLedger.lean:995-999` (rename consumer id; two new entries)
- Modify: `crates/gents/src/lean_vocab_test/prompt_assembly.rs:75-84` (two structs)
- Modify: `crates/gents/src/lean_vocab_test/support.rs:169-170` (two fields), `:1013-1016` (two accessors)
- Modify: `crates/gents/tests/conformance/coverage.rs:817-822` (two emitted-set blocks)
- Modify: `crates/gents/tests/support/conformance_consumers.rs:674-680` (rename; two registry entries)
- Modify: `crates/gents/tests/conformance/prompt_assembly.rs:27-31,325-422` (imports, re-pointed driver, two new drivers)
- Modify: `crates/gents/src/claude_messages.rs` (`parse_messages_sse` and `build_messages_body` become `pub` if not already; add `pub fn claude_code_identity() -> &'static str` is unnecessary — `CLAUDE_CODE_IDENTITY` is already `pub`)

**Interfaces:**
- Consumes: `claude_messages::parse_messages_sse(&str, &HashSet<String>) -> Result<Vec<RawStreamingChoice<ClaudeStreamResponse>>, CompletionError>`; `claude_messages::build_messages_body(&str, &CompletionRequest) -> Value`; `claude_messages::CLAUDE_CODE_IDENTITY`; error Display strings from Task 1.
- Produces (snapshot keys and Rust types used by Task 6's green check):
  - `prompt_assembly_claude_body_cases: Vec<LeanPromptAssemblyClaudeBodyCase { name, preamble: Option<String>, rows: Vec<String>, tools: Vec<String>, system: Vec<String>, tools_present: bool }>`
  - `prompt_assembly_claude_stream_cases: Vec<LeanPromptAssemblyClaudeStreamCase { name, surface: Vec<String>, events: Vec<String>, outcome: String, calls: Vec<String> }>` where `events` tags are `text:<t>`, `start:<id>:<name>:<input-or-empty>`, `delta:<partial>`, `stop`; `calls` entries are `<id>=<argsJson>`.
  - Lean names: `ClaudeMap.identity`, `systemBlocks`, `splitSystem`, `toolsField`, `StreamEvent`, `accumulate`, `runStream`, `MapError.overlappingBlock`.

- [ ] **Step 1: Extend `MapError` in `ClaudeMap.lean`**

Replace the `MapError` inductive and `errorName`:

```lean
inductive MapError where
  | emptySurface
  | unmappedName (name : String)
  | duplicateId (id : ToolCallId)
  | overlappingBlock (id : ToolCallId)
  deriving DecidableEq, Repr

def errorName : MapError → String
  | .emptySurface => "emptySurface"
  | .unmappedName name => "unmappedName:" ++ name
  | .duplicateId id => "duplicateId:" ++ toString id
  | .overlappingBlock id => "overlappingBlock:" ++ toString id
```

- [ ] **Step 2: Append the system-assembly model to `ClaudeMap.lean`** (before `end PromptAssembly.ClaudeMap`)

```lean
/-! ## System assembly (single-wire Messages HTTP)

`Transcript.MessageRole` has no `system`; the wire-side row type here carries
just what assembly needs: a `System` row's text, or "some other row". -/

inductive Msg where
  | system (text : String)
  | other (tag : String)
  deriving DecidableEq, Repr

/-- The Claude Code identity block. `system[0]` on every request; the seat's
oat routes on it. Checked against Rust `CLAUDE_CODE_IDENTITY` by the vocab test. -/
def identity : String := "You are Claude Code, Anthropic's official CLI for Claude."

/-- Pull `System` rows out in order; everything else is untouched. -/
def splitSystem : List Msg → List String × List Msg
  | [] => ([], [])
  | .system t :: rest =>
    let (sys, others) := splitSystem rest
    (t :: sys, others)
  | m :: rest =>
    let (sys, others) := splitSystem rest
    (sys, m :: others)

def systemBlocks (preamble : Option String) (rows : List String) : List String :=
  identity :: (preamble.toList ++ rows)

theorem systemBlocks_head (preamble : Option String) (rows : List String) :
    (systemBlocks preamble rows).head? = some identity := rfl

theorem systemBlocks_tail_verbatim (preamble : Option String) (rows : List String) :
    (systemBlocks preamble rows).tail = preamble.toList ++ rows := rfl

def isSystem : Msg → Bool
  | .system _ => true
  | .other _ => false

/-- The remaining list contains no `System` row, and the split loses nothing:
the system texts are exactly the `System` rows in order. -/
theorem splitSystem_partition (msgs : List Msg) :
    (splitSystem msgs).2.all (fun m => !isSystem m) = true ∧
    (splitSystem msgs).1 = (msgs.filter isSystem).map (fun m =>
      match m with | .system t => t | .other _ => "") ∧
    (splitSystem msgs).2 = msgs.filter (fun m => !isSystem m) := by
  induction msgs with
  | nil => simp [splitSystem]
  | cons m rest ih =>
    obtain ⟨h1, h2, h3⟩ := ih
    cases m with
    | system t => simp [splitSystem, isSystem, List.filter, h1, h2, h3]
    | other tag => simp [splitSystem, isSystem, List.filter, h1, h2, h3]

/-- `tools` is absent from the wire for an empty surface. -/
def toolsField : List String → Option (List String)
  | [] => none
  | tools => some tools

theorem toolsField_empty : toolsField [] = none := rfl

theorem toolsField_nonempty (t : String) (rest : List String) :
    toolsField (t :: rest) = some (t :: rest) := rfl
```

If `splitSystem_partition` does not close with `simp` as written, split it into three theorems (`splitSystem_no_system_left`, `splitSystem_systems`, `splitSystem_others`) each by the same induction; the obligation is the same. Zero `sorry`.

- [ ] **Step 3: Append the stream-accumulation model to `ClaudeMap.lean`**

```lean
/-! ## Tool-block accumulation (SSE)

One `tool_use` block arrives as `content_block_start` (with a usually-empty
`input`), zero or more `input_json_delta` fragments, and `content_block_stop`.
Defect C1 seeded the start input and appended deltas (`{}{...}`). Here the
deltas are the arguments whenever any arrived. -/

inductive StreamEvent where
  | text (t : String)
  | start (id : ToolCallId) (name : String) (input : Option String)
  | delta (partial : String)
  | stop
  deriving DecidableEq, Repr

def accumulate (start : Option String) (deltas : List String) : String :=
  match deltas with
  | [] => start.getD "{}"
  | _ => String.join deltas

theorem accumulate_ignores_start_when_streamed (start : Option String)
    (deltas : List String) (h : deltas ≠ []) :
    accumulate start deltas = String.join deltas := by
  cases deltas with
  | nil => exact absurd rfl h
  | cons d rest => rfl

theorem accumulate_uses_start_when_no_deltas (start : Option String) :
    accumulate start [] = start.getD "{}" := rfl

structure Pending where
  id : ToolCallId
  name : String
  start : Option String
  /-- Deltas in reverse arrival order (consed); flushed as `deltas.reverse`. -/
  deltas : List String
  deriving Repr

structure StreamState where
  pending : Option Pending
  seen : List ToolCallId
  out : List (ToolCallId × String)
  deriving Repr

def StreamState.init : StreamState := { pending := none, seen := [], out := [] }

/-- Flush the pending block: duplicate id, then surface map, then arguments. -/
def flush (surface : Surface) (st : StreamState) : Except MapError StreamState :=
  match st.pending with
  | none => .ok st
  | some p =>
    if p.id ∈ st.seen then
      .error (.duplicateId p.id)
    else
      match mapToolUse surface p.id p.name with
      | .error e => .error e
      | .ok _ =>
        .ok { pending := none
            , seen := p.id :: st.seen
            , out := st.out ++ [(p.id, accumulate p.start p.deltas.reverse)] }

def step (surface : Surface) (st : StreamState) : StreamEvent → Except MapError StreamState
  | .text _ => .ok st
  | .start id name input =>
    match st.pending with
    | some _ => .error (.overlappingBlock id)
    | none => .ok { st with pending := some { id := id, name := name, start := input, deltas := [] } }
  | .delta partial =>
    match st.pending with
    | none => .ok st
    | some p => .ok { st with pending := some { p with deltas := partial :: p.deltas } }
  | .stop => flush surface st

/-- Left to right, first error wins; end of stream flushes an unterminated block. -/
def runStream (surface : Surface) (events : List StreamEvent) :
    Except MapError (List (ToolCallId × String)) :=
  (events.foldlM (step surface) StreamState.init >>= flush surface) |>.map (·.out)

theorem runStream_text_only (surface : Surface) :
    runStream surface [.text "hi"] = .ok [] := rfl

theorem runStream_deltas_win :
    runStream {("echo" : String)}
      [.start 1 "echo" (some "{}"), .delta "{\"text\":", .delta " \"hi\"}", .stop] =
      .ok [(1, "{\"text\": \"hi\"}")] := by
  native_decide

theorem runStream_start_input_without_deltas :
    runStream {("echo" : String)} [.start 1 "echo" (some "{\"a\":1}"), .stop] =
      .ok [(1, "{\"a\":1}")] := by
  native_decide

theorem runStream_overlap :
    runStream {("echo" : String)} [.start 1 "echo" none, .start 2 "echo" none, .stop] =
      .error (.overlappingBlock 2) := by
  native_decide

theorem runStream_duplicate :
    runStream {("echo" : String)}
      [.start 1 "echo" none, .stop, .start 1 "echo" none, .stop] =
      .error (.duplicateId 1) := by
  native_decide

theorem runStream_unterminated_flushes :
    runStream {("echo" : String)} [.start 1 "echo" none, .delta "{}"] = .ok [(1, "{}")] := by
  native_decide
```

`ToolCallId` is a `Nat` (see `ToolExecution`); literals `1`, `2` are fine. If `native_decide` is unavailable in this project's Mathlib pin, use `decide` or `rfl`; if `String` decidability blocks `decide`, use `by simp [runStream, step, flush, accumulate, mapToolUse]`.

- [ ] **Step 4: `lake build` the model**

```bash
cd crates/gents/proofs && lake build 2>&1 | tail -5 && grep -rn 'sorry' Proofs/PromptAssembly/ClaudeMap.lean ; echo "sorry-scan exit=$?"
```
Expected: build succeeds; no `sorry`.

- [ ] **Step 5: Witnesses in `Conformance/ContractCases/PromptAssembly.lean`**

Replace `surfaceOf`:

```lean
private def surfaceOf (names : List String) : PromptAssembly.ClaudeMap.Surface :=
  names.toFinset
```

Append before `end Conformance.ContractCases`:

```lean
/-! ## Claude Messages body (single wire)

`system` is `systemBlocks preamble (splitSystem rows).1`; `toolsPresent` is
`(toolsField tools).isSome`. Rows are tagged `system:<text>` / `other:<tag>`. -/

structure PromptAssemblyClaudeBodyCase where
  name : String
  preamble : Option String
  rows : List String
  tools : List String
  system : List String
  toolsPresent : Bool
  deriving Repr

private def msgTag : PromptAssembly.ClaudeMap.Msg → String
  | .system t => "system:" ++ t
  | .other tag => "other:" ++ tag

private def claudeBodyCase (name : String) (preamble : Option String)
    (rows : List PromptAssembly.ClaudeMap.Msg) (tools : List String) :
    PromptAssemblyClaudeBodyCase :=
  let (sys, _) := PromptAssembly.ClaudeMap.splitSystem rows
  { name := name
  , preamble := preamble
  , rows := rows.map msgTag
  , tools := tools
  , system := PromptAssembly.ClaudeMap.systemBlocks preamble sys
  , toolsPresent := (PromptAssembly.ClaudeMap.toolsField tools).isSome }

def promptAssemblyClaudeBodyCases : List PromptAssemblyClaudeBodyCase :=
  [ claudeBodyCase "identity-only" none [.other "user"] []
  , claudeBodyCase "preamble-after-identity" (some "You are helpful.") [.other "user"] ["echo"]
  , claudeBodyCase "system-rows-follow-preamble" (some "P")
      [.system "S1", .other "user", .system "S2", .other "assistant"] ["echo"]
  , claudeBodyCase "system-rows-without-preamble" none
      [.system "workspace context", .other "user"] []
  , claudeBodyCase "empty-tools-omitted" (some "P") [.other "user"] []
  , claudeBodyCase "two-tools-present" none [.other "user"] ["echo", "list_files"]
  ]

/-! ## Claude Messages SSE stream (single wire)

`outcome` / `calls` are `runStream`, not a hand oracle. Event tags:
`text:<t>`, `start:<id>:<name>:<input>` (empty input = none), `delta:<partial>`, `stop`. -/

structure PromptAssemblyClaudeStreamCase where
  name : String
  surface : List String
  events : List String
  outcome : String
  calls : List String
  deriving Repr

private def streamEventTag : PromptAssembly.ClaudeMap.StreamEvent → String
  | .text t => "text:" ++ t
  | .start id name input => s!"start:{id}:{name}:{input.getD ""}"
  | .delta partial => "delta:" ++ partial
  | .stop => "stop"

private def claudeStreamCase (name : String) (surface : List String)
    (events : List PromptAssembly.ClaudeMap.StreamEvent) : PromptAssemblyClaudeStreamCase :=
  match PromptAssembly.ClaudeMap.runStream (surfaceOf surface) events with
  | .ok calls =>
    { name := name, surface := surface, events := events.map streamEventTag
    , outcome := "ok", calls := calls.map (fun (id, args) => s!"{id}={args}") }
  | .error e =>
    { name := name, surface := surface, events := events.map streamEventTag
    , outcome := PromptAssembly.ClaudeMap.errorName e, calls := [] }

def promptAssemblyClaudeStreamCases : List PromptAssemblyClaudeStreamCase :=
  [ claudeStreamCase "text-only" [] [.text "hi"]
  , claudeStreamCase "deltas-win-over-start" ["echo"]
      [.start 1 "echo" (some "{}"), .delta "{\"text\":", .delta " \"hi\"}", .stop]
  , claudeStreamCase "start-input-without-deltas" ["echo"]
      [.start 1 "echo" (some "{\"a\":1}"), .stop]
  , claudeStreamCase "no-input-is-empty-object" ["echo"] [.start 1 "echo" none, .stop]
  , claudeStreamCase "overlapping-block" ["echo"]
      [.start 1 "echo" none, .start 2 "echo" none, .stop]
  , claudeStreamCase "duplicate-id" ["echo"]
      [.start 1 "echo" none, .stop, .start 1 "echo" none, .stop]
  , claudeStreamCase "unmapped-name" ["bash"] [.start 1 "Bash" none, .stop]
  , claudeStreamCase "empty-surface-tool-use" [] [.start 1 "echo" none, .stop]
  , claudeStreamCase "unterminated-flushes" ["echo"] [.start 1 "echo" none, .delta "{}"]
  , claudeStreamCase "two-blocks-in-order" ["echo", "list_files"]
      [.start 1 "echo" none, .delta "{}", .stop, .text "then", .start 2 "list_files" none, .delta "{\"path\":\".\"}", .stop]
  ]
```

- [ ] **Step 6: JSON renderers in `Contracts/Json/PromptAssembly.lean`** (before `end Conformance.Contracts`)

```lean
def promptAssemblyClaudeBodyCaseJson (witness : PromptAssemblyClaudeBodyCase) : String :=
  "{"
    ++ "\"name\":" ++ jsonString witness.name ++ ","
    ++ "\"preamble\":" ++ jsonOptionalString witness.preamble ++ ","
    ++ "\"rows\":" ++ jsonStringArray witness.rows ++ ","
    ++ "\"tools\":" ++ jsonStringArray witness.tools ++ ","
    ++ "\"system\":" ++ jsonStringArray witness.system ++ ","
    ++ "\"tools_present\":" ++ boolString witness.toolsPresent
    ++ "}"

def promptAssemblyClaudeBodyCasesJson : String :=
  jsonArray (promptAssemblyClaudeBodyCases.map promptAssemblyClaudeBodyCaseJson)

def promptAssemblyClaudeStreamCaseJson (witness : PromptAssemblyClaudeStreamCase) : String :=
  "{"
    ++ "\"name\":" ++ jsonString witness.name ++ ","
    ++ "\"surface\":" ++ jsonStringArray witness.surface ++ ","
    ++ "\"events\":" ++ jsonStringArray witness.events ++ ","
    ++ "\"outcome\":" ++ jsonString witness.outcome ++ ","
    ++ "\"calls\":" ++ jsonStringArray witness.calls
    ++ "}"

def promptAssemblyClaudeStreamCasesJson : String :=
  jsonArray (promptAssemblyClaudeStreamCases.map promptAssemblyClaudeStreamCaseJson)
```

`boolString` comes from `Conformance.ContractCases.Types` (already opened via `open Conformance.ContractCases`); `jsonOptionalString` from `ContractTypes`.

- [ ] **Step 7: Snapshot keys and ledger**

`Contracts/Json/Snapshot.lean`, after the `prompt_assembly_claude_map_cases` pair:

```lean
    ++ "\"prompt_assembly_claude_body_cases\":"
      ++ promptAssemblyClaudeBodyCasesJson ++ ","
    ++ "\"prompt_assembly_claude_stream_cases\":"
      ++ promptAssemblyClaudeStreamCasesJson ++ ","
```

`CoverageLedger.lean`: change the existing consumer id to `conformance::prompt_assembly::generated_claude_map_cases_drive_the_messages_parser`, and add after it:

```lean
  , tagged (consumerCoverage
      "prompt_assembly_cases"
      "PromptAssemblyClaudeBodyCases"
      "conformance::prompt_assembly::generated_claude_body_cases_drive_the_body_builder")
      "prompt-assembly" [Surface.runtimeInternal]
  , tagged (consumerCoverage
      "prompt_assembly_cases"
      "PromptAssemblyClaudeStreamCases"
      "conformance::prompt_assembly::generated_claude_stream_cases_drive_the_messages_parser")
      "prompt-assembly" [Surface.runtimeInternal]
```

```bash
cd crates/gents/proofs && lake build 2>&1 | tail -3 && lake env lean --run Proofs/Conformance/Contracts.lean | grep -o '"prompt_assembly_claude_[a-z_]*_cases"' 
```
Expected: three keys printed (`map`, `body`, `stream`).

- [ ] **Step 8: Rust snapshot structs, accessors, coverage**

`crates/gents/src/lean_vocab_test/prompt_assembly.rs`, after `LeanPromptAssemblyClaudeMapCase`:

```rust
/// A Claude Messages body witness computed by `ClaudeMap.systemBlocks` /
/// `splitSystem` / `toolsField`.
#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
pub(crate) struct LeanPromptAssemblyClaudeBodyCase {
    pub(crate) name: String,
    pub(crate) preamble: Option<String>,
    pub(crate) rows: Vec<String>,
    pub(crate) tools: Vec<String>,
    pub(crate) system: Vec<String>,
    pub(crate) tools_present: bool,
}

/// A Claude Messages SSE witness computed by `ClaudeMap.runStream`.
#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
pub(crate) struct LeanPromptAssemblyClaudeStreamCase {
    pub(crate) name: String,
    pub(crate) surface: Vec<String>,
    pub(crate) events: Vec<String>,
    pub(crate) outcome: String,
    pub(crate) calls: Vec<String>,
}
```

`support.rs` snapshot struct (after `prompt_assembly_claude_map_cases`):

```rust
    #[serde(default)]
    pub(crate) prompt_assembly_claude_body_cases: Vec<LeanPromptAssemblyClaudeBodyCase>,
    #[serde(default)]
    pub(crate) prompt_assembly_claude_stream_cases: Vec<LeanPromptAssemblyClaudeStreamCase>,
```

accessors (after `lean_prompt_assembly_claude_map_cases`):

```rust
pub(crate) fn lean_prompt_assembly_claude_body_cases() -> &'static [LeanPromptAssemblyClaudeBodyCase] {
    &lean_contract_snapshot().prompt_assembly_claude_body_cases
}

pub(crate) fn lean_prompt_assembly_claude_stream_cases() -> &'static [LeanPromptAssemblyClaudeStreamCase]
{
    &lean_contract_snapshot().prompt_assembly_claude_stream_cases
}
```

`tests/conformance/coverage.rs` after the claude_map block:

```rust
    if !snapshot.prompt_assembly_claude_body_cases.is_empty() {
        emitted.insert((
            "prompt_assembly_cases".to_string(),
            "PromptAssemblyClaudeBodyCases".to_string(),
        ));
    }
    if !snapshot.prompt_assembly_claude_stream_cases.is_empty() {
        emitted.insert((
            "prompt_assembly_cases".to_string(),
            "PromptAssemblyClaudeStreamCases".to_string(),
        ));
    }
```

`tests/support/conformance_consumers.rs`: rename the existing entry's `id` and `function` to `generated_claude_map_cases_drive_the_messages_parser`, and add two entries with the same `package`/`source_path`/`module_path` for `generated_claude_body_cases_drive_the_body_builder` and `generated_claude_stream_cases_drive_the_messages_parser`.

- [ ] **Step 9: Drivers in `tests/conformance/prompt_assembly.rs`**

Replace the `claude_completer` import and extend the lean_vocab import:

```rust
use gents::claude_messages::{CLAUDE_CODE_IDENTITY, build_messages_body, parse_messages_sse};
use rig::streaming::RawStreamingChoice;

use crate::lean_vocab_test::{
    LeanPromptAssemblyItem, LeanPromptAssemblyRow, lean_prompt_assembly_claude_body_cases,
    lean_prompt_assembly_claude_map_cases, lean_prompt_assembly_claude_stream_cases,
    lean_prompt_assembly_sanitize_cases,
};
```

Replace the map driver and `claude_map_assistant_line` with:

```rust
/// Fence: the Messages SSE parser reproduces `ClaudeMap.mapTurn` on every
/// generated witness (mapped ids, empty-surface / unmapped / duplicate fail closed).
#[test]
fn generated_claude_map_cases_drive_the_messages_parser() {
    let cases = lean_prompt_assembly_claude_map_cases();
    assert!(!cases.is_empty(), "Lean emitted no PromptAssembly Claude map cases");
    for case in cases {
        let surface: HashSet<String> = case.surface.iter().cloned().collect();
        let sse = claude_map_blocks_as_sse(&case.blocks);
        let parsed = parse_messages_sse(&sse, &surface);
        match case.outcome.as_str() {
            "ok" => {
                let events = parsed.unwrap_or_else(|err| panic!("case {} should map: {err}", case.name));
                let got_ids: Vec<u64> = events
                    .iter()
                    .filter_map(|event| match event {
                        RawStreamingChoice::ToolCall(call) => Some(
                            call.id.parse::<u64>().unwrap_or_else(|_| {
                                panic!("case {} mapped id {} is not a Nat", case.name, call.id)
                            }),
                        ),
                        _ => None,
                    })
                    .collect();
                assert_eq!(got_ids, case.ids, "mapped ids ({})", case.name);
            }
            "emptySurface" => {
                let err = parsed.expect_err("empty surface must fail closed").to_string();
                assert!(err.contains("fail-closed: tool_use observed"), "case {}: {err}", case.name);
            }
            outcome if outcome.starts_with("unmappedName:") => {
                let name = outcome.strip_prefix("unmappedName:").expect("prefix");
                let err = parsed.expect_err("unmapped name must fail closed").to_string();
                assert!(
                    err.contains("fail-closed: tool_use observed") && err.contains(name),
                    "case {}: {err}",
                    case.name
                );
            }
            outcome if outcome.starts_with("duplicateId:") => {
                let id = outcome.strip_prefix("duplicateId:").expect("prefix");
                let err = parsed.expect_err("duplicate id must fail closed").to_string();
                assert!(
                    err.contains(&format!("fail-closed: duplicate tool_use id {id}")),
                    "case {}: {err}",
                    case.name
                );
            }
            other => panic!("case {} unknown outcome {other}", case.name),
        }
    }
}

fn sse_event(payload: serde_json::Value) -> String {
    let kind = payload["type"].as_str().expect("type").to_string();
    format!("event: {kind}\ndata: {payload}\n\n")
}

/// Render ClaudeMap block tags (`text`, `toolUse:id:name`, `toolResult:id`) as
/// an assistant-turn SSE body. `toolResult` never appears in an assistant turn
/// and is skipped.
fn claude_map_blocks_as_sse(blocks: &[String]) -> String {
    let mut sse = String::new();
    for (index, block) in blocks.iter().enumerate() {
        if block == "text" {
            sse.push_str(&sse_event(serde_json::json!({
                "type": "content_block_start", "index": index,
                "content_block": {"type": "text", "text": ""}
            })));
            sse.push_str(&sse_event(serde_json::json!({
                "type": "content_block_delta", "index": index,
                "delta": {"type": "text_delta", "text": "hi"}
            })));
            sse.push_str(&sse_event(serde_json::json!({"type": "content_block_stop", "index": index})));
        } else if let Some(rest) = block.strip_prefix("toolUse:") {
            let (id, name) = rest
                .split_once(':')
                .unwrap_or_else(|| panic!("toolUse tag must be toolUse:id:name, got {block}"));
            sse.push_str(&sse_event(serde_json::json!({
                "type": "content_block_start", "index": index,
                "content_block": {"type": "tool_use", "id": id, "name": name, "input": {}}
            })));
            sse.push_str(&sse_event(serde_json::json!({"type": "content_block_stop", "index": index})));
        } else if block.starts_with("toolResult:") {
            continue;
        } else {
            panic!("unknown Claude map block tag {block}");
        }
    }
    sse.push_str(&sse_event(serde_json::json!({"type": "message_stop"})));
    sse
}

/// Fence: the Messages SSE parser reproduces `ClaudeMap.runStream` — deltas
/// win over the start input, overlap / duplicate / unmapped fail closed, an
/// unterminated block flushes at end of stream.
#[test]
fn generated_claude_stream_cases_drive_the_messages_parser() {
    let cases = lean_prompt_assembly_claude_stream_cases();
    assert!(!cases.is_empty(), "Lean emitted no PromptAssembly Claude stream cases");
    for case in cases {
        let surface: HashSet<String> = case.surface.iter().cloned().collect();
        let sse = claude_stream_events_as_sse(&case.events);
        let parsed = parse_messages_sse(&sse, &surface);
        match case.outcome.as_str() {
            "ok" => {
                let events = parsed.unwrap_or_else(|err| panic!("case {} should parse: {err}", case.name));
                let got: Vec<String> = events
                    .iter()
                    .filter_map(|event| match event {
                        RawStreamingChoice::ToolCall(call) => Some(format!("{}={}", call.id, call.arguments)),
                        _ => None,
                    })
                    .collect();
                let want: Vec<String> = case
                    .calls
                    .iter()
                    .map(|entry| {
                        let (id, args) = entry.split_once('=').expect("id=args");
                        let value: serde_json::Value = serde_json::from_str(args)
                            .unwrap_or_else(|err| panic!("case {} witness args {args}: {err}", case.name));
                        format!("{id}={value}")
                    })
                    .collect();
                assert_eq!(got, want, "calls ({})", case.name);
            }
            "emptySurface" => {
                let err = parsed.expect_err("empty surface must fail closed").to_string();
                assert!(err.contains("fail-closed: tool_use observed"), "case {}: {err}", case.name);
            }
            outcome if outcome.starts_with("unmappedName:") => {
                let name = outcome.strip_prefix("unmappedName:").expect("prefix");
                let err = parsed.expect_err("unmapped must fail closed").to_string();
                assert!(err.contains("fail-closed: tool_use observed") && err.contains(name), "case {}: {err}", case.name);
            }
            outcome if outcome.starts_with("duplicateId:") => {
                let id = outcome.strip_prefix("duplicateId:").expect("prefix");
                let err = parsed.expect_err("duplicate must fail closed").to_string();
                assert!(err.contains(&format!("fail-closed: duplicate tool_use id {id}")), "case {}: {err}", case.name);
            }
            outcome if outcome.starts_with("overlappingBlock:") => {
                let id = outcome.strip_prefix("overlappingBlock:").expect("prefix");
                let err = parsed.expect_err("overlap must fail closed").to_string();
                assert!(err.contains(&format!("fail-closed: overlapping tool_use block {id}")), "case {}: {err}", case.name);
            }
            other => panic!("case {} unknown outcome {other}", case.name),
        }
    }
}

/// Render `StreamEvent` tags as SSE. `start:<id>:<name>:<input>` with empty
/// input emits no `input` key (Lean `none`); a non-empty input is embedded as
/// the parsed JSON object.
fn claude_stream_events_as_sse(events: &[String]) -> String {
    let mut sse = String::new();
    for tag in events {
        if let Some(text) = tag.strip_prefix("text:") {
            sse.push_str(&sse_event(serde_json::json!({
                "type": "content_block_delta", "index": 0,
                "delta": {"type": "text_delta", "text": text}
            })));
        } else if let Some(rest) = tag.strip_prefix("start:") {
            let mut parts = rest.splitn(3, ':');
            let id = parts.next().expect("id");
            let name = parts.next().expect("name");
            let input = parts.next().unwrap_or("");
            let mut block = serde_json::json!({"type": "tool_use", "id": id, "name": name});
            if !input.is_empty() {
                block["input"] = serde_json::from_str(input)
                    .unwrap_or_else(|err| panic!("witness start input {input}: {err}"));
            }
            sse.push_str(&sse_event(serde_json::json!({
                "type": "content_block_start", "index": 0, "content_block": block
            })));
        } else if let Some(partial) = tag.strip_prefix("delta:") {
            sse.push_str(&sse_event(serde_json::json!({
                "type": "content_block_delta", "index": 0,
                "delta": {"type": "input_json_delta", "partial_json": partial}
            })));
        } else if tag == "stop" {
            sse.push_str(&sse_event(serde_json::json!({"type": "content_block_stop", "index": 0})));
        } else {
            panic!("unknown Claude stream event tag {tag}");
        }
    }
    sse
}

/// Fence: `build_messages_body` reproduces `ClaudeMap.systemBlocks` /
/// `splitSystem` / `toolsField` — identity first, preamble, then `System`
/// rows in order; `tools` absent for an empty surface.
#[test]
fn generated_claude_body_cases_drive_the_body_builder() {
    use rig::completion::message::{Message, Text, UserContent};
    use rig::one_or_many::OneOrMany;
    let cases = lean_prompt_assembly_claude_body_cases();
    assert!(!cases.is_empty(), "Lean emitted no PromptAssembly Claude body cases");
    for case in cases {
        let history: Vec<Message> = case
            .rows
            .iter()
            .map(|row| {
                if let Some(text) = row.strip_prefix("system:") {
                    Message::System { content: text.to_string() }
                } else if row == "other:assistant" {
                    Message::assistant("ok")
                } else {
                    Message::User {
                        content: OneOrMany::one(UserContent::Text(Text { text: "hi".into() })),
                    }
                }
            })
            .collect();
        let request = rig::completion::CompletionRequest {
            model: None,
            preamble: case.preamble.clone(),
            chat_history: OneOrMany::many(history).expect("at least one row"),
            documents: Vec::new(),
            tools: case
                .tools
                .iter()
                .map(|name| rig::completion::ToolDefinition {
                    name: name.clone(),
                    description: name.clone(),
                    parameters: serde_json::json!({"type": "object"}),
                })
                .collect(),
            temperature: None,
            max_tokens: None,
            tool_choice: None,
            additional_params: None,
            output_schema: None,
        };
        let body = build_messages_body("claude-sonnet-5", &request);
        let system: Vec<String> = body["system"]
            .as_array()
            .unwrap_or_else(|| panic!("case {} system array: {body}", case.name))
            .iter()
            .map(|block| block["text"].as_str().expect("text").to_string())
            .collect();
        assert_eq!(system, case.system, "system blocks ({})", case.name);
        assert_eq!(system[0], CLAUDE_CODE_IDENTITY, "case {}", case.name);
        assert_eq!(body.get("tools").is_some(), case.tools_present, "tools key ({})", case.name);
        let leaked = body["messages"].as_array().expect("messages").iter().any(|message| {
            message["content"].as_array().is_some_and(|blocks| {
                blocks.iter().any(|block| block["text"].as_str().is_some_and(|t| t.starts_with("system: ")))
            })
        });
        assert!(!leaked, "System row leaked into messages ({}): {body}", case.name);
    }
}
```

Add `use std::collections::HashSet;` to the file's imports if it is not there. `Message::System { content }` is the rig variant already matched in `anthropic_messages`.

- [ ] **Step 10: Vocab test for the identity constant**

In `crates/gents/src/claude_messages.rs` `mod tests`, add:

```rust
    /// Lean `ClaudeMap.identity` and Rust `CLAUDE_CODE_IDENTITY` are the same
    /// bytes; the body-cases fence checks `system[0]` against the witness, so
    /// this pins the constant even for a witness set with no rows.
    #[test]
    fn identity_matches_lean_body_witness_head() {
        let cases = crate::lean_vocab_test::lean_prompt_assembly_claude_body_cases();
        assert!(!cases.is_empty());
        for case in cases {
            assert_eq!(case.system[0], CLAUDE_CODE_IDENTITY, "{}", case.name);
        }
    }
```

If `lean_vocab_test` is only compiled under `#[cfg(test)]` in `lib.rs` (check `crates/gents/src/lib.rs` for `mod lean_vocab_test`), this test compiles as written.

- [ ] **Step 11: Run and record the expected red**

```bash
cargo test -p gents --test conformance -- prompt_assembly::generated_claude 2>&1 | tail -25
cargo test -p gents --lib claude_messages 2>&1 | tail -8
```
Expected:
- `generated_claude_map_cases_drive_the_messages_parser` PASS
- `generated_claude_stream_cases_drive_the_messages_parser` PASS (Task 1 already implements accumulate/dup/overlap)
- `generated_claude_body_cases_drive_the_body_builder` **FAIL** on `system-rows-follow-preamble` and `system-rows-without-preamble` (System rows currently land in `messages` as `system: …` user blocks). This is the fence for Task 6.
- `identity_matches_lean_body_witness_head` PASS.

If the conformance test binary is named differently, find it with `ls crates/gents/tests/*.rs` and use `--test <name>`.

- [ ] **Step 12: Gates and commit**

```bash
cargo test -p gents 2>&1 | grep -E 'test result|FAILED|failed' | head
cargo check --workspace --all-targets 2>&1 | tail -3
git add crates/gents/proofs crates/gents/src/lean_vocab_test crates/gents/tests crates/gents/src/claude_messages.rs
git commit -m "spec(claude): model system assembly and SSE tool-block accumulation; fence the Messages parser

ClaudeMap gains splitSystem/systemBlocks/toolsField and a StreamEvent
accumulation model (deltas win over start input; overlap, duplicate and
unmapped fail closed). Two witness sets drive build_messages_body and
parse_messages_sse; the existing map witnesses are re-pointed from the
JSONL parser to the Messages parser. The body fence is red on System-row
placement until the single-wire cut lands."
```
Expected: exactly one failing test in the package (`generated_claude_body_cases_drive_the_body_builder`); say so in the commit's PR notes. Do not add `docs/superpowers/`.

### Task 6: Single-wire cut — body, incremental streaming, delete the process-CLI wire → live #10

This is the largest task. It is one slice because the Task 5 fence (`generated_claude_body_cases_drive_the_body_builder`) defines its green and the CLI wire cannot be deleted piecemeal without leaving dead code behind. The subagent executing it should work through sub-steps 6.1 → 6.8 in order, compiling after each.

**Files:**
- Rewrite: `crates/gents/src/claude_messages.rs` (all of it; ~713 lines → ~650 with tests)
- Modify: `crates/gents/src/claude_subscription.rs` (seat struct L46-78; delete L122-168 probe stays until Task 7; delete `spawn_completer` L271-343, `stream_child_stdout` L344-490, `capture_claude_cli_request` L491-526, `flatten_completion_request` L527-598, `rig_usage_from_completer` L195-210; `stream` L253-269; tests L600-1428)
- Modify: `crates/gents/src/claude_completer/mod.rs` (keep `DEFAULT_MODEL_ID`, `STRIPPED_ENV_VARS`, `sanitize_child_env`, `parse_auth_status_logged_in`; delete everything else)
- Delete: `crates/gents/src/claude_completer/fixtures/` (5 files)
- Modify: `crates/gents/src/config_client/inference_backend.rs` (restore from `main`)
- Modify: `crates/gents-protocol/src/rendered_request.rs:652-666,725-745,1084-1099`
- Modify: `crates/gents/src/rendered_request/mod.rs:240-257,468-478,832-861`; `crates/gents/src/rendered_request/scope.rs:375-443,670-769`
- Rewrite: `crates/gents/src/agent/loop_stream/tests/claude.rs`
- Modify: `crates/gents/src/backend_health.rs:729-739` (`install_claude_seat` helper only)
- Modify: `crates/gents-cli/src/cli/args.rs:856-875` (drop `--claude-workdir`, `--claude-log-dir`, `--claude-fake-completer`), `crates/gents-cli/src/cli/args/tests.rs:878-916`, `crates/gents-cli/src/commands/serve.rs:82-137,859-874,1462-1515`
- Modify: `crates/gents/tests/conformance/prompt_assembly.rs` (no change expected; the seam scan must still list exactly two files)

**Interfaces:**
- Consumes: Task 1 parser semantics, Task 4 routing, Task 5 witnesses.
- Produces:
  - `claude_messages::install_messages_sse_fixtures(Vec<String>)` (test seam; FIFO, one per `stream_messages` call), `claude_messages::sse_fixture_tool_use(id: &str, name: &str, partial_json: &str) -> String`, `claude_messages::sse_fixture_text(text: &str) -> String` (both `#[cfg(test)] pub(crate)`).
  - `claude_messages::MessagesParseError` (Display strings from Task 1, minus `at line N`).
  - `claude_messages::MessagesSseState::new(surface: HashSet<String>)`, `push_line(&mut self, line: &str) -> Result<Vec<RawStreamingChoice<ClaudeStreamResponse>>, CompletionError>`, `finish(self) -> Result<Vec<…>, CompletionError>`; `parse_messages_sse` unchanged signature.
  - `claude_subscription::ClaudeSeatConfig { config_dir: PathBuf, write_approved: bool, claude_bin: PathBuf, http: ReqwestClient }` with `ClaudeSeatConfig::new(config_dir: PathBuf, write_approved: bool, claude_bin: Option<PathBuf>) -> Self`; `#[cfg(test)] pub(crate) fn install_fake_seat() -> tempfile::TempDir` (installs a seat with `write_approved: false` under a fresh tempdir and returns the guard). Task 7 removes `claude_bin`.
  - `gents_protocol::rendered_request::CaptureSeam` back to the single `TransportBody` variant; `ProvenanceManifest::captured_only` only; `gents::rendered_request::build_rendered_completion_request(context, capture_scope, source, provider_endpoint, turn_index, attempt, assembly_trace, components)` (no seam parameter).

Deviations recorded here: (1) `ClaudeSeatConfig.http` is rig's `ReqwestClient`, which is `pub use reqwest::Client` (already `Clone` + `Arc`-backed), rather than a second `Arc` wrapper; it is constructed once in `ClaudeSeatConfig::new`. (2) The fixture must be served *behind* `RenderedRequestCapturingHttpClient` so persist-before-send still runs in tests, so a 35-line `SeatTransport` (fixture-or-live `HttpClientExt`) remains; the spec's `ClaudeMessagesTransport` with whole-body buffering is gone.

#### 6.1 `claude_messages.rs` — fixture queue, error enum, body

- [ ] **Step 1: Write the failing body tests** (in `mod tests`)

```rust
    fn request_with_system_rows() -> CompletionRequest {
        let mut request = echo_request();
        request.chat_history = OneOrMany::many(vec![
            Message::System { content: "workspace context".into() },
            Message::User {
                content: OneOrMany::one(UserContent::Text(Text { text: "use echo".into() })),
            },
        ])
        .expect("two rows");
        request
    }

    #[test]
    fn messages_body_routes_system_rows_after_identity_and_preamble() {
        let body = build_messages_body("claude-sonnet-5", &request_with_system_rows());
        let system = body["system"].as_array().expect("system");
        assert_eq!(system.len(), 3, "{body}");
        assert_eq!(system[0]["text"], CLAUDE_CODE_IDENTITY);
        assert_eq!(system[1]["text"], "You are helpful.");
        assert_eq!(system[2]["text"], "workspace context");
        assert_eq!(body["messages"].as_array().expect("messages").len(), 1);
        assert_eq!(body["messages"][0]["role"], "user");
    }

    #[test]
    fn messages_body_marks_two_cache_breakpoints() {
        let body = build_messages_body("claude-sonnet-5", &request_with_system_rows());
        let system = body["system"].as_array().expect("system");
        assert_eq!(system.last().unwrap()["cache_control"]["type"], "ephemeral");
        assert!(system[0].get("cache_control").is_none());
        let last_message = body["messages"].as_array().unwrap().last().unwrap().clone();
        let last_block = last_message["content"].as_array().unwrap().last().unwrap().clone();
        assert_eq!(last_block["cache_control"]["type"], "ephemeral");
    }

    #[test]
    fn messages_body_never_carries_a_system_prefixed_user_block() {
        let body = build_messages_body("claude-sonnet-5", &request_with_system_rows());
        let leaked = body["messages"].as_array().unwrap().iter().any(|m| {
            m["content"].as_array().unwrap().iter().any(|b| {
                b["text"].as_str().is_some_and(|t| t.starts_with("system: "))
            })
        });
        assert!(!leaked, "{body}");
    }
```

- [ ] **Step 2: Run to verify failure**

```bash
cargo test -p gents --lib claude_messages::tests::messages_body_ 2>&1 | tail -12
```
Expected: the three new tests FAIL (system has 2 blocks; a `system: workspace context` user block exists; no `cache_control`).

- [ ] **Step 3: Replace the top of the file (header through `anthropic_messages`)**

```rust
//! Claude subscription seat over Anthropic Messages HTTP — the only wire.
//!
//! Every turn (tool-capable or not) is `POST /v1/messages` with the seat's
//! OAuth token, `system[0]` = [`CLAUDE_CODE_IDENTITY`], Gents preamble and
//! `Message::System` rows after it, and `tools` only when the surface is
//! non-empty. The SSE body is parsed incrementally by [`MessagesSseState`];
//! `tool_use` blocks map onto the gents surface or fail closed. Lean model:
//! `Proofs/PromptAssembly/ClaudeMap.lean` (system assembly, accumulation).

use std::collections::{HashSet, VecDeque};
use std::fmt;
use std::sync::{Mutex, OnceLock};

use bytes::Bytes;
use futures::StreamExt;
use rig::completion::{CompletionError, CompletionRequest};
use rig::http_client::{
    self, HeaderValue, HttpClientExt, LazyBody, MultipartForm, Request, ReqwestClient, Response,
    StreamingResponse,
};
use rig::streaming::{RawStreamingChoice, RawStreamingToolCall};
use rig::wasm_compat::WasmCompatSend;
use serde_json::{Value, json};
use thiserror::Error;

use crate::claude_seat_auth::read_seat_access_token;
use crate::claude_subscription::{ClaudeSeatConfig, ClaudeStreamResponse};
use crate::rendered_request::RenderedRequestCapturingHttpClient;

pub const MESSAGES_URI: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";
const OAUTH_BETA: &str = "oauth-2025-04-20";
const DEFAULT_MAX_TOKENS: u64 = 4096;

/// First `system` block. The seat's oat was minted for Claude Code; without
/// this identity the same token 429s on every model (write request #7).
/// Lean: `ClaudeMap.identity`.
pub const CLAUDE_CODE_IDENTITY: &str = "You are Claude Code, Anthropic's official CLI for Claude.";

/// Fail-closed outcomes of the Messages tool-block parser. Display strings are
/// matched by the conformance drivers; keep them stable.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum MessagesParseError {
    #[error("fail-closed: tool_use observed ({names})")]
    ToolUse { names: String },
    #[error("fail-closed: duplicate tool_use id {id}")]
    DuplicateToolUseId { id: String },
    #[error("fail-closed: malformed tool_use: {message}")]
    MalformedToolUse { message: String },
    #[error("fail-closed: overlapping tool_use block {id}")]
    OverlappingToolUse { id: String },
}

impl From<MessagesParseError> for CompletionError {
    fn from(error: MessagesParseError) -> Self {
        CompletionError::ProviderError(error.to_string())
    }
}

fn messages_sse_fixture_queue() -> &'static Mutex<VecDeque<String>> {
    static QUEUE: OnceLock<Mutex<VecDeque<String>>> = OnceLock::new();
    QUEUE.get_or_init(|| Mutex::new(VecDeque::new()))
}

/// Test-only: SSE bodies served instead of the network, one per
/// `stream_messages` call, in order. Cleared by `lock_process_seat_for_test`.
pub fn install_messages_sse_fixtures(bodies: Vec<String>) {
    *messages_sse_fixture_queue()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner()) = bodies.into_iter().collect();
}

fn take_messages_sse_fixture() -> Option<String> {
    messages_sse_fixture_queue()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .pop_front()
}

/// Anthropic Messages JSON body. Lean: `systemBlocks`, `splitSystem`,
/// `toolsField`. Two `cache_control` breakpoints: the last `system` block
/// (identity + preamble + System rows + tools prefix) and the last content
/// block of the last message (moving breakpoint across tool_result turns).
pub fn build_messages_body(model: &str, request: &CompletionRequest) -> Value {
    let mut system: Vec<Value> = vec![json!({ "type": "text", "text": CLAUDE_CODE_IDENTITY })];
    if let Some(preamble) = request
        .preamble
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        system.push(json!({ "type": "text", "text": preamble }));
    }
    for row in system_rows(request) {
        system.push(json!({ "type": "text", "text": row }));
    }
    mark_ephemeral(system.last_mut());

    let mut messages = anthropic_messages(request);
    if let Some(last) = messages.last_mut() {
        if let Some(blocks) = last.get_mut("content").and_then(Value::as_array_mut) {
            mark_ephemeral(blocks.last_mut());
        }
    }

    let tools: Vec<Value> = request
        .tools
        .iter()
        .map(|tool| {
            json!({
                "name": tool.name,
                "description": tool.description,
                "input_schema": tool.parameters,
            })
        })
        .collect();

    let mut body = json!({
        "model": model,
        "max_tokens": request.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
        "stream": true,
        "system": system,
        "messages": messages,
    });
    if !tools.is_empty() {
        body["tools"] = Value::Array(tools);
    }
    // No sampling keys: live claude-sonnet-5 400s on `temperature` / `top_p`
    // / `top_k`; `additional_params` carries those and is not merged.
    body
}

fn mark_ephemeral(block: Option<&mut Value>) {
    if let Some(Value::Object(map)) = block {
        map.insert("cache_control".to_string(), json!({ "type": "ephemeral" }));
    }
}

/// `Message::System` rows in transcript order (Lean `splitSystem`).
fn system_rows(request: &CompletionRequest) -> Vec<String> {
    request
        .chat_history
        .iter()
        .filter_map(|message| match message {
            rig::completion::Message::System { content } if !content.trim().is_empty() => {
                Some(content.clone())
            }
            _ => None,
        })
        .collect()
}
```

Then in `anthropic_messages`, replace the `Message::System` arm with `rig::completion::Message::System { .. } => {}` (rows were lifted into `system`). Keep the `User` and `Assistant` arms exactly as they are.

- [ ] **Step 4: Run the body tests**

```bash
cargo test -p gents --lib claude_messages::tests::messages_body_ 2>&1 | tail -12
```
Expected: all `messages_body_*` PASS (the file will not fully compile until 6.2 replaces the parser and `stream_messages`; if the subagent prefers, do 6.1–6.2 as one edit and run the tests after).

#### 6.2 `claude_messages.rs` — `MessagesSseState`, `parse_messages_sse`, streaming `stream_messages`

- [ ] **Step 1: Write the failing streaming tests** (in `mod tests`)

```rust
    #[test]
    fn push_line_yields_text_before_the_body_ends() {
        let mut state = MessagesSseState::new(HashSet::new());
        assert!(state.push_line("event: content_block_delta").unwrap().is_empty());
        let events = state
            .push_line(r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"hel"}}"#)
            .unwrap();
        assert!(events.is_empty(), "a data line is not complete until the blank line");
        let events = state.push_line("").unwrap();
        assert!(matches!(&events[..], [RawStreamingChoice::Message(t)] if t == "hel"));
        let events = state.finish().unwrap();
        assert!(matches!(&events[..], [RawStreamingChoice::FinalResponse(_)]));
    }

    #[test]
    fn sse_error_event_becomes_provider_error() {
        let sse = "event: error\ndata: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"Overloaded\"}}\n\n";
        let err = parse_messages_sse(sse, &HashSet::new()).expect_err("error event");
        let msg = err.to_string();
        assert!(msg.contains("overloaded_error") && msg.contains("Overloaded"), "{msg}");
    }

    #[tokio::test]
    async fn chunk_boundaries_do_not_change_the_event_sequence() {
        let sse = format!(
            "{}{}",
            sse_fixture_text("hello world"),
            sse_fixture_tool_use("toolu_1", "echo", "{\"text\":\"hi\"}")
        );
        let surface = HashSet::from(["echo".to_string()]);
        let whole: Vec<String> = parse_messages_sse(&sse, &surface)
            .unwrap()
            .iter()
            .map(|e| format!("{e:?}"))
            .collect();
        for chunk_len in [1usize, 3, 7, 64, 4096] {
            let chunks: Vec<Result<Bytes, http_client::Error>> = sse
                .as_bytes()
                .chunks(chunk_len)
                .map(|c| Ok(Bytes::copy_from_slice(c)))
                .collect();
            let body: http_client::sse::BoxedStream = Box::pin(futures::stream::iter(chunks));
            let events: Vec<String> = stream_sse_body(body, MessagesSseState::new(surface.clone()))
                .map(|e| format!("{:?}", e.expect("event")))
                .collect()
                .await;
            assert_eq!(events, whole, "chunk_len={chunk_len}");
        }
    }

    #[tokio::test]
    async fn first_text_event_is_observable_before_the_body_is_exhausted() {
        let (mut tx, rx) = futures::channel::mpsc::unbounded::<Result<Bytes, http_client::Error>>();
        let body: http_client::sse::BoxedStream = Box::pin(rx);
        let mut events = stream_sse_body(body, MessagesSseState::new(HashSet::new()));
        tx.unbounded_send(Ok(Bytes::from(sse_fixture_text("first")))).unwrap();
        let first = events.next().await.expect("event").expect("ok");
        assert!(matches!(first, RawStreamingChoice::Message(ref t) if t == "first"));
        tx.unbounded_send(Ok(Bytes::from_static(b"event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n"))).unwrap();
        drop(tx);
        let rest: Vec<_> = events.collect().await;
        assert!(matches!(rest.last().unwrap().as_ref().unwrap(), RawStreamingChoice::FinalResponse(_)));
    }
```

`sse_fixture_text` emits `content_block_start` (text) + one `text_delta` + `content_block_stop`; it must **not** emit `message_stop` (so the test above can append it). `sse_fixture_tool_use` emits `content_block_start` (tool_use, `input: {}`) + one `input_json_delta` with `partial_json` + `content_block_stop` + `message_delta` (with `usage: {input_tokens: 10, output_tokens: 5}`) + `message_stop`.

- [ ] **Step 2: Replace the parser section** (from `/// Parse Anthropic Messages SSE` through `sse_data_payloads`)

```rust
/// Incremental SSE parser for one Messages response.
///
/// Feed lines with [`push_line`]; each completed event (terminated by a blank
/// line) may yield zero or more `RawStreamingChoice`s. [`finish`] flushes an
/// unterminated `tool_use` block and guarantees exactly one `FinalResponse`.
/// Lean: `ClaudeMap.runStream` (`step` / `flush`).
pub struct MessagesSseState {
    surface: HashSet<String>,
    pending: Option<PendingTool>,
    seen_ids: HashSet<String>,
    usage: Option<rig::completion::Usage>,
    data: String,
    finished: bool,
    /// `request-id` response header, carried into stream-error messages only.
    request_id: Option<String>,
}

impl MessagesSseState {
    pub fn new(surface: HashSet<String>) -> Self {
        Self {
            surface,
            pending: None,
            seen_ids: HashSet::new(),
            usage: None,
            data: String::new(),
            finished: false,
            request_id: None,
        }
    }

    pub fn with_request_id(mut self, request_id: Option<String>) -> Self {
        self.request_id = request_id;
        self
    }

    /// One SSE line without its trailing newline. `data:` lines accumulate;
    /// a blank line dispatches the accumulated payload.
    pub fn push_line(
        &mut self,
        line: &str,
    ) -> Result<Vec<RawStreamingChoice<ClaudeStreamResponse>>, CompletionError> {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if let Some(rest) = line.strip_prefix("data:") {
            if !self.data.is_empty() {
                self.data.push('\n');
            }
            self.data.push_str(rest.trim());
            return Ok(Vec::new());
        }
        if !line.is_empty() {
            // `event:`, `id:`, comments — the payload's own `type` is authoritative.
            return Ok(Vec::new());
        }
        self.dispatch_pending_data()
    }

    /// End of body: flush an open block and emit `FinalResponse` if none seen.
    pub fn finish(
        mut self,
    ) -> Result<Vec<RawStreamingChoice<ClaudeStreamResponse>>, CompletionError> {
        let mut events = self.dispatch_pending_data()?;
        if let Some(tool) = self.pending.take() {
            events.push(mapped_tool_call(tool, &self.surface, &mut self.seen_ids)?);
        }
        if !self.finished {
            self.finished = true;
            events.push(RawStreamingChoice::FinalResponse(ClaudeStreamResponse {
                usage: self.usage,
            }));
        }
        Ok(events)
    }

    fn dispatch_pending_data(
        &mut self,
    ) -> Result<Vec<RawStreamingChoice<ClaudeStreamResponse>>, CompletionError> {
        if self.data.is_empty() {
            return Ok(Vec::new());
        }
        let raw = std::mem::take(&mut self.data);
        let payload: Value = match serde_json::from_str(&raw) {
            Ok(value) => value,
            Err(_) => return Ok(Vec::new()),
        };
        self.handle_payload(&payload)
    }

    fn handle_payload(
        &mut self,
        payload: &Value,
    ) -> Result<Vec<RawStreamingChoice<ClaudeStreamResponse>>, CompletionError> {
        let mut events = Vec::new();
        let Some(kind) = payload.get("type").and_then(Value::as_str) else {
            return Ok(events);
        };
        match kind {
            "content_block_start" => {
                let Some(block) = payload.get("content_block") else {
                    return Ok(events);
                };
                if block.get("type").and_then(Value::as_str) != Some("tool_use") {
                    return Ok(events);
                }
                let id = block.get("id").and_then(Value::as_str).unwrap_or("").to_string();
                if self.pending.is_some() {
                    return Err(MessagesParseError::OverlappingToolUse { id }.into());
                }
                let name = block.get("name").and_then(Value::as_str).unwrap_or("").to_string();
                let start_input = match block.get("input") {
                    Some(Value::Object(_)) => Some(block["input"].to_string()),
                    _ => None,
                };
                self.pending = Some(PendingTool { id, name, start_input, deltas: String::new() });
            }
            "content_block_delta" => {
                let Some(delta) = payload.get("delta") else {
                    return Ok(events);
                };
                match delta.get("type").and_then(Value::as_str) {
                    Some("text_delta") => {
                        if let Some(text) = delta.get("text").and_then(Value::as_str) {
                            if !text.is_empty() {
                                events.push(RawStreamingChoice::Message(text.to_string()));
                            }
                        }
                    }
                    Some("input_json_delta") => {
                        if let (Some(tool), Some(partial)) = (
                            self.pending.as_mut(),
                            delta.get("partial_json").and_then(Value::as_str),
                        ) {
                            tool.deltas.push_str(partial);
                        }
                    }
                    _ => {}
                }
            }
            "content_block_stop" => {
                if let Some(tool) = self.pending.take() {
                    events.push(mapped_tool_call(tool, &self.surface, &mut self.seen_ids)?);
                }
            }
            "message_delta" => {
                if let Some(value) = payload.get("usage") {
                    self.usage = Some(usage_from_sse(value));
                }
            }
            "message_stop" => {
                if !self.finished {
                    self.finished = true;
                    events.push(RawStreamingChoice::FinalResponse(ClaudeStreamResponse {
                        usage: self.usage,
                    }));
                }
            }
            "error" => {
                let error = payload.get("error").cloned().unwrap_or(Value::Null);
                let error_type = error.get("type").and_then(Value::as_str).unwrap_or("error");
                let message = error.get("message").and_then(Value::as_str).unwrap_or("");
                let request_id = self.request_id.as_deref().unwrap_or("-");
                return Err(CompletionError::ProviderError(format!(
                    "Claude Messages stream error {error_type}: {message} (request-id {request_id})"
                )));
            }
            _ => {}
        }
        Ok(events)
    }
}

/// All-lines wrapper over [`MessagesSseState`] for tests and the conformance
/// drivers.
pub fn parse_messages_sse(
    sse: &str,
    surface: &HashSet<String>,
) -> Result<Vec<RawStreamingChoice<ClaudeStreamResponse>>, CompletionError> {
    let mut state = MessagesSseState::new(surface.clone());
    let mut events = Vec::new();
    for line in sse.lines() {
        events.extend(state.push_line(line)?);
    }
    events.extend(state.finish()?);
    Ok(events)
}

struct PendingTool {
    id: String,
    name: String,
    /// `content_block.input` from `content_block_start`, serialized. Anthropic
    /// sends `{}` here and streams the real arguments as deltas.
    start_input: Option<String>,
    /// Concatenated `input_json_delta.partial_json` fragments, in order.
    deltas: String,
}

impl PendingTool {
    /// Lean `ClaudeMap.accumulate`: deltas win when any arrived; otherwise the
    /// start input; otherwise `{}`.
    fn arguments_json(&self) -> String {
        if !self.deltas.is_empty() {
            self.deltas.clone()
        } else {
            self.start_input.clone().unwrap_or_else(|| "{}".to_string())
        }
    }
}

fn mapped_tool_call(
    tool: PendingTool,
    surface: &HashSet<String>,
    seen_ids: &mut HashSet<String>,
) -> Result<RawStreamingChoice<ClaudeStreamResponse>, CompletionError> {
    if tool.id.trim().is_empty() || tool.name.trim().is_empty() {
        return Err(MessagesParseError::MalformedToolUse {
            message: "missing id or name".to_string(),
        }
        .into());
    }
    if !seen_ids.insert(tool.id.clone()) {
        return Err(MessagesParseError::DuplicateToolUseId { id: tool.id }.into());
    }
    if surface.is_empty() || !surface.contains(&tool.name) {
        return Err(MessagesParseError::ToolUse { names: tool.name }.into());
    }
    let raw = tool.arguments_json();
    let input: Value = serde_json::from_str(&raw).map_err(|error| {
        MessagesParseError::MalformedToolUse {
            message: format!("tool_use {} input is not JSON: {error}", tool.id),
        }
    })?;
    Ok(RawStreamingChoice::ToolCall(RawStreamingToolCall::new(tool.id, tool.name, input)))
}
```

Keep `usage_from_sse` as it is. Delete `sse_data_payloads`.

- [ ] **Step 3: Replace `stream_messages` and the transport**

```rust
/// Incremental line-splitter over a response body.
pub(crate) fn stream_sse_body(
    body: http_client::sse::BoxedStream,
    state: MessagesSseState,
) -> impl futures::Stream<Item = Result<RawStreamingChoice<ClaudeStreamResponse>, CompletionError>>
{
    async_stream::stream! {
        let mut body = body;
        let mut state = Some(state);
        let mut buffer: Vec<u8> = Vec::new();
        while let Some(chunk) = body.next().await {
            let chunk = match chunk {
                Ok(chunk) => chunk,
                Err(error) => {
                    yield Err(CompletionError::ProviderError(format!("Claude Messages body: {error}")));
                    return;
                }
            };
            buffer.extend_from_slice(&chunk);
            while let Some(newline) = buffer.iter().position(|byte| *byte == b'\n') {
                let line: Vec<u8> = buffer.drain(..=newline).collect();
                let line = String::from_utf8_lossy(&line[..line.len() - 1]).into_owned();
                let Some(current) = state.as_mut() else { return; };
                match current.push_line(&line) {
                    Ok(events) => {
                        for event in events {
                            yield Ok(event);
                        }
                    }
                    Err(error) => {
                        state = None;
                        yield Err(error);
                        return;
                    }
                }
            }
        }
        let Some(mut current) = state.take() else { return; };
        if !buffer.is_empty() {
            let line = String::from_utf8_lossy(&buffer).into_owned();
            match current.push_line(&line) {
                Ok(events) => for event in events { yield Ok(event); },
                Err(error) => { yield Err(error); return; }
            }
        }
        match current.finish() {
            Ok(events) => for event in events { yield Ok(event); },
            Err(error) => yield Err(error),
        }
    }
}

pub async fn stream_messages(
    model: &str,
    request: &CompletionRequest,
    surface: HashSet<String>,
) -> Result<
    impl futures::Stream<Item = Result<RawStreamingChoice<ClaudeStreamResponse>, CompletionError>>,
    CompletionError,
> {
    let seat: ClaudeSeatConfig = crate::claude_subscription::require_process_seat()?;
    let fixture = take_messages_sse_fixture();
    if fixture.is_none() && !seat.write_approved {
        return Err(CompletionError::ProviderError(
            "live Claude path refused: pass --claude-write-approved after an explicit numbered write approval"
                .to_string(),
        ));
    }

    let body = build_messages_body(model, request);
    let body_bytes = serde_json::to_vec(&body).map_err(|error| {
        CompletionError::ProviderError(format!("encode Claude Messages body: {error}"))
    })?;

    let mut builder = Request::builder()
        .method("POST")
        .uri(MESSAGES_URI)
        .header("content-type", "application/json")
        .header("anthropic-version", ANTHROPIC_VERSION)
        .header("anthropic-beta", OAUTH_BETA);
    if fixture.is_none() {
        let token = read_seat_access_token(&seat.config_dir).map_err(|error| {
            CompletionError::ProviderError(format!("Claude Messages seat auth: {error}"))
        })?;
        builder = builder.header(
            "authorization",
            HeaderValue::from_str(&token.authorization_value()).map_err(|error| {
                CompletionError::ProviderError(format!("Claude Messages auth header: {error}"))
            })?,
        );
        tracing::info!(
            model = %model,
            config_dir = %seat.config_dir.display(),
            "live Claude Messages HTTP send (write gate open; this process may bill Claude)"
        );
    }
    let http_request = builder.body(Bytes::from(body_bytes)).map_err(|error| {
        CompletionError::ProviderError(format!("Claude Messages request: {error}"))
    })?;

    let client = RenderedRequestCapturingHttpClient::new(SeatTransport {
        fixture,
        live: seat.http.clone(),
    });
    let response = client.send_streaming(http_request).await?;
    let status = response.status();
    let request_id = response
        .headers()
        .get("request-id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    if !status.is_success() {
        return Err(CompletionError::ProviderError(format!(
            "Claude Messages HTTP {status} (request-id {})",
            request_id.as_deref().unwrap_or("-")
        )));
    }
    Ok(stream_sse_body(
        response.into_body(),
        MessagesSseState::new(surface).with_request_id(request_id),
    ))
}

/// Serves a queued SSE fixture or forwards to the seat's shared client. Sits
/// behind `RenderedRequestCapturingHttpClient` so persist-before-send runs
/// for fixtures too.
#[derive(Clone)]
struct SeatTransport {
    fixture: Option<String>,
    live: ReqwestClient,
}
```

Keep the existing `impl HttpClientExt for SeatTransport` bodies (rename from `ClaudeMessagesTransport`; `sse_fixture` → `fixture`) and the `Debug` impl. Delete `MESSAGES_BODY_ALLOWED_KEYS`, `messages_body_has_only_allowed_keys`, `messages_sse_fixture`, `install_messages_sse_fixture`.

- [ ] **Step 4: Test helpers** (module level, before `mod tests`)

```rust
#[cfg(test)]
pub(crate) fn sse_fixture_text(text: &str) -> String {
    let text = serde_json::to_string(text).expect("escape");
    format!(
        "event: content_block_start\ndata: {{\"type\":\"content_block_start\",\"index\":0,\"content_block\":{{\"type\":\"text\",\"text\":\"\"}}}}\n\n\
         event: content_block_delta\ndata: {{\"type\":\"content_block_delta\",\"index\":0,\"delta\":{{\"type\":\"text_delta\",\"text\":{text}}}}}\n\n\
         event: content_block_stop\ndata: {{\"type\":\"content_block_stop\",\"index\":0}}\n\n"
    )
}

#[cfg(test)]
pub(crate) fn sse_fixture_tool_use(id: &str, name: &str, partial_json: &str) -> String {
    let partial = serde_json::to_string(partial_json).expect("escape");
    format!(
        "event: content_block_start\ndata: {{\"type\":\"content_block_start\",\"index\":0,\"content_block\":{{\"type\":\"tool_use\",\"id\":\"{id}\",\"name\":\"{name}\",\"input\":{{}}}}}}\n\n\
         event: content_block_delta\ndata: {{\"type\":\"content_block_delta\",\"index\":0,\"delta\":{{\"type\":\"input_json_delta\",\"partial_json\":{partial}}}}}\n\n\
         event: content_block_stop\ndata: {{\"type\":\"content_block_stop\",\"index\":0}}\n\n\
         event: message_delta\ndata: {{\"type\":\"message_delta\",\"delta\":{{\"stop_reason\":\"tool_use\"}},\"usage\":{{\"input_tokens\":10,\"output_tokens\":5}}}}\n\n\
         event: message_stop\ndata: {{\"type\":\"message_stop\"}}\n\n"
    )
}

#[cfg(test)]
pub(crate) fn sse_fixture_final_text(text: &str) -> String {
    format!(
        "{}event: message_delta\ndata: {{\"type\":\"message_delta\",\"delta\":{{\"stop_reason\":\"end_turn\"}},\"usage\":{{\"input_tokens\":12,\"output_tokens\":3}}}}\n\nevent: message_stop\ndata: {{\"type\":\"message_stop\"}}\n\n",
        sse_fixture_text(text)
    )
}
```

In `mod tests`, replace the hand-written `sse_tool_use_block` helper from Task 1 with calls to these where the shapes coincide, delete `claude_code_identity_stays_off_the_process_cli_wire`, and keep every other Task 1 test (their expected strings drop `at line 0`; the `contains("fail-closed: malformed tool_use")` assertions still hold).

- [ ] **Step 5: Run the module tests**

```bash
cargo test -p gents --lib claude_messages 2>&1 | tail -20
```
Expected: all PASS, including the four streaming tests from Step 1. (The crate will not compile until 6.3 updates `claude_subscription.rs`; do 6.1–6.3 before the first `cargo test` if needed.)

#### 6.3 `claude_subscription.rs` — seat struct, delegate `stream`, delete the CLI wire

- [ ] **Step 1: Seat struct and constructor** (replace L46-78)

```rust
#[derive(Debug, Clone)]
pub struct ClaudeSeatConfig {
    pub config_dir: PathBuf,
    pub write_approved: bool,
    /// Used only by the health probe until Task 7 replaces it with a token read.
    pub claude_bin: PathBuf,
    /// Shared HTTP client for every Messages request on this seat.
    pub http: rig::http_client::ReqwestClient,
}

impl ClaudeSeatConfig {
    pub fn new(config_dir: PathBuf, write_approved: bool, claude_bin: Option<PathBuf>) -> Self {
        Self {
            config_dir,
            write_approved,
            claude_bin: claude_bin.unwrap_or_else(|| PathBuf::from("claude")),
            http: rig::http_client::ReqwestClient::new(),
        }
    }
}

/// Test-only: install a refuse-closed seat under a fresh tempdir. The
/// returned guard owns the directory; keep it alive for the test.
#[cfg(test)]
pub(crate) fn install_fake_seat() -> tempfile::TempDir {
    let temp = tempfile::tempdir().expect("seat tempdir");
    let config_dir = temp.path().join("claude-config");
    std::fs::create_dir_all(&config_dir).expect("config dir");
    install_process_seat(Some(ClaudeSeatConfig::new(config_dir, false, None)));
    temp
}
```

`lock_process_seat_for_test` clears the queue: `crate::claude_messages::install_messages_sse_fixtures(Vec::new());`.

- [ ] **Step 2: `stream` delegates unconditionally** (replace L253-269)

```rust
    async fn stream(
        &self,
        request: CompletionRequest,
    ) -> Result<StreamingCompletionResponse<Self::StreamingResponse>, CompletionError> {
        let surface: HashSet<String> = request.tools.iter().map(|tool| tool.name.clone()).collect();
        let stream = crate::claude_messages::stream_messages(&self.model, &request, surface).await?;
        Ok(StreamingCompletionResponse::stream(Box::pin(stream)))
    }
```

- [ ] **Step 3: Delete the CLI wire**

Remove `spawn_completer`, `stream_child_stdout`, `capture_claude_cli_request`, `flatten_completion_request`, `rig_usage_from_completer`, and the imports they carried (`Stdio`, `AsyncBufReadExt`, `BufReader`, `Command` stays only if `probe_seat_auth_status` still uses it — it does until Task 7; `StreamJsonlEvent`, `StreamJsonlState`, `completer_argv`, `CompleterUsage`). Update the module doc comment to: "Claude subscription seat: process-local `--claude-config-dir` state, one Messages HTTP wire (`claude_messages`), refuse-closed without `--claude-write-approved`."

- [ ] **Step 4: Rewrite `mod tests`**

Delete: `workspace_tempdir`, `write_fake_completer`, `write_delayed_jsonl_fake`, `process_cli_capture_claims_armed_scope_before_fake_completer`, `stream_yields_jsonl_text_before_completer_exits`, `stream_reports_result_usage_on_final`, `process_cli_unexplained_send_inside_scope_does_not_spawn`, `process_cli_capture_failure_does_not_spawn_fake_completer`, `flatten_includes_preamble_and_user_text`, `fake_completer_stream_returns_text_without_write_approval`, `stream_maps_gents_named_tool_use_from_fake_jsonl`, `stream_fail_closes_bash_when_surface_is_bash`, `flatten_includes_tool_call_and_result`, `fake_second_turn_sees_flattened_tool_result`, the old `install_fake_seat` and `echo_tool_use_sse`.

Keep `parse_aliases_round_trip`. Keep one `ping_request()` (the text-only `CompletionRequest` from `live_path_refuses_without_write_approval`, `preamble: None`, `tools: Vec::new()`) and one `echo_tool_request()`. Rewrite the remaining tests onto the helpers:

```rust
    #[tokio::test]
    async fn live_path_refuses_without_write_approval() {
        let _guard = lock_process_seat_for_test();
        let _seat = install_fake_seat();
        let model = ClaudeSubscriptionClient::new().completion_model("claude-sonnet-5");
        let err = model.completion(ping_request()).await.expect_err("refused");
        assert!(err.to_string().contains("--claude-write-approved"), "{err}");
    }

    #[tokio::test]
    async fn messages_http_fixture_maps_gents_tool_use_with_arguments() {
        let _guard = lock_process_seat_for_test();
        let _seat = install_fake_seat();
        crate::claude_messages::install_messages_sse_fixtures(vec![
            crate::claude_messages::sse_fixture_tool_use("toolu_1", "echo", "{\"text\":\"hi\"}"),
        ]);
        let model = ClaudeSubscriptionClient::new().completion_model("claude-sonnet-5");
        let mut stream = model.stream(echo_tool_request()).await.expect("stream");
        use futures::StreamExt;
        use rig::streaming::StreamedAssistantContent;
        let mut calls = Vec::new();
        let mut usage = None;
        while let Some(item) = stream.next().await {
            match item.expect("chunk") {
                StreamedAssistantContent::ToolCall { tool_call, .. } => {
                    calls.push((tool_call.id, tool_call.function.name, tool_call.function.arguments));
                }
                StreamedAssistantContent::Final(final_response) => {
                    usage = final_response.token_usage();
                    break;
                }
                other => panic!("unexpected chunk: {other:?}"),
            }
        }
        assert_eq!(
            calls,
            vec![("toolu_1".to_string(), "echo".to_string(), serde_json::json!({"text": "hi"}))]
        );
        let usage = usage.expect("usage from message_delta");
        assert_eq!((usage.input_tokens, usage.output_tokens), (10, 5));
    }

    #[tokio::test]
    async fn messages_http_fixture_streams_text_turn_on_empty_surface() {
        let _guard = lock_process_seat_for_test();
        let _seat = install_fake_seat();
        crate::claude_messages::install_messages_sse_fixtures(vec![
            crate::claude_messages::sse_fixture_final_text("pong"),
        ]);
        let model = ClaudeSubscriptionClient::new().completion_model("claude-sonnet-5");
        let response = model.completion(ping_request()).await.expect("text turn");
        let text = response
            .choice
            .iter()
            .filter_map(|content| match content {
                rig::completion::AssistantContent::Text(text) => Some(text.text.clone()),
                _ => None,
            })
            .collect::<String>();
        assert_eq!(text, "pong");
    }

    #[tokio::test]
    async fn fixture_queue_serves_one_body_per_call_then_refuses() {
        let _guard = lock_process_seat_for_test();
        let _seat = install_fake_seat();
        crate::claude_messages::install_messages_sse_fixtures(vec![
            crate::claude_messages::sse_fixture_final_text("one"),
        ]);
        let model = ClaudeSubscriptionClient::new().completion_model("claude-sonnet-5");
        model.completion(ping_request()).await.expect("first call served");
        let err = model.completion(ping_request()).await.expect_err("queue drained → live gate");
        assert!(err.to_string().contains("--claude-write-approved"), "{err}");
    }
```

If `StreamedAssistantContent::Final` does not expose `token_usage()` directly, read it via the `GetTokenUsage` impl on `ClaudeStreamResponse` (`final_response.token_usage()` after `use rig::completion::GetTokenUsage;`). If `CompletionResponse.choice` is not the field name in rig 0.35, use the accessor the existing `completion()` impl in this file returns (`CompletionResponse { choice, usage, raw_response }`).

#### 6.4 `claude_completer/mod.rs` — keep the login-time residue only

- [ ] **Step 1: Trim**

Delete `PATH_A_MODEL_IDS`, `live_claude_allowed`, `CompleterParseError`, `StreamJsonlEvent`, `CompleterUsage`, `StreamJsonlState`, `parse_stream_jsonl`, `completer_argv`, and every test in the file except those covering `sanitize_child_env` / `STRIPPED_ENV_VARS` / `parse_auth_status_logged_in`. Delete the `fixtures/` directory:

```bash
git rm -r crates/gents/src/claude_completer/fixtures
```

Module doc: "Login-time child-process hygiene for `gents claude-login` (`sanitize_child_env`) and the model-id default. No completer lives here any more."

#### 6.5 Rendered-request seam back to `main`'s shape

- [ ] **Step 1: `gents-protocol`**

In `crates/gents-protocol/src/rendered_request.rs`: `CaptureSeam` keeps only `TransportBody` (doc: "The last `HttpClientExt` before the network client. The only seam version 1 emits."). Fold `captured_only_at` into `captured_only` (hardcode `capture_seam: CaptureSeam::TransportBody`). Delete `process_cli_seam_round_trips_in_the_manifest`.

- [ ] **Step 2: `gents`**

`crates/gents/src/rendered_request/mod.rs`: rename `build_rendered_completion_request_at_seam` → `build_rendered_completion_request`, drop the `capture_seam` parameter, pass `CaptureSeam::TransportBody` where the manifest is built. Update the single caller (`~L469`) and the test `build` helper; delete `process_cli_seam_is_recorded_positively`. `scope.rs`: delete `claim_and_capture_process_cli` and the four `process_cli_*` tests (keep `arming_without_a_scope_is_a_noop`). If `capture_request_json` took a seam argument only for this caller, drop the argument.

- [ ] **Step 3: `inference_backend.rs`**

```bash
git checkout main -- crates/gents/src/config_client/inference_backend.rs
cargo check -p gents 2>&1 | grep -E 'error|warning: unused' | head
```
Expected: no errors (the spike's `write_inference_backend_document_with_clear_fields` had no callers outside the file).

#### 6.6 Owned-loop test, health helper, CLI flags

- [ ] **Step 1: Rewrite `agent/loop_stream/tests/claude.rs`**

```rust
/// Two Messages turns through the owned loop on SSE fixtures: `tool_use echo`
/// with streamed arguments → gents executes echo → `tool_result` continuation
/// → text `done`. Asserts the persisted `AgentToolCall.args` carries the
/// streamed arguments (defect C1, live-confirmed by write request #8).
#[tokio::test]
async fn claude_messages_tool_round_trip_through_owned_loop() {
    use crate::claude_messages::{install_messages_sse_fixtures, sse_fixture_final_text, sse_fixture_tool_use};
    use crate::claude_subscription::{ClaudeSubscriptionClient, install_fake_seat, lock_process_seat_for_test};
    use crate::rendered_request::scope::{ambient_arming_sink, scope_request, test_scope};
    use crate::rendered_request::{CaptureScopeKind, RenderedRequestCaptureSink, RenderedRequestContext};
    use rig::client::CompletionClient;

    let _guard = lock_process_seat_for_test();
    let _seat = install_fake_seat();
    install_messages_sse_fixtures(vec![
        sse_fixture_tool_use("toolu_1", "echo", "{\"text\":\"hi\"}"),
        sse_fixture_final_text("done"),
    ]);
    let (node, hook) = test_hook().await;
    ready_hook_for(&hook).await;

    let sink: RenderedRequestCaptureSink = Arc::new(|_| Box::pin(async { Ok(()) }));
    let scope = test_scope(
        RenderedRequestContext {
            request_doc_id: "doc-loop-claude".to_string(),
            request_commit_cid: "bafy-request-commit".to_string(),
            request_id: "req-loop-claude".to_string(),
            agent_did: "did:key:agent".to_string(),
            requester_did: String::new(),
            behavior_id: "general".to_string(),
            session_id: "session-loop-claude".to_string(),
            model_name: "claude-sonnet-5".to_string(),
        },
        sink,
    );
    let mut loop_config = config(4);
    loop_config.on_rendered_request = Some(ambient_arming_sink(CaptureScopeKind::Inference));
    let model = ClaudeSubscriptionClient::new().completion_model("claude-sonnet-5");
    let tools: Arc<Vec<Box<dyn ToolDyn>>> = Arc::new(vec![echo_tool()]);

    let (tool_results, final_text) = scope_request(scope, async move {
        let stream = run_loop_stream(model, Some(hook), Message::user("use the echo tool"), Vec::new(), tools, loop_config);
        futures::pin_mut!(stream);
        let mut tool_results = Vec::new();
        let mut final_text = None;
        while let Some(item) = stream.next().await {
            match item.expect("loop item should be Ok") {
                LoopStreamItem::Item(MultiTurnStreamItem::StreamUserItem(StreamedUserContent::ToolResult { tool_result, .. })) => {
                    tool_results.push(
                        tool_result_text(&crate::llm::rig_compat::from_rig_tool_result_content(&tool_result.content.first())).to_string(),
                    );
                }
                LoopStreamItem::Item(MultiTurnStreamItem::FinalResponse(final_response)) => {
                    final_text = Some(final_response.response().to_string());
                }
                _ => {}
            }
        }
        (tool_results, final_text)
    })
    .await;

    assert_eq!(tool_results, vec!["ECHOED".to_string()]);
    assert_eq!(final_text.as_deref(), Some("done"));

    let resp = node.execute("query { AgentToolCall { tool_name args lifecycle_state result } }").await;
    assert!(!resp.has_errors(), "AgentToolCall query failed: {:?}", resp.errors);
    let rows = resp.data.as_ref().and_then(|d| d.get("AgentToolCall")).and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let echo = rows
        .iter()
        .find(|row| row.get("tool_name").and_then(|v| v.as_str()) == Some("echo"))
        .unwrap_or_else(|| panic!("expected an echo AgentToolCall; rows: {rows:?}"));
    assert_eq!(echo["lifecycle_state"], "completed");
    assert!(echo["result"].as_str().is_some_and(|r| r.contains("ECHOED")), "{echo}");
    let args: serde_json::Value = serde_json::from_str(echo["args"].as_str().expect("args string")).expect("args json");
    assert_eq!(args, serde_json::json!({"text": "hi"}), "streamed arguments must persist");
}
```

Keep the file's existing `use` prelude for `test_hook`, `ready_hook_for`, `config`, `echo_tool`, `run_loop_stream`, `LoopStreamItem`, `MultiTurnStreamItem`, `StreamedUserContent`, `tool_result_text`, `Message`, `ToolDyn`, `Arc`, `StreamExt` (they come from the parent `tests` module's `use super::*`).

- [ ] **Step 2: `backend_health.rs` helper** (L729-739 only)

```rust
    fn install_claude_seat(claude_bin: std::path::PathBuf) {
        let config_dir = std::env::temp_dir().join("gents-claude-probe-config");
        let _ = std::fs::create_dir_all(&config_dir);
        crate::claude_subscription::install_process_seat(Some(
            crate::claude_subscription::ClaudeSeatConfig::new(config_dir, false, Some(claude_bin)),
        ));
    }
```

- [ ] **Step 3: CLI flags**

`crates/gents-cli/src/cli/args.rs`: delete the `claude_workdir`, `claude_log_dir`, `claude_fake_completer` args (keep `claude_config_dir`, `claude_bin`, `claude_write_approved`); on `claude_write_approved` and `claude_bin` add `requires = "claude_config_dir"`, and reword the `claude_write_approved` help to end at "…before setting it." (drop "Fake completer bypasses the gate.").

`crates/gents-cli/src/commands/serve.rs`:

```rust
fn install_claude_subscription_seat(args: &ServeArgs) -> Result<()> {
    let Some(config_dir) = args.claude_config_dir.clone() else {
        gents::claude_subscription::install_process_seat(None);
        return Ok(());
    };
    if !config_dir.is_dir() {
        anyhow::bail!("--claude-config-dir {} is not a directory", config_dir.display());
    }
    let seat = gents::claude_subscription::ClaudeSeatConfig::new(
        config_dir,
        args.claude_write_approved,
        args.claude_bin.clone(),
    );
    if seat.write_approved {
        tracing::info!(config_dir = %seat.config_dir.display(), "Claude write gate OPEN: this process may bill Claude until restarted without --claude-write-approved");
    } else {
        tracing::info!(config_dir = %seat.config_dir.display(), "Claude seat installed; live Messages sends refuse-closed (pass --claude-write-approved only after numbered write approval)");
    }
    gents::claude_subscription::install_process_seat(Some(seat));
    Ok(())
}
```

Delete `validate_claude_seat_args` and its call (clap's `requires` now enforces the orphan-flag rule). Status JSON: drop the `fake_completer` key. Tests: `a2b_claude_config_dir_installs_seat_flags` asserts `process_seat()` fields `config_dir` and `write_approved`; `claude_seat_orphan_flags_require_config_dir` becomes a clap parse-error test (`Cli::try_parse_from(["gents","server","--claude-write-approved"])` is `Err`); `install_claude_subscription_seat_from_server_flags` drops the fake-completer assertion. `args/tests.rs::server_parses_a2b_claude_seat_flags` drops the removed flags.

#### 6.7 Gates

- [ ] **Step 1: Full package, workspace, Lean, seam scan**

```bash
export GENTS_SKIP_LENS_BUILD=1 GENTS_SKIP_CALLBACK_WASM_BUILD=1 TMPDIR="$PWD/.scratch/tmp"
cargo test -p gents 2>&1 | grep -E 'test result|FAILED|panicked' | head -20
cargo test -p gents-cli 2>&1 | grep -E 'test result|FAILED' | head
cargo test -p gents-protocol 2>&1 | grep -E 'test result|FAILED' | head
cargo check --workspace --all-targets 2>&1 | tail -3
grep -rn 'println!' crates/gents/src/claude_messages.rs crates/gents/src/claude_subscription.rs ; echo "println scan exit=$?"
git grep -n 'fake_completer\|claude_workdir\|claude_log_dir\|ProcessCli\|process_cli\|StreamJsonl\|flatten_completion_request\|spawn_completer\|completer_argv\|PATH_A_MODEL_IDS\|live_claude_allowed\|captured_only_at\|_at_seam' -- crates ; echo "residue scan exit=$?"
```
Expected: every suite green, including `generated_claude_body_cases_drive_the_body_builder` (Task 5's red) and `provider_invocations_are_confined_to_the_owned_loop_seam` (still exactly two files). Residue scan prints nothing (exit 1).

- [ ] **Step 2: Commit**

```bash
git add -A crates/gents crates/gents-cli crates/gents-protocol
git status --porcelain | grep -v '^??' | grep -v 'docs/design-notes/SPEC-claude-a2b' 
git commit -m "feat(claude): single Messages HTTP wire with incremental SSE; delete the process-CLI completer

build_messages_body lifts Message::System rows into system[] behind the
identity block and preamble, marks two cache_control breakpoints, and
omits tools for an empty surface (ClaudeMap body witnesses now green).
MessagesSseState parses line by line so text deltas reach the loop during
generation; SSE error events fail the stream with the provider's type and
message. One shared reqwest client per seat; the fixture queue serves one
body per call behind the capturing client.

Removed: spawn_completer, stream_child_stdout, flatten_completion_request,
the stream-json parser and its fixtures, CaptureSeam::ProcessCli,
claim_and_capture_process_cli, --claude-workdir/--claude-log-dir/
--claude-fake-completer, and the fake-completer test harnesses. The
owned-loop test now runs two SSE turns and asserts streamed arguments
persist on AgentToolCall."
```
Confirm the `git status` line printed nothing tracked-but-unstaged other than the a2b spec note; never add `docs/superpowers/`.

#### 6.8 Live #10

- [ ] **Step 1: Write request #10 and wait**

`.scratch/claude-spike/logs/write-request-10.md`. Hypothesis: body + streaming on the single wire. Scope: same server flags; one tool turn (the Task 2 `list_files` prompt) in a fresh session. Bar (spec §6 #10):
- captured `request_json` for `inference.1` turn 0: `system[0]` is the identity, `system[1]` exists (behavior preamble or System row), no `messages[]` block whose text starts with `system: `, last `system` block and last content block carry `cache_control`;
- the second `inference_call` event (`call_seq` 2) records `cached_input_tokens` (any value; record it);
- timeline shows at least one assistant `message`/delta event timestamped before the response `completed_at` with a gap consistent with streaming (record the first-delta and completion timestamps);
- `AgentToolCall.args.path == "."`, response `listed`, no 4xx/429, token scan clean.

Ask "Approve #10?" and wait for an explicit approval naming #10.

- [ ] **Step 2: Run, collect, stop, evidence**

Same commands as Task 2 with prefix `b3-live-single-wire-`; additionally:

```bash
L=.scratch/claude-spike/logs; REQ=<request id>
./target/debug/gents query --home ~/.gents --collection RenderedRequest --field capture_scope --field turn_index --field request_json \
  --filter "{\"request_id\":{\"_eq\":\"$REQ\"}}" > $L/b3-live-single-wire-captures.json
jq '.results[] | {capture_scope, turn_index, system: ((.request_json|fromjson).system | map(.text[0:40])), cache: ((.request_json|fromjson).system | last | .cache_control), leaked: ((.request_json|fromjson).messages | map(.content[]? | .text? // "") | map(startswith("system: ")) | any)}' $L/b3-live-single-wire-captures.json
jq '.events[] | select(.kind=="inference_call") | {call_seq, attempt, cached_input_tokens, prompt_tokens, completion_tokens, started_at, ended_at}' $L/b3-live-single-wire-timeline.json
jq '[.events[] | select(.kind=="message" and .role=="assistant") | .timestamp] | first' $L/b3-live-single-wire-timeline.json
```
Write `b3-live-single-wire-evidence.md` with the PASS/FAIL table. FAIL stops the plan for a user decision.

#### 6.9 Seam scans must be green at the end of Task 6 (ruling added during execution)

Two conformance scans have failed since `3643bf7f` and are this task's to fix, because it rewrites both flagged files:

- `docs::rig_vocabulary_confined_to_the_seam` (`tests/conformance/docs.rs:127`) walks every `.rs` under `crates/` and fails on any file outside its allowlist containing the markers `rig::completion::message::`, `rig::completion::Message`, `rig::one_or_many` (and rig tool/hook names). `claude_messages.rs`, `claude_subscription.rs`, and (since Task 5) `tests/conformance/prompt_assembly.rs` carry those markers.
- `prompt_assembly::provider_invocations_are_confined_to_the_owned_loop_seam` (`tests/conformance/prompt_assembly.rs`) walks `crates/gents/src` excluding paths ending `/tests.rs` or containing `/tests/`, and fails on any file outside `{agent/loop_stream.rs, admission/client.rs}` containing `.completion_request(`, `.completion(`, `.stream(`. `claude_subscription.rs` carries them both in its inline `mod tests` (`model.stream(...)`) and in its `completion()` impl (`self.stream(request)`).

Resolution (do this as part of 6.1–6.3, not as an afterthought):

- [ ] **Native body assembly.** `build_messages_body(model: &str, request: &CompletionRequest) -> Value` becomes a thin wrapper: it maps `request.chat_history.iter().map(crate::llm::rig_compat::from_rig_message).collect::<Vec<_>>()` and delegates to

  ```rust
  /// Body assembly over the native message family (no rig vocabulary).
  pub fn build_messages_body_native(
      model: &str,
      preamble: Option<&str>,
      max_tokens: Option<u64>,
      history: &[gents_protocol::message::Message],
      tools: &[rig::completion::ToolDefinition],
  ) -> Value
  ```

  `system_rows`, `anthropic_messages`, and `mark_ephemeral` operate on `gents_protocol::message::{Message, UserContent, AssistantContent, ToolResultContent}` (native `Message::System { content }` exists and `from_rig_message` maps it). The literal strings `rig::completion::message::`, `rig::completion::Message`, and `rig::one_or_many` must not appear anywhere in `claude_messages.rs`, `claude_subscription.rs`, their test files, or `tests/conformance/prompt_assembly.rs`. `rig::completion::CompletionRequest`, `rig::completion::ToolDefinition`, `rig::completion::CompletionError`, `rig::streaming::*`, and `rig::OneOrMany` (the crate-root re-export) are not markers and stay.
- [ ] **Test requests from native messages.** Unit tests build `CompletionRequest`s through one helper in the test module:

  ```rust
  fn request_from_native(
      preamble: Option<&str>,
      history: Vec<gents_protocol::message::Message>,
      tools: Vec<rig::completion::ToolDefinition>,
  ) -> CompletionRequest {
      let rig_history = crate::llm::rig_compat::to_rig_messages(&history);
      CompletionRequest {
          model: None,
          preamble: preamble.map(str::to_string),
          chat_history: rig::OneOrMany::many(rig_history).expect("at least one row"),
          documents: Vec::new(),
          tools,
          temperature: None,
          max_tokens: Some(128),
          tool_choice: None,
          additional_params: None,
          output_schema: None,
      }
  }
  ```

  with native rows such as `gents_protocol::message::Message::System { content: "workspace context".into() }` and `Message::user("use echo")` (use whatever constructor `gents_protocol::message` provides; check the file). `echo_request()` and `ping_request()` are built through this helper.
- [ ] **Conformance body driver** (`generated_claude_body_cases_drive_the_body_builder`, from Task 5) is rewritten to call `gents::claude_messages::build_messages_body_native("claude-sonnet-5", case.preamble.as_deref(), None, &history, &tools)` with `history: Vec<gents::llm::message::Message>` (`Message::System { content }` for `system:` rows, `Message::user("hi")` / `Message::assistant("ok")` for `other:` rows) — no rig message types in that file.
- [ ] **Test modules move out of production paths.** `claude_messages.rs` → `#[cfg(test)] #[path = "claude_messages/tests.rs"] mod tests;` with the tests in `crates/gents/src/claude_messages/tests.rs`; `claude_subscription.rs` → `crates/gents/src/claude_subscription/tests.rs` the same way. Test-only helpers that other modules use (`install_fake_seat`, `sse_fixture_*`) stay in the parent file under `#[cfg(test)]`.
- [ ] **`completion()` without the `.stream(` token.** In `impl CompletionModel for ClaudeSubscriptionModel`, `completion` calls `crate::claude_messages::stream_messages(&self.model, &request, surface).await?` directly and drains that stream (the same fold it does today over `self.stream(request)`), so the production file contains no `.stream(` / `.completion(` text. `stream()` keeps delegating to `stream_messages` as in 6.3 Step 2.
- [ ] **Verify** before the full gate:

  ```bash
  cargo test -p gents --test conformance -- docs::rig_vocabulary_confined_to_the_seam prompt_assembly::provider_invocations_are_confined_to_the_owned_loop_seam 2>&1 | tail -6
  ```
  Expected: both PASS. From this task on, the expected full-gate outcome is fully green: no "known failures" remain.

### Task 7: Seat auth without Security.framework; health probes the token; promotion → live #11

**Files:**
- Modify: `crates/gents/src/claude_seat_auth.rs:45-61` (enum), `:86-106` (`read_seat_access_token_at`), `:115-131` (`parse_credentials`), `:148-235` (Keychain), `:337-362` (tests)
- Modify: `crates/gents/Cargo.toml:83-85` (delete the macOS `security-framework` table)
- Modify: `crates/gents/src/claude_subscription.rs` (`ClaudeSeatConfig` drops `claude_bin`; `probe_process_seat_health` → token read; delete `probe_seat_auth_status`)
- Modify: `crates/gents/src/claude_completer/mod.rs` (delete `parse_auth_status_logged_in` if `gents-cli` does not import it — `claude_auth_probe.rs` has its own parser; verify with `git grep parse_auth_status_logged_in`)
- Modify: `crates/gents/src/backend_health.rs:233-262,304-359,688-856`
- Modify: `crates/gents-cli/src/commands/serve.rs` (`ClaudeSeatConfig::new` loses the third argument; `--claude-bin` on `server` is removed here since nothing reads it), `crates/gents-cli/src/cli/args.rs` (`ServeArgs.claude_bin`), `args/tests.rs`

**Interfaces:**
- Consumes: `read_seat_access_token(&Path) -> Result<SeatAccessToken, SeatAuthError>`.
- Produces:
  - `SeatAuthError::{MissingFile { path: String }, Io { path: String, message: String }, Malformed { reason: String }, KeychainNotFound { service: String }, KeychainAccessDenied { service: String }, KeychainAccountUnset, Expired { expires_at: String }}`.
  - `SeatAccessToken::source() -> SeatTokenSource` (`File | Keychain`) and `SeatAccessToken::expires_at() -> Option<DateTime<Utc>>`; `Debug` redacted.
  - `claude_subscription::probe_process_seat_health() -> Result<String, String>` (Ok = healthy detail `source=file|keychain expires_at=<rfc3339|none>`; Err = Display of the error plus the login hint for `Expired`/`MissingFile`).
  - `backend_health::record_probe_event` without `promote_document_on_unknown`; Claude backends promote `unknown → healthy` like HTTP backends.
  - `ClaudeSeatConfig::new(config_dir: PathBuf, write_approved: bool)`.

- [ ] **Step 1: Failing seat-auth tests** (replace the Keychain tests in `claude_seat_auth.rs` `mod tests`)

```rust
    fn write_credentials(dir: &Path, expires_at_millis: i64) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(
            dir.join(".credentials.json"),
            format!(r#"{{"claudeAiOauth":{{"accessToken":"sk-ant-oat01-TESTTOKEN","expiresAt":{expires_at_millis}}}}}"#),
        )
        .unwrap();
    }

    #[test]
    fn valid_file_reports_source_and_expiry() {
        let temp = tempfile::tempdir().unwrap();
        write_credentials(temp.path(), 4_102_444_800_000); // 2100-01-01
        let token = read_seat_access_token_at(temp.path(), || 1_000).expect("token");
        assert_eq!(token.source(), SeatTokenSource::File);
        assert_eq!(token.expires_at().map(|t| t.to_rfc3339()).as_deref(), Some("2100-01-01T00:00:00+00:00"));
        assert!(!format!("{token:?}").contains("TESTTOKEN"), "Debug must redact");
    }

    #[test]
    fn expired_file_reports_expires_at() {
        let temp = tempfile::tempdir().unwrap();
        write_credentials(temp.path(), 1_000);
        let err = read_seat_access_token_at(temp.path(), || 2_000).expect_err("expired");
        assert!(matches!(err, SeatAuthError::Expired { .. }));
        assert!(err.to_string().contains("1970-01-01T00:00:01"), "{err}");
    }

    #[test]
    fn malformed_file_is_malformed_not_missing() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join(".credentials.json"), "{not json").unwrap();
        let err = read_seat_access_token_at(temp.path(), || 0).expect_err("malformed");
        assert!(matches!(err, SeatAuthError::Malformed { .. }), "{err:?}");
    }

    #[test]
    fn missing_file_and_missing_keychain_item_is_missing_file() {
        let temp = tempfile::tempdir().unwrap();
        let err = read_seat_access_token_at(temp.path(), || 0).expect_err("missing");
        // On macOS the Keychain lookup for this fresh dir yields KeychainNotFound,
        // which folds back to MissingFile so the operator hint names the path.
        assert!(matches!(err, SeatAuthError::MissingFile { .. }), "{err:?}");
        assert!(err.to_string().contains(&temp.path().display().to_string()));
    }

    #[test]
    fn macos_keychain_service_is_sha256_prefix_of_absolute_config_dir() {
        let dir = Path::new("relative/claude-config");
        let absolute = std::env::current_dir().unwrap().join(dir);
        let digest = hex::encode(sha2::Sha256::digest(absolute.to_string_lossy().as_bytes()));
        assert_eq!(
            macos_keychain_service_name(dir),
            format!("Claude Code-credentials-{}", &digest[..8])
        );
        assert_ne!(
            macos_keychain_service_name(Path::new("relative/claude-config/")),
            macos_keychain_service_name(dir),
            "trailing slash must not silently match"
        );
    }

    #[test]
    fn keychain_error_display_does_not_include_secrets() {
        let err = SeatAuthError::KeychainAccessDenied { service: "Claude Code-credentials-deadbeef".into() };
        assert!(!err.to_string().contains("sk-ant"));
    }
```

Use whichever hashing crates `macos_keychain_service_name` already uses (`sha2` + `hex` are in the file today; if `hex` is absent, format with `{:x}`). If `macos_keychain_service_name` is `#[cfg(target_os = "macos")]`, gate the two Keychain tests the same way.

- [ ] **Step 2: Run to verify failure**

```bash
cargo test -p gents --lib claude_seat_auth 2>&1 | tail -15
```
Expected: compile errors for `source()`, `expires_at()`, `Expired { .. }`, `Malformed`.

- [ ] **Step 3: Implement**

Enum:

```rust
#[derive(Debug, Error, PartialEq, Eq)]
pub enum SeatAuthError {
    #[error("Claude seat credentials file missing: {path}")]
    MissingFile { path: String },
    #[error("Claude seat credentials file unreadable: {path}: {message}")]
    Io { path: String, message: String },
    #[error("Claude seat credentials malformed: {reason}")]
    Malformed { reason: String },
    #[error("Claude seat Keychain item not found for service {service}")]
    KeychainNotFound { service: String },
    #[error("Claude seat Keychain access denied for service {service}")]
    KeychainAccessDenied { service: String },
    #[error("Claude seat Keychain account is unset (USER)")]
    KeychainAccountUnset,
    #[error("Claude seat OAuth access token expired at {expires_at}")]
    Expired { expires_at: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeatTokenSource {
    File,
    Keychain,
}

pub struct SeatAccessToken {
    token: String,
    source: SeatTokenSource,
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl SeatAccessToken {
    pub fn authorization_value(&self) -> String { format!("Bearer {}", self.token) }
    pub fn source(&self) -> SeatTokenSource { self.source }
    pub fn expires_at(&self) -> Option<chrono::DateTime<chrono::Utc>> { self.expires_at }
}

impl fmt::Debug for SeatAccessToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SeatAccessToken")
            .field("source", &self.source)
            .field("expires_at", &self.expires_at)
            .finish_non_exhaustive()
    }
}
```

Reader:

```rust
fn read_seat_access_token_at(config_dir: &Path, now_millis: fn() -> i64) -> Result<SeatAccessToken, SeatAuthError> {
    match read_credentials_file(config_dir) {
        Ok(raw) => parse_credentials(&raw, now_millis, SeatTokenSource::File),
        Err(file_err @ SeatAuthError::MissingFile { .. }) => match read_macos_keychain_credentials(config_dir) {
            Ok(raw) => parse_credentials(&raw, now_millis, SeatTokenSource::Keychain),
            Err(SeatAuthError::KeychainNotFound { .. }) => Err(file_err),
            Err(other) => Err(other),
        },
        Err(error) => Err(error),
    }
}
```

`read_credentials_file`: `NotFound` → `MissingFile { path }`, other io errors → `Io { path, message }`. `parse_credentials(raw, now_millis, source)`: JSON failure or missing `claudeAiOauth.accessToken` → `Malformed { reason }`; expiry → `Expired { expires_at: chrono::DateTime::<Utc>::from_timestamp_millis(expires_at).map(|t| t.to_rfc3339()).unwrap_or_else(|| expires_at.to_string()) }`; success carries `expires_at` as a `DateTime<Utc>` when present. Keychain: delete `read_macos_keychain_via_framework` and `ERR_SEC_ITEM_NOT_FOUND`; `read_macos_keychain_credentials` calls `read_macos_keychain_via_security_cli` directly; the CLI wrapper maps exit status 44 (`errSecItemNotFound`) → `KeychainNotFound`, other non-zero → `KeychainAccessDenied`, non-UTF-8 output → `Malformed { reason: "keychain payload is not UTF-8" }`. Non-macOS: `read_macos_keychain_credentials` returns `KeychainNotFound { service }`. Delete `crates/gents/Cargo.toml` L83-85.

- [ ] **Step 4: Seat and health**

`claude_subscription.rs`: `ClaudeSeatConfig { config_dir, write_approved, http }`, `ClaudeSeatConfig::new(config_dir, write_approved)`; delete `probe_seat_auth_status`; replace `probe_process_seat_health`:

```rust
/// Health probe = the same token read the wire performs. No spawn, no refresh.
pub fn probe_process_seat_health() -> Result<String, String> {
    let seat = process_seat().ok_or_else(|| "process seat not installed".to_string())?;
    match crate::claude_seat_auth::read_seat_access_token(&seat.config_dir) {
        Ok(token) => Ok(format!(
            "source={} expires_at={}",
            match token.source() {
                crate::claude_seat_auth::SeatTokenSource::File => "file",
                crate::claude_seat_auth::SeatTokenSource::Keychain => "keychain",
            },
            token.expires_at().map(|t| t.to_rfc3339()).unwrap_or_else(|| "none".to_string())
        )),
        Err(error @ (crate::claude_seat_auth::SeatAuthError::Expired { .. }
        | crate::claude_seat_auth::SeatAuthError::MissingFile { .. })) => Err(format!(
            "{error}; run gents claude-login --config-dir {}",
            seat.config_dir.display()
        )),
        Err(error) => Err(error.to_string()),
    }
}
```

(`gents claude-login` takes `--config-dir`, not `--claude-config-dir`; the spec's hint text is corrected here.)

`backend_health.rs` Claude branch:

```rust
        if backend.provider_kind == crate::backend_provider::BackendProviderKind::ClaudeCliSubscription {
            probed_ids.insert(backend.backend_id.clone());
            let (event, error_text) = match crate::claude_subscription::probe_process_seat_health() {
                Ok(detail) => {
                    tracing::debug!(backend_id = %backend.backend_id, detail = %detail, "claude seat probe ok");
                    (ProbeEvent::ProbeSuccess, None)
                }
                Err(reason) => (ProbeEvent::ProbeFail, Some(reason)),
            };
            record_probe_event(backend, event, error_text, now, health_map, options, &mut outcome).await;
            continue;
        }
```

`record_probe_event`: drop the `promote_document_on_unknown` parameter and the `#[allow(clippy::too_many_arguments)]`; the promotion condition becomes `event == ProbeEvent::ProbeSuccess && backend.probe_status == UNKNOWN_PROBE_STATUS`. Update the HTTP-branch call site to the seven-argument form.

- [ ] **Step 5: Health tests** (replace `write_auth_status_fake`, `install_claude_seat`, and the four Claude tests)

```rust
    fn install_claude_seat_with_credentials(expires_at_millis: Option<i64>) -> tempfile::TempDir {
        let temp = tempfile::tempdir().expect("seat dir");
        let config_dir = temp.path().join("claude-config");
        std::fs::create_dir_all(&config_dir).unwrap();
        if let Some(expires) = expires_at_millis {
            std::fs::write(
                config_dir.join(".credentials.json"),
                format!(r#"{{"claudeAiOauth":{{"accessToken":"sk-ant-oat01-TEST","expiresAt":{expires}}}}}"#),
            )
            .unwrap();
        }
        crate::claude_subscription::install_process_seat(Some(
            crate::claude_subscription::ClaudeSeatConfig::new(config_dir, false),
        ));
        temp
    }

    #[tokio::test]
    async fn cycle_does_not_http_probe_claude_cli_subscription_backends() {
        let _guard = crate::claude_subscription::lock_process_seat_for_test();
        crate::claude_subscription::install_process_seat(None);
        let (options, client, health_map) = (probe_options(), reqwest::Client::new(), BackendHealthMap::new());
        let mut claude = claude_backend();
        claude.endpoint = "http://127.0.0.1:1/v1".to_string();
        let outcome = probe_backends_cycle(&client, &[claude], Utc::now(), &health_map, &options).await;
        assert!(outcome.promotable.is_empty());
        let snap = health_map.get("claude").await.expect("measured entry");
        assert_eq!(snap.state, BackendHealthState::Degraded);
        assert!(snap.last_error.as_deref().is_some_and(|e| e.contains("process seat not installed")), "{:?}", snap.last_error);
    }

    #[tokio::test]
    async fn cycle_demotes_claude_after_k_expired_probes_with_login_hint() {
        let _guard = crate::claude_subscription::lock_process_seat_for_test();
        let _seat = install_claude_seat_with_credentials(Some(1_000));
        let (options, client, health_map) = (probe_options(), reqwest::Client::new(), BackendHealthMap::new());
        let backends = vec![claude_backend()];
        for cycle in 1..=3u32 {
            let outcome = probe_backends_cycle(&client, &backends, Utc::now(), &health_map, &options).await;
            assert!(outcome.promotable.is_empty());
            let snap = health_map.get("claude").await.expect("entry");
            assert_eq!(snap.failure_count, cycle);
            let err = snap.last_error.clone().unwrap_or_default();
            assert!(err.contains("expired at") && err.contains("gents claude-login --config-dir"), "{err}");
            if cycle < 3 {
                assert_eq!(snap.state, BackendHealthState::Degraded);
            } else {
                assert_eq!(snap.state, BackendHealthState::Unhealthy);
                assert_eq!(outcome.flipped, vec!["claude".to_string()]);
            }
        }
        crate::claude_subscription::install_process_seat(None);
    }

    #[tokio::test]
    async fn cycle_marks_claude_healthy_and_promotes_unknown_document() {
        let _guard = crate::claude_subscription::lock_process_seat_for_test();
        let _seat = install_claude_seat_with_credentials(Some(4_102_444_800_000));
        let (options, client, health_map) = (probe_options(), reqwest::Client::new(), BackendHealthMap::new());
        let mut claude = claude_backend();
        claude.probe_status = "unknown".to_string();
        let outcome = probe_backends_cycle(&client, &[claude], Utc::now(), &health_map, &options).await;
        assert_eq!(outcome.promotable, vec!["claude".to_string()], "C9: unknown Claude document promotes on first pass");
        let snap = health_map.get("claude").await.expect("entry");
        assert_eq!(snap.state, BackendHealthState::Healthy);
        assert!(snap.last_error.is_none());
        crate::claude_subscription::install_process_seat(None);
    }

    #[tokio::test]
    async fn cycle_fails_claude_when_credentials_are_missing() {
        let _guard = crate::claude_subscription::lock_process_seat_for_test();
        let _seat = install_claude_seat_with_credentials(None);
        let (options, client, health_map) = (probe_options(), reqwest::Client::new(), BackendHealthMap::new());
        probe_backends_cycle(&client, &[claude_backend()], Utc::now(), &health_map, &options).await;
        let snap = health_map.get("claude").await.expect("entry");
        assert_eq!(snap.state, BackendHealthState::Degraded);
        let err = snap.last_error.clone().unwrap_or_default();
        assert!(err.contains("credentials file missing") && err.contains("gents claude-login --config-dir"), "{err}");
        crate::claude_subscription::install_process_seat(None);
    }
```

The missing-credentials test depends on the Keychain also lacking an item for the tempdir's digest, which is true for a fresh tempdir on any machine.

- [ ] **Step 6: CLI follow-through**

`serve.rs`: `ClaudeSeatConfig::new(config_dir, args.claude_write_approved)`; remove `ServeArgs.claude_bin` and its mention in `server_parses_a2b_claude_seat_flags`. `git grep -n 'claude_bin' crates/gents-cli/src/commands/serve.rs` must print nothing.

- [ ] **Step 7: Gates and commit**

```bash
cargo test -p gents 2>&1 | grep -E 'test result|FAILED|panicked' | head
cargo test -p gents-cli 2>&1 | grep -E 'test result|FAILED' | head
cargo check --workspace --all-targets 2>&1 | tail -3
git grep -n 'security_framework\|security-framework\|ERR_SEC_ITEM_NOT_FOUND\|probe_seat_auth_status\|parse_auth_status_logged_in' -- crates ; echo "residue exit=$?"
git add crates/gents/Cargo.toml Cargo.lock crates/gents/src crates/gents-cli/src
git commit -m "feat(claude): health probes the seat token; promote unknown Claude documents

The probe now performs the same read the wire performs (credentials file,
then security(1) Keychain) and reports source and expiry; expired or
missing seats carry the claude-login hint. record_probe_event no longer
special-cases Claude, so a document born unknown promotes to healthy on
its first passing cycle (C9, C6). Security.framework fallback and the
claude auth status spawn are gone; the error enum names each failure."
```

- [ ] **Step 8: Live #11**

Write `.scratch/claude-spike/logs/write-request-11.md`. Bar (spec §6 #11), three server starts with no chat turns:
1. Start with `--claude-config-dir <tempdir with an expired .credentials.json>` (write the file with `expiresAt: 1000` in a fresh dir under `.scratch/claude-spike/tmp/expired-seat`); after one probe interval, `gents query --collection InferenceBackend --field backend_id --field probe_status` plus the server stderr line must show `Unhealthy`/degraded detail containing `gents claude-login --config-dir`. Stop.
2. Start with the real spike config dir and a backend document whose `probe_status` is `unknown` (set it via the existing `gents` backend update path — `gents backend set` or a GraphQL mutation on `InferenceBackend` using `escape_graphql_string`-safe literal; record the mutation in the evidence). After one probe cycle the document reads `healthy` without hand-editing. Stop.
3. Token scan on all `b3-live-health-*` files.
This run sends no Messages request, but it reads the seat and writes an `InferenceBackend` document, so it stays a numbered approval. Ask "Approve #11?" and wait.

### Task 8: CLI and docs trim; ponytail residue

**Files:**
- Modify: `crates/gents-cli/src/cli/args.rs:93-98` (`ClaudeAuthProbe` variant), `:546-566` (`console`, `sso`, `ClaudeAuthProbeArgs`)
- Modify: `crates/gents-cli/src/cli/args/tests.rs:784-835` (`claude_login_and_auth_probe_parse_and_require_config_dir` → `claude_login_parses_and_requires_config_dir`)
- Delete: `crates/gents-cli/src/commands/claude_auth_probe.rs`
- Modify: `crates/gents-cli/src/commands/mod.rs:3`, `crates/gents-cli/src/lib.rs:451-453`, `crates/gents-cli/src/commands/claude_login.rs:50-100`
- Modify: `crates/gents/src/backend_provider.rs:58-64,89-93` and its parse tests
- Modify: `docs/design-notes/PR-STACK-claude-track-b-tools.md`, `docs/design-notes/SPEC-claude-a2c-tool-bridging.md`, `crates/gents/proofs/README.md`, `CLAUDE.md` (the "External code is held at arm's length" bullet)

**Interfaces:**
- Produces: `gents claude-login --config-dir <dir> [--claude-bin] [--email] [--dry-run] [--claude-write-approved]`; `BackendProviderKind::parse_optional` accepts `ClaudeCliSubscription`, `claude-cli-subscription`, `claude_cli_subscription` only.

- [ ] **Step 1: Failing tests**

`args/tests.rs`: rename the test; drop its `claude-auth-probe` parse case; add:

```rust
    #[test]
    fn claude_login_rejects_removed_console_and_sso_flags() {
        for flag in ["--console", "--sso"] {
            let parsed = Cli::try_parse_from(["gents", "claude-login", "--config-dir", "/tmp/x", flag]);
            assert!(parsed.is_err(), "{flag} must be gone");
        }
        assert!(Cli::try_parse_from(["gents", "claude-auth-probe", "--config-dir", "/tmp/x"]).is_err());
    }
```

`backend_provider.rs` tests:

```rust
    #[test]
    fn claude_max_cli_spellings_are_not_accepted() {
        for spelling in ["claude-max-cli", "claude_max_cli"] {
            assert!(BackendProviderKind::parse_optional(Some(spelling)).is_err(), "{spelling}");
        }
        assert_eq!(
            BackendProviderKind::parse_optional(Some("claude_cli_subscription")).unwrap(),
            BackendProviderKind::ClaudeCliSubscription
        );
        let parsed: BackendProviderKind = serde_json::from_str("\"claude-max-cli\"").unwrap_or(BackendProviderKind::OpenAiCompatible);
        assert_ne!(parsed, BackendProviderKind::ClaudeCliSubscription, "serde alias must be gone");
    }
```

(Use whatever `Cli` type and `try_parse_from` pattern the neighbouring tests in `args/tests.rs` use.)

- [ ] **Step 2: Run to verify failure**

```bash
cargo test -p gents-cli --lib cli::args::tests::claude_ 2>&1 | tail -8
cargo test -p gents --lib backend_provider 2>&1 | tail -8
```
Expected: the new tests FAIL (flags still parse; spellings still accepted).

- [ ] **Step 3: Implement**

- `args.rs`: delete `Command::ClaudeAuthProbe` + its `#[command]` block, `ClaudeAuthProbeArgs`, and `ClaudeLoginArgs.console` / `.sso`. Reword the `claude-login` `about` to "Sign in to the Claude subscription seat via the Claude CLI (credentials stay in --config-dir; no oat in DefraDB)".
- `commands/mod.rs`: remove `pub(crate) mod claude_auth_probe;`. `lib.rs`: remove the `Command::ClaudeAuthProbe` arm. `git rm crates/gents-cli/src/commands/claude_auth_probe.rs`.
- `claude_login.rs`: `plan_claude_login` always pushes `--claudeai`; delete the `console`/`sso` branches and the mutual-exclusion bail. The post-login `probe` JSON field that called `claude_auth_probe_result_json` is replaced by `"seat": gents::claude_subscription::probe_seat_detail(&config_dir)` where `probe_seat_detail(config_dir: &Path) -> serde_json::Value` is a new `pub fn` in `claude_subscription.rs` returning `{"ok": bool, "detail": String}` built from `read_seat_access_token` exactly as `probe_process_seat_health` formats it (factor the formatting into a shared `fn seat_detail(config_dir: &Path) -> Result<String, String>` used by both).
- `backend_provider.rs`: delete the two `claude-max-cli` / `claude_max_cli` serde aliases and the two `parse_optional` arms; the kind's doc comment becomes "Claude subscription seat over Messages HTTP. Seat truth lives in the process `--claude-config-dir`; the `claude` binary is a login-time dependency only. `is_agent_scoped_oauth()` stays false."

- [ ] **Step 4: Docs**

- `docs/design-notes/PR-STACK-claude-track-b-tools.md`: add a dated status block at the top: B3 done (write requests #7–#10), the two-wire Completer retired on 2026-09-03 in favour of the single Messages wire (link the spec path), B4 still later.
- `docs/design-notes/SPEC-claude-a2c-tool-bridging.md`: in the C2 lock section record: identity block as `system[0]`, Keychain via `security(1)`, single wire, `tools` omitted when empty, two cache breakpoints; mark open questions #1, #3, #5, #9 closed with one line each pointing at the evidence files.
- `crates/gents/proofs/README.md`: in the map table, the ClaudeMap row lists `splitSystem_partition`, `systemBlocks_head`, `systemBlocks_tail_verbatim`, `toolsField_empty`, `accumulate_ignores_start_when_streamed`, `runStream_*`; add one paragraph: "Provider-input assembly for Claude: the body's `system[]` order and tools omission, and SSE tool-block accumulation, are modelled and witnessed; byte framing, usage and headers are Rust-tested."
- `CLAUDE.md`, "External code is held at arm's length" bullet: append the sentence "Claude subscription seats are reached over Anthropic Messages HTTP with the seat's token read from `--claude-config-dir`; the `claude` binary is a login-time dependency only (`gents claude-login`)."
- `.scratch/claude-spike/logs/b3-live-identity-evidence.md`: the Task 2 correction is already there; add a pointer to `b3-live-single-wire-evidence.md`.

- [ ] **Step 5: Gates and commit**

```bash
cargo test -p gents 2>&1 | grep -E 'test result|FAILED' | head
cargo test -p gents-cli 2>&1 | grep -E 'test result|FAILED' | head
cargo check --workspace --all-targets 2>&1 | tail -3
git grep -n 'claude-auth-probe\|claude_auth_probe\|claude-max-cli\|claude_max_cli\|--console\|--sso' -- crates docs/design-notes CLAUDE.md ; echo "residue exit=$?"
git add crates/gents-cli crates/gents/src/backend_provider.rs crates/gents/src/claude_subscription.rs crates/gents/proofs/README.md docs/design-notes/PR-STACK-claude-track-b-tools.md docs/design-notes/SPEC-claude-a2c-tool-bridging.md CLAUDE.md
git commit -m "chore(claude): drop claude-auth-probe, --console/--sso, claude-max-cli spellings; document the single wire"
```
`docs/design-notes/SPEC-claude-a2b-in-process.md` stays unstaged (pre-existing dirty leftover).

### Task 9: Carve the PR stack off `main`

No new code. Four branches, each compiling and green on its own, carved by path from the final spike tree (spec §8).

**Files:** branches `claude/pr1-protocol-seat-health`, `claude/pr2-messages-wire`, `claude/pr3-lean-fence`, `claude/pr4-cli-docs`.

- [ ] **Step 1: Snapshot the final tree**

```bash
git tag spike-final-2026-09-03
git worktree add .scratch/wt-carve main
```

- [ ] **Step 2: PR1 — protocol vocabulary, seat auth, health, promotion**

```bash
cd .scratch/wt-carve && git checkout -b claude/pr1-protocol-seat-health main
git checkout spike-final-2026-09-03 -- \
  crates/gents-protocol/src/rendered_request.rs \
  crates/gents/src/backend_provider.rs \
  crates/gents/src/claude_seat_auth.rs \
  crates/gents/src/backend_health.rs \
  crates/gents/src/backend_registry.rs \
  crates/gents/src/config_client/inference_backend.rs \
  crates/gents/Cargo.toml Cargo.lock
```
Then `git checkout spike-final-2026-09-03 -- <path>` for any file `cargo check --workspace --all-targets` reports as missing a symbol (`claude_subscription.rs` will be needed because `backend_health.rs` calls `probe_process_seat_health`; when it is, take `claude_subscription.rs`, `claude_messages.rs`, `claude_completer/mod.rs`, `lib.rs` module lines and the `completion_factory` / `agent/runtime/context` / `oneshot` arms together and move them into PR2's scope instead — the rule is: PR1 must compile without `claude_messages.rs`; if it cannot, PR1 = protocol + seat auth + backend_provider only and health moves to PR2). Run:

```bash
export GENTS_SKIP_LENS_BUILD=1 GENTS_SKIP_CALLBACK_WASM_BUILD=1 TMPDIR="$PWD/.scratch/tmp"
cargo check --workspace --all-targets 2>&1 | tail -3 && cargo test -p gents -p gents-protocol 2>&1 | grep -E 'test result|FAILED' | head
git commit -am "feat(claude): protocol vocabulary, seat auth, health probe and promotion"
```
(This worktree is cold; expect the ~40 min first build. Run it in the background with `run_in_background` and continue carving.)

- [ ] **Step 3: PR2 — Messages wire and owned-loop test** (stacks on PR1)

```bash
git checkout -b claude/pr2-messages-wire claude/pr1-protocol-seat-health
git checkout spike-final-2026-09-03 -- \
  crates/gents/src/claude_messages.rs crates/gents/src/claude_subscription.rs \
  crates/gents/src/claude_completer crates/gents/src/lib.rs \
  crates/gents/src/rendered_request crates/gents/src/agent/loop_stream/tests/claude.rs \
  crates/gents/src/completion_factory.rs crates/gents/src/agent/runtime/context.rs crates/gents/src/oneshot.rs
git diff --cached --stat
```
Verify `lib.rs` differs from `main` only by the four `pub mod claude_*` lines (the no-rustfmt rule): `git diff main -- crates/gents/src/lib.rs`. If it shows more, hand-apply only those lines. Gates as in Step 2; commit "feat(claude): single Messages HTTP wire, streaming parser, owned-loop test".

- [ ] **Step 4: PR3 — Lean fence** (stacks on PR2)

```bash
git checkout -b claude/pr3-lean-fence claude/pr2-messages-wire
git checkout spike-final-2026-09-03 -- crates/gents/proofs crates/gents/src/lean_vocab_test crates/gents/tests/conformance crates/gents/tests/support/conformance_consumers.rs
(cd crates/gents/proofs && lake build 2>&1 | tail -2)
cargo test -p gents 2>&1 | grep -E 'test result|FAILED' | head
git commit -am "spec(claude): ClaudeMap system assembly and stream accumulation; conformance witnesses"
```

- [ ] **Step 5: PR4 — CLI and docs** (stacks on PR3)

```bash
git checkout -b claude/pr4-cli-docs claude/pr3-lean-fence
git checkout spike-final-2026-09-03 -- crates/gents-cli docs/design-notes/PR-STACK-claude-track-b-tools.md docs/design-notes/SPEC-claude-a2c-tool-bridging.md CLAUDE.md
git rm -q --cached docs/superpowers 2>/dev/null; true
cargo test -p gents-cli 2>&1 | grep -E 'test result|FAILED' | head
cargo check --workspace --all-targets 2>&1 | tail -3
git commit -am "feat(cli): claude-login trim, serve seat flags, single-wire docs"
```

- [ ] **Step 6: Confirm the stack equals the spike**

```bash
git diff spike-final-2026-09-03 claude/pr4-cli-docs --stat
```
Expected: empty except the untracked spec/plan/TODO/tasks leftovers (which are not in either tree) and `docs/design-notes/SPEC-claude-a2b-in-process.md` (uncommitted on the spike). Report the four branch names and the two side branches from Task 3 to the user; opening the PRs is the user's call.
