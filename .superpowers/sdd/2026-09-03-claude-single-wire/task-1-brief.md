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

