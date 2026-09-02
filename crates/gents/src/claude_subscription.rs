//! Claude Max / Claude Code CLI subscription provider (Path A in-process).
//!
//! Seat truth lives in process-local `--claude-config-dir` state, not DefraDB
//! `OAuthCredential` documents. A2b is text-only: tools are never forwarded to
//! the CLI (`--tools ""`), and any `tool_use` in stdout fails closed.

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Mutex, OnceLock};

use futures::StreamExt;
use rig::client::CompletionClient;
use rig::completion::{
    CompletionError, CompletionModel, CompletionRequest, CompletionResponse, GetTokenUsage, Usage,
};
use rig::one_or_many::OneOrMany;
use rig::streaming::{RawStreamingChoice, StreamingCompletionResponse};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tracing::warn;

use crate::claude_completer::{
    CompleterUsage, StreamJsonlState, completer_argv, sanitize_child_env,
};

/// Placeholder endpoint for ClaudeCliSubscription InferenceBackend rows.
///
/// The seat path comes from process-local server flags, not this field.
pub const DEFAULT_BACKEND_ENDPOINT: &str = "claude-cli://subscription";

pub fn default_backend_endpoint() -> &'static str {
    DEFAULT_BACKEND_ENDPOINT
}

pub fn default_model_name() -> &'static str {
    crate::claude_completer::DEFAULT_MODEL_ID
}

#[derive(Debug, Clone)]
pub struct ClaudeSeatConfig {
    pub config_dir: PathBuf,
    pub write_approved: bool,
    pub workdir: PathBuf,
    pub log_dir: Option<PathBuf>,
    pub claude_bin: PathBuf,
    /// Test-only: spawn this binary instead of the live Claude CLI.
    pub fake_completer: Option<PathBuf>,
}

impl ClaudeSeatConfig {
    pub fn from_server_flags(
        config_dir: PathBuf,
        write_approved: bool,
        workdir: Option<PathBuf>,
        log_dir: Option<PathBuf>,
        claude_bin: Option<PathBuf>,
        fake_completer: Option<PathBuf>,
    ) -> Self {
        let workdir = workdir.unwrap_or_else(|| {
            config_dir
                .parent()
                .map(|parent| parent.join("workdir"))
                .unwrap_or_else(|| config_dir.join("workdir"))
        });
        Self {
            config_dir,
            write_approved,
            workdir,
            log_dir,
            claude_bin: claude_bin.unwrap_or_else(|| PathBuf::from("claude")),
            fake_completer,
        }
    }
}

fn process_seat_slot() -> &'static Mutex<Option<ClaudeSeatConfig>> {
    static PROCESS_SEAT: OnceLock<Mutex<Option<ClaudeSeatConfig>>> = OnceLock::new();
    PROCESS_SEAT.get_or_init(|| Mutex::new(None))
}

/// Install process-local Claude seat config (server startup / tests).
///
/// Passing `None` means ClaudeCliSubscription backends are disabled for this
/// process.
pub fn install_process_seat(config: Option<ClaudeSeatConfig>) {
    *process_seat_slot()
        .lock()
        .expect("claude process seat mutex poisoned") = config;
}

pub fn process_seat() -> Option<ClaudeSeatConfig> {
    process_seat_slot()
        .lock()
        .expect("claude process seat mutex poisoned")
        .clone()
}

pub fn require_process_seat() -> Result<ClaudeSeatConfig, CompletionError> {
    process_seat().ok_or_else(|| {
        CompletionError::ProviderError(
            "ClaudeCliSubscription requires gents server --claude-config-dir (process seat not installed)"
                .to_string(),
        )
    })
}

#[derive(Debug, Clone, Default)]
pub struct ClaudeSubscriptionClient;

impl ClaudeSubscriptionClient {
    pub fn new() -> Self {
        Self
    }
}

impl CompletionClient for ClaudeSubscriptionClient {
    type CompletionModel = ClaudeSubscriptionModel;
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct ClaudeStreamResponse {
    pub usage: Option<Usage>,
}

impl GetTokenUsage for ClaudeStreamResponse {
    fn token_usage(&self) -> Option<Usage> {
        self.usage
    }
}

fn rig_usage_from_completer(usage: CompleterUsage) -> Usage {
    Usage {
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
        total_tokens: usage.input_tokens.saturating_add(usage.output_tokens),
        cached_input_tokens: usage.cache_read_input_tokens,
        cache_creation_input_tokens: usage.cache_creation_input_tokens,
    }
}

#[derive(Debug, Clone)]
pub struct ClaudeSubscriptionModel {
    model: String,
}

impl CompletionModel for ClaudeSubscriptionModel {
    type Response = ();
    type StreamingResponse = ClaudeStreamResponse;
    type Client = ClaudeSubscriptionClient;

    fn make(_: &Self::Client, model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
        }
    }

    async fn completion(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse<Self::Response>, CompletionError> {
        let mut stream = self.stream(request).await?;
        let mut text = String::new();
        let mut usage = Usage::new();
        while let Some(item) = stream.next().await {
            match item? {
                rig::streaming::StreamedAssistantContent::Text(chunk) => text.push_str(&chunk.text),
                rig::streaming::StreamedAssistantContent::Final(raw) => {
                    if let Some(reported) = raw.token_usage() {
                        usage = reported;
                    }
                    break;
                }
                _ => {}
            }
        }
        if text.trim().is_empty() {
            return Err(CompletionError::ProviderError(
                "completer returned empty assistant text".to_string(),
            ));
        }
        Ok(CompletionResponse {
            choice: OneOrMany::one(rig::completion::AssistantContent::text(text)),
            usage,
            raw_response: (),
            message_id: None,
        })
    }

    async fn stream(
        &self,
        request: CompletionRequest,
    ) -> Result<StreamingCompletionResponse<Self::StreamingResponse>, CompletionError> {
        let (child, label) = spawn_completer(&self.model, &request).await?;
        Ok(StreamingCompletionResponse::stream(Box::pin(
            stream_child_stdout(child, label),
        )))
    }
}

async fn spawn_completer(
    model: &str,
    request: &CompletionRequest,
) -> Result<(tokio::process::Child, String), CompletionError> {
    let seat = require_process_seat()?;
    if !request.tools.is_empty() {
        warn!(
            tool_count = request.tools.len(),
            "ClaudeCliSubscription A2b is text-only; ignoring owned-loop tool definitions (CLI forced --tools \"\")"
        );
    }
    let prompt = flatten_completion_request(request);
    let prompt = if prompt.trim().is_empty() {
        "Reply with exactly: pong".to_string()
    } else {
        prompt
    };

    // Non-HTTP Completer: claim+persist the armed rendered-request capture
    // before spawning Claude. Without this the owned loop's "response arrived
    // with capture still armed" fence treats Claude as a mis-wired stack.
    capture_claude_cli_request(model, &prompt, request).await?;

    if let Some(fake) = &seat.fake_completer {
        let child = Command::new(fake)
            .arg(&prompt)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|err| {
                CompletionError::ProviderError(format!(
                    "spawn fake completer {}: {err}",
                    fake.display()
                ))
            })?;
        return Ok((child, format!("fake completer {}", fake.display())));
    }

    if !crate::claude_completer::live_claude_allowed(seat.write_approved) {
        return Err(CompletionError::ProviderError(
            "live Claude path refused: pass --claude-write-approved after an explicit numbered write approval"
                .to_string(),
        ));
    }

    let mut argv = completer_argv(&prompt, Some(model));
    if let Some(first) = argv.first_mut() {
        *first = seat.claude_bin.as_os_str().to_os_string();
    }

    let mut cmd = Command::new(&argv[0]);
    cmd.args(&argv[1..])
        .current_dir(&seat.workdir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .env_clear()
        .envs(sanitize_child_env(std::env::vars_os()))
        .env("CLAUDE_CONFIG_DIR", &seat.config_dir)
        .env("CLAUDE_WRITE_APPROVED", "1");
    for key in crate::claude_completer::STRIPPED_ENV_VARS {
        cmd.env_remove(key);
    }
    let child = cmd.spawn().map_err(|err| {
        CompletionError::ProviderError(format!("spawn {}: {err}", seat.claude_bin.display()))
    })?;
    Ok((child, seat.claude_bin.display().to_string()))
}

fn stream_child_stdout(
    mut child: tokio::process::Child,
    label: String,
) -> impl futures::Stream<Item = Result<RawStreamingChoice<ClaudeStreamResponse>, CompletionError>>
{
    async_stream::stream! {
        let Some(stdout) = child.stdout.take() else {
            yield Err(CompletionError::ProviderError(format!(
                "{label} spawned without stdout pipe"
            )));
            return;
        };
        let stderr = child.stderr.take();
        let stderr_task = tokio::spawn(async move {
            let mut buf = Vec::new();
            if let Some(mut pipe) = stderr {
                let _ = tokio::io::copy(&mut pipe, &mut buf).await;
            }
            buf
        });

        let mut lines = BufReader::new(stdout).lines();
        let mut state = StreamJsonlState::new();
        let mut jsonl_mode: Option<bool> = None;
        let mut plain = String::new();
        let mut yielded_text = false;

        let read_error = loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    if jsonl_mode.is_none() {
                        let trimmed = line.trim_start();
                        if trimmed.is_empty() {
                            continue;
                        }
                        jsonl_mode = Some(trimmed.starts_with('{'));
                    }
                    if jsonl_mode == Some(true) {
                        match state.push_line(&line) {
                            Ok(Some(text)) => {
                                yielded_text = true;
                                yield Ok(RawStreamingChoice::Message(text));
                            }
                            Ok(None) => {}
                            Err(error) => {
                                let _ = child.start_kill();
                                yield Err(CompletionError::ProviderError(error.to_string()));
                                return;
                            }
                        }
                    } else if jsonl_mode == Some(false) {
                        if !plain.is_empty() {
                            plain.push('\n');
                        }
                        plain.push_str(&line);
                    }
                }
                Ok(None) => break None,
                Err(error) => break Some(error),
            }
        };
        if let Some(error) = read_error {
            let _ = child.start_kill();
            yield Err(CompletionError::ProviderError(format!(
                "{label} stdout: {error}"
            )));
            return;
        }

        let status = match child.wait().await {
            Ok(status) => status,
            Err(error) => {
                yield Err(CompletionError::ProviderError(format!(
                    "wait {label}: {error}"
                )));
                return;
            }
        };
        let stderr_buf = stderr_task.await.unwrap_or_else(|_| Vec::new());
        let stderr = String::from_utf8_lossy(&stderr_buf);
        if !stderr.trim().is_empty() {
            warn!(
                completer = %label,
                stderr = %stderr.trim(),
                "claude completer stderr"
            );
        }
        if !status.success() {
            yield Err(CompletionError::ProviderError(format!(
                "{label} exit {}: {}",
                status.code().unwrap_or(-1),
                stderr.trim().chars().take(500).collect::<String>()
            )));
            return;
        }

        if jsonl_mode == Some(true) {
            let usage = state.usage().map(rig_usage_from_completer);
            match state.finish() {
                Ok(text) => {
                    if !yielded_text {
                        yield Ok(RawStreamingChoice::Message(text));
                    }
                    yield Ok(RawStreamingChoice::FinalResponse(ClaudeStreamResponse {
                        usage,
                    }));
                }
                Err(error) => {
                    yield Err(CompletionError::ProviderError(error.to_string()));
                }
            }
        } else {
            let text = plain.trim().to_string();
            if text.is_empty() {
                yield Err(CompletionError::ProviderError(format!(
                    "{label} returned empty assistant text"
                )));
                return;
            }
            yield Ok(RawStreamingChoice::Message(text));
            yield Ok(RawStreamingChoice::FinalResponse(ClaudeStreamResponse {
                usage: None,
            }));
        }
    }
}

async fn capture_claude_cli_request(
    model: &str,
    prompt: &str,
    request: &CompletionRequest,
) -> Result<(), CompletionError> {
    let request_json = serde_json::json!({
        "model": model,
        "messages": [{
            "role": "user",
            "content": prompt,
        }],
        "tools": [],
        "tool_choice": null,
        "temperature": request.temperature,
        "max_tokens": request.max_tokens,
        "provider": "ClaudeCliSubscription",
        "endpoint": DEFAULT_BACKEND_ENDPOINT,
    });
    crate::rendered_request::scope::claim_and_capture_process_cli(
        request_json,
        Some(DEFAULT_BACKEND_ENDPOINT.to_string()),
    )
    .await
    .map_err(|error| {
        CompletionError::ProviderError(format!(
            "rendered-request capture failed before Claude CLI spawn: {error:#}"
        ))
    })
}

/// Flatten a rig CompletionRequest into the text prompt the CLI completer expects.
pub fn flatten_completion_request(request: &CompletionRequest) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(preamble) = request
        .preamble
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        parts.push(format!("system: {preamble}"));
    }
    for message in request.chat_history.iter() {
        match message {
            rig::completion::Message::User { content } => {
                let text = content
                    .iter()
                    .filter_map(|block| match block {
                        rig::completion::message::UserContent::Text(text) => {
                            Some(text.text.as_str())
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("");
                if !text.is_empty() {
                    parts.push(format!("user: {text}"));
                }
            }
            rig::completion::Message::Assistant { content, .. } => {
                let text = content
                    .iter()
                    .filter_map(|block| match block {
                        rig::completion::message::AssistantContent::Text(text) => {
                            Some(text.text.as_str())
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("");
                if !text.is_empty() {
                    parts.push(format!("assistant: {text}"));
                }
            }
            other => {
                // System / other roles: best-effort Display via Debug-ish string.
                let rendered = format!("{other:?}");
                if rendered.contains("System") {
                    // Prefer extracting text if present in Debug is brittle; skip empty.
                }
                let _ = other;
            }
        }
    }
    // Also fold leading system messages from chat_history when preamble is absent.
    // rig may place system instructions as Message::User with role metadata in some
    // paths; the preamble branch above covers the owned-loop common case.
    parts.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rig::completion::message::{Message, Text, UserContent};
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    fn workspace_tempdir(label: &str) -> PathBuf {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../.scratch/claude-spike/tmp")
            .join(label);
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create workspace tempdir");
        root
    }

    fn write_fake_completer(dir: &Path, body: &str) -> PathBuf {
        let path = dir.join("fake-completer.sh");
        let mut file = std::fs::File::create(&path).expect("create fake");
        writeln!(file, "#!/bin/sh").unwrap();
        writeln!(file, "{body}").unwrap();
        drop(file);
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).unwrap();
        path
    }

    /// JSONL fake that emits one assistant line, sleeps, then the rest — used
    /// to prove the Completer yields before the child exits.
    fn write_delayed_jsonl_fake(dir: &Path) -> PathBuf {
        let py = dir.join("fake-completer.py");
        std::fs::write(
            &py,
            r#"
import sys, time
def emit(line):
    sys.stdout.write(line + "\n")
    sys.stdout.flush()
emit('{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"po"}]}}')
time.sleep(1.2)
emit('{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"ng"}]}}')
emit('{"type":"result","subtype":"success","result":"pong","is_error":false}')
"#,
        )
        .expect("write python fake");
        write_fake_completer(dir, &format!("exec python3 -u '{}' \"$@\"", py.display()))
    }

    #[tokio::test]
    async fn process_cli_capture_claims_armed_scope_before_fake_completer() {
        use crate::rendered_request::scope::{
            ambient_arming_sink, pending_is_armed, scope_request, test_scope,
        };
        use crate::rendered_request::{
            AssemblyBuildPath, AssemblyTrace, CaptureScopeKind, RenderedCompletionRequest,
            RenderedRequestCaptureSink, RenderedRequestContext, RenderedRequestSource,
        };
        use tokio::sync::Mutex;

        let temp = workspace_tempdir("process-cli-capture");
        let fake = write_fake_completer(&temp, "printf 'pong\\n'");
        install_process_seat(Some(ClaudeSeatConfig {
            config_dir: temp.join("claude-config"),
            write_approved: false,
            workdir: temp.join("workdir"),
            log_dir: None,
            claude_bin: PathBuf::from("claude"),
            fake_completer: Some(fake),
        }));
        std::fs::create_dir_all(temp.join("workdir")).unwrap();

        let seen: Arc<Mutex<Vec<RenderedCompletionRequest>>> = Arc::new(Mutex::new(Vec::new()));
        let sink_seen = seen.clone();
        let sink: RenderedRequestCaptureSink = Arc::new(move |rendered| {
            let sink_seen = sink_seen.clone();
            Box::pin(async move {
                sink_seen.lock().await.push(rendered);
                Ok(())
            })
        });
        let context = RenderedRequestContext {
            request_doc_id: "doc-claude".to_string(),
            request_commit_cid: "bafy-request-commit".to_string(),
            request_id: "req-claude".to_string(),
            agent_did: "did:key:agent".to_string(),
            requester_did: String::new(),
            behavior_id: "behavior".to_string(),
            session_id: "session-claude".to_string(),
            model_name: "claude-sonnet-5".to_string(),
        };
        let scope = test_scope(context, sink);

        scope_request(scope, async {
            let arm = ambient_arming_sink(CaptureScopeKind::Inference);
            arm(
                0,
                0,
                ping_request(),
                AssemblyTrace::from_effective_messages(AssemblyBuildPath::Budgeted, Vec::new()),
            )
            .await
            .expect("arm");
            assert!(pending_is_armed(), "capture must be armed before Completer");

            let client = ClaudeSubscriptionClient::new();
            let model = client.completion_model("claude-sonnet-5");
            let mut stream = model.stream(ping_request()).await.expect("stream");
            use futures::StreamExt;
            use rig::streaming::StreamedAssistantContent;
            let mut texts = Vec::new();
            while let Some(item) = stream.next().await {
                match item.expect("chunk") {
                    StreamedAssistantContent::Text(text) => texts.push(text.text),
                    StreamedAssistantContent::Final(_) => break,
                    _ => {}
                }
            }
            assert_eq!(texts.join(""), "pong");
            assert!(
                !pending_is_armed(),
                "Claude Completer must claim the armed capture"
            );
        })
        .await;

        let captured = seen.lock().await;
        assert_eq!(captured.len(), 1, "exactly one durable capture");
        assert_eq!(
            captured[0].source,
            RenderedRequestSource::ClaudeCliSubscription
        );
        assert_eq!(
            captured[0]
                .provenance_json
                .get("capture_seam")
                .and_then(|v| v.as_str()),
            Some("process_cli")
        );
        assert_eq!(captured[0].model_name, "claude-sonnet-5");
        assert!(
            captured[0]
                .request_json
                .get("endpoint")
                .and_then(|v| v.as_str())
                == Some(DEFAULT_BACKEND_ENDPOINT)
        );
        assert_eq!(
            captured[0].request_json.get("tools"),
            Some(&serde_json::json!([]))
        );
    }

    #[tokio::test]
    async fn stream_yields_jsonl_text_before_completer_exits() {
        let temp = workspace_tempdir("process-cli-stream-delay");
        let fake = write_delayed_jsonl_fake(&temp);
        install_process_seat(Some(ClaudeSeatConfig {
            config_dir: temp.join("claude-config"),
            write_approved: false,
            workdir: temp.join("workdir"),
            log_dir: None,
            claude_bin: PathBuf::from("claude"),
            fake_completer: Some(fake),
        }));
        std::fs::create_dir_all(temp.join("workdir")).unwrap();

        let started = Instant::now();
        let client = ClaudeSubscriptionClient::new();
        let model = client.completion_model("claude-sonnet-5");
        let mut stream = model.stream(ping_request()).await.expect("stream");
        use futures::StreamExt;
        use rig::streaming::StreamedAssistantContent;
        let first = loop {
            match stream.next().await.expect("item").expect("chunk") {
                StreamedAssistantContent::Text(text) => break text.text,
                StreamedAssistantContent::Final(_) => panic!("final before text"),
                _ => {}
            }
        };
        let first_wait = started.elapsed();
        assert_eq!(first, "po");
        assert!(
            first_wait < Duration::from_millis(800),
            "first JSONL chunk must arrive before the 1.2s completer sleep, waited {first_wait:?}"
        );

        let mut rest = Vec::new();
        while let Some(item) = stream.next().await {
            match item.expect("chunk") {
                StreamedAssistantContent::Text(text) => rest.push(text.text),
                StreamedAssistantContent::Final(_) => break,
                _ => {}
            }
        }
        assert_eq!(rest.join(""), "ng");
        assert!(
            started.elapsed() >= Duration::from_millis(1000),
            "completer sleep must still run after the first yield"
        );
    }

    #[tokio::test]
    async fn stream_reports_result_usage_on_final() {
        let temp = workspace_tempdir("process-cli-stream-usage");
        let jsonl = temp.join("stdout.jsonl");
        std::fs::write(
            &jsonl,
            include_str!("claude_completer/fixtures/assistant_ok_with_usage.jsonl"),
        )
        .expect("write usage fixture");
        let fake = write_fake_completer(&temp, &format!("exec cat '{}'", jsonl.display()));
        install_process_seat(Some(ClaudeSeatConfig {
            config_dir: temp.join("claude-config"),
            write_approved: false,
            workdir: temp.join("workdir"),
            log_dir: None,
            claude_bin: PathBuf::from("claude"),
            fake_completer: Some(fake),
        }));
        std::fs::create_dir_all(temp.join("workdir")).unwrap();

        let client = ClaudeSubscriptionClient::new();
        let model = client.completion_model("claude-sonnet-5");
        let mut stream = model.stream(ping_request()).await.expect("stream");
        use futures::StreamExt;
        use rig::streaming::StreamedAssistantContent;
        let mut usage = None;
        while let Some(item) = stream.next().await {
            match item.expect("chunk") {
                StreamedAssistantContent::Final(raw) => {
                    usage = raw.token_usage();
                    break;
                }
                _ => {}
            }
        }
        assert_eq!(
            usage,
            Some(Usage {
                input_tokens: 2,
                output_tokens: 4,
                total_tokens: 6,
                cached_input_tokens: 0,
                cache_creation_input_tokens: 2774,
            })
        );
    }

    fn ping_request() -> CompletionRequest {
        CompletionRequest {
            model: None,
            preamble: None,
            chat_history: OneOrMany::one(Message::User {
                content: OneOrMany::one(UserContent::Text(Text {
                    text: "Reply with exactly: pong".into(),
                })),
            }),
            documents: Vec::new(),
            tools: Vec::new(),
            temperature: None,
            max_tokens: None,
            tool_choice: None,
            additional_params: None,
            output_schema: None,
        }
    }

    fn install_fake_seat(temp: &Path, fake_body: &str) -> PathBuf {
        let fake = write_fake_completer(temp, fake_body);
        install_process_seat(Some(ClaudeSeatConfig {
            config_dir: temp.join("claude-config"),
            write_approved: false,
            workdir: temp.join("workdir"),
            log_dir: None,
            claude_bin: PathBuf::from("claude"),
            fake_completer: Some(fake),
        }));
        std::fs::create_dir_all(temp.join("workdir")).unwrap();
        temp.join("spawned")
    }

    #[tokio::test]
    async fn process_cli_unexplained_send_inside_scope_does_not_spawn() {
        use crate::rendered_request::scope::{scope_request, test_scope};
        use crate::rendered_request::{RenderedRequestCaptureSink, RenderedRequestContext};

        let temp = workspace_tempdir("process-cli-unexplained");
        let spawned = install_fake_seat(
            &temp,
            &format!(
                "touch '{}' && printf 'pong\\n'",
                temp.join("spawned").display()
            ),
        );
        let sink: RenderedRequestCaptureSink = Arc::new(|_| Box::pin(async { Ok(()) }));
        let scope = test_scope(
            RenderedRequestContext {
                request_doc_id: "doc-unexplained".to_string(),
                request_commit_cid: "bafy-request-commit".to_string(),
                request_id: "req-unexplained".to_string(),
                agent_did: "did:key:agent".to_string(),
                requester_did: String::new(),
                behavior_id: "behavior".to_string(),
                session_id: "session-unexplained".to_string(),
                model_name: "claude-sonnet-5".to_string(),
            },
            sink,
        );

        let error = scope_request(scope, async {
            let client = ClaudeSubscriptionClient::new();
            let model = client.completion_model("claude-sonnet-5");
            match model.stream(ping_request()).await {
                Err(err) => err,
                Ok(_) => panic!("unexplained Completer send must be refused"),
            }
        })
        .await;
        let message = error.to_string();
        assert!(
            message.contains("no armed rendered-request capture"),
            "{message}"
        );
        assert!(
            !spawned.exists(),
            "unexplained send must not spawn the fake completer"
        );
    }

    #[tokio::test]
    async fn process_cli_capture_failure_does_not_spawn_fake_completer() {
        use crate::rendered_request::scope::{ambient_arming_sink, scope_request, test_scope};
        use crate::rendered_request::{
            AssemblyBuildPath, AssemblyTrace, CaptureScopeKind, RenderedRequestCaptureSink,
            RenderedRequestContext,
        };

        let temp = workspace_tempdir("process-cli-capture-fail");
        let spawned = install_fake_seat(
            &temp,
            &format!(
                "touch '{}' && printf 'pong\\n'",
                temp.join("spawned").display()
            ),
        );
        let sink: RenderedRequestCaptureSink =
            Arc::new(|_| Box::pin(async { anyhow::bail!("injected capture failure") }));
        let scope = test_scope(
            RenderedRequestContext {
                request_doc_id: "doc-fail".to_string(),
                request_commit_cid: "bafy-request-commit".to_string(),
                request_id: "req-fail".to_string(),
                agent_did: "did:key:agent".to_string(),
                requester_did: String::new(),
                behavior_id: "behavior".to_string(),
                session_id: "session-fail".to_string(),
                model_name: "claude-sonnet-5".to_string(),
            },
            sink,
        );

        let error = scope_request(scope, async {
            let arm = ambient_arming_sink(CaptureScopeKind::Inference);
            arm(
                0,
                0,
                ping_request(),
                AssemblyTrace::from_effective_messages(AssemblyBuildPath::Budgeted, Vec::new()),
            )
            .await
            .expect("arm");
            let client = ClaudeSubscriptionClient::new();
            let model = client.completion_model("claude-sonnet-5");
            match model.stream(ping_request()).await {
                Err(err) => err,
                Ok(_) => panic!("persist failure must refuse spawn"),
            }
        })
        .await;
        let message = error.to_string();
        assert!(message.contains("was not issued"), "{message}");
        assert!(
            !spawned.exists(),
            "capture failure must not spawn the fake completer"
        );
    }

    #[test]
    fn flatten_includes_preamble_and_user_text() {
        let request = CompletionRequest {
            model: None,
            preamble: Some("You are helpful.".into()),
            chat_history: OneOrMany::one(Message::User {
                content: OneOrMany::one(UserContent::Text(Text {
                    text: "ping".into(),
                })),
            }),
            documents: Vec::new(),
            tools: Vec::new(),
            temperature: None,
            max_tokens: None,
            tool_choice: None,
            additional_params: None,
            output_schema: None,
        };
        let prompt = flatten_completion_request(&request);
        assert!(prompt.contains("system: You are helpful."));
        assert!(prompt.contains("user: ping"));
    }

    #[tokio::test]
    async fn fake_completer_stream_returns_text_without_write_approval() {
        let temp = workspace_tempdir("fake-completer-stream");
        let fake = write_fake_completer(&temp, "printf 'pong\\n'");
        install_process_seat(Some(ClaudeSeatConfig {
            config_dir: temp.join("claude-config"),
            write_approved: false,
            workdir: temp.join("workdir"),
            log_dir: None,
            claude_bin: PathBuf::from("claude"),
            fake_completer: Some(fake),
        }));
        std::fs::create_dir_all(temp.join("workdir")).unwrap();

        let client = ClaudeSubscriptionClient::new();
        let model = client.completion_model("claude-sonnet-5");
        let request = CompletionRequest {
            model: None,
            preamble: None,
            chat_history: OneOrMany::one(Message::User {
                content: OneOrMany::one(UserContent::Text(Text {
                    text: "Reply with exactly: pong".into(),
                })),
            }),
            documents: Vec::new(),
            tools: Vec::new(),
            temperature: None,
            max_tokens: None,
            tool_choice: None,
            additional_params: None,
            output_schema: None,
        };
        let mut stream = model.stream(request).await.expect("stream");
        use futures::StreamExt;
        use rig::streaming::StreamedAssistantContent;
        let mut texts = Vec::new();
        while let Some(item) = stream.next().await {
            match item.expect("chunk") {
                StreamedAssistantContent::Text(text) => texts.push(text.text),
                StreamedAssistantContent::Final(_) => break,
                _ => {}
            }
        }
        assert_eq!(texts.join(""), "pong");
    }

    #[tokio::test]
    async fn live_path_refuses_without_write_approval() {
        let temp = workspace_tempdir("live-refuse");
        install_process_seat(Some(ClaudeSeatConfig {
            config_dir: temp.join("claude-config"),
            write_approved: false,
            workdir: temp.join("workdir"),
            log_dir: None,
            claude_bin: PathBuf::from("claude"),
            fake_completer: None,
        }));
        std::fs::create_dir_all(temp.join("workdir")).unwrap();

        let client = ClaudeSubscriptionClient::new();
        let model = client.completion_model("claude-sonnet-5");
        let request = CompletionRequest {
            model: None,
            preamble: None,
            chat_history: OneOrMany::one(Message::User {
                content: OneOrMany::one(UserContent::Text(Text {
                    text: "ping".into(),
                })),
            }),
            documents: Vec::new(),
            tools: Vec::new(),
            temperature: None,
            max_tokens: None,
            tool_choice: None,
            additional_params: None,
            output_schema: None,
        };
        let err = model.completion(request).await.expect_err("refused");
        let msg = err.to_string();
        assert!(
            msg.contains("--claude-write-approved"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn parse_aliases_round_trip() {
        assert_eq!(
            crate::BackendProviderKind::parse_optional(Some("ClaudeCliSubscription")).unwrap(),
            crate::BackendProviderKind::ClaudeCliSubscription
        );
        assert_eq!(
            crate::BackendProviderKind::parse_optional(Some("claude-cli-subscription")).unwrap(),
            crate::BackendProviderKind::ClaudeCliSubscription
        );
        assert_eq!(
            crate::BackendProviderKind::ClaudeCliSubscription.as_str(),
            "ClaudeCliSubscription"
        );
        assert!(!crate::BackendProviderKind::ClaudeCliSubscription.is_agent_scoped_oauth());
        assert!(crate::BackendProviderKind::ClaudeCliSubscription.skips_fleet_http_probe());
        assert!(crate::BackendProviderKind::ChatGptCodex.skips_fleet_http_probe());
        assert!(!crate::BackendProviderKind::OpenAiCompatible.skips_fleet_http_probe());
    }
}
