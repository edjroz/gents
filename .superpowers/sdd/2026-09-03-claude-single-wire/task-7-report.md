# Task 7 report: seat auth without Security.framework; health probes the token; promotion

Branch `spike/claude-b3-live-tools`, base HEAD `39bacaf1`. Steps 1-7 only (Step 8 dispatched separately).

## Per step

**Step 1-2 (RED).** Replaced the `claude_seat_auth.rs` test module with the brief's tests (plus one extra: `missing_access_token_field_is_malformed`, covering the blank-token path that the old `empty_access_token_is_missing` covered). The digest test uses `format!("{:x}", ...)` because `hex` is not a `gents` dependency (the brief allows this). RED: `cargo test -p gents --lib claude_seat_auth` failed to compile with exactly the expected errors: `no method named source`, `cannot find type SeatTokenSource`, `no method named expires_at`, `no variant named Malformed` (x2).

**Step 3 (implement).** `crates/gents/src/claude_seat_auth.rs`:
- Enum is the brief's seven-variant `SeatAuthError` verbatim; `SeatTokenSource { File, Keychain }`; `SeatAccessToken { token, source, expires_at }` with `authorization_value()`, `source()`, `expires_at()`, redacted `Debug` via `finish_non_exhaustive`. Dropped `as_str()` (only the old test used it; `claude_messages.rs` uses `authorization_value()`). Kept `#[derive(Clone)]`.
- `read_credentials_file`: `NotFound` -> `MissingFile`, other io -> `Io { path, message }`.
- `parse_credentials(raw, now_millis, source)`: JSON failure / no `claudeAiOauth` / blank token -> `Malformed { reason }` (reasons are fixed strings; serde's error text is never echoed because it can quote file bytes). Expiry -> `Expired { expires_at: rfc3339 }` via `DateTime::<Utc>::from_timestamp_millis`, falling back to the raw millis. `expiresAt <= 0` is treated as "no expiry" (matches previous behavior).
- `read_seat_access_token_at` is the brief's reader verbatim; `KeychainNotFound` folds back to the file error.
- Keychain: `read_macos_keychain_via_framework` and `ERR_SEC_ITEM_NOT_FOUND` deleted from this module; `read_macos_keychain_credentials` calls `security(1)` directly. Exit 44 -> `KeychainNotFound` (verified on this machine: `security find-generic-password` for a nonexistent item exits 44), other non-zero -> `KeychainAccessDenied`, non-UTF-8 -> `Malformed { reason: "keychain payload is not UTF-8" }`. Non-macOS returns `KeychainNotFound { service }`. Added `stdin(null)` to the `security` spawn so a headless server can never block on it.
- `macos_keychain_service_name` now resolves a relative path against `current_dir()` before hashing (the brief's test requires it); absolute paths hash identically to before.

**Step 4 (seat and health).** `claude_subscription.rs`: `ClaudeSeatConfig { config_dir, write_approved, http }`, `::new(config_dir, write_approved)`; `probe_seat_auth_status` deleted; `probe_process_seat_health()` is the brief's synchronous token read (imports shortened to `SeatAuthError`/`SeatTokenSource`/`read_seat_access_token`); dropped the now-unused `Stdio`/`tokio::process::Command`/`claude_completer` imports. `install_fake_seat` updated. `claude_completer/mod.rs`: `parse_auth_status_logged_in` and its test deleted (`git grep` showed no other caller; the CLI `claude_auth_probe.rs` has its own parser); `serde_json::Value` import dropped; `DEFAULT_MODEL_ID`, `STRIPPED_ENV_VARS`, `sanitize_child_env` kept. `backend_health.rs`: Claude branch is the brief's code (no timeout wrapper since the probe is a synchronous file read); `record_probe_event` is seven-arg, `#[allow(clippy::too_many_arguments)]` removed, promotion condition is `ProbeSuccess && probe_status == UNKNOWN_PROBE_STATUS`; HTTP call site updated.

**Step 5 (health tests).** Replaced `write_auth_status_fake`, `install_claude_seat`, and the four Claude tests with the brief's `install_claude_seat_with_credentials` and four tests (`cycle_does_not_http_probe_claude_cli_subscription_backends`, `cycle_demotes_claude_after_k_expired_probes_with_login_hint`, `cycle_marks_claude_healthy_and_promotes_unknown_document`, `cycle_fails_claude_when_credentials_are_missing`). Kept the old tests' extra `failure_count`/`flipped`/`measured_blocks_routing` assertions and added `!err.contains("sk-ant")` on the probe error.

**Step 6 (CLI).** `serve.rs`: two-arg `ClaudeSeatConfig::new`; `--claude-bin` assertion removed from `claude_seat_orphan_flags_require_config_dir` and its doc comment. `args.rs`: `ServeArgs.claude_bin` removed. `args/tests.rs`: `server_parses_a2b_claude_seat_flags` no longer passes/asserts `--claude-bin`. `ClaudeLoginArgs.claude_bin` and `ClaudeAuthProbeArgs` untouched. `git grep claude_bin -- crates/gents-cli/src/commands/serve.rs` prints nothing.

**Step 7.** Gates below; committed.

## Deviations

1. **`security-framework` dependency kept (brief Step 3 / Cargo.toml L83-85).** The crate is pre-existing on `main` (added in `c5c41abb`) and used by `crates/gents/src/identity.rs` for Secure Enclave identity keys (`SecKeychain`, `SecKey`, `ItemSearchOptions`, ...). Deleting the table breaks `identity.rs` with 11 compile errors. The substance of the task -- seat auth no longer uses Security.framework -- is done; only the unrelated identity use remains. `Cargo.lock` is therefore unchanged and not staged.
2. **Residue scan** consequently hits `identity.rs` and the Cargo table (pre-existing `ERR_SEC_ITEM_NOT_FOUND` const and `security_framework` paths there). With those two files excluded the scan prints nothing.
3. **Flaky-test fix in `serve.rs` tests.** `a2b_claude_config_dir_installs_seat_flags` and `install_claude_subscription_seat_from_server_flags` both install the process-global seat with no serialization, and the first gate run lost the race (assertion on `seat.config_dir` saw the other test's tempdir). Added a static-mutex `lock_process_seat()` guard to both tests (same pattern as `gents::claude_subscription::lock_process_seat_for_test`). Rerun: 668/668.
4. `hex` is not a `gents` dependency; the digest test formats with `{:x}` (brief permits).
5. Added `missing_access_token_field_is_malformed` seat-auth test to keep blank-token coverage.
6. rustfmt of `claude_subscription.rs` also reformatted pre-existing code in `claude_subscription/tests.rs` (via `#[path]`); reverted that file so the diff stays in scope.

## TDD evidence

- RED (seat auth): compile errors E0599 `source`, E0433 `SeatTokenSource`, E0599 `expires_at`, E0599 `Malformed` x2.
- GREEN (seat auth): `cargo test -p gents --lib claude_seat_auth` -> `test result: ok. 7 passed; 0 failed`.
- Health tests were written together with the Step 4 implementation (the old tests could not compile against the new `ClaudeSeatConfig::new` arity, so there was no meaningful intermediate RED for them). GREEN: `cargo test -p gents --lib backend_health` -> `test result: ok. 11 passed; 0 failed` (all four Claude cases listed `ok`).
- `cargo test -p gents --lib claude_` -> `test result: ok. 39 passed; 0 failed`.

## Gates

`cargo test -p gents --no-fail-fast` (`task-7-gate-test.log`, EXIT=0):
- lib: `test result: ok. 1939 passed; 0 failed; 2 ignored`
- conformance / e2e_lifecycle 44 / e2e_runtime 57 (1 ignored) / e2e_subagent 107 / e2e_triggers 8 / misc 27 / doctests 0: all `ok`, 0 failed. `deadline_tight_fails_cleanly` passed first time.

`cargo test -p gents-cli --no-fail-fast` (`task-7-gate-cli.log`, EXIT=101 on the first run):
- lib: `FAILED. 667 passed; 1 failed` -- `commands::serve::shim_host_tests::a2b_claude_config_dir_installs_seat_flags` (the race in Deviation 3).
- all integration binaries `ok` (31/8 ignored, 28, 5, 0, 3, 27, 16, 37, 14, 13); codex-shim cases passed, no port collision.
- Rerun after the mutex fix (`task-7-gate-cli-lib-rerun.log`): `test result: ok. 668 passed; 0 failed`, EXIT=0.

`cargo check --workspace --all-targets` (`task-7-gate-check.log`, rerun after the serve.rs fix, EXIT=0): `warning: field description is never read --> crates/gents-cli/src/commands/demo/secscan/matchers.rs:28:9` (known), `Finished dev profile`.

Residue scan (brief exact): hits only `crates/gents/Cargo.toml:84` and `crates/gents/src/identity.rs` (pre-existing Secure Enclave code). Excluding those two files: prints nothing (`exit=1`).

Token scan: `grep -l 'sk-ant\|Bearer'` over all `task-7-gate-*.log` -> no files. Test-source literals `sk-ant-oat01-TEST`/`TESTTOKEN` exist only inside test code and are never printed.

rustfmt `--check` on all seven touched files: clean. `crates/gents/src/lib.rs` untouched.

## Self-review

- `Debug` for `SeatAccessToken` prints `source`/`expires_at` only; no `Display`; `authorization_value` is the sole way to read the token and is only called by `claude_messages.rs`.
- Health probe never spawns anything and never refreshes; the Keychain path is reached only when the file is absent, and `security` gets `stdin(null)`.
- `Expired`/`MissingFile` carry the `gents claude-login --config-dir <dir>` hint; other errors are Display only.
- Promotion: `record_probe_event` no longer distinguishes provider kinds; the `cycle_marks_claude_healthy_and_promotes_unknown_document` test pins C9.
- `tracing::debug!` for the healthy probe logs `detail` (`source=... expires_at=...`) only -- no token.

## Concerns

- `security(1)` exit 44 for `errSecItemNotFound` was verified on this macOS (Darwin 24.4); other macOS releases are assumed to agree. Any other non-zero status maps to `KeychainAccessDenied`, which is fail-closed.
- `macos_keychain_service_name` on a relative `config_dir` now depends on `current_dir()`; `serve.rs` passes the path as given by the operator, so a relative `--claude-config-dir` hashes to whatever the server's cwd was. Previously it hashed the relative bytes, which could never match the Claude CLI's Keychain item anyway, so this is strictly better but worth noting for Task 8.
- The brief's residue scan cannot print nothing while `identity.rs` uses Security.framework; if the dependency is meant to go entirely, that is a separate (identity) task.
