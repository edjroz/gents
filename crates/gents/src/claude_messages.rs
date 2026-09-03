//! C2 Anthropic Messages HTTP for tool-capable Claude turns.
//!
//! Empty-surface turns stay on the process CLI. This path never enables Claude
//! Code built-in tools.

use std::collections::HashSet;
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

use crate::claude_completer::CompleterParseError;
use crate::claude_seat_auth::read_seat_access_token;
use crate::claude_subscription::ClaudeStreamResponse;
use crate::rendered_request::RenderedRequestCapturingHttpClient;

pub const MESSAGES_URI: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";
const OAUTH_BETA: &str = "oauth-2025-04-20";
const DEFAULT_MAX_TOKENS: u64 = 4096;

/// First `system` block on Messages HTTP. The seat's oat was minted for
/// Claude Code; without this identity the same token 429s (`"Error"`, no
/// `retry-after`) on every model while the process CLI keeps working. Sourced
/// from third-party reports, not Anthropic docs — a numbered live experiment,
/// never sent on the process-CLI wire.
pub const CLAUDE_CODE_IDENTITY: &str = "You are Claude Code, Anthropic's official CLI for Claude.";

/// Keys this path may emit. Sampling (`temperature`, `top_p`, `top_k`) and
/// `CompletionRequest.additional_params` stay off the wire: live
/// `claude-sonnet-5` returns 400 "`temperature` is deprecated for this model"
/// (same class for `top_p` / `top_k`). Grok / OpenAI keep profile sampling on
/// their own HTTP bodies.
const MESSAGES_BODY_ALLOWED_KEYS: &[&str] = &[
    "model",
    "max_tokens",
    "stream",
    "messages",
    "tools",
    "system",
];

fn messages_sse_fixture_slot() -> &'static Mutex<Option<String>> {
    static SLOT: OnceLock<Mutex<Option<String>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

/// Test-only: Messages SSE body used instead of the network. Cleared by
/// `lock_process_seat_for_test`.
pub fn install_messages_sse_fixture(sse: Option<String>) {
    *messages_sse_fixture_slot()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner()) = sse;
}

pub fn messages_sse_fixture() -> Option<String> {
    messages_sse_fixture_slot()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .clone()
}

/// Build the Anthropic Messages JSON body (stream + gents tools). `system` is
/// always present: [`CLAUDE_CODE_IDENTITY`] then the Gents preamble, if any.
pub fn build_messages_body(model: &str, request: &CompletionRequest) -> Value {
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
    // Identity first, Gents preamble second; the preamble itself is untouched.
    let mut system = vec![json!({ "type": "text", "text": CLAUDE_CODE_IDENTITY })];
    if let Some(preamble) = request
        .preamble
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        system.push(json!({ "type": "text", "text": preamble }));
    }
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
    // Do not copy `request.temperature` or merge `request.additional_params`
    // (`top_p` / `top_k` / seed / penalties ride that map).
    debug_assert!(
        messages_body_has_only_allowed_keys(&body),
        "Claude Messages body grew an unallowlisted key: {body}"
    );
    body
}

fn messages_body_has_only_allowed_keys(body: &Value) -> bool {
    body.as_object().is_some_and(|object| {
        object
            .keys()
            .all(|key| MESSAGES_BODY_ALLOWED_KEYS.contains(&key.as_str()))
    })
}

fn anthropic_messages(request: &CompletionRequest) -> Vec<Value> {
    let mut out = Vec::new();
    for message in request.chat_history.iter() {
        match message {
            rig::completion::Message::User { content } => {
                let mut blocks = Vec::new();
                for block in content.iter() {
                    match block {
                        rig::completion::message::UserContent::Text(text)
                            if !text.text.is_empty() =>
                        {
                            blocks.push(json!({"type": "text", "text": text.text}));
                        }
                        rig::completion::message::UserContent::ToolResult(result) => {
                            let body: String = result
                                .content
                                .iter()
                                .filter_map(|item| match item {
                                    rig::completion::message::ToolResultContent::Text(text) => {
                                        Some(text.text.as_str())
                                    }
                                    _ => None,
                                })
                                .collect::<Vec<_>>()
                                .join("");
                            blocks.push(json!({
                                "type": "tool_result",
                                "tool_use_id": result.id,
                                "content": body,
                            }));
                        }
                        _ => {}
                    }
                }
                if !blocks.is_empty() {
                    out.push(json!({"role": "user", "content": blocks}));
                }
            }
            rig::completion::Message::Assistant { content, .. } => {
                let mut blocks = Vec::new();
                for block in content.iter() {
                    match block {
                        rig::completion::message::AssistantContent::Text(text)
                            if !text.text.is_empty() =>
                        {
                            blocks.push(json!({"type": "text", "text": text.text}));
                        }
                        rig::completion::message::AssistantContent::ToolCall(call) => {
                            blocks.push(json!({
                                "type": "tool_use",
                                "id": call.id,
                                "name": call.function.name,
                                "input": call.function.arguments,
                            }));
                        }
                        _ => {}
                    }
                }
                if !blocks.is_empty() {
                    out.push(json!({"role": "assistant", "content": blocks}));
                }
            }
            rig::completion::Message::System { content } => {
                if !content.trim().is_empty() {
                    out.push(json!({
                        "role": "user",
                        "content": [{ "type": "text", "text": format!("system: {content}") }],
                    }));
                }
            }
        }
    }
    out
}

/// Parse Anthropic Messages SSE into Completer events. Mapped `tool_use` of a
/// gents name becomes `ToolCall`; unmapped names fail closed.
pub fn parse_messages_sse(
    sse: &str,
    surface: &HashSet<String>,
) -> Result<Vec<RawStreamingChoice<ClaudeStreamResponse>>, CompletionError> {
    let mut events = Vec::new();
    let mut pending: Option<PendingTool> = None;
    let mut seen_ids: HashSet<String> = HashSet::new();
    let mut usage = None;
    for payload in sse_data_payloads(sse) {
        let Some(kind) = payload.get("type").and_then(Value::as_str) else {
            continue;
        };
        match kind {
            "content_block_start" => {
                if let Some(block) = payload.get("content_block") {
                    if block.get("type").and_then(Value::as_str) == Some("tool_use") {
                        let id = block
                            .get("id")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string();
                        if pending.is_some() {
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
            "message_delta" => {
                if let Some(value) = payload.get("usage") {
                    usage = Some(usage_from_sse(value));
                }
            }
            "message_stop" => {
                events.push(RawStreamingChoice::FinalResponse(ClaudeStreamResponse {
                    usage,
                }));
            }
            _ => {}
        }
    }
    if let Some(tool) = pending.take() {
        events.push(mapped_tool_call(tool, surface, &mut seen_ids)?);
    }
    if !events
        .iter()
        .any(|event| matches!(event, RawStreamingChoice::FinalResponse(_)))
    {
        events.push(RawStreamingChoice::FinalResponse(ClaudeStreamResponse {
            usage,
        }));
    }
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

fn usage_from_sse(value: &Value) -> rig::completion::Usage {
    let input_tokens = value
        .get("input_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let output_tokens = value
        .get("output_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    rig::completion::Usage {
        input_tokens,
        output_tokens,
        total_tokens: input_tokens.saturating_add(output_tokens),
        cached_input_tokens: value
            .get("cache_read_input_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        cache_creation_input_tokens: value
            .get("cache_creation_input_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0),
    }
}

fn sse_data_payloads(sse: &str) -> Vec<Value> {
    let mut payloads = Vec::new();
    let mut data = String::new();
    for line in sse.lines() {
        if let Some(rest) = line.strip_prefix("data:") {
            if !data.is_empty() {
                data.push('\n');
            }
            data.push_str(rest.trim());
        } else if line.is_empty() {
            if !data.is_empty() {
                if let Ok(value) = serde_json::from_str::<Value>(&data) {
                    payloads.push(value);
                }
                data.clear();
            }
        }
    }
    if !data.is_empty() {
        if let Ok(value) = serde_json::from_str::<Value>(&data) {
            payloads.push(value);
        }
    }
    payloads
}

pub async fn stream_messages(
    model: &str,
    request: &CompletionRequest,
    surface: HashSet<String>,
) -> Result<
    impl futures::Stream<Item = Result<RawStreamingChoice<ClaudeStreamResponse>, CompletionError>>,
    CompletionError,
> {
    let seat = crate::claude_subscription::require_process_seat()?;
    let fixture = messages_sse_fixture();
    if fixture.is_none() && !crate::claude_completer::live_claude_allowed(seat.write_approved) {
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

    let transport = ClaudeMessagesTransport {
        sse_fixture: fixture,
        live: ReqwestClient::new(),
    };
    let client = RenderedRequestCapturingHttpClient::new(transport);
    let response = client.send_streaming(http_request).await?;
    if !response.status().is_success() {
        return Err(CompletionError::ProviderError(format!(
            "Claude Messages HTTP {}",
            response.status()
        )));
    }
    let mut sse_bytes = Vec::new();
    let mut body = response.into_body();
    while let Some(chunk) = body.next().await {
        let chunk = chunk?;
        sse_bytes.extend_from_slice(&chunk);
    }
    let sse = String::from_utf8_lossy(&sse_bytes).into_owned();
    let events = parse_messages_sse(&sse, &surface)?;
    Ok(futures::stream::iter(events.into_iter().map(Ok)))
}

#[derive(Clone)]
struct ClaudeMessagesTransport {
    sse_fixture: Option<String>,
    live: ReqwestClient,
}

impl HttpClientExt for ClaudeMessagesTransport {
    fn send<T, U>(
        &self,
        req: Request<T>,
    ) -> impl std::future::Future<Output = http_client::Result<Response<LazyBody<U>>>>
    + WasmCompatSend
    + 'static
    where
        T: Into<Bytes> + WasmCompatSend,
        U: From<Bytes>,
        U: WasmCompatSend + 'static,
    {
        let inner = self.live.clone();
        let fixture = self.sse_fixture.clone();
        let (parts, body) = req.into_parts();
        let body: Bytes = body.into();
        async move {
            if fixture.is_some() {
                let body: LazyBody<U> = Box::pin(async { Ok(U::from(Bytes::from_static(b"{}"))) });
                return Ok(Response::builder().status(200).body(body)?);
            }
            let req = Request::from_parts(parts, body);
            HttpClientExt::send::<Bytes, U>(&inner, req).await
        }
    }

    fn send_multipart<U>(
        &self,
        req: Request<MultipartForm>,
    ) -> impl std::future::Future<Output = http_client::Result<Response<LazyBody<U>>>>
    + WasmCompatSend
    + 'static
    where
        U: From<Bytes>,
        U: WasmCompatSend + 'static,
    {
        let inner = self.live.clone();
        async move { HttpClientExt::send_multipart::<U>(&inner, req).await }
    }

    fn send_streaming<T>(
        &self,
        req: Request<T>,
    ) -> impl std::future::Future<Output = http_client::Result<StreamingResponse>> + WasmCompatSend
    where
        T: Into<Bytes>,
    {
        let inner = self.live.clone();
        let fixture = self.sse_fixture.clone();
        let (parts, body) = req.into_parts();
        let body: Bytes = body.into();
        async move {
            if let Some(sse) = fixture {
                let stream: rig::http_client::sse::BoxedStream =
                    Box::pin(futures::stream::iter([Ok(Bytes::from(sse))]));
                return Ok(Response::builder().status(200).body(stream)?);
            }
            let req = Request::from_parts(parts, body);
            HttpClientExt::send_streaming(&inner, req).await
        }
    }
}

impl fmt::Debug for ClaudeMessagesTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClaudeMessagesTransport")
            .field("sse_fixture", &self.sse_fixture.is_some())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rig::completion::message::{Message, Text, UserContent};
    use rig::one_or_many::OneOrMany;

    fn echo_request() -> CompletionRequest {
        CompletionRequest {
            model: None,
            preamble: Some("You are helpful.".into()),
            chat_history: OneOrMany::one(Message::User {
                content: OneOrMany::one(UserContent::Text(Text {
                    text: "use echo".into(),
                })),
            }),
            documents: Vec::new(),
            tools: vec![rig::completion::ToolDefinition {
                name: "echo".into(),
                description: "echo".into(),
                parameters: serde_json::json!({"type":"object","properties":{}}),
            }],
            temperature: None,
            max_tokens: Some(128),
            tool_choice: None,
            additional_params: None,
            output_schema: None,
        }
    }

    #[test]
    fn messages_body_includes_gents_tools_and_system() {
        let body = build_messages_body("claude-sonnet-5", &echo_request());
        assert_eq!(body["model"], "claude-sonnet-5");
        assert_eq!(body["stream"], true);
        assert_eq!(body["max_tokens"], 128);
        assert_eq!(body["tools"][0]["name"], "echo");
        assert_eq!(body["system"][1]["text"], "You are helpful.");
        assert_eq!(body["messages"][0]["role"], "user");
        assert_messages_body_has_no_sampling(&body);
    }

    /// Messages HTTP leads `system` with the Claude Code identity block and
    /// keeps the Gents preamble intact after it. Order matters: the identity
    /// is a wire-level prefix, not a rewrite of what the loop assembled.
    #[test]
    fn messages_body_system_leads_with_claude_code_identity_then_preamble() {
        let body = build_messages_body("claude-sonnet-5", &echo_request());
        let system = body["system"].as_array().expect("system array");
        assert_eq!(system.len(), 2, "{body}");
        assert_eq!(system[0]["type"], "text");
        assert_eq!(system[0]["text"], CLAUDE_CODE_IDENTITY);
        assert_eq!(system[1]["type"], "text");
        assert_eq!(system[1]["text"], "You are helpful.");
    }

    #[test]
    fn messages_body_system_is_identity_only_without_preamble() {
        for preamble in [None, Some(String::new()), Some("   ".to_string())] {
            let mut request = echo_request();
            request.preamble = preamble;
            let body = build_messages_body("claude-sonnet-5", &request);
            let system = body["system"].as_array().expect("system array");
            assert_eq!(system.len(), 1, "{body}");
            assert_eq!(system[0]["text"], CLAUDE_CODE_IDENTITY);
        }
    }

    /// The identity block is Messages-HTTP-only: neither the flattened
    /// process-CLI prompt nor the CLI argv may carry it.
    #[test]
    fn claude_code_identity_stays_off_the_process_cli_wire() {
        let request = echo_request();
        let prompt = crate::claude_subscription::flatten_completion_request(&request);
        assert!(!prompt.contains(CLAUDE_CODE_IDENTITY), "{prompt}");
        let argv = crate::claude_completer::completer_argv(&prompt, Some("claude-sonnet-5"));
        assert!(
            argv.iter()
                .all(|arg| !arg.to_string_lossy().contains(CLAUDE_CODE_IDENTITY)),
            "{argv:?}"
        );
    }

    #[test]
    fn messages_body_omits_sampling_even_when_request_sets_it() {
        let mut request = echo_request();
        request.temperature = Some(0.7);
        request.additional_params = Some(json!({
            "temperature": 0.2,
            "top_p": 0.9,
            "top_k": 40,
            "seed": 1,
            "min_p": 0.05,
            "frequency_penalty": 0.1,
            "presence_penalty": 0.1,
        }));
        let body = build_messages_body("claude-sonnet-5", &request);
        assert_messages_body_has_no_sampling(&body);
        assert_eq!(body["tools"][0]["name"], "echo");
    }

    fn assert_messages_body_has_no_sampling(body: &Value) {
        assert!(
            messages_body_has_only_allowed_keys(body),
            "unexpected Messages key: {body}"
        );
        for forbidden in [
            "temperature",
            "top_p",
            "top_k",
            "seed",
            "min_p",
            "frequency_penalty",
            "presence_penalty",
        ] {
            assert!(
                body.get(forbidden).is_none(),
                "{forbidden} must not appear on Claude Messages: {body}"
            );
        }
    }

    #[test]
    fn messages_body_threads_tool_result() {
        use rig::completion::message::{
            AssistantContent, ToolCall, ToolFunction, ToolResult, ToolResultContent,
        };
        let request = CompletionRequest {
            model: None,
            preamble: None,
            chat_history: OneOrMany::many(vec![
                Message::Assistant {
                    id: None,
                    content: OneOrMany::one(AssistantContent::ToolCall(ToolCall::new(
                        "toolu_1".into(),
                        ToolFunction::new("echo".into(), json!({})),
                    ))),
                },
                Message::User {
                    content: OneOrMany::one(UserContent::ToolResult(ToolResult {
                        id: "toolu_1".into(),
                        call_id: None,
                        content: OneOrMany::one(ToolResultContent::Text(Text {
                            text: "ECHOED".into(),
                        })),
                    })),
                },
            ])
            .expect("history"),
            documents: Vec::new(),
            tools: vec![rig::completion::ToolDefinition {
                name: "echo".into(),
                description: "echo".into(),
                parameters: json!({"type":"object"}),
            }],
            temperature: None,
            max_tokens: None,
            tool_choice: None,
            additional_params: None,
            output_schema: None,
        };
        let body = build_messages_body("claude-sonnet-5", &request);
        assert_eq!(body["messages"][0]["content"][0]["type"], "tool_use");
        assert_eq!(body["messages"][1]["content"][0]["type"], "tool_result");
        assert_eq!(body["messages"][1]["content"][0]["content"], "ECHOED");
    }

    #[test]
    fn sse_maps_echo_and_rejects_bash() {
        let sse = r#"
event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"toolu_1","name":"echo","input":{}}}

event: content_block_stop
data: {"type":"content_block_stop","index":0}

event: message_stop
data: {"type":"message_stop"}
"#;
        let surface = HashSet::from(["echo".to_string()]);
        let events = parse_messages_sse(sse, &surface).expect("map");
        assert!(matches!(
            &events[0],
            RawStreamingChoice::ToolCall(call) if call.name == "echo" && call.id == "toolu_1"
        ));

        let bash = sse.replace("echo", "Bash");
        let err = parse_messages_sse(&bash, &surface).expect_err("Bash");
        assert!(err.to_string().contains("Bash"), "{err}");
    }

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
        sse.push_str(
            "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
        );
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
        assert!(
            err.to_string().contains("fail-closed: malformed tool_use"),
            "{err}"
        );
    }

    #[test]
    fn sse_duplicate_tool_use_id_fails_closed() {
        let mut sse = sse_tool_use_block("toolu_1", "echo", "{}", &["{}"]);
        sse.push_str(&sse_tool_use_block("toolu_1", "echo", "{}", &["{}"]));
        let surface = HashSet::from(["echo".to_string()]);
        let err = parse_messages_sse(&sse, &surface).expect_err("duplicate id");
        assert!(
            err.to_string()
                .contains("fail-closed: duplicate tool_use id toolu_1"),
            "{err}"
        );
    }

    #[test]
    fn sse_overlapping_tool_use_block_fails_closed() {
        let first = sse_tool_use_block("toolu_1", "echo", "{}", &[]);
        let (start, _stop) = first.split_once("event: content_block_stop").expect("stop");
        let mut sse = start.to_string();
        sse.push_str(&sse_tool_use_block("toolu_2", "echo", "{}", &[]));
        let surface = HashSet::from(["echo".to_string()]);
        let err = parse_messages_sse(&sse, &surface).expect_err("overlap");
        assert!(
            err.to_string()
                .contains("fail-closed: overlapping tool_use block toolu_2"),
            "{err}"
        );
    }

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
}
