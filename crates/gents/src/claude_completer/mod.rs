//! Claude Max subscription completer adapter (Path A).
//!
//! Parses Claude Code `--output-format stream-json` JSONL into plain assistant
//! text and builds a sanitized child-process environment / argv for the CLI.
//!
//! A2b uses this from the in-process `ClaudeCliSubscription` Completer. The
//! transitional HTTP `claude-proxy` adapter was deleted in A2b-3.

use std::collections::HashMap;
use std::ffi::{OsStr, OsString};

use serde_json::Value;
use thiserror::Error;

/// Default client-facing model slug for ClaudeCliSubscription.
///
/// Full Claude model IDs only — not the old invented `claude-plan` seat label.
pub const DEFAULT_MODEL_ID: &str = "claude-sonnet-5";

/// Official Claude full model IDs for ClaudeCliSubscription backends.
pub const PATH_A_MODEL_IDS: &[&str] = &[
    "claude-opus-5",
    "claude-sonnet-5",
    "claude-haiku-4-5-20251001",
    "claude-fable-5",
];

/// Live Claude path requires `--claude-write-approved`.
pub fn live_claude_allowed(write_approved: bool) -> bool {
    write_approved
}

/// Environment variable names that must not reach the Claude CLI child.
///
/// These flip the seat onto API-key / cloud-provider billing paths. Keep in
/// sync with the spike completer (`.scratch/claude-spike/bin/claude-completer.sh`).
pub const STRIPPED_ENV_VARS: &[&str] = &[
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_API_KEY_OLD",
    "CLAUDE_CODE_OAUTH_TOKEN",
    "AWS_BEARER_TOKEN_BEDROCK",
    "ANTHROPIC_BEDROCK_BASE_URL",
    "ANTHROPIC_VERTEX_PROJECT_ID",
    "CLOUD_ML_REGION",
    "ANTHROPIC_FOUNDRY_API_KEY",
];

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CompleterParseError {
    #[error("invalid JSONL at line {line}: {message}")]
    InvalidJson { line: usize, message: String },
    #[error("fail-closed: tool_use observed ({names})")]
    ToolUse { names: String },
    #[error("claude result is_error=true: {message}")]
    ResultError { message: String },
    #[error("fail-closed: empty assistant text / missing result")]
    EmptyAssistantText,
}

/// Incremental fail-closed parser for Claude Code `--output-format stream-json`.
///
/// One JSONL line at a time so the Completer can yield assistant text before
/// the child process exits. `parse_stream_jsonl` is the buffered oracle over
/// the same state machine. `tool_use` still fail-closes (A2b); A2c will map
/// gents names on this type.
/// Token counts from a Claude Code `result.usage` object.
///
/// Field names pinned from a live `stream-json` `result` line
/// (`.scratch/claude-spike/logs/completer-20260830T001534Z.jsonl`):
/// `input_tokens`, `output_tokens`, `cache_read_input_tokens`,
/// `cache_creation_input_tokens`. Absent object / no known keys → `None`
/// (owned-loop ledger treats zeros/missing as `Missing`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CompleterUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_input_tokens: u64,
    pub cache_creation_input_tokens: u64,
}

impl CompleterUsage {
    /// Parse a `result.usage` JSON object. Returns `None` when no known token
    /// field is present so we never invent a report.
    pub fn from_result_usage(value: &Value) -> Option<Self> {
        let obj = value.as_object()?;
        let input_tokens = json_u64(obj, "input_tokens");
        let output_tokens = json_u64(obj, "output_tokens");
        let cache_read_input_tokens = json_u64(obj, "cache_read_input_tokens");
        let cache_creation_input_tokens = json_u64(obj, "cache_creation_input_tokens");
        if input_tokens.is_none()
            && output_tokens.is_none()
            && cache_read_input_tokens.is_none()
            && cache_creation_input_tokens.is_none()
        {
            return None;
        }
        Some(Self {
            input_tokens: input_tokens.unwrap_or(0),
            output_tokens: output_tokens.unwrap_or(0),
            cache_read_input_tokens: cache_read_input_tokens.unwrap_or(0),
            cache_creation_input_tokens: cache_creation_input_tokens.unwrap_or(0),
        })
    }
}

fn json_u64(obj: &serde_json::Map<String, Value>, key: &str) -> Option<u64> {
    let value = obj.get(key)?;
    value
        .as_u64()
        .or_else(|| value.as_i64().and_then(|n| u64::try_from(n).ok()))
}

#[derive(Debug, Default)]
pub struct StreamJsonlState {
    texts: Vec<String>,
    tool_names: Vec<String>,
    result_text: String,
    usage: Option<CompleterUsage>,
    line: usize,
}

impl StreamJsonlState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Push one JSONL line. `Some(text)` is newly extracted assistant text
    /// (content-block `type=text` only — same sources as the buffered parser).
    pub fn push_line(&mut self, raw: &str) -> Result<Option<String>, CompleterParseError> {
        self.line += 1;
        let line = raw.trim();
        if line.is_empty() {
            return Ok(None);
        }
        let obj: Value =
            serde_json::from_str(line).map_err(|err| CompleterParseError::InvalidJson {
                line: self.line,
                message: err.to_string(),
            })?;
        let Some(obj) = obj.as_object() else {
            return Ok(None);
        };

        let mut line_tool_names = Vec::new();
        let mut new_text = String::new();
        for block in content_blocks(obj) {
            let Some(block) = block.as_object() else {
                continue;
            };
            match block.get("type").and_then(Value::as_str) {
                Some("tool_use") => {
                    let name = block
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown");
                    line_tool_names.push(name.to_string());
                }
                Some("text") => {
                    if let Some(t) = block.get("text").and_then(Value::as_str) {
                        if !t.is_empty() {
                            new_text.push_str(t);
                        }
                    }
                }
                _ => {}
            }
        }
        if !line_tool_names.is_empty() {
            self.tool_names.extend(line_tool_names);
            return Err(CompleterParseError::ToolUse {
                names: if self.tool_names.is_empty() {
                    "unknown".to_string()
                } else {
                    self.tool_names.join(", ")
                },
            });
        }
        if !new_text.is_empty() {
            self.texts.push(new_text.clone());
        }

        if obj.get("type").and_then(Value::as_str) == Some("result") {
            if let Some(r) = obj.get("result").and_then(Value::as_str) {
                self.result_text = r.to_string();
            }
            if let Some(usage) = obj.get("usage").and_then(CompleterUsage::from_result_usage) {
                self.usage = Some(usage);
            }
            if obj.get("is_error").and_then(Value::as_bool) == Some(true) {
                return Err(CompleterParseError::ResultError {
                    message: self.result_text.clone(),
                });
            }
        }

        if new_text.is_empty() {
            Ok(None)
        } else {
            Ok(Some(new_text))
        }
    }

    pub fn usage(&self) -> Option<CompleterUsage> {
        self.usage
    }

    /// Finish after stdout EOF. Empty assistant text fail-closes; `result`
    /// text is the fallback when no content-block text was seen.
    pub fn finish(self) -> Result<String, CompleterParseError> {
        if !self.tool_names.is_empty() {
            return Err(CompleterParseError::ToolUse {
                names: self.tool_names.join(", "),
            });
        }
        let mut out = self.texts.concat();
        out = out.trim().to_string();
        if out.is_empty() {
            out = self.result_text.trim().to_string();
        }
        if out.is_empty() {
            return Err(CompleterParseError::EmptyAssistantText);
        }
        Ok(out)
    }
}

/// Fail-closed parse of Claude Code stream-json JSONL → plain assistant text.
pub fn parse_stream_jsonl(text: &str) -> Result<String, CompleterParseError> {
    let mut state = StreamJsonlState::new();
    for line in text.lines() {
        let _ = state.push_line(line)?;
    }
    state.finish()
}

fn content_blocks<'a>(obj: &'a serde_json::Map<String, Value>) -> &'a [Value] {
    if let Some(msg) = obj.get("message").and_then(Value::as_object) {
        if let Some(content) = msg.get("content").and_then(Value::as_array) {
            return content.as_slice();
        }
    }
    if let Some(content) = obj.get("content").and_then(Value::as_array) {
        return content.as_slice();
    }
    &[]
}

/// Remove Anthropic/cloud override vars from an inherited environment map.
pub fn sanitize_child_env<K, V>(
    env: impl IntoIterator<Item = (K, V)>,
) -> HashMap<OsString, OsString>
where
    K: Into<OsString>,
    V: Into<OsString>,
{
    let strip: std::collections::HashSet<&str> = STRIPPED_ENV_VARS.iter().copied().collect();
    env.into_iter()
        .map(|(k, v)| (k.into(), v.into()))
        .filter(|(k, _)| k.to_str().map(|s| !strip.contains(s)).unwrap_or(true))
        .collect()
}

/// Argv for a text-only Claude Code print-mode completer (no `--bare`).
///
/// When `model` is `Some(non-empty)`, forwards `--model <id>` so Path A can
/// select among Claude full model IDs instead of the CLI seat default.
pub fn completer_argv(prompt: impl AsRef<OsStr>, model: Option<&str>) -> Vec<OsString> {
    let mut argv = vec![
        OsString::from("claude"),
        OsString::from("-p"),
        OsString::from("--output-format"),
        OsString::from("stream-json"),
        OsString::from("--verbose"),
        OsString::from("--tools"),
        OsString::from(""),
        OsString::from("--permission-mode"),
        OsString::from("dontAsk"),
        OsString::from("--no-session-persistence"),
        OsString::from("--system-prompt"),
        OsString::from("You are a text-only completer. Reply with plain text only."),
    ];
    if let Some(model) = model.map(str::trim).filter(|m| !m.is_empty()) {
        argv.push(OsString::from("--model"));
        argv.push(OsString::from(model));
    }
    argv.push(prompt.as_ref().to_os_string());
    argv
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    const ASSISTANT_OK: &str = include_str!("fixtures/assistant_ok.jsonl");
    const ASSISTANT_OK_WITH_USAGE: &str = include_str!("fixtures/assistant_ok_with_usage.jsonl");
    const TOOL_USE: &str = include_str!("fixtures/tool_use.jsonl");
    const EMPTY_RESULT: &str = include_str!("fixtures/empty_result.jsonl");

    #[test]
    fn parses_assistant_ok_fixture() {
        let text = parse_stream_jsonl(ASSISTANT_OK).expect("assistant_ok");
        assert_eq!(text, "pong");
    }

    #[test]
    fn incremental_parser_matches_buffered_oracle_on_fixtures() {
        for fixture in [
            ASSISTANT_OK,
            ASSISTANT_OK_WITH_USAGE,
            TOOL_USE,
            EMPTY_RESULT,
        ] {
            let buffered = parse_stream_jsonl(fixture);
            let mut state = StreamJsonlState::new();
            let incremental = (|| {
                for line in fixture.lines() {
                    let _ = state.push_line(line)?;
                }
                state.finish()
            })();
            assert_eq!(incremental, buffered, "fixture mismatch");
        }
        let jsonl =
            r#"{"type":"result","subtype":"success","result":"Not logged in","is_error":true}"#;
        let buffered = parse_stream_jsonl(jsonl);
        let mut state = StreamJsonlState::new();
        let incremental = state.push_line(jsonl).and_then(|_| state.finish());
        assert_eq!(incremental, buffered);
    }

    #[test]
    fn result_usage_fixture_maps_pinned_fields() {
        let mut state = StreamJsonlState::new();
        for line in ASSISTANT_OK_WITH_USAGE.lines() {
            let _ = state.push_line(line).expect("usage fixture line");
        }
        assert_eq!(
            parse_stream_jsonl(ASSISTANT_OK_WITH_USAGE).expect("text"),
            "pong"
        );
        assert_eq!(
            state.usage(),
            Some(CompleterUsage {
                input_tokens: 2,
                output_tokens: 4,
                cache_read_input_tokens: 0,
                cache_creation_input_tokens: 2774,
            })
        );
    }

    #[test]
    fn result_without_usage_object_reports_none() {
        let mut state = StreamJsonlState::new();
        for line in ASSISTANT_OK.lines() {
            let _ = state.push_line(line).expect("assistant_ok line");
        }
        assert_eq!(state.usage(), None);
    }

    #[test]
    fn usage_object_without_known_token_fields_is_ignored() {
        let mut state = StreamJsonlState::new();
        state
            .push_line(
                r#"{"type":"result","subtype":"success","result":"pong","is_error":false,"usage":{"service_tier":"standard"}}"#,
            )
            .expect("result");
        assert_eq!(state.usage(), None);
        assert_eq!(state.finish().expect("text"), "pong");
    }

    #[test]
    fn incremental_parser_yields_text_before_result_line() {
        let mut state = StreamJsonlState::new();
        let first = state
            .push_line(
                r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"po"}]}}"#,
            )
            .expect("assistant line");
        assert_eq!(first.as_deref(), Some("po"));
        let second = state
            .push_line(
                r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"ng"}]}}"#,
            )
            .expect("second assistant line");
        assert_eq!(second.as_deref(), Some("ng"));
        let result = state
            .push_line(r#"{"type":"result","subtype":"success","result":"pong","is_error":false}"#)
            .expect("result line");
        assert_eq!(result, None);
        assert_eq!(state.finish().expect("finish"), "pong");
    }

    #[test]
    fn rejects_tool_use_fixture() {
        let err = parse_stream_jsonl(TOOL_USE).expect_err("tool_use must fail closed");
        match err {
            CompleterParseError::ToolUse { names } => assert!(names.contains("Bash")),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn rejects_empty_result_fixture() {
        let err = parse_stream_jsonl(EMPTY_RESULT).expect_err("empty must fail closed");
        assert_eq!(err, CompleterParseError::EmptyAssistantText);
    }

    #[test]
    fn rejects_result_is_error() {
        let jsonl =
            r#"{"type":"result","subtype":"success","result":"Not logged in","is_error":true}"#;
        let err = parse_stream_jsonl(jsonl).expect_err("is_error");
        match err {
            CompleterParseError::ResultError { message } => {
                assert!(message.contains("Not logged in"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn sanitize_child_env_strips_anthropic_and_cloud_vars() {
        let inherited = [
            ("PATH", "/usr/bin"),
            ("ANTHROPIC_API_KEY", "sk-ant-api03-secret"),
            ("CLAUDE_CODE_OAUTH_TOKEN", "oat-secret"),
            ("AWS_BEARER_TOKEN_BEDROCK", "bedrock"),
            ("CLAUDE_CONFIG_DIR", "/tmp/claude-config"),
            ("HOME", "/tmp"),
        ];
        let cleaned = sanitize_child_env(inherited);
        assert_eq!(
            cleaned.get(OsStr::new("PATH")).map(OsString::as_os_str),
            Some(OsStr::new("/usr/bin"))
        );
        assert_eq!(
            cleaned
                .get(OsStr::new("CLAUDE_CONFIG_DIR"))
                .map(OsString::as_os_str),
            Some(OsStr::new("/tmp/claude-config"))
        );
        for key in STRIPPED_ENV_VARS {
            assert!(
                !cleaned.contains_key(OsStr::new(key)),
                "expected {key} stripped"
            );
        }
    }

    #[test]
    fn completer_argv_is_text_only_without_bare() {
        let argv = completer_argv("Reply with exactly: pong", None);
        let as_str: Vec<&str> = argv
            .iter()
            .map(|s| s.to_str().expect("utf8 argv"))
            .collect();
        assert_eq!(as_str[0], "claude");
        assert!(as_str.contains(&"-p"));
        assert!(as_str.contains(&"stream-json"));
        assert!(!as_str.iter().any(|a| *a == "--bare"));
        assert!(!as_str.iter().any(|a| *a == "--model"));
        let tools_idx = as_str
            .iter()
            .position(|a| *a == "--tools")
            .expect("--tools");
        assert_eq!(as_str[tools_idx + 1], "");
        assert_eq!(*as_str.last().unwrap(), "Reply with exactly: pong");
    }

    #[test]
    fn completer_argv_forwards_full_model_id() {
        let argv = completer_argv("Reply with exactly: pong", Some("claude-sonnet-5"));
        let as_str: Vec<&str> = argv
            .iter()
            .map(|s| s.to_str().expect("utf8 argv"))
            .collect();
        let model_idx = as_str
            .iter()
            .position(|a| *a == "--model")
            .expect("--model");
        assert_eq!(as_str[model_idx + 1], "claude-sonnet-5");
        assert_eq!(*as_str.last().unwrap(), "Reply with exactly: pong");
    }

    #[test]
    fn completer_argv_skips_blank_model() {
        let argv = completer_argv("hi", Some("   "));
        let as_str: Vec<&str> = argv
            .iter()
            .map(|s| s.to_str().expect("utf8 argv"))
            .collect();
        assert!(!as_str.iter().any(|a| *a == "--model"));
    }
}
