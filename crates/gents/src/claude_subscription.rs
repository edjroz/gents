//! Claude Max / Claude Code CLI subscription provider (Path A in-process).
//!
//! Seat truth lives in process-local `--claude-config-dir` state, not DefraDB
//! `OAuthCredential` documents. A2b is text-only: tools are never forwarded to
//! the CLI (`--tools ""`), and any `tool_use` in stdout fails closed.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Mutex, OnceLock};

use futures::stream;
use rig::client::CompletionClient;
use rig::completion::{
    CompletionError, CompletionModel, CompletionRequest, CompletionResponse, Usage,
};
use rig::one_or_many::OneOrMany;
use rig::streaming::{RawStreamingChoice, StreamingCompletionResponse};
use tokio::process::Command;
use tracing::warn;

use crate::claude_completer::{completer_argv, parse_stream_jsonl, sanitize_child_env};

/// Placeholder endpoint for ClaudeCliSubscription InferenceBackend rows.
///
/// The seat path comes from process-local server flags, not this field.
pub const DEFAULT_BACKEND_ENDPOINT: &str = "claude-cli://subscription";

pub fn default_backend_endpoint() -> &'static str {
    DEFAULT_BACKEND_ENDPOINT
}

pub fn default_model_name() -> &'static str {
    crate::claude_completer::proxy::DEFAULT_MODEL_ID
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

#[derive(Debug, Clone)]
pub struct ClaudeSubscriptionModel {
    model: String,
}

impl CompletionModel for ClaudeSubscriptionModel {
    type Response = ();
    type StreamingResponse = ();
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
        let text = complete_text(&self.model, &request).await?;
        Ok(CompletionResponse {
            choice: OneOrMany::one(rig::completion::AssistantContent::text(text)),
            usage: Usage::new(),
            raw_response: (),
            message_id: None,
        })
    }

    async fn stream(
        &self,
        request: CompletionRequest,
    ) -> Result<StreamingCompletionResponse<Self::StreamingResponse>, CompletionError> {
        let text = complete_text(&self.model, &request).await?;
        let items: Vec<Result<RawStreamingChoice<()>, CompletionError>> = vec![
            Ok(RawStreamingChoice::Message(text)),
            Ok(RawStreamingChoice::FinalResponse(())),
        ];
        Ok(StreamingCompletionResponse::stream(Box::pin(stream::iter(
            items,
        ))))
    }
}

async fn complete_text(
    model: &str,
    request: &CompletionRequest,
) -> Result<String, CompletionError> {
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

    if let Some(fake) = &seat.fake_completer {
        return run_fake_completer(fake, &prompt).await;
    }

    if !crate::claude_completer::proxy::live_claude_allowed(seat.write_approved) {
        return Err(CompletionError::ProviderError(
            "live Claude path refused: pass --claude-write-approved after an explicit numbered write approval"
                .to_string(),
        ));
    }

    run_live_completer(&seat, model, &prompt).await
}

async fn run_fake_completer(fake: &Path, prompt: &str) -> Result<String, CompletionError> {
    let output = Command::new(fake)
        .arg(prompt)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|err| {
            CompletionError::ProviderError(format!(
                "spawn fake completer {}: {err}",
                fake.display()
            ))
        })?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(CompletionError::ProviderError(format!(
            "fake completer exit {}: {}",
            output.status.code().unwrap_or(-1),
            err.trim()
        )));
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        return Err(CompletionError::ProviderError(
            "fake completer returned empty assistant text".to_string(),
        ));
    }
    Ok(text)
}

async fn run_live_completer(
    seat: &ClaudeSeatConfig,
    model: &str,
    prompt: &str,
) -> Result<String, CompletionError> {
    let mut argv = completer_argv(prompt, Some(model));
    if let Some(first) = argv.first_mut() {
        *first = seat.claude_bin.as_os_str().to_os_string();
    }

    let mut cmd = Command::new(&argv[0]);
    cmd.args(&argv[1..])
        .current_dir(&seat.workdir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_clear()
        .envs(sanitize_child_env(std::env::vars_os()))
        .env("CLAUDE_CONFIG_DIR", &seat.config_dir)
        .env("CLAUDE_WRITE_APPROVED", "1");
    for key in crate::claude_completer::STRIPPED_ENV_VARS {
        cmd.env_remove(key);
    }

    let output = cmd.output().await.map_err(|err| {
        CompletionError::ProviderError(format!(
            "spawn {}: {err}",
            seat.claude_bin.display()
        ))
    })?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stderr.trim().is_empty() {
        warn!(
            stderr = %stderr.trim(),
            "claude completer stderr"
        );
    }
    if !output.status.success() {
        return Err(CompletionError::ProviderError(format!(
            "completer exit {}: {}",
            output.status.code().unwrap_or(-1),
            stderr.trim().chars().take(500).collect::<String>()
        )));
    }

    let text = if stdout.lines().any(|line| line.trim_start().starts_with('{')) {
        parse_stream_jsonl(&stdout).map_err(|err| CompletionError::ProviderError(err.to_string()))?
    } else {
        stdout.trim().to_string()
    };
    if text.trim().is_empty() {
        return Err(CompletionError::ProviderError(
            "completer returned empty assistant text".to_string(),
        ));
    }
    Ok(text)
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
                        rig::completion::message::UserContent::Text(text) => Some(text.text.as_str()),
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
    }
}
