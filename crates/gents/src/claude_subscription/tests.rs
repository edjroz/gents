use rig::completion::ToolDefinition;

use super::*;
use crate::llm::message::Message;

fn request_from_native(
    preamble: Option<&str>,
    history: Vec<Message>,
    tools: Vec<ToolDefinition>,
) -> CompletionRequest {
    let rig_history = crate::llm::rig_compat::to_rig_messages(&history);
    CompletionRequest {
        model: None,
        preamble: preamble.map(str::to_string),
        chat_history: OneOrMany::many(rig_history).expect("at least one row"),
        documents: Vec::new(),
        tools,
        temperature: None,
        max_tokens: Some(128),
        tool_choice: None,
        additional_params: None,
        output_schema: None,
    }
}

/// Text-only request: empty surface, no preamble.
fn ping_request() -> CompletionRequest {
    request_from_native(None, vec![Message::user("ping")], Vec::new())
}

fn echo_tool_request() -> CompletionRequest {
    request_from_native(
        None,
        vec![Message::user("use echo")],
        vec![ToolDefinition {
            name: "echo".into(),
            description: "echo".into(),
            parameters: serde_json::json!({"type":"object","properties":{}}),
        }],
    )
}

#[tokio::test]
async fn live_path_refuses_without_a_readable_seat() {
    let _guard = lock_process_seat_for_test();
    let _seat = install_fake_seat();
    let model = ClaudeSubscriptionClient::new().completion_model("claude-sonnet-5");
    let err = model.completion(ping_request()).await.expect_err("refused");
    assert_seat_auth_refusal(&err.to_string());
}

/// The fake seat's tempdir holds no credentials (and on macOS the Keychain
/// has no item for a fresh tempdir digest), so the live path must refuse at
/// the token read with the `SeatAuthError` message.
fn assert_seat_auth_refusal(message: &str) {
    assert!(message.contains("Claude Messages seat auth:"), "{message}");
    assert!(message.contains("credentials file missing"), "{message}");
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
    use rig::streaming::StreamedAssistantContent;
    let mut calls = Vec::new();
    let mut usage = None;
    while let Some(item) = stream.next().await {
        match item.expect("chunk") {
            StreamedAssistantContent::ToolCall { tool_call, .. } => {
                calls.push((
                    tool_call.id,
                    tool_call.function.name,
                    tool_call.function.arguments,
                ));
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
        vec![(
            "toolu_1".to_string(),
            "echo".to_string(),
            serde_json::json!({"text": "hi"})
        )]
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
    model
        .completion(ping_request())
        .await
        .expect("first call served");
    let err = model
        .completion(ping_request())
        .await
        .expect_err("queue drained → live token read");
    assert_seat_auth_refusal(&err.to_string());
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

/// `claude-login` prints this after a live login: the wire's own token read,
/// never the token.
#[test]
fn probe_seat_detail_reports_missing_seat_with_login_hint() {
    let temp = tempfile::tempdir().unwrap();
    let value = probe_seat_detail(temp.path());
    assert_eq!(value["ok"], false, "{value}");
    let detail = value["detail"].as_str().unwrap();
    assert!(
        detail.contains("gents claude-login --config-dir"),
        "{detail}"
    );
    assert!(
        detail.contains(&temp.path().display().to_string()),
        "{detail}"
    );
}
