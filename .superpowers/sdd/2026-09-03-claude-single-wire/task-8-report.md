# Task 8 report: CLI and docs trim; ponytail residue

**Status:** DONE. Commit `26193523` on `spike/claude-b3-live-tools` (parent `1fd23d99`):
`chore(claude): drop claude-auth-probe, --console/--sso, claude-max-cli spellings; residue cleanup; document the single wire`
(trailers: Co-Authored-By + Claude-Session). `git status --porcelain | grep -v '^??'` shows only ` M docs/design-notes/SPEC-claude-a2b-in-process.md` (left unstaged as instructed).

## Per checkbox

### Step 1: Failing tests
- `crates/gents-cli/src/cli/args/tests.rs`: `claude_login_and_auth_probe_parse_and_require_config_dir` renamed to `claude_login_parses_and_requires_config_dir`; the `claude-auth-probe` parse case and the `!args.console` / `!args.sso` asserts dropped; new `claude_login_rejects_removed_console_and_sso_flags` added verbatim from the brief (`Cli::try_parse_from`, the neighbouring pattern).
- `crates/gents/src/backend_provider.rs` `mod tests`: new `claude_max_cli_spellings_are_not_accepted` (brief body, rustfmt-wrapped). No other parse tests existed in that module.

### Step 2: Run to verify failure (`task-8-red.log`)
```
test cli::args::tests::claude_login_rejects_removed_console_and_sso_flags ... FAILED   (panicked at args/tests.rs:816 — "--console must be gone")
test result: FAILED. 2 passed; 1 failed; ... EXIT_CLI=101
test backend_provider::tests::claude_max_cli_spellings_are_not_accepted ... FAILED    (panicked at backend_provider.rs:365 — "claude-max-cli")
test result: FAILED. 6 passed; 1 failed; ... EXIT_GENTS=101
```
Both red for the intended reason (flags still parsed; spellings still accepted). Both green in the final gates (see below) — TDD evidence for the two brief-mandated tests. Two further tests were added alongside implementation (`non_success_error_carries_status_request_id_and_bounded_body_prefix`, `probe_seat_detail_reports_missing_seat_with_login_hint`); both pass.

### Step 3: Implement
- `args.rs`: `Command::ClaudeAuthProbe` + its `#[command]` block, `ClaudeAuthProbeArgs`, and `ClaudeLoginArgs.console`/`.sso` deleted; `claude-login` `about` reworded per brief.
- `commands/mod.rs`: `claude_auth_probe` module line removed. `lib.rs`: dispatch arm removed. `git rm crates/gents-cli/src/commands/claude_auth_probe.rs`.
- `claude_login.rs`: `plan_claude_login` always pushes `--claudeai`; console/sso branches, the mutual-exclusion bail, the `console_flag_replaces_claudeai` test, and the `ClaudeAuthProbeArgs` import are gone. Post-login JSON field `probe` replaced by `"seat": gents::claude_subscription::probe_seat_detail(&plan.config_dir)`. `sanitize_child_env` / `STRIPPED_ENV_VARS` stay in `gents::claude_completer` (this file is their last user).
- `claude_subscription.rs`: new `fn seat_detail(config_dir: &Path) -> Result<String, String>` holds the `source=… expires_at=…` / login-hint formatting; `probe_process_seat_health` delegates to it; new `pub fn probe_seat_detail(config_dir: &Path) -> serde_json::Value` returns `{"ok": bool, "detail": String}`. Test added in `claude_subscription/tests.rs`.
- `backend_provider.rs`: both `claude-max-cli`/`claude_max_cli` serde aliases and the two `parse_optional` arms deleted; doc comment replaced with the brief's wording.

### Step 4: Docs
- `docs/design-notes/PR-STACK-claude-track-b-tools.md`: dated status block under the header — B3 done (#7–#10, citing `b3-live-args-evidence.md`, `b3-live-http-text-evidence.md`, and `b3-live-single-wire-evidence.md` as an environmental FAIL on an expired seat pending re-run), two-wire Completer retired 2026-09-03 for the single Messages wire (links the spec path), B4 still later.
- `docs/design-notes/SPEC-claude-a2c-tool-bridging.md`: "Recorded 2026-09-03 (single wire, as shipped)" block in the C2 auth lock section (identity as `system[0]`, Keychain via `security(1)`, single wire, `tools` omitted when empty, two cache breakpoints); open questions 1, 3, 5, 9 marked closed with one line each pointing at the evidence files.
- `crates/gents/proofs/README.md`: new `Proofs/PromptAssembly/ClaudeMap.lean` map-table row listing the six witness families; the one-paragraph "Provider-input assembly for Claude…" sentence placed above the table.
- `CLAUDE.md`: single-wire sentence appended to the "External code is held at arm's length" bullet.
- `.scratch/claude-spike/logs/b3-live-identity-evidence.md`: pointer line to `b3-live-single-wire-evidence.md` appended (untracked; not committed).

### Step 5: Gates and commit
See "Gate lines" below; staged exactly the changed files and committed with the extended subject and trailers.

### 8.x Ponytail residue
- `stream_sse_body`: `state` is a plain `MessagesSseState`; the `Option`, the `let Some(current) = state.as_mut() else { return; }` guard and the `take()` are gone; "yield Err then return" kept.
- `stream_messages` non-2xx: new pure `fn non_success_error(status: reqwest::StatusCode, request_id: Option<&str>, body_prefix: &str) -> CompletionError` (`reqwest::StatusCode` is `http::StatusCode`; `http` is not a direct dep and rig does not re-export it), `body_prefix(&[u8])` (512-byte cap, lossy, trimmed), `read_body_prefix` for the streamed body. The post-response `!status.is_success()` branch appends ` body={prefix}`. Also found while implementing: rig's reqwest `send_streaming` pre-checks the status and returns `Err(InvalidStatusCodeWithMessage(status, body))` for non-2xx, so the live path never reached the old branch; that error is now mapped through the same helper (bounded to 512 bytes) instead of surfacing as an unbounded `HttpError`. Request headers never included. Unit test on the pure helper (no transport stub).
- `dispatch_pending_data`: `tracing::debug!(len = raw.len(), "claude messages: ignoring non-JSON SSE payload")` before `Ok(vec![])`; payload text never logged.
- `impl CompletionModel::completion`: doc comment states text-only (owned loop uses `stream()`; tool-only turn surfaces as the empty-text error).
- `rig_compat.rs`: `#[cfg_attr(not(test), allow(dead_code))]` removed from `from_rig_message` only (the two helper converters it calls keep theirs, untouched per brief).
- `claude_seat_auth.rs` `read_seat_access_token_at`: `KeychainAccountUnset` folds into the file error alongside `KeychainNotFound`; doc comment updated. `missing_file_and_missing_keychain_item_is_missing_file` passes.
- `ClaudeMap.lean`: `instDecidableEqExcept` is now `local instance`; docstrings on `splitSystem` and `systemBlocks` note the Rust-only whitespace-preamble trim and blank-`System`-row drop that the model does not represent. `lake build` EXIT=0, zero `sorry`.
- `messages_body_omits_tools_key_when_surface_is_empty`: asserts `with_tools["tools"].as_array().map(Vec::len) == Some(1)`.
- `CLAUDE.md` "Sharp edges": worktree warm-up sentence amended (`cargo build --tests` does not warm `cargo test` because of the `[profile.test]` override; use `cargo test -p <crate> --no-run`); new bullet on `TMPDIR="$PWD/.scratch/tmp"` breaking git-sensitive tests (`gents-cli` codex-shim `thread_metadata`) — run `cargo test -p gents-cli` with the system `TMPDIR`.
- Recorded: `MESSAGES_BODY_ALLOWED_KEYS` survives as a test-only allowlist in `crates/gents/src/claude_messages/tests.rs` (intentional; the production `debug_assert` is gone).

## Deviations
1. **Residue grep cannot print nothing.** Post-commit output of the Step 5 grep (`residue exit=0`):
   - 5 hits are the literal strings inside the two brief-mandated tests (`args/tests.rs:814,818`, `backend_provider.rs:358,359,369`) — the tests exist precisely to assert those spellings are rejected.
   - `docs/design-notes/SPEC-claude-a2b-in-process.md:142` — file is off-limits per the controller.
   No production code, no CLI surface, and no other doc carries the retired names.
2. **Two historical design notes edited and staged beyond the brief's list:** `docs/design-notes/PR-STACK-claude-prod-hardening.md` (1 line) and `docs/design-notes/SPEC-claude-phase6-packaging.md` (6 spots) still named `gents claude-auth-probe` / `claude_auth_probe.rs`; each reference now says the probe command was retired 2026-09-03 and points at `claude-login`'s `seat` field / `probe_process_seat_health`. Chosen so the residue grep over `docs/design-notes` is clean apart from the off-limits file. Staged and committed with the rest.
3. **No ClaudeMap row existed** in the proofs README map table (the controller note said one did); a new row was added under the `Proofs/PromptAssembly/` row.
4. **rig pre-check mapping** (see 8.x non-2xx above) — a small addition beyond the brief's wording so the body prefix actually reaches live 4xx errors.
5. Formatting: the repo is not `cargo fmt`-clean (pre-existing diffs in `loop_stream/tests`, `backend_health.rs`, `claude_messages/tests.rs`, `claude_login.rs`, …). I hand-applied `rustfmt --edition 2024` shape to only the lines I wrote; pre-existing unformatted lines were left alone. `crates/gents/src/lib.rs` untouched.

## Gate lines
```
### lake (task-8-gate-lake.log)
✔ [1021/1022] Built Proofs
Build completed successfully.
EXIT=0
sorry count (Proofs/**/*.lean, comments excluded): 0

### cargo test -p gents --no-fail-fast (task-8-gate-test.log) EXIT=0, FAILED count 0
unittests src/lib.rs      test result: ok. 1942 passed; 0 failed; 2 ignored
tests/conformance.rs      test result: ok. 366 passed; 0 failed
tests/e2e_lifecycle.rs    test result: ok. 44 passed; 0 failed
tests/e2e_runtime.rs      test result: ok. 57 passed; 0 failed; 1 ignored   (completion_retry_tape::deadline_tight_fails_cleanly ... ok, first try)
tests/e2e_subagent.rs     test result: ok. 107 passed; 0 failed
tests/e2e_triggers.rs     test result: ok. 8 passed; 0 failed
tests/misc.rs             test result: ok. 27 passed; 0 failed
doc-tests                 test result: ok. 0 passed; 0 failed

### env -u TMPDIR cargo test -p gents-cli (task-8-gate-cli.log) EXIT=0, FAILED count 0
unittests src/lib.rs      test result: ok. 665 passed; 0 failed
tests/cli_codex_shim.rs   test result: ok. 31 passed; 0 failed; 8 ignored
tests/cli_config.rs       test result: ok. 28 passed; 0 failed; 1 ignored
tests/cli_demo.rs         test result: ok. 5 passed; 0 failed
tests/cli_graph.rs        test result: ok. 3 passed; 0 failed
tests/cli_offline.rs      test result: ok. 27 passed; 0 failed
tests/cli_p2p_suite.rs    test result: ok. 16 passed; 0 failed; 1 ignored
tests/cli_runtime.rs      test result: ok. 37 passed; 0 failed   (codex_shim_streams_claimed_background_completion_and_replays_it_once ... ok, first try)
tests/cli_seeded.rs       test result: ok. 14 passed; 0 failed; 1 ignored
tests/cli_server.rs       test result: ok. 13 passed; 0 failed
(main.rs / secscan_live / doc-tests: 0 passed; 0 failed)

### focused recheck after the rustfmt touch-ups (task-8-gate-fmt-recheck.log)
cargo test -p gents --lib -- claude_ backend_provider   test result: ok. 48 passed; 0 failed   EXIT=0
(the first attempt at this recheck failed identity_matches_lean_body_witness_head with "failed to build Lean conformance contract target ... os error 2" — I had omitted the elan PATH export; rerun with PATH green. Not a code defect.)
cargo check --workspace --all-targets (rerun after touch-ups): Finished, only the known matchers.rs warning.

### cargo check --workspace --all-targets (task-8-gate-check.log)
warning: field `description` is never read   (crates/gents-cli/src/commands/demo/secscan/matchers.rs — known)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 33.77s
EXIT=0

### residue grep (git grep -n ... -- crates docs/design-notes CLAUDE.md)
crates/gents-cli/src/cli/args/tests.rs:814:    for flag in ["--console", "--sso"] {
crates/gents-cli/src/cli/args/tests.rs:818:    assert!(Cli::try_parse_from(["gents", "claude-auth-probe", ...]).is_err());
crates/gents/src/backend_provider.rs:358:    fn claude_max_cli_spellings_are_not_accepted() {
crates/gents/src/backend_provider.rs:359:        for spelling in ["claude-max-cli", "claude_max_cli"] {
crates/gents/src/backend_provider.rs:369:        let parsed: BackendProviderKind = serde_json::from_str("\"claude-max-cli\"")
docs/design-notes/SPEC-claude-a2b-in-process.md:142:- `gents claude-auth-probe --config-dir …`
residue exit=0
```
New tests observed passing in the gate logs: `claude_login_parses_and_requires_config_dir`, `claude_login_rejects_removed_console_and_sso_flags`, `claude_max_cli_spellings_are_not_accepted`, `non_success_error_carries_status_request_id_and_bounded_body_prefix`, `probe_seat_detail_reports_missing_seat_with_login_hint`, `messages_body_omits_tools_key_when_surface_is_empty`, `missing_file_and_missing_keychain_item_is_missing_file`, `identity_matches_lean_body_witness_head`.

## Self-review
- No token appears in any output: `probe_seat_detail` / `seat_detail` only format source + expiry or the error `Display`; `non_success_error` carries response body only, never request headers; the SSE debug line logs a length.
- `tracing` only; no `println`. No DefraDB mutations or GraphQL touched.
- `stream_sse_body` semantics unchanged (line splitting, tail flush, `finish`, error-then-return); the only behavioural change on the wire path is the richer non-2xx message.
- The conformance consumer registry was not touched (no Rust test renamed that it lists; the renamed CLI arg test is not a conformance consumer).
- `lake build` used the whole project; the `local instance` change compiled with the six `runStream_*` `rfl` witnesses intact.

## Concerns
- `b3-live-single-wire-evidence.md` remains an environmental FAIL (expired seat); the docs say so and cite it as pending re-run. The new `body=` prefix on 4xx will make the next live re-run's failures self-describing.
- The two `from_rig_user_content` / `from_rig_assistant_content` converters still carry `#[cfg_attr(not(test), allow(dead_code))]` although they are reachable from production via `from_rig_message`; harmless but stale — left per the brief's scope.
- The repo is not rustfmt-clean overall (pre-existing); a follow-up formatting pass would be a separate, mechanical commit.
- Residue grep deviation #1 means the brief's "prints nothing" expectation is structurally unreachable while the mandated tests exist; suggest the grep exclude `tests.rs` / `mod tests` bodies going forward.

## Fix round 1 (review findings)

Commit `f20239b1` — `docs(claude): retire claude-auth-probe and the auth-status health note` (three files staged: `docs/design-notes/SPEC-claude-a2c-tool-bridging.md`, `docs/backends.md`, `crates/gents/src/claude_messages.rs`).

1. SPEC-claude-a2c C2 lock bullet amended in place: `is_agent_scoped_oauth()` still stays false; health is now described as the seat-token read (`probe_process_seat_health` → `read_seat_access_token`, no spawn; source + expiry in the detail; `claude-login` hint on Expired/MissingFile; `unknown` promoted to `healthy` on the first passing cycle like HTTP backends), marked superseded 2026-09-03.
2. `docs/backends.md`: the login block (~296) and Credential storage paragraph (~359) now describe `gents claude-login` printing the `seat` object (`ok`, `detail`) and the server health cycle reporting the same detail, no separate probe command. Also fixed the two `claude auth status` health mentions in the same file (matrix row line 23, prober bullet line 47) so the file carries no retired name. No restructuring.
3. `stream_messages`: one-line comment on the post-response `!status.is_success()` branch (reachable only for non-reqwest transports such as the fixture; rig's reqwest client pre-checks the status).

Covering test (comment-only code touch): `cargo test -p gents --lib claude_` → `test result: ok. 42 passed; 0 failed` (task-8-fix1-test.log, EXIT=0).

Residue grep `git grep -n 'claude-auth-probe\|claude_auth_probe\|claude auth status' -- docs crates CLAUDE.md` after the commit:
- `crates/gents-cli/src/cli/args/tests.rs:818` — brief-mandated test literal.
- `docs/design-notes/SPEC-claude-a2b-in-process.md:142` — off-limits file.
- `docs/design-notes/PR-STACK-claude-prod-hardening.md:91` and `docs/design-notes/SPEC-claude-phase6-packaging.md:44` — historical `claude auth status` mentions (a Verify note and a decision-table row) in files outside the three the coordinator allowed for this commit; left untouched and reported. Two lines total if a later round wants them annotated.
