//! Claude subscription seat: process-local `--claude-config-dir` state, one
//! Messages HTTP wire (`claude_messages`), refuse-closed without
//! `--claude-write-approved`.

use std::collections::HashSet;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Mutex, OnceLock};

use futures::StreamExt;
use rig::OneOrMany;
use rig::client::CompletionClient;
use rig::completion::{
    CompletionError, CompletionModel, CompletionRequest, CompletionResponse, GetTokenUsage, Usage,
};
use rig::streaming::{RawStreamingChoice, StreamingCompletionResponse};
use serde::{Deserialize, Serialize};
use tokio::process::Command;

use crate::claude_completer::{parse_auth_status_logged_in, sanitize_child_env};

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
        .unwrap_or_else(|poison| poison.into_inner()) = config;
}

#[cfg(test)]
pub(crate) fn lock_process_seat_for_test() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let guard = LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    crate::claude_messages::install_messages_sse_fixtures(Vec::new());
    guard
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

/// Read-only process-local seat probe for BackendHealth.
///
/// Spawns `claude auth status --json` under `CLAUDE_CONFIG_DIR`. Never `-p`,
/// never writes oat. `Ok(())` means the binary ran and `loggedIn` is true.
pub async fn probe_process_seat_health() -> Result<(), String> {
    let seat = process_seat().ok_or_else(|| "process seat not installed".to_string())?;
    probe_seat_auth_status(&seat).await
}

async fn probe_seat_auth_status(seat: &ClaudeSeatConfig) -> Result<(), String> {
    let mut cmd = Command::new(&seat.claude_bin);
    cmd.args(["auth", "status", "--json"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .env_clear()
        .envs(sanitize_child_env(std::env::vars_os()))
        .env("CLAUDE_CONFIG_DIR", &seat.config_dir);
    for key in crate::claude_completer::STRIPPED_ENV_VARS {
        cmd.env_remove(key);
    }
    let output = cmd
        .output()
        .await
        .map_err(|error| format!("spawn {} auth status: {error}", seat.claude_bin.display()))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Real `claude auth status --json` may exit non-zero when logged out while
    // still printing JSON. Prefer `loggedIn` over the process status.
    if let Ok(logged_in) = parse_auth_status_logged_in(stdout.trim()) {
        return if logged_in {
            Ok(())
        } else {
            Err("loggedIn=false".to_string())
        };
    }
    if !output.status.success() {
        let detail = if !stderr.trim().is_empty() {
            stderr.trim()
        } else {
            stdout.trim()
        };
        return Err(format!(
            "claude auth status exit {}: {detail}",
            output.status.code().unwrap_or(-1)
        ));
    }
    Err(parse_auth_status_logged_in(stdout.trim()).unwrap_err())
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

#[derive(Debug, Clone)]
pub struct ClaudeSubscriptionModel {
    model: String,
}

fn surface_of(request: &CompletionRequest) -> HashSet<String> {
    request.tools.iter().map(|tool| tool.name.clone()).collect()
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

    /// Non-streaming turn: drains the same Messages stream `stream` returns
    /// and folds the text. Calls `stream_messages` directly so the only
    /// provider invocations in this crate stay inside the owned loop.
    async fn completion(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse<Self::Response>, CompletionError> {
        let surface = surface_of(&request);
        let stream =
            crate::claude_messages::stream_messages(&self.model, &request, surface).await?;
        futures::pin_mut!(stream);
        let mut text = String::new();
        let mut usage = Usage::new();
        while let Some(item) = stream.next().await {
            match item? {
                RawStreamingChoice::Message(chunk) => text.push_str(&chunk),
                RawStreamingChoice::FinalResponse(raw) => {
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
                "Claude Messages returned empty assistant text".to_string(),
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
        let surface = surface_of(&request);
        let stream =
            crate::claude_messages::stream_messages(&self.model, &request, surface).await?;
        Ok(StreamingCompletionResponse::stream(Box::pin(stream)))
    }
}

#[cfg(test)]
#[path = "claude_subscription/tests.rs"]
mod tests;
