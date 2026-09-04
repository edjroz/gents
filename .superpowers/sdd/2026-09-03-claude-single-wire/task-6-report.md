# Task 6 report — single-wire cut (body, incremental streaming, delete the process-CLI wire)

Branch `spike/claude-b3-live-tools`, base `ad97dd5a`. Sub-steps 6.1–6.7 plus the
6.9/6.10 rulings applied inline. 6.8 (live #10) deliberately not run.

## Per sub-step

### 6.1 / 6.2 / 6.9 — `crates/gents/src/claude_messages.rs` (rewritten) + `claude_messages/tests.rs` (new)
- Module doc rewritten to the single-wire statement.
- `MessagesParseError` (moved here from `claude_completer::CompleterParseError`), Display strings without
  `at line N`; `From<MessagesParseError> for CompletionError`.
- Fixture queue: `install_messages_sse_fixtures(Vec<String>)` (FIFO, one per `stream_messages` call) +
  private `take_messages_sse_fixture`. Old single-slot `install_messages_sse_fixture` / `messages_sse_fixture`
  deleted.
- Body: `build_messages_body(model, &CompletionRequest)` is a thin wrapper that maps history through
  `rig_compat::from_rig_message` and delegates to the new
  `build_messages_body_native(model, preamble, max_tokens, &[Message], &[ToolDefinition])`.
  `system_rows` / `anthropic_messages` / `mark_ephemeral` operate on `crate::llm::message::*`
  (native family). `Message::System` rows are lifted into `system[]` behind identity + preamble;
  two `cache_control: ephemeral` breakpoints (last system block, last content block of last message);
  `tools` omitted when empty. `MESSAGES_BODY_ALLOWED_KEYS` + `messages_body_has_only_allowed_keys`
  + the `debug_assert!` deleted (allowed-key check now lives in the test helper).
- Parser: `MessagesSseState::{new, with_request_id, push_line, finish}` incremental line parser;
  `parse_messages_sse` is the all-lines wrapper; `sse_data_payloads` deleted. SSE `error` events
  become `CompletionError::ProviderError("Claude Messages stream error {type}: {message} (request-id …)")`.
- Transport: `stream_sse_body(BoxedStream, MessagesSseState)` line-splits the body incrementally
  (`async_stream`); `stream_messages` builds the request, reads the seat token only on the live path,
  wraps `SeatTransport { fixture, live: seat.http.clone() }` in `RenderedRequestCapturingHttpClient`,
  surfaces `request-id` on non-2xx, and returns the incremental stream. `ClaudeMessagesTransport`
  renamed to `SeatTransport` (`sse_fixture` → `fixture`), `HttpClientExt` bodies unchanged.
- Test helpers (`#[cfg(test)] pub(crate)`): `sse_fixture_text`, `sse_fixture_tool_use`,
  `sse_fixture_final_text`.
- Tests moved to `claude_messages/tests.rs` (`#[path]` mod). Requests built through
  `request_from_native(preamble, Vec<native Message>, tools)`; no rig message vocabulary in the file.
  Kept every Task 1/4 test; deleted `claude_code_identity_stays_off_the_process_cli_wire`; added the
  three body tests, the four streaming tests, and `sse_error_event_becomes_provider_error`.

### 6.3 / 6.9 — `crates/gents/src/claude_subscription.rs` (rewritten) + `claude_subscription/tests.rs` (new)
- `ClaudeSeatConfig { config_dir, write_approved, claude_bin, http: ReqwestClient }` with
  `ClaudeSeatConfig::new(config_dir, write_approved, claude_bin: Option<PathBuf>)`;
  `from_server_flags`, `workdir`, `log_dir`, `fake_completer` gone.
- `#[cfg(test)] install_fake_seat() -> tempfile::TempDir` (refuse-closed seat under a fresh tempdir).
- `lock_process_seat_for_test` now clears the fixture queue (`install_messages_sse_fixtures(Vec::new())`).
- `completion()` calls `claude_messages::stream_messages` directly and folds `RawStreamingChoice`s
  (no `.stream(` / `.completion(` token in the production file); `stream()` delegates unconditionally.
- Deleted: `spawn_completer`, `stream_child_stdout`, `capture_claude_cli_request`,
  `flatten_completion_request`, `rig_usage_from_completer`, and their imports
  (`AsyncBufReadExt`, `BufReader`, `warn`, `StreamJsonl*`, `completer_argv`, `CompleterUsage`).
  `Command`/`Stdio` stay for the auth-status probe (Task 7).
- Tests: kept `parse_aliases_round_trip`; `ping_request` / `echo_tool_request` via `request_from_native`;
  the four brief tests (`live_path_refuses_without_write_approval`,
  `messages_http_fixture_maps_gents_tool_use_with_arguments`,
  `messages_http_fixture_streams_text_turn_on_empty_surface`,
  `fixture_queue_serves_one_body_per_call_then_refuses`). All 14 listed fake-completer / flatten /
  process-CLI tests and the old `install_fake_seat` / `echo_tool_use_sse` / `workspace_tempdir` /
  `write_fake_completer` / `write_delayed_jsonl_fake` helpers deleted.

### 6.4 — `crates/gents/src/claude_completer/mod.rs` (trimmed) + `fixtures/` (git rm, 5 files)
- Keeps `DEFAULT_MODEL_ID`, `parse_auth_status_logged_in`, `STRIPPED_ENV_VARS`, `sanitize_child_env`
  and their two tests. Deleted `PATH_A_MODEL_IDS`, `live_claude_allowed`, `CompleterParseError`,
  `StreamJsonlEvent`, `CompleterUsage`, `StreamJsonlState`, `parse_stream_jsonl`, `content_blocks`,
  `json_u64`, `completer_argv`, all JSONL tests, and the `thiserror` import.

### 6.5 — rendered-request seam back to `main`'s shape
- `crates/gents-protocol/src/rendered_request.rs`: `CaptureSeam` is the single `TransportBody` variant;
  `captured_only_at` folded into `captured_only` (hardcodes `TransportBody`); test
  `process_cli_seam_round_trips_in_the_manifest` deleted; test
  `claude_cli_subscription_classifies_messages_http_and_not_process_cli_urls` renamed to
  `..._classifies_messages_http_urls_only` (body unchanged) so the residue scan is clean.
- `crates/gents/src/rendered_request/mod.rs`: `build_rendered_completion_request_at_seam` →
  `build_rendered_completion_request` (no `capture_seam` param); test `build` helper updated;
  `process_cli_seam_is_recorded_positively` deleted.
- `crates/gents/src/rendered_request/scope.rs`: `claim_and_capture_process_cli` deleted;
  `capture_request_json` drops the seam argument; the four `process_cli_*` tests deleted
  (`arming_without_a_scope_is_a_noop` kept).
- `crates/gents/src/config_client/inference_backend.rs`: `git checkout main --` (spike's
  `write_inference_backend_document_with_clear_fields` had no external callers).

### 6.6 — owned-loop test, health helper, CLI flags
- `crates/gents/src/agent/loop_stream/tests/claude.rs`: rewritten as
  `claude_messages_tool_round_trip_through_owned_loop` on two SSE fixtures; asserts
  `AgentToolCall.args == {"text":"hi"}` (PASS, see below).
- `crates/gents/src/backend_health.rs`: `install_claude_seat` uses `ClaudeSeatConfig::new`.
- `crates/gents-cli/src/cli/args.rs`: `--claude-workdir`, `--claude-log-dir`, `--claude-fake-completer`
  removed; `--claude-bin` and `--claude-write-approved` gain `requires = "claude_config_dir"`;
  write-gate help ends at "…before setting it."
- `crates/gents-cli/src/cli/args/tests.rs`: `server_parses_a2b_claude_seat_flags` covers
  `--claude-config-dir --claude-write-approved --claude-bin`.
- `crates/gents-cli/src/commands/serve.rs`: `validate_claude_seat_args` and its call deleted;
  `install_claude_subscription_seat` per the brief; status JSON drops `fake_completer`; the three seat
  tests rewritten (orphan-flag test is now a clap parse-error test; tempdirs via `tempfile::tempdir()`,
  no `.scratch` paths).

### 6.10 — `crates/gents/tests/conformance/prompt_assembly.rs`
- `assert_fail_closed(case_name, outcome, parsed)` shared by the map and stream drivers (each keeps
  its own `"ok"` arm).
- Body driver calls `build_messages_body_native("claude-sonnet-5", case.preamble.as_deref(), None,
  &history, &tools)` with native `gents::llm::message::Message` rows; no rig message types in the file.
  Imports: `build_messages_body_native`, `gents::claude_subscription::ClaudeStreamResponse`,
  `rig::completion::CompletionError`.
- `tests/support/conformance_consumers.rs` untouched (no driver renamed).

### Other
- `crates/gents/proofs/Proofs/RenderedCapture.lean` lines 8–12: doc-comment only — the sentence
  referencing `CaptureSeam::ProcessCli` / `claim_and_capture_process_cli` replaced with the single-seam
  statement. No definitions/theorems touched; `cargo test` rebuilt the Lean contract snapshot
  successfully as part of the gate.

## Deviations from the brief
1. **`build_messages_body` is a wrapper over `build_messages_body_native`** (6.9 ruling) rather than
   the brief's 6.1 Step 3 rig-typed body; `system_rows`/`anthropic_messages` are native-typed.
2. **`stream_sse_body`: dropped the dead `state = None;` before an unconditional `return`** in the
   error arm (rustc `unused_assignments` warning). Behaviour identical.
3. **`chunk_boundaries_do_not_change_the_event_sequence` compares a stable projection
   (`event_key`: text / `id:name:args` / usage) instead of `{:?}`** — rig's `RawStreamingToolCall`
   mints a random `internal_call_id` per value, so `Debug` of two parses of the same bytes never
   matches. The brief's version failed for that reason on first run.
4. **Lean doc comment edit** in `RenderedCapture.lean` (above) so the residue scan does not flag the
   proofs tree; comment only.
5. **Protocol test rename** (`..._and_not_process_cli_urls` → `..._urls_only`) for the same reason.
6. **`first_text_event_is_observable_before_the_body_is_exhausted`** boxes the stream
   (`Box::pin(stream_sse_body(..))`) so `.next()` compiles (`async_stream` streams are `!Unpin`).
7. `sse_tool_use_deltas_yield_exact_arguments` additionally checks the `sse_fixture_tool_use` shape;
   `sse_tool_use_with_unparseable_input_fails_closed` uses `sse_fixture_tool_use`. The local
   multi-delta `sse_tool_use_block` helper is kept for the custom-start-input / two-delta / overlap
   shapes the module fixtures do not express.
8. The serve.rs seat tests use `tempfile::tempdir()` (already a `gents-cli` dependency) instead of
   the old `.scratch/claude-spike/tmp` paths.

## Seam scans (6.9)
- `docs::rig_vocabulary_confined_to_the_seam` … **PASS**
- `prompt_assembly::provider_invocations_are_confined_to_the_owned_loop_seam` … **PASS**
- Marker grep over `claude_messages.rs`, `claude_messages/tests.rs`, `claude_subscription.rs`,
  `claude_subscription/tests.rs`, `tests/conformance/prompt_assembly.rs`: no hits.

## Owned-loop test
`agent::loop_stream::tests::claude_messages_tool_round_trip_through_owned_loop … ok` — tool result
`ECHOED`, final text `done`, persisted `AgentToolCall.args == {"text":"hi"}`,
`lifecycle_state == completed`.

## Conformance body driver (Task 5's red)
`prompt_assembly::generated_claude_body_cases_drive_the_body_builder … ok` (plus map and stream
drivers ok).

## Residue scan (6.7 Step 1)
```
grep -rn 'println!' claude_messages.rs claude_subscription.rs (+ tests.rs)  → nothing (exit 1)
git grep -n '<residue pattern>' -- crates
  → crates/gents/src/agent/p2p_reconcile/engine/tests.rs:2349: async fn add_replicator_records_filters_at_seam() {
```
The single hit is a pre-existing, unrelated p2p test name matching the `_at_seam` pattern; not touched.
Token scan (`sk-ant`, `Bearer`) over the new/rewritten files: nothing. No `/Users/` paths in touched tests.

## Commit
`39bacaf1` — feat(claude): single Messages HTTP wire with incremental SSE; delete the process-CLI completer
(trailers: Co-Authored-By + Claude-Session). `git status --porcelain | grep -v '^??'` after commit
shows only the pre-existing ` M docs/design-notes/SPEC-claude-a2b-in-process.md` (left unstaged).
`docs/superpowers/` never added.

## Full gate (6.7 Step 1) — logs under `.superpowers/sdd/2026-09-03-claude-single-wire/`

### `cargo test -p gents --no-fail-fast` (`task-6-gate-test.log`, EXIT=0) — fully green
```
unittests src/lib.rs      test result: ok. 1941 passed; 0 failed; 2 ignored
tests/conformance.rs      test result: ok. 366 passed; 0 failed; 0 ignored
tests/e2e_lifecycle.rs    test result: ok. 44 passed; 0 failed; 0 ignored
tests/e2e_runtime.rs      test result: ok. 57 passed; 0 failed; 1 ignored
tests/e2e_subagent.rs     test result: ok. 107 passed; 0 failed; 0 ignored
tests/e2e_triggers.rs     test result: ok. 8 passed; 0 failed; 0 ignored
tests/misc.rs             test result: ok. 27 passed; 0 failed; 0 ignored
doc-tests                 test result: ok. 0 passed; 0 failed
```
Failing tests: none. (`e2e_runtime::completion_retry_tape::deadline_tight_fails_cleanly` passed
first time.) `cargo test` rebuilt the Lean contract snapshot (`lake` on PATH; "Build completed
successfully").

### `cargo test -p gents-protocol` (`task-6-gate-protocol.log`, EXIT=0)
```
unittests src/lib.rs      test result: ok. 143 passed; 0 failed; 0 ignored
```

### `cargo test -p gents-cli` — green modulo two environment artifacts (see note)
Run 1, repo `TMPDIR` (`task-6-gate-cli.log` / `task-6-gate-cli-rerun.log` with `--no-fail-fast`):
```
unittests src/lib.rs        test result: ok. 668 passed; 0 failed
tests/cli_codex_shim.rs     test result: FAILED. 30 passed; 1 failed; 8 ignored
  -> thread_metadata::codex_shim_derives_git_info_and_keeps_empty_thread_ephemeral
tests/cli_config.rs         test result: ok. 28 passed; 0 failed; 1 ignored
tests/cli_demo.rs           test result: ok. 5 passed; 0 failed
tests/cli_demo_secscan_live test result: ok. 0 passed; 1 ignored
tests/cli_graph.rs          test result: ok. 3 passed; 0 failed
tests/cli_offline.rs        test result: ok. 27 passed; 0 failed
tests/cli_p2p_suite.rs      test result: ok. 16 passed; 0 failed; 1 ignored
tests/cli_runtime.rs        test result: ok. 37 passed; 0 failed
tests/cli_seeded.rs         test result: ok. 14 passed; 0 failed; 1 ignored
tests/cli_server.rs         test result: ok. 13 passed; 0 failed
```
Run 2, `env -u TMPDIR cargo test -p gents-cli` (`task-6-gate-cli-systmp.log`): lib 668 ok; the
git-info test passed; `cli_codex_shim` failed a *different* test,
`background_continuations::codex_shim_streams_claimed_background_completion_and_replays_it_once`,
with `embedded HTTP listener cannot bind to 127.0.0.1:51328 … Address already in use (os error 48)`
(ephemeral-port collision in the shim harness). Later binaries did not run (no `--no-fail-fast`),
but all of them were green in run 1.

Isolated reruns (`task-6-gate-cli-isolated.log`, `task-6-gate-cli-bg-isolated.log`): each of the
two tests run twice alone → **4/4 ok**.

**Environment note:** the build-env convention `TMPDIR="$PWD/.scratch/tmp"` places
`tempfile::tempdir()` inside this git checkout, so any test that assumes a temp dir is *not* inside
a repository (`codex_shim_derives_git_info_and_keeps_empty_thread_ephemeral` creates a "plain" cwd
and expects `git_info == None`) sees sha `ad97dd5a` / branch `spike/claude-b3-live-tools` and
fails. Run that crate's tests with the system temp dir. The second failure is a pre-existing port
race in the codex-shim harness, unrelated to this change; neither test touches Claude code.

### `cargo check --workspace --all-targets` (`task-6-gate-check.log`, EXIT=0)
```
warning: field `description` is never read
  --> crates/gents-cli/src/commands/demo/secscan/matchers.rs:28:9      (known, pre-existing)
warning: `gents-cli` (lib test) generated 1 warning
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1m 02s
```
No other warnings or errors.

## Self-review
- Every provider request in the crate is still born in the owned loop: `claude_subscription.rs`
  no longer contains `.stream(` / `.completion(`; the seam scan enforces it.
- Live path unchanged in shape: token is read only when no fixture is queued and
  `write_approved` is true; the refusal string still names `--claude-write-approved`.
- Persist-before-send still runs for fixtures: `SeatTransport` sits inside
  `RenderedRequestCapturingHttpClient`; the owned-loop test arms the capture and passes.
- `install_messages_sse_fixtures` is `pub` (not `cfg(test)`) because the queue is consulted by
  production `stream_messages`; it was `pub` before too. It is harmless when empty.
- `from_rig_message` keeps its `#[cfg_attr(not(test), allow(dead_code))]`; now used in production
  by `build_messages_body` — the attribute is inert. Could be dropped in Task 7/#438 cleanup.
- No `println!`, no `[]` mutations, no GraphQL interpolation added. `lib.rs` untouched.
- Persisted vocabulary kept: `RenderedRequestSource::ClaudeCliSubscription`,
  `BackendProviderKind::ClaudeCliSubscription`, `claude-cli://subscription`.

## Concerns
- `completion()` still fails closed on empty assistant text (a tool-only turn through the
  non-streaming path would error). The owned loop uses `stream()`, so this matches prior behaviour;
  flagging in case the non-streaming path is ever used for tool turns.
- The residue-scan pattern `_at_seam` matches the unrelated, pre-existing
  `p2p_reconcile::engine::tests::add_replicator_records_filters_at_seam`; left alone.
- `ClaudeSeatConfig.claude_bin` and the `claude auth status` probe remain until Task 7.
