//! Loopback OpenAI Chat Completions adapter helpers for Claude Path A.
//!
//! Pure request shaping + mode/gate logic. The HTTP server lives in
//! `gents-cli` (`gents claude-proxy`) so this crate does not take an axum
//! runtime dependency for an experimental operator path.

use serde_json::{json, Value};
use uuid::Uuid;

use super::STRIPPED_ENV_VARS;

/// Default client-facing model slug for the Path A adapter.
pub const DEFAULT_MODEL_ID: &str = "claude-plan";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyMode {
    /// No Claude; return canned text.
    Canned,
    /// Live wiring with a fake completer script (tests).
    ClaudeFake,
    /// Live Claude CLI completer (requires write approval).
    Claude,
}

impl ProxyMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Canned => "canned",
            Self::ClaudeFake => "claude-fake",
            Self::Claude => "claude",
        }
    }
}

/// Resolve adapter mode from the double gate + optional fake completer.
pub fn resolve_mode(use_claude: bool, fake_completer: bool) -> ProxyMode {
    if !use_claude {
        return ProxyMode::Canned;
    }
    if fake_completer {
        ProxyMode::ClaudeFake
    } else {
        ProxyMode::Claude
    }
}

/// Live Claude path requires `CLAUDE_WRITE_APPROVED=1`.
pub fn live_claude_allowed(write_approved: bool) -> bool {
    write_approved
}

/// Refuse non-loopback binds.
pub fn is_loopback_host(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "localhost" | "::1")
}

/// Flatten OpenAI-style chat messages into a text prompt for the CLI completer.
pub fn flatten_messages(messages: &[Value]) -> String {
    let mut parts: Vec<String> = Vec::new();
    for msg in messages {
        let Some(obj) = msg.as_object() else {
            continue;
        };
        let role = obj
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("user");
        let content = match obj.get("content") {
            Some(Value::String(s)) => s.clone(),
            Some(Value::Array(blocks)) => {
                let mut texts = String::new();
                for block in blocks {
                    match block {
                        Value::String(s) => texts.push_str(s),
                        Value::Object(map) => {
                            if map.get("type").and_then(Value::as_str) == Some("text") {
                                if let Some(t) = map.get("text").and_then(Value::as_str) {
                                    texts.push_str(t);
                                }
                            }
                        }
                        _ => {}
                    }
                }
                texts
            }
            Some(other) => other.to_string(),
            None => String::new(),
        };
        if role == "system" && content.trim().is_empty() {
            continue;
        }
        parts.push(format!("{role}: {content}"));
    }
    parts.join("\n").trim().to_string()
}

pub fn completion_id() -> String {
    format!("chatcmpl-claude-{}", &Uuid::new_v4().simple().to_string()[..12])
}

pub fn non_stream_body(text: &str, model: &str) -> Value {
    json!({
        "id": completion_id(),
        "object": "chat.completion",
        "created": chrono_like_unix_now(),
        "model": model,
        "choices": [{
            "index": 0,
            "message": {"role": "assistant", "content": text},
            "finish_reason": "stop",
        }],
        "usage": {
            "prompt_tokens": 0,
            "completion_tokens": 0,
            "total_tokens": 0,
        },
    })
}

/// SSE payload for a single-shot assistant completion (role+content, stop, DONE).
pub fn sse_payload(text: &str, model: &str) -> String {
    let cid = completion_id();
    let created = chrono_like_unix_now();
    let first = json!({
        "id": cid,
        "object": "chat.completion.chunk",
        "created": created,
        "model": model,
        "choices": [{
            "index": 0,
            "delta": {"role": "assistant", "content": text},
            "finish_reason": Value::Null,
        }],
    });
    let last = json!({
        "id": cid,
        "object": "chat.completion.chunk",
        "created": created,
        "model": model,
        "choices": [{
            "index": 0,
            "delta": {},
            "finish_reason": "stop",
        }],
        "usage": {
            "prompt_tokens": 0,
            "completion_tokens": 0,
            "total_tokens": 0,
        },
    });
    format!(
        "data: {}\n\ndata: {}\n\ndata: [DONE]\n\n",
        first, last
    )
}

pub fn models_list_body(model: &str) -> Value {
    json!({
        "object": "list",
        "data": [{
            "id": model,
            "object": "model",
            "created": chrono_like_unix_now(),
            "owned_by": "claude-path-a",
        }],
    })
}

pub fn health_body(mode: ProxyMode, model: &str, write_approved: bool, fake: bool) -> Value {
    json!({
        "ok": true,
        "mode": mode.as_str(),
        "model": model,
        "write_approved": write_approved,
        "fake_completer": fake,
    })
}

/// Env keys the proxy must strip before spawning a live completer child.
pub fn stripped_env_vars() -> &'static [&'static str] {
    STRIPPED_ENV_VARS
}

fn chrono_like_unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn resolve_mode_defaults_to_canned() {
        assert_eq!(resolve_mode(false, false), ProxyMode::Canned);
        assert_eq!(resolve_mode(false, true), ProxyMode::Canned);
        assert_eq!(resolve_mode(true, true), ProxyMode::ClaudeFake);
        assert_eq!(resolve_mode(true, false), ProxyMode::Claude);
    }

    #[test]
    fn live_claude_requires_write_approval() {
        assert!(!live_claude_allowed(false));
        assert!(live_claude_allowed(true));
    }

    #[test]
    fn loopback_hosts_only() {
        assert!(is_loopback_host("127.0.0.1"));
        assert!(is_loopback_host("localhost"));
        assert!(is_loopback_host("::1"));
        assert!(!is_loopback_host("0.0.0.0"));
        assert!(!is_loopback_host("192.168.1.1"));
    }

    #[test]
    fn flatten_messages_keeps_roles_and_text_blocks() {
        let messages = vec![
            json!({"role": "system", "content": ""}),
            json!({"role": "user", "content": "hi"}),
            json!({"role": "assistant", "content": [{"type": "text", "text": "yo"}]}),
        ];
        let prompt = flatten_messages(&messages);
        assert_eq!(prompt, "user: hi\nassistant: yo");
    }

    #[test]
    fn sse_payload_ends_with_done_and_includes_content() {
        let payload = sse_payload("pong", DEFAULT_MODEL_ID);
        assert!(payload.contains("\"content\":\"pong\""));
        assert!(payload.contains("data: [DONE]"));
        assert!(payload.contains("chat.completion.chunk"));
    }

    #[test]
    fn non_stream_body_is_chat_completion() {
        let body = non_stream_body("pong", DEFAULT_MODEL_ID);
        assert_eq!(body["object"], "chat.completion");
        assert_eq!(body["choices"][0]["message"]["content"], "pong");
        assert_eq!(body["model"], DEFAULT_MODEL_ID);
    }

    #[test]
    fn stripped_env_includes_anthropic_api_key() {
        assert!(stripped_env_vars().contains(&"ANTHROPIC_API_KEY"));
        assert!(stripped_env_vars().contains(&"CLAUDE_CODE_OAUTH_TOKEN"));
    }
}
