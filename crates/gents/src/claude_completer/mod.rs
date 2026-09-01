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

/// Fail-closed parse of Claude Code stream-json JSONL → plain assistant text.
pub fn parse_stream_jsonl(text: &str) -> Result<String, CompleterParseError> {
    let mut texts: Vec<String> = Vec::new();
    let mut saw_tool_use = false;
    let mut tool_names: Vec<String> = Vec::new();
    let mut result_text = String::new();

    for (lineno, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let obj: Value = serde_json::from_str(line).map_err(|err| CompleterParseError::InvalidJson {
            line: lineno + 1,
            message: err.to_string(),
        })?;
        let Some(obj) = obj.as_object() else {
            continue;
        };

        for block in content_blocks(obj) {
            let Some(block) = block.as_object() else {
                continue;
            };
            match block.get("type").and_then(Value::as_str) {
                Some("tool_use") => {
                    saw_tool_use = true;
                    let name = block
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown");
                    tool_names.push(name.to_string());
                }
                Some("text") => {
                    if let Some(t) = block.get("text").and_then(Value::as_str) {
                        if !t.is_empty() {
                            texts.push(t.to_string());
                        }
                    }
                }
                _ => {}
            }
        }

        if obj.get("type").and_then(Value::as_str) == Some("result") {
            if let Some(r) = obj.get("result").and_then(Value::as_str) {
                result_text = r.to_string();
            }
            if obj.get("is_error").and_then(Value::as_bool) == Some(true) {
                return Err(CompleterParseError::ResultError {
                    message: result_text,
                });
            }
        }
    }

    if saw_tool_use {
        let names = if tool_names.is_empty() {
            "unknown".to_string()
        } else {
            tool_names.join(", ")
        };
        return Err(CompleterParseError::ToolUse { names });
    }

    let mut out = texts.concat();
    out = out.trim().to_string();
    if out.is_empty() {
        out = result_text.trim().to_string();
    }
    if out.is_empty() {
        return Err(CompleterParseError::EmptyAssistantText);
    }
    Ok(out)
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
pub fn sanitize_child_env<K, V>(env: impl IntoIterator<Item = (K, V)>) -> HashMap<OsString, OsString>
where
    K: Into<OsString>,
    V: Into<OsString>,
{
    let strip: std::collections::HashSet<&str> = STRIPPED_ENV_VARS.iter().copied().collect();
    env.into_iter()
        .map(|(k, v)| (k.into(), v.into()))
        .filter(|(k, _)| {
            k.to_str()
                .map(|s| !strip.contains(s))
                .unwrap_or(true)
        })
        .collect()
}

/// Argv for a text-only Claude Code print-mode completer (no `--bare`).
///
/// When `model` is `Some(non-empty)`, forwards `--model <id>` so Path A can
/// select among Claude full model IDs instead of the CLI seat default.
pub fn completer_argv(
    prompt: impl AsRef<OsStr>,
    model: Option<&str>,
) -> Vec<OsString> {
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
    const TOOL_USE: &str = include_str!("fixtures/tool_use.jsonl");
    const EMPTY_RESULT: &str = include_str!("fixtures/empty_result.jsonl");

    #[test]
    fn parses_assistant_ok_fixture() {
        let text = parse_stream_jsonl(ASSISTANT_OK).expect("assistant_ok");
        assert_eq!(text, "pong");
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
        let jsonl = r#"{"type":"result","subtype":"success","result":"Not logged in","is_error":true}"#;
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
        let tools_idx = as_str.iter().position(|a| *a == "--tools").expect("--tools");
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
