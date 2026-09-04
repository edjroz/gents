# Fix wave report — final whole-branch review

Branch `spike/claude-b3-live-tools`, base `f20239b1` → commit `9a92c586`.
All 11 items done in the one commit
`fix(claude): final-review wave — fmt, backends doc, diagnose gating, test-only fixture queue, stale comments`.

Post-commit `git status --porcelain | grep -v '^??'` → ` M docs/design-notes/SPEC-claude-a2b-in-process.md` only.
Untracked docs under `docs/superpowers/` were edited (items 9, 10) and not added. `TODO.md`, `tasks/`, SPEC-a2b untouched.

## Per item

1. **fmt (Important 1)** — `rustup run 1.97.1 cargo fmt --all`. `git diff --stat` after the run listed exactly the 13 review files:
   `crates/gents-cli/src/cli/args/tests.rs`, `crates/gents-cli/src/commands/claude_login.rs`, `crates/gents-cli/src/commands/serve.rs`,
   `crates/gents-protocol/src/rendered_request.rs`, `crates/gents/src/agent/loop_stream/tests/mod.rs`, `crates/gents/src/backend_health.rs`,
   `crates/gents/src/claude_messages.rs`, `crates/gents/src/claude_messages/tests.rs`, `crates/gents/src/claude_subscription.rs`,
   `crates/gents/src/claude_subscription/tests.rs`, `crates/gents/src/lean_vocab_test/support.rs`, `crates/gents/src/rendered_request/scope.rs`,
   `crates/gents/tests/conformance/prompt_assembly.rs` (plus the pre-existing SPEC-a2b working-tree edit, which fmt did not touch).
   `crates/gents/src/lib.rs` was not in the diff; nothing reverted. `rustup run 1.97.1 cargo fmt --all --check` → clean (no output), rerun clean after all later edits. Remaining drift: none.
2. **docs/backends.md (Important 2)** — rewrote the `ClaudeCliSubscription` table row (single Messages HTTP wire, tool-capable / fail closed, `system[0]` identity, two `cache_control` breakpoints, `tools` omitted when empty, no sampling keys, seat-token health), the §"Claude Max subscription" opener, the health bullet in "Probe lifecycle and health" (`.credentials.json` → Keychain via `security(1)`, `Expired`/`MissingFile` hint with `gents claude-login --config-dir <dir>`, unknown→healthy promotion, measured detail in the runtime health snapshot vs `probe_status`/`last_probe` on the document), the login-comment, the endpoint/billing row, the failure-table rows (dropped `logged_in=false` and "keep text-only"), "Credential storage", and the closing "text-only under A2b" line. `grep -i 'text-only\|completer\|logged_in\|fake' docs/backends.md` → no hits. Structure unchanged; no new sections.
3. **`gents diagnose` (Important 3)** — `crates/gents-cli/src/commands/diagnose/backends.rs`: discovery now gated on `!provider_kind.skips_fleet_http_probe()`; `ClaudeCliSubscription` skips discovery and reports `"note": "claude subscription seat: discovery not applicable; health is the seat-token probe"` (new `note` field in the report JSON, `null` otherwise). `is_agent_scoped_oauth()` untouched. Test `commands::diagnose::backends::tests::claude_subscription_backend_skips_discovery_and_stays_ok` asserts `ok=true`, `error=null`, the note, and empty `discovered_models` — passed in the CLI gate.
4. **Fixture queue test-only (Important 4)** — `claude_messages.rs`: `messages_sse_fixture_queue`, `install_messages_sse_fixtures` (now `pub(crate)`), `take_messages_sse_fixture` and their `VecDeque`/`Mutex`/`OnceLock` imports are `#[cfg(test)]`; `stream_messages` uses a `#[cfg(test)]` / `#[cfg(not(test))] let fixture: Option<String> = None;` pair. `git grep install_messages_sse_fixtures -- crates` confirmed all callers are test code (`claude_messages/tests.rs`, `claude_subscription/tests.rs`, `claude_subscription.rs::lock_process_seat_for_test` (already `#[cfg(test)]`), `agent/loop_stream/tests/claude.rs`). `cargo check --workspace --all-targets` clean confirms non-test builds compile without the queue.
5. **Stale comments (Minor 6)** — updated `rendered_request/mod.rs` header (transport seam only, restored from `main`), `gents-protocol/src/rendered_request.rs` (`ClaudeCliSubscription` stamped by the capturing HTTP transport only), `completion_factory.rs` (no sampling/reasoning keys), `openai_wire.rs` (Messages wire ignores `openai_wire_api`), `ClaudeMap.lean` header (Messages HTTP only; no C1/Completer framing — `lake build` EXIT=0), `proofs/README.md` (fences `tests/conformance/prompt_assembly.rs::generated_claude_{map,stream,body}_cases_*`; identity pin in `claude_messages::tests`), `cli/args.rs` after_help and `commands/claude_login.rs:1` (no "Path A"), `claude_completer/mod.rs` (dropped scratch-script pointer).
6. **Inline `capture_request_json` (Minor 7)** — folded into `capture_body` in `rendered_request/scope.rs`; behaviour unchanged, existing scope tests green in the gents gate.
7. **`message_stop` before `content_block_stop` (Minor 10)** — `handle_payload`'s `"message_stop"` arm flushes `pending` through `mapped_tool_call` before `FinalResponse`. New test `claude_messages::tests::sse_message_stop_flushes_pending_tool_before_final` (start + one delta, then `message_stop`): events `[ToolCall({"text":"hi"}), FinalResponse]` and `finish()` yields nothing — passed.
8. **`set_sensitive` (Minor 12)** — authorization `HeaderValue` is built, `.set_sensitive(true)` applied, then passed to `.header("authorization", …)`.
9. **Naming deviation (Minor 13)** — appended to the plan's "Naming deviations from the spec": "Spec §5 says the serve status JSON keeps `claude_seat`; the pre-existing key is `claude_subscription` and it stays." (untracked file; not added).
10. **Spec §5 / §4** — §5 hint now `run gents claude-login --config-dir <dir>`; §4 Send time sentence carries the parenthetical about rig's reqwest transport (no headers on the pre-checked non-2xx path; status plus bounded body prefix; follow-up: transport-level status check). Untracked file; not added.
11. **SPEC-a2c plumbing list (Minor 16)** — the "Parse stream-json…", "Fake-completer fixtures…", and "Server flags already exist…" bullets each carry a bracketed "[Superseded 2026-09-03 by the single wire: …]" note; historical text kept.

## Gates

Environment: `GENTS_SKIP_LENS_BUILD=1 GENTS_SKIP_CALLBACK_WASM_BUILD=1 TMPDIR=$PWD/.scratch/tmp`, elan on PATH. Logs in this directory.

- `lake build` (crates/gents/proofs) — `fixwave-gate-lake.log`: `Build completed successfully.` EXIT=0.
- `cargo test -p gents --no-fail-fast` — `fixwave-gate-test.log`: EXIT=0. Targets: 1943/0 (lib, 2 ignored), 366/0, 107/0, 57/0 (1 ignored), 44/0, 27/0, 8/0, 0/0. `completion_retry_tape::deadline_tight_fails_cleanly ... ok` on the first run (no rerun needed). New test `sse_message_stop_flushes_pending_tool_before_final ... ok`.
- `env -u TMPDIR cargo test -p gents-cli` — `fixwave-gate-cli.log`: EXIT=0. Targets: 666/0, 37/0, 31/0 (8 ignored), 28/0, 27/0, 16/0, 14/0, 13/0, 5/0, 3/0, and empty targets. New test `claude_subscription_backend_skips_discovery_and_stays_ok ... ok`. No rerun needed. Only warning: the known `matchers.rs` `description` never read.
- `cargo check --workspace --all-targets` — `fixwave-gate-check.log`: EXIT=0; warnings are only the two stub-WASM build-script notices and the known `matchers.rs` warning.
- `rustup run 1.97.1 cargo fmt --all --check` — clean.
