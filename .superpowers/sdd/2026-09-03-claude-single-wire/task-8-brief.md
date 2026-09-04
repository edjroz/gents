### Task 8: CLI and docs trim; ponytail residue

**Files:**
- Modify: `crates/gents-cli/src/cli/args.rs:93-98` (`ClaudeAuthProbe` variant), `:546-566` (`console`, `sso`, `ClaudeAuthProbeArgs`)
- Modify: `crates/gents-cli/src/cli/args/tests.rs:784-835` (`claude_login_and_auth_probe_parse_and_require_config_dir` → `claude_login_parses_and_requires_config_dir`)
- Delete: `crates/gents-cli/src/commands/claude_auth_probe.rs`
- Modify: `crates/gents-cli/src/commands/mod.rs:3`, `crates/gents-cli/src/lib.rs:451-453`, `crates/gents-cli/src/commands/claude_login.rs:50-100`
- Modify: `crates/gents/src/backend_provider.rs:58-64,89-93` and its parse tests
- Modify: `docs/design-notes/PR-STACK-claude-track-b-tools.md`, `docs/design-notes/SPEC-claude-a2c-tool-bridging.md`, `crates/gents/proofs/README.md`, `CLAUDE.md` (the "External code is held at arm's length" bullet)

**Interfaces:**
- Produces: `gents claude-login --config-dir <dir> [--claude-bin] [--email] [--dry-run] [--claude-write-approved]`; `BackendProviderKind::parse_optional` accepts `ClaudeCliSubscription`, `claude-cli-subscription`, `claude_cli_subscription` only.

- [ ] **Step 1: Failing tests**

`args/tests.rs`: rename the test; drop its `claude-auth-probe` parse case; add:

```rust
    #[test]
    fn claude_login_rejects_removed_console_and_sso_flags() {
        for flag in ["--console", "--sso"] {
            let parsed = Cli::try_parse_from(["gents", "claude-login", "--config-dir", "/tmp/x", flag]);
            assert!(parsed.is_err(), "{flag} must be gone");
        }
        assert!(Cli::try_parse_from(["gents", "claude-auth-probe", "--config-dir", "/tmp/x"]).is_err());
    }
```

`backend_provider.rs` tests:

```rust
    #[test]
    fn claude_max_cli_spellings_are_not_accepted() {
        for spelling in ["claude-max-cli", "claude_max_cli"] {
            assert!(BackendProviderKind::parse_optional(Some(spelling)).is_err(), "{spelling}");
        }
        assert_eq!(
            BackendProviderKind::parse_optional(Some("claude_cli_subscription")).unwrap(),
            BackendProviderKind::ClaudeCliSubscription
        );
        let parsed: BackendProviderKind = serde_json::from_str("\"claude-max-cli\"").unwrap_or(BackendProviderKind::OpenAiCompatible);
        assert_ne!(parsed, BackendProviderKind::ClaudeCliSubscription, "serde alias must be gone");
    }
```

(Use whatever `Cli` type and `try_parse_from` pattern the neighbouring tests in `args/tests.rs` use.)

- [ ] **Step 2: Run to verify failure**

```bash
cargo test -p gents-cli --lib cli::args::tests::claude_ 2>&1 | tail -8
cargo test -p gents --lib backend_provider 2>&1 | tail -8
```
Expected: the new tests FAIL (flags still parse; spellings still accepted).

- [ ] **Step 3: Implement**

- `args.rs`: delete `Command::ClaudeAuthProbe` + its `#[command]` block, `ClaudeAuthProbeArgs`, and `ClaudeLoginArgs.console` / `.sso`. Reword the `claude-login` `about` to "Sign in to the Claude subscription seat via the Claude CLI (credentials stay in --config-dir; no oat in DefraDB)".
- `commands/mod.rs`: remove `pub(crate) mod claude_auth_probe;`. `lib.rs`: remove the `Command::ClaudeAuthProbe` arm. `git rm crates/gents-cli/src/commands/claude_auth_probe.rs`.
- `claude_login.rs`: `plan_claude_login` always pushes `--claudeai`; delete the `console`/`sso` branches and the mutual-exclusion bail. The post-login `probe` JSON field that called `claude_auth_probe_result_json` is replaced by `"seat": gents::claude_subscription::probe_seat_detail(&config_dir)` where `probe_seat_detail(config_dir: &Path) -> serde_json::Value` is a new `pub fn` in `claude_subscription.rs` returning `{"ok": bool, "detail": String}` built from `read_seat_access_token` exactly as `probe_process_seat_health` formats it (factor the formatting into a shared `fn seat_detail(config_dir: &Path) -> Result<String, String>` used by both).
- `backend_provider.rs`: delete the two `claude-max-cli` / `claude_max_cli` serde aliases and the two `parse_optional` arms; the kind's doc comment becomes "Claude subscription seat over Messages HTTP. Seat truth lives in the process `--claude-config-dir`; the `claude` binary is a login-time dependency only. `is_agent_scoped_oauth()` stays false."

- [ ] **Step 4: Docs**

- `docs/design-notes/PR-STACK-claude-track-b-tools.md`: add a dated status block at the top: B3 done (write requests #7–#10), the two-wire Completer retired on 2026-09-03 in favour of the single Messages wire (link the spec path), B4 still later.
- `docs/design-notes/SPEC-claude-a2c-tool-bridging.md`: in the C2 lock section record: identity block as `system[0]`, Keychain via `security(1)`, single wire, `tools` omitted when empty, two cache breakpoints; mark open questions #1, #3, #5, #9 closed with one line each pointing at the evidence files.
- `crates/gents/proofs/README.md`: in the map table, the ClaudeMap row lists `splitSystem_partition`, `systemBlocks_head`, `systemBlocks_tail_verbatim`, `toolsField_empty`, `accumulate_ignores_start_when_streamed`, `runStream_*`; add one paragraph: "Provider-input assembly for Claude: the body's `system[]` order and tools omission, and SSE tool-block accumulation, are modelled and witnessed; byte framing, usage and headers are Rust-tested."
- `CLAUDE.md`, "External code is held at arm's length" bullet: append the sentence "Claude subscription seats are reached over Anthropic Messages HTTP with the seat's token read from `--claude-config-dir`; the `claude` binary is a login-time dependency only (`gents claude-login`)."
- `.scratch/claude-spike/logs/b3-live-identity-evidence.md`: the Task 2 correction is already there; add a pointer to `b3-live-single-wire-evidence.md`.

- [ ] **Step 5: Gates and commit**

```bash
cargo test -p gents 2>&1 | grep -E 'test result|FAILED' | head
cargo test -p gents-cli 2>&1 | grep -E 'test result|FAILED' | head
cargo check --workspace --all-targets 2>&1 | tail -3
git grep -n 'claude-auth-probe\|claude_auth_probe\|claude-max-cli\|claude_max_cli\|--console\|--sso' -- crates docs/design-notes CLAUDE.md ; echo "residue exit=$?"
git add crates/gents-cli crates/gents/src/backend_provider.rs crates/gents/src/claude_subscription.rs crates/gents/proofs/README.md docs/design-notes/PR-STACK-claude-track-b-tools.md docs/design-notes/SPEC-claude-a2c-tool-bridging.md CLAUDE.md
git commit -m "chore(claude): drop claude-auth-probe, --console/--sso, claude-max-cli spellings; document the single wire"
```
`docs/design-notes/SPEC-claude-a2b-in-process.md` stays unstaged (pre-existing dirty leftover).


#### 8.x Ponytail residue folded in from the task reviews (controller rulings)

Do these in the same commit; each is small and was deferred to this task by an earlier review. None changes wire behaviour.

- [ ] `crates/gents/src/claude_messages.rs` `stream_sse_body`: `state` is no longer conditionally taken — make it a plain `MessagesSseState` (drop the `Option`, the `let Some(current) = state.as_mut() else { return; }` guard and the `state.take()`), keeping the "yield Err then return" behaviour.
- [ ] `crates/gents/src/claude_messages.rs` `stream_messages` non-2xx path: read up to 512 bytes of the response body (`String::from_utf8_lossy`, trimmed) and append it to the `ProviderError` as ` body={prefix}` so live 4xx failures (e.g. Anthropic's `error.type`/`error.message`) are diagnosable. Never include request headers. Add a unit test that installs a fixture-less seat with `write_approved: true` only if you can stub the transport; otherwise cover the formatting with a small pure helper `fn non_success_error(status: StatusCode, request_id: Option<&str>, body_prefix: &str) -> CompletionError` and test that.
- [ ] `crates/gents/src/claude_messages.rs` `MessagesSseState::dispatch_pending_data`: on a `data:` payload that is not JSON, emit `tracing::debug!(len = raw.len(), "claude messages: ignoring non-JSON SSE payload")` (never the payload text) before returning `Ok(vec![])`.
- [ ] `crates/gents/src/claude_subscription.rs` `impl CompletionModel::completion`: add a doc comment stating the path is text-only (the owned loop uses `stream()`; a tool-only turn here surfaces as the empty-text error) — no behaviour change.
- [ ] `crates/gents/src/llm/rig_compat.rs`: remove the `#[cfg_attr(not(test), allow(dead_code))]` from `from_rig_message` (it is production code now).
- [ ] `crates/gents/src/claude_seat_auth.rs` `read_seat_access_token_at`: fold `KeychainAccountUnset` into the file error like `KeychainNotFound` (so an env-scrubbed CI runner without `USER`/`LOGNAME` reports the credentials path, not the account); update the doc comment; the existing `missing_file_and_missing_keychain_item_is_missing_file` test covers it.
- [ ] `crates/gents/proofs/Proofs/PromptAssembly/ClaudeMap.lean`: make the `DecidableEq (Except ε α)` instance `local instance` (or `private`), and add one docstring sentence on `systemBlocks`/`splitSystem` noting that Rust trims a whitespace-only preamble and drops blank `System` rows, which the model does not represent (no witness covers it). `lake build` must stay green with zero `sorry`.
- [ ] `crates/gents/src/claude_messages/tests.rs` `messages_body_omits_tools_key_when_surface_is_empty`: also assert `with_tools["tools"].as_array().map(Vec::len) == Some(1)`.
- [ ] `CLAUDE.md` "Sharp edges": (1) amend the worktree bullet's warm-up sentence: `cargo build --tests` does NOT warm `cargo test` here because the workspace `[profile.test]` override builds different artifacts — warm with `cargo test -p <crate> --no-run`. (2) Add one bullet: the build-env convention `TMPDIR="$PWD/.scratch/tmp"` puts temp dirs inside the git repo, which breaks tests that assume a non-git temp dir (`gents-cli` codex-shim `thread_metadata`); run `cargo test -p gents-cli` with the system `TMPDIR`.
- [ ] Record in the report that `MESSAGES_BODY_ALLOWED_KEYS` survives as a test-only allowlist in `claude_messages/tests.rs` (intentional; the production debug_assert is gone).
