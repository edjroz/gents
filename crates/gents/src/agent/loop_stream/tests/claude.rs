#[tokio::test]
async fn claude_fake_completer_tool_round_trip_through_owned_loop() {
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};

    use crate::claude_subscription::{
        ClaudeSeatConfig, ClaudeSubscriptionClient, install_process_seat,
        lock_process_seat_for_test,
    };
    use crate::rendered_request::scope::{ambient_arming_sink, scope_request, test_scope};
    use crate::rendered_request::{
        CaptureScopeKind, RenderedRequestCaptureSink, RenderedRequestContext,
    };
    use rig::client::CompletionClient;

    let _seat = lock_process_seat_for_test();
    let (node, hook) = test_hook().await;
    ready_hook_for(&hook).await;

    let temp = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.scratch/claude-spike/tmp")
        .join("loop-stream-claude-round-trip");
    let _ = std::fs::remove_dir_all(&temp);
    std::fs::create_dir_all(&temp).expect("tempdir");
    let echo_jsonl = temp.join("echo.jsonl");
    std::fs::write(
        &echo_jsonl,
        include_str!("../../../claude_completer/fixtures/tool_use_echo.jsonl"),
    )
    .expect("write echo fixture");
    let fake = temp.join("fake-completer.sh");
    {
        let mut file = std::fs::File::create(&fake).expect("create fake");
        writeln!(file, "#!/bin/sh").unwrap();
        writeln!(
            file,
            r#"if printf '%s' "$1" | grep -q 'tool_result'; then
printf '%s\n' '{{"type":"assistant","message":{{"role":"assistant","content":[{{"type":"text","text":"done"}}]}}}}'
printf '%s\n' '{{"type":"result","subtype":"success","result":"done","is_error":false}}'
else
exec cat '{}'
fi"#,
            echo_jsonl.display()
        )
        .unwrap();
    }
    let mut perms = std::fs::metadata(&fake).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&fake, perms).unwrap();
    install_process_seat(Some(ClaudeSeatConfig {
        config_dir: temp.join("claude-config"),
        write_approved: false,
        workdir: temp.join("workdir"),
        log_dir: None,
        claude_bin: PathBuf::from("claude"),
        fake_completer: Some(fake),
    }));
    std::fs::create_dir_all(temp.join("workdir")).unwrap();

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
        let stream = run_loop_stream(
            model,
            Some(hook),
            Message::user("use the echo tool"),
            Vec::new(),
            tools,
            loop_config,
        );
        futures::pin_mut!(stream);
        let mut tool_results = Vec::new();
        let mut final_text = None;
        while let Some(item) = stream.next().await {
            match item.expect("loop item should be Ok") {
                LoopStreamItem::Item(MultiTurnStreamItem::StreamUserItem(
                    StreamedUserContent::ToolResult { tool_result, .. },
                )) => {
                    tool_results.push(
                        tool_result_text(&crate::llm::rig_compat::from_rig_tool_result_content(
                            &tool_result.content.first(),
                        ))
                        .to_string(),
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

    let resp = node
        .execute("query { AgentToolCall { tool_name lifecycle_state result } }")
        .await;
    assert!(
        !resp.has_errors(),
        "AgentToolCall query failed: {:?}",
        resp.errors
    );
    let rows = resp
        .data
        .as_ref()
        .and_then(|data| data.get("AgentToolCall"))
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        rows.iter().any(|row| {
            row.get("tool_name").and_then(|value| value.as_str()) == Some("echo")
                && row.get("lifecycle_state").and_then(|value| value.as_str()) == Some("completed")
                && row
                    .get("result")
                    .and_then(|value| value.as_str())
                    .is_some_and(|result| result.contains("ECHOED"))
        }),
        "expected a completed echo AgentToolCall; rows: {rows:?}"
    );
}
