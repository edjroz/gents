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

#### 6.10 Conformance driver de-duplication (ruling from the Task 5 review)

- [ ] In `tests/conformance/prompt_assembly.rs`, extract the `emptySurface` / `unmappedName:` / `duplicateId:` / `overlappingBlock:` outcome handling shared by `generated_claude_map_cases_drive_the_messages_parser` and `generated_claude_stream_cases_drive_the_messages_parser` into one helper:

  ```rust
  /// Shared fail-closed oracle for the ClaudeMap and stream witnesses: the
  /// Lean `errorName` tag must correspond to the parser's frozen Display text.
  fn assert_fail_closed(case_name: &str, outcome: &str, parsed: Result<Vec<RawStreamingChoice<ClaudeStreamResponse>>, CompletionError>) {
      let err = parsed.expect_err("witness outcome is an error; parser must fail closed").to_string();
      let ok = match outcome {
          "emptySurface" => err.contains("fail-closed: tool_use observed"),
          o if o.starts_with("unmappedName:") => {
              let name = o.strip_prefix("unmappedName:").expect("prefix");
              err.contains("fail-closed: tool_use observed") && err.contains(name)
          }
          o if o.starts_with("duplicateId:") => {
              err.contains(&format!("fail-closed: duplicate tool_use id {}", o.strip_prefix("duplicateId:").expect("prefix")))
          }
          o if o.starts_with("overlappingBlock:") => {
              err.contains(&format!("fail-closed: overlapping tool_use block {}", o.strip_prefix("overlappingBlock:").expect("prefix")))
          }
          other => panic!("case {case_name} unknown outcome {other}"),
      };
      assert!(ok, "case {case_name}: outcome {outcome} but error was: {err}");
  }
  ```
  Both drivers keep their own `"ok"` arm and call `assert_fail_closed(&case.name, &case.outcome, parsed)` for every other outcome. (`ClaudeStreamResponse` is `gents::claude_subscription::ClaudeStreamResponse`; `CompletionError` is `rig::completion::CompletionError` — neither is a rig-vocabulary marker.)
