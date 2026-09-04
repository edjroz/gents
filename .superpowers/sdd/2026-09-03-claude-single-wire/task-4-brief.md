### Task 4: Route every turn over Messages HTTP, omit `tools` when empty → live #9

**Files:**
- Modify: `crates/gents/src/claude_subscription.rs:253-269` (`ClaudeSubscriptionModel::stream`)
- Modify: `crates/gents/src/claude_messages.rs:74-111` (`build_messages_body`)
- Test: `crates/gents/src/claude_messages.rs` (`mod tests`)
- Create: `.scratch/claude-spike/logs/write-request-9.md`, `b3-live-http-text-*` evidence

**Interfaces:**
- Consumes: `stream_messages(model, &request, surface)` (existing).
- Produces: `build_messages_body` never emits a `tools` key for an empty surface (Lean `toolsField`, Task 5 witness `emptyTools`). Routing predicate is `seat.fake_completer.is_none()` only; the fake-completer branch survives until Task 6 deletes it.

- [ ] **Step 1: Write the failing test**

Append inside `mod tests` in `crates/gents/src/claude_messages.rs`:

```rust
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
```

- [ ] **Step 2: Run it to verify it fails**

```bash
cargo test -p gents --lib claude_messages::tests::messages_body_omits_tools_key_when_surface_is_empty
```
Expected: FAIL (`tools` is `[]`).

- [ ] **Step 3: Implement**

In `build_messages_body`, replace the `let body = json!({ ... })` block with:

```rust
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
```

In `ClaudeSubscriptionModel::stream` (`claude_subscription.rs:259`), change

```rust
        if !request.tools.is_empty() && seat.fake_completer.is_none() {
```
to
```rust
        if seat.fake_completer.is_none() {
```

- [ ] **Step 4: Run the tests**

```bash
cargo test -p gents --lib claude_ 2>&1 | tail -15
```
Expected: all green. `live_path_refuses_without_write_approval` now exercises the Messages refusal (`live Claude path refused: pass --claude-write-approved …`), which still contains `--claude-write-approved`.

- [ ] **Step 5: Gates and commit**

```bash
cargo test -p gents 2>&1 | tail -20
cargo check --workspace --all-targets 2>&1 | tail -5
git add crates/gents/src/claude_subscription.rs crates/gents/src/claude_messages.rs
git commit -m "feat(claude): route every turn over Messages HTTP; omit tools when empty

Write request #7 showed the OAuth 429 was identity routing, not a tool-turn
property. Text-only turns (and title generation) now take the same wire as
tool turns. tools is omitted rather than sent as []."
```

- [ ] **Step 6: Write request #9 and wait for approval**

`.scratch/claude-spike/logs/write-request-9.md`, same shape as #8. Hypothesis: a text-only turn (empty surface, no `tools` key) and the title-generation call both succeed over Messages HTTP with the identity block. Scope: server flags as in Task 2, one `gents chat` turn, fresh session, prompt `Reply with exactly: pong`. Success bar:
- response `pong`, `status=complete`;
- `RenderedRequest` rows for both `inference.1` and `title.1` have `source = claude_cli_subscription` and `capture_seam = transport_body` (no `process_cli` row);
- the `title.1` captured `request_json` has no `tools` key and no `temperature`;
- no 4xx/429 in `b3-live-http-text-server.stderr.log`;
- `OAuthCredential` Claude rows 0, token scan clean, default model unchanged.
`stop_reason` is not persisted by gents; `status=complete` stands in for `end_turn` (deviation from spec §6, recorded in the evidence).

Ask: "Write request #9 is ready. Approve #9?" Wait for an explicit approval naming #9.

- [ ] **Step 7: Run it**

Same commands as Task 2 steps 2, 4, 5 with the `b3-live-http-text-` prefix and the `pong` prompt. Then:

```bash
L=.scratch/claude-spike/logs; REQ=<request id>
./target/debug/gents query --home ~/.gents --collection RenderedRequest \
  --field capture_scope --field source --field provenance_json --field request_json \
  --filter "{\"request_id\":{\"_eq\":\"$REQ\"}}" > $L/b3-live-http-text-captures.json
jq '.results[] | {capture_scope, source, seam: (.provenance_json | fromjson | .capture_seam), has_tools: ((.request_json | fromjson) | has("tools"))}' $L/b3-live-http-text-captures.json
kill "$(cat $L/b3-live-http-text-server.pid)"
grep -l 'sk-ant\|Bearer ' $L/b3-live-http-text-* ; echo "token scan exit=$?"
```
Expected: two rows, both `transport_body`, `has_tools: false`.

- [ ] **Step 8: Evidence**

Write `.scratch/claude-spike/logs/b3-live-http-text-evidence.md` (PASS/FAIL table as in Task 2). **If FAIL, the cut stops here** (spec §6): report to the user and do not start Task 5 without a decision.

