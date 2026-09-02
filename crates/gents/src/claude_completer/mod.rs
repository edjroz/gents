//! Claude Max subscription completer adapter (Path A).
//!
//! Parses Claude Code `--output-format stream-json` JSONL into assistant text
//! and mapped gents `tool_use` events, and builds a sanitized child-process
//! environment / argv for the CLI. Live argv stays `--tools ""`.
//!
//! A2b uses this from the in-process `ClaudeCliSubscription` Completer. The
//! transitional HTTP `claude-proxy` adapter was deleted in A2b-3.

use std::collections::{HashMap, HashSet};
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

/// Read `loggedIn` from `claude auth status --json` stdout (read-only probe).
///
/// Tolerates leading/trailing noise around the JSON object, matching the CLI
/// `claude-auth-probe` parser. Missing `loggedIn` is an error, not `false`.
pub fn parse_auth_status_logged_in(text: &str) -> Result<bool, String> {
    let start = text
        .find('{')
        .ok_or_else(|| "no JSON object in auth status output".to_string())?;
    let end = text
        .rfind('}')
        .ok_or_else(|| "unterminated JSON object in auth status output".to_string())?;
    let value: Value = serde_json::from_str(&text[start..=end])
        .map_err(|error| format!("decode Claude auth status JSON: {error}"))?;
    value
        .get("loggedIn")
        .and_then(Value::as_bool)
        .ok_or_else(|| "loggedIn missing from auth status JSON".to_string())
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
    #[error("fail-closed: duplicate tool_use id {id}")]
    DuplicateToolUseId { id: String },
    #[error("fail-closed: malformed tool_use at line {line}: {message}")]
    MalformedToolUse { line: usize, message: String },
    #[error("claude result is_error=true: {message}")]
    ResultError { message: String },
    #[error("fail-closed: empty assistant text / missing result")]
    EmptyAssistantText,
}

/// Incremental event from one stream-json line.
#[derive(Debug, Clone, PartialEq)]
pub enum StreamJsonlEvent {
    Text(String),
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
}

/// Incremental fail-closed parser for Claude Code `--output-format stream-json`.
///
/// One JSONL line at a time so the Completer can yield assistant text (and
/// mapped `tool_use`) before the child process exits. `parse_stream_jsonl` is
/// the buffered text oracle over the same state machine with an empty surface.
/// Names on this turn's gents surface map; empty surface, Claude-native, and
/// unknown names still fail closed (`Bash` is not `bash`).
///
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
    /// Gents names advertised this turn. Empty = A2b text-only fence.
    surface: HashSet<String>,
    mapped: Vec<StreamJsonlEvent>,
    seen_ids: HashSet<String>,
    unmapped_names: Vec<String>,
    result_text: String,
    usage: Option<CompleterUsage>,
    line: usize,
}

impl StreamJsonlState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Restrict mapped `tool_use` to these gents names. Empty surface fail-closes
    /// on any `tool_use` (A2b). No aliases: `Bash` is not `bash`.
    pub fn with_surface(names: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            surface: names.into_iter().map(Into::into).collect(),
            ..Self::default()
        }
    }

    /// Push one JSONL line. Text and mapped `tool_use` become events; unmapped
    /// / empty-surface / duplicate / malformed `tool_use` fail closed.
    pub fn push_line(&mut self, raw: &str) -> Result<Vec<StreamJsonlEvent>, CompleterParseError> {
        self.line += 1;
        let line = raw.trim();
        if line.is_empty() {
            return Ok(Vec::new());
        }
        let obj: Value =
            serde_json::from_str(line).map_err(|err| CompleterParseError::InvalidJson {
                line: self.line,
                message: err.to_string(),
            })?;
        let Some(obj) = obj.as_object() else {
            return Ok(Vec::new());
        };

        let mut events = Vec::new();
        let mut line_mapped = Vec::new();
        let mut line_unmapped = Vec::new();
        let mut line_ids = HashSet::new();
        let mut new_text = String::new();
        for block in content_blocks(obj) {
            let Some(block) = block.as_object() else {
                continue;
            };
            match block.get("type").and_then(Value::as_str) {
                Some("tool_use") => {
                    let id = block
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    let name = block
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    if id.is_empty() {
                        return Err(CompleterParseError::MalformedToolUse {
                            line: self.line,
                            message: "missing id".to_string(),
                        });
                    }
                    if name.is_empty() {
                        return Err(CompleterParseError::MalformedToolUse {
                            line: self.line,
                            message: "missing name".to_string(),
                        });
                    }
                    if self.seen_ids.contains(&id) || !line_ids.insert(id.clone()) {
                        return Err(CompleterParseError::DuplicateToolUseId { id });
                    }
                    let input = match block.get("input") {
                        None => Value::Object(serde_json::Map::new()),
                        Some(value) if value.is_object() => value.clone(),
                        Some(_) => Value::Object(serde_json::Map::new()),
                    };
                    if self.surface.is_empty() || !self.surface.contains(&name) {
                        line_unmapped.push(name);
                    } else {
                        line_mapped.push(StreamJsonlEvent::ToolUse { id, name, input });
                    }
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
        if !line_unmapped.is_empty() {
            self.unmapped_names.extend(line_unmapped);
            return Err(CompleterParseError::ToolUse {
                names: self.unmapped_names.join(", "),
            });
        }
        if !new_text.is_empty() {
            self.texts.push(new_text.clone());
            events.push(StreamJsonlEvent::Text(new_text));
        }
        for event in line_mapped {
            if let StreamJsonlEvent::ToolUse { id, .. } = &event {
                self.seen_ids.insert(id.clone());
            }
            self.mapped.push(event.clone());
            events.push(event);
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

        Ok(events)
    }

    pub fn usage(&self) -> Option<CompleterUsage> {
        self.usage
    }

    /// Finish after stdout EOF. Empty assistant text fail-closes unless this
    /// turn mapped at least one gents `tool_use`. `result` text is the fallback
    /// when no content-block text was seen.
    pub fn finish(self) -> Result<String, CompleterParseError> {
        if !self.unmapped_names.is_empty() {
            return Err(CompleterParseError::ToolUse {
                names: self.unmapped_names.join(", "),
            });
        }
        let mut out = self.texts.concat();
        out = out.trim().to_string();
        if out.is_empty() {
            out = self.result_text.trim().to_string();
        }
        if out.is_empty() && self.mapped.is_empty() {
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
        assert_eq!(first, vec![StreamJsonlEvent::Text("po".into())]);
        let second = state
            .push_line(
                r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"ng"}]}}"#,
            )
            .expect("second assistant line");
        assert_eq!(second, vec![StreamJsonlEvent::Text("ng".into())]);
        let result = state
            .push_line(r#"{"type":"result","subtype":"success","result":"pong","is_error":false}"#)
            .expect("result line");
        assert!(result.is_empty());
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
    fn maps_gents_named_tool_use_on_surface() {
        let mut state = StreamJsonlState::with_surface(["echo"]);
        let events = state
            .push_line(
                r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"toolu_1","name":"echo","input":{}}]}}"#,
            )
            .expect("mapped tool_use");
        assert_eq!(
            events,
            vec![StreamJsonlEvent::ToolUse {
                id: "toolu_1".into(),
                name: "echo".into(),
                input: serde_json::json!({}),
            }]
        );
        assert_eq!(state.finish().expect("mapped turn may have empty text"), "");
    }

    #[test]
    fn bash_is_not_bash_on_gents_surface() {
        let mut state = StreamJsonlState::with_surface(["bash"]);
        let err = state
            .push_line(TOOL_USE.lines().next().expect("assistant line"))
            .expect_err("Bash is not bash");
        match err {
            CompleterParseError::ToolUse { names } => assert!(names.contains("Bash")),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn empty_surface_still_fail_closes_gents_named_tool_use() {
        let mut state = StreamJsonlState::new();
        let err = state
            .push_line(
                r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"toolu_1","name":"echo","input":{}}]}}"#,
            )
            .expect_err("empty surface");
        match err {
            CompleterParseError::ToolUse { names } => assert!(names.contains("echo")),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn duplicate_tool_use_id_fail_closes() {
        let mut state = StreamJsonlState::with_surface(["echo"]);
        state
            .push_line(
                r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"toolu_1","name":"echo","input":{}}]}}"#,
            )
            .expect("first");
        let err = state
            .push_line(
                r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"toolu_1","name":"echo","input":{}}]}}"#,
            )
            .expect_err("duplicate");
        match err {
            CompleterParseError::DuplicateToolUseId { id } => assert_eq!(id, "toolu_1"),
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
    fn parse_auth_status_logged_in_reads_logged_in() {
        assert!(
            parse_auth_status_logged_in(r#"{"loggedIn":true,"authMethod":"claude.ai"}"#)
                .expect("parse")
        );
        assert!(!parse_auth_status_logged_in(r#"{"loggedIn":false}"#).expect("parse"));
        assert!(parse_auth_status_logged_in("note\n{\"loggedIn\":true}\n").expect("noise"));
        assert!(parse_auth_status_logged_in("no json").is_err());
        assert!(parse_auth_status_logged_in(r#"{"authMethod":"claude.ai"}"#).is_err());
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
