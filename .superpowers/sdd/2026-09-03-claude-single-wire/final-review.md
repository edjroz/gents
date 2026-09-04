# Final whole-branch review: `spike/claude-b3-live-tools` (ca615b1c..f20239b1)

Reviewer: Senior Code Reviewer (read-only). Date: 2026-09-04.

### Passes taken

1. **Orientation** — spec §2–§6/§9, plan header (Global Constraints, Naming deviations), ledger rulings and deferred minors, commit list (50 commits, 61 files, no deletions/renames at the range level).
2. **Runtime wire + credentials (final state)** — `claude_seat_auth.rs`, `claude_messages.rs` (+ `tests.rs`), `claude_subscription.rs` (+ `tests.rs`), `claude_completer/mod.rs`, `rendered_request/{transport,scope,mod}.rs`, `gents-protocol/rendered_request.rs`; verified against the pinned rig fork (`~/.cargo/git/checkouts/rig-ca46c0fecedc09c9/1ebeeae`) that `ReqwestClient::send_streaming` pre-checks status and returns `InvalidStatusCodeWithMessage`.
3. **Health / promotion / registry / factory arms** — `backend_health.rs`, `backend_provider.rs`, `backend_registry.rs`, `completion_factory.rs`, `agent/runtime/context.rs`, `oneshot.rs`, `openai_wire.rs`.
4. **Lean + fences** — `ClaudeMap.lean`, `ContractCases/PromptAssembly.lean`, `Contracts/Json/*`, `CoverageLedger.lean`, `RenderedCapture.lean` doc, `tests/conformance/prompt_assembly.rs`, `lean_vocab_test/*`, `conformance_consumers.rs`, `coverage.rs`. Ran `lake build Proofs.PromptAssembly.ClaudeMap Proofs.Conformance.ContractCases.PromptAssembly` (green, zero `sorry`).
5. **Owned-loop seam** — read both seam scans (`provider_invocations_are_confined_to_the_owned_loop_seam`, `docs::rig_vocabulary_confined_to_the_seam`) and confirmed neither allowlist was widened in the range.
6. **CLI + docs + deletions** — `gents-cli` diff, `docs/backends.md`, PR-STACK / SPEC-a2c added lines, grep for every §2 "Removed" symbol across tracked `.rs/.lean/.md/.toml`, token-pattern grep across the tracked tree at `f20239b1` and across `.scratch/claude-spike/logs/`, evidence-file summaries (#8, #9, #10).
7. **CI-style check** — `rustup run 1.97.1 cargo fmt --all --check` (read-only; no build).

### Strengths

- **The wire is small and honest.** `build_messages_body_native` is ~60 lines that read exactly like the Lean `systemBlocks`/`splitSystem`/`toolsField`; `MessagesSseState` is a single state machine with the Lean `step`/`flush` shape, and `parse_messages_sse` is a trivial wrapper over the same `push_line`/`finish` the live body goes through. The chunk-boundary test and the "first text event before body exhausted" test are the right two properties for the incremental parser.
- **Credential handling is careful end to end.** `SeatAccessToken` has a redacted `Debug` and no `Display`; `parse_credentials` deliberately does not echo serde errors; Keychain reads use `security(1)` with argv (no shell), stderr nulled; the only header construction error type (`InvalidHeaderValue`) does not carry the value; the capture seam persists the **body only** (transport.rs `capture_or_refuse` hashes and stores `body`; `provider_endpoint_of` keeps scheme+authority, never path/query/headers); the live-send log line carries model + config_dir only. A token-pattern grep over the tracked tree and the evidence logs is clean.
- **Write gate is a single predicate** (`fixture.is_none() && !seat.write_approved`) at the top of the only send path, and the token is only read when a live send will happen. Both fixture and live paths go through `RenderedRequestCapturingHttpClient` (persist-before-send holds for fixtures too).
- **Seam scans went green the principled way.** `build_messages_body` converts once via `rig_compat::from_rig_message`, the body is assembled over the native family, `completion()` calls `stream_messages` directly, and the conformance body driver uses native `Message`. Neither allowlist changed.
- **The Lean model is real.** `splitSystem_partition` is an actual induction, `accumulate_ignores_start_when_streamed` makes C1 unrepresentable, `runStream_*` are `native_decide` on concrete instances, the `DecidableEq (Except …)` instance is `local`, and the witnesses are computed by `mapTurn`/`runStream`/`systemBlocks`, not hand-written. Drivers compare against the production parser/builder.
- **Health branch never spawns** and falls through to the shared `record_probe_event` (the seven-arg extraction is a pure lift of the old inline block); promotion is the same `probe_status == "unknown" && ProbeSuccess` predicate every HTTP backend uses. Tests exercise real credentials files under tempdirs, K=3 demotion with the login hint, and the C9 promotion.
- **The §2 "Removed" list is actually gone.** No `ProcessCli`, `captured_only_at`, `StreamJsonl*`, `spawn_completer`, `PATH_A_MODEL_IDS`, `live_claude_allowed`, `parse_auth_status*`, `validate_claude_seat_args`, `claude-max-cli`, `--claude-bin/--claude-workdir/--claude-log-dir/--claude-fake-completer` on serve, `claude-auth-probe`, `--console/--sso` anywhere in tracked Rust/Lean. The two creep commits are cleanly reverted (net-zero on their files).
- Persisted vocabulary unchanged: `ClaudeCliSubscription` rename + two aliases, `claude-cli://subscription`, `RenderedRequestSource::ClaudeCliSubscription` with `/messages` path classification.

### Issues

#### Critical (Must Fix)

None found.

#### Important (Should Fix)

1. **`cargo fmt --all --check` fails on 13 files, all in this range** — CI `rust-and-cli` will be red.
   `rustup run 1.97.1 cargo fmt --all --check` reports diffs in: `crates/gents/src/{backend_health.rs, claude_messages.rs, claude_messages/tests.rs, claude_subscription.rs, claude_subscription/tests.rs, lean_vocab_test/support.rs, rendered_request/scope.rs, agent/loop_stream/tests/mod.rs}`, `crates/gents/tests/conformance/prompt_assembly.rs`, `crates/gents-protocol/src/rendered_request.rs`, `crates/gents-cli/src/{cli/args/tests.rs, commands/claude_login.rs, commands/serve.rs}`. The base versions were in CI's style; the branch reflowed them into 2024-style-edition ordering (`use tokio::sync::{RwLock, mpsc}`, `fn f()\n-> &'static …`), which is also the §9 "import-order churn" the spec asked to keep out. Fix: `rustup run 1.97.1 cargo fmt --all` (leaves `crates/gents/src/lib.rs` untouched — it is not in the list), then re-check. Do this before the carve so PR1–PR4 each pass fmt on their own.

2. **`docs/backends.md` still documents the deleted process-CLI, text-only architecture** — operator-facing guidance is wrong.
   `docs/backends.md:23` (table row): "In-process Claude CLI completer", "Stream-json → owned loop SSE", "Text-only: completer `--tools ""` + fail-closed on `tool_use`; tools never forwarded", "Unit/fake completer fixtures". `:262-275`: "gents-owned text completer without a native Anthropic Messages provider… dispatches … in-process to the Claude CLI completer". `:359-373` failure table: "Probe `logged_in=false`", "Completer errors on `tool_use` | Tools leaked into Claude path | Keep text-only; do not enable Claude tools". `:397`: "Claude remains text-only under A2b". Task 8 only touched the health paragraph and the credential-storage paragraph. Fix: rewrite the table row and §"Claude Max subscription" to the single Messages wire (tool-capable, `system[0]` identity, `tools` omitted when empty, seat-token health, promotion), drop the `logged_in` / "keep text-only" rows. This is PR4 content per spec §8.

3. **`gents diagnose` reports every `ClaudeCliSubscription` backend as broken.**
   `crates/gents-cli/src/commands/diagnose/backends.rs:175` gates HTTP discovery on `!provider_kind.is_agent_scoped_oauth()`, which is `false` for Claude by design; `discover_models` then bails ("does not support HTTP model discovery") and diagnose sets `ok=false, error="backend discovery failed: …"`. The runtime side was switched to `skips_fleet_http_probe()` (`backend_registry.rs:397`, `backend_health.rs:262`) but the CLI diagnostic was not. Fix: gate on `skips_fleet_http_probe()` and, for Claude, report `claude_subscription::probe_seat_detail(config_dir)` when a config dir is known (or simply skip discovery with a note). Add a test.

4. **The SSE fixture queue is a production-visible API, not a test seam.**
   `crates/gents/src/claude_messages.rs:≈67` `pub fn install_messages_sse_fixtures(...)` is unconditional and `pub` in a `pub mod`; anything linking `gents` can queue a body that bypasses both the network and the write gate check. Spec §2 says "One test seam … the fixture queue test-only". No non-test caller exists (verified by grep), so this is `#[cfg(test)] pub(crate)` today at zero cost; the `take_messages_sse_fixture()` call in `stream_messages` becomes `#[cfg(test)]`-guarded (or `None` in non-test builds). Not a billing hole — a fixture means no send — but it is an unrecorded deviation from the spec's test-only guarantee and a footgun for future callers.

5. **Spec §6 live bars #10 and #11 are unmet.** `b3-live-single-wire-evidence.md` is an environmental FAIL (expired seat; 0 rows, nothing billed) and #11 has not run. The body (`system[1]`, no `system:` user block, cache breakpoints) and the streaming incrementality have been fixture- and unit-verified but never observed on the wire; health-gating/promotion likewise. The ledger ruling to continue was reasonable for *development*, but the spec's own gate for *merge* is the live evidence. Fix: run #10b and #11 on a refreshed seat before the PR stack is opened, and update the "pending re-run" lines in `PR-STACK-claude-track-b-tools.md` and `SPEC-claude-a2c-tool-bridging.md:359`.

#### Minor (Nice to Have)

6. **Stale doc comments that describe the deleted seam.**
   - `crates/gents/src/rendered_request/mod.rs:8-12` — "process-CLI Completers immediately before spawn … or the CLI is spawned". Revert to the pre-spike wording (transport seam only), which is what `RenderedCapture.lean` now says.
   - `crates/gents-protocol/src/rendered_request.rs:63-66` — "`ClaudeCliSubscription` is stamped by the in-process Completer for process-CLI captures, and by the capturing HTTP transport…". Only the second half is true.
   - `crates/gents/src/completion_factory.rs:337` — "Claude CLI subscription is text-only in A2b".
   - `crates/gents/src/openai_wire.rs:63-65` — "The in-process completer ignores openai_wire_api".
   - `crates/gents/proofs/Proofs/PromptAssembly/ClaudeMap.lean:4-18` — header still frames the model as shared by "Claude CLI stream-json (C1)" and mentions "the Completer's `toolu_*` → `Nat` injection".
   - `crates/gents/proofs/README.md:217` — "Fence: `claude_messages::tests`"; the fences are `tests/conformance/prompt_assembly.rs::generated_claude_{map,stream,body}_cases_*` (only the identity pin lives in `claude_messages::tests`).
   - `crates/gents-cli/src/cli/args.rs:89` after_help and `commands/claude_login.rs:1` — "Path A auth" (Path A was the retired proxy era).
   - `crates/gents/src/claude_completer/mod.rs:15-16` — "Keep in sync with the spike completer (`.scratch/claude-spike/bin/claude-completer.sh`)" points at an untracked scratch file.

7. **`rendered_request/scope.rs:361-390` — `capture_request_json` is a one-caller indirection left over from the process-CLI capture path.** Inline it back into `capture_body` (this also removes one of the fmt diffs).

8. **Blocking work inside async contexts.** `claude_seat_auth::read_seat_access_token` does a synchronous file read and, on macOS with no file, a synchronous `security(1)` spawn. It is called from `stream_messages` (`claude_messages.rs:≈601`) and from the probe cycle (`backend_health.rs:238`), both on the tokio runtime. Cheap today, but a slow Keychain prompt/timeout would stall a worker. Consider `tokio::task::spawn_blocking` at both call sites (spec §5 lists token caching as a follow-up; this is orthogonal).

9. **`request-id` is always `-` on a live non-2xx.** `claude_messages.rs:≈612-620`: rig's `ReqwestClient::send_streaming` pre-checks the status and returns `InvalidStatusCodeWithMessage(status, body_text)` without headers, so `non_success_error(status, None, …)` is the only live path; the header-bearing branch is reachable only for non-reqwest transports (as the new comment says). Spec §4 promises status + `request-id`. Either accept and say so in the spec, or have `SeatTransport::send_streaming` do its own status check on the reqwest response before handing it to rig's wrapper.

10. **`message_stop` arriving before `content_block_stop` emits `FinalResponse` before the flushed tool call.** `handle_payload` "message_stop" arm sets `finished` and emits `FinalResponse`; `finish()` then flushes the pending block *after* it. Malformed-stream only; consider flushing `pending` on `message_stop` first (Lean's `runStream` flushes at end-of-stream, which is after all events, so the model is silent here).

11. **Non-text user content is silently dropped.** `anthropic_messages` (`claude_messages.rs`, `UserContent`/`AssistantContent` `_ => {}` arms) discards images/documents/reasoning without a trace line. A `tracing::debug!` with the variant name would make a "why did Claude not see my image" investigation trivial.

12. **Consider `HeaderValue::set_sensitive(true)`** on the authorization value (`claude_messages.rs:≈604`) so any future `{:?}` of the `http::Request` (e.g. in a rig error path) redacts it. Defence in depth; nothing prints the request today.

13. **Unrecorded naming deviation:** spec §5 says the serve status JSON "keeps `claude_seat`"; the key is `claude_subscription` (`serve.rs:≈829-841`), as it was before this plan. Harmless; add to the plan's "Naming deviations" list.

14. **Health `Ok` detail is only a debug log.** `backend_health.rs:240-246` logs `source=… expires_at=…` at `debug` and stores `last_error: None`; nothing operator-visible carries the expiry. `backends.md` claims "the server's health cycle reports the same detail" (ledger already flagged the phrasing). Either surface the detail in the health map entry or soften the doc.

15. **`tasks/todo.md` (749 lines) and `tasks/plan.md` (173 lines)** are proxy-era spike bookkeeping (claude-proxy, `:8787`, GATED write requests) committed in `c535376e`. They will read as current guidance in the PR stack. Recommend leaving them out of PR4 (or moving under `.scratch/`, which is now ignored) rather than carrying them onto `main`.

16. **`SPEC-claude-a2c-tool-bridging.md:76-80`** still lists "Parse stream-json…", "Fake-completer fixtures…", "Server flags already exist (… `--claude-bin`, workdir, log-dir)". Dated draft, but §8 says this doc's C2 lock is updated in PR4 — one more pass on the "What is plumbing" list would make it consistent.

17. **The flaky `e2e_runtime::completion_retry_tape::deadline_tight_fails_cleanly` has not been filed** (ledger line 57: "file after the plan completes"). CLAUDE.md: flaky tests are defects — file the issue before the stack opens.

### Deferred-minor triage (from the ledger)

| Ledger item | Status at f20239b1 | Verdict |
|---|---|---|
| Task 1: no test for "input absent + no deltas → `{}`" | Covered by witness `no-input-is-empty-object` and `sse_tool_use_with_no_input_at_all_is_empty_object` | can-ship (done) |
| Task 1: `input_json_delta` with no pending block silently dropped | Matches Lean `step (.delta) none => ok` | can-ship (by design) |
| Task 1: `MalformedToolUse.line: 0` placeholder | Field removed in Task 6 | can-ship (done) |
| Task 3: CLAUDE.md warm-up wording | Fixed in `26193523` | can-ship (done) |
| Task 4: tools test assert array length 1 | `messages_body_omits_tools_key_when_surface_is_empty` asserts `Some(1)` | can-ship (done) |
| Task 5: body driver rig imports / seam-scan hits | Both scans green with unchanged allowlists | can-ship (done) |
| Task 5: flaky `deadline_tight_fails_cleanly` | Not filed | **must-do before merge** (file the issue; no code change required here) |
| Task 5: `DecidableEq (Except …)` should be local | `local instance` | can-ship (done) |
| Task 5: Rust trims blank preamble/System rows, Lean does not | Documented in `splitSystem` / `systemBlocks` docstrings | can-ship (done) |
| Task 5: no witness for `[start 1, start 1]`; text_delta without text start | Rust and Lean agree (overlap check precedes dup check in both); text without start is accepted by both | can-ship |
| Task 6: TMPDIR-in-repo note | In CLAUDE.md | can-ship (done) |
| Task 6: dead `Option<MessagesSseState>`, non-2xx body prefix, `completion()` tool drop documented, allowed-keys test list, `from_rig_message` dead-code allowance, debug on unparseable data | All addressed in `26193523` | can-ship (done) |
| Task 7: fold `KeychainAccountUnset` into file error | Done (`read_seat_access_token_at`) | can-ship (done) |
| Task 7: `tracing::warn` on `current_dir()` fallback | Skipped as unreachable | can-ship |
| Task 8: `backends.md` "reports the same detail on the backend document" loose | Still loose, and the surrounding section is stale (Important #2) | **must-fix before merge** as part of #2 |
| Task 8: historical `claude auth status` in PR-STACK-prod-hardening.md:91, SPEC-phase6:44 | Dated historical notes | can-ship |

### Spec/plan issues

- **Spec §5 hint text is wrong, the code is right.** Spec says the Expired/MissingFile detail includes `run gents claude-login --claude-config-dir <dir>`; `claude-login`'s flag is `--config-dir` (`args.rs` `ClaudeLoginArgs`), and `seat_detail` prints `--config-dir`. Fix the spec, not the code.
- **Spec §4 "status + `request-id`" on non-2xx is not achievable through rig's reqwest transport** (Minor #9). Either amend §4 or add the transport-level check.
- **Spec §5 status JSON key `claude_seat`** does not match the pre-existing `claude_subscription` key (Minor #13); the plan's deviation list should record it.
- **Spec §2 "the fixture queue test-only"** is not what shipped (Important #4) — the plan's Task 6 did not carry the `#[cfg(test)]` requirement into the file map.
- **Spec §6 gates merge on live #10/#11**, but §7/§8 sequencing let the carve start with #10b/#11 outstanding. The plan should state explicitly that PR2 (wire) and PR1 (health) are not opened until #10b/#11 evidence exists.
- **Spec §8 Docs (PR4)** lists four docs; `docs/backends.md` — the operator doc most affected — is not on the list, which is how Important #2 slipped through two Task 8 reviews.

### Recommendations

1. Before the carve: `rustup run 1.97.1 cargo fmt --all` (13 files), inline `capture_request_json`, `#[cfg(test)]` the fixture queue, fix `diagnose` gating, rewrite `backends.md` §Claude and the table row, sweep the stale comments in Minor #6. All are small and local; none touch Lean or the wire semantics.
2. Run live #10b and #11 on the refreshed seat and update the two "pending re-run" references.
3. File the flaky-test issue.
4. Keep `tasks/todo.md` / `tasks/plan.md` out of the PR stack.
5. Follow-ups (tracked, not blocking): `spawn_blocking` around the seat read; transport-level non-2xx check for `request-id`; `set_sensitive` on the auth header; `message_stop`-before-`content_block_stop` ordering; trace on dropped non-text content.

### Assessment

**Ready to merge?** With fixes

**Reasoning:** The runtime, credential path, Lean model, and fences are sound and I found nothing that leaks the token, bypasses the write gate, or breaks persist-before-send; but the branch fails CI's `cargo fmt --check` on 13 files, `gents diagnose` falsely reports Claude backends broken, the operator doc still describes the deleted text-only CLI completer, the fixture seam is not test-only, and the spec's own live bars (#10b, #11) are unmet — all fixable in one short round before the carve.
