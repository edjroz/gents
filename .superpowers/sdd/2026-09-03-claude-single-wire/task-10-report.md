# Task 10 report: retire the server-side Claude write gate

Branch `spike/claude-b3-live-tools`, base `eaf5c1b2`.
Commit A (runtime, `crates/gents/**`): `07189d37` — feat(claude): installed seat is the live opt-in; retire the server write gate
Commit B (CLI + docs): `25b6a986` — feat(cli): drop gents server --claude-write-approved; document the seat opt-in

Commit A alone was not compiled in isolation: `crates/gents-cli` still calls the two-argument
`ClaudeSeatConfig::new` at that SHA, so A alone is expected not to build gents-cli. Gates ran on A+B.

## Per item

1. `crates/gents/src/claude_subscription.rs` — `ClaudeSeatConfig { config_dir, http }`, `new(config_dir: PathBuf)`;
   module doc now ends "Live sends happen whenever a seat is installed; a seat whose token cannot be read refuses
   closed at the token read."; `install_fake_seat()` calls `::new(config_dir)` (doc comment updated to say why it refuses).
2. `crates/gents/src/claude_messages.rs` `stream_messages` — the `!seat.write_approved` refusal block deleted; live-send
   log reworded to "live Claude Messages HTTP send (seat installed; this process bills the Claude subscription)".
3. `crates/gents/src/claude_subscription/tests.rs` — `live_path_refuses_without_write_approval` renamed to
   `live_path_refuses_without_a_readable_seat`; it and `fixture_queue_serves_one_body_per_call_then_refuses` now
   assert (via a shared `assert_seat_auth_refusal` helper) that the error contains `Claude Messages seat auth:` and
   `credentials file missing`. Note: no test named `messages_http_without_write_approval_is_refused` existed on the
   branch (only the two above referenced the flag), so there was nothing to rename there. Fixture tests untouched.
4. `crates/gents/src/backend_health.rs` test — `ClaudeSeatConfig::new(config_dir)`.
5. `git grep -n 'write_approved' -- crates/gents` → empty.
6. `crates/gents-cli/src/cli/args.rs` — `ServeArgs.claude_write_approved` and its `#[arg]` removed; `--claude-config-dir`
   help ends with the required sentence. `ClaudeLoginArgs` untouched.
7. `crates/gents-cli/src/cli/args/tests.rs` — `server_parses_a2b_claude_seat_flags` drops the flag/asserts;
   `claude_write_gate_help_is_opt_in_not_prod_default` repointed as `claude_seat_help_names_the_config_dir_opt_in`
   (asserts the server help no longer mentions `--claude-write-approved` and carries the new sentence; keeps the
   `claude-login` "Off by default" assertion). `claude_login_*` tests untouched.
8. `crates/gents-cli/src/commands/serve.rs` — `ClaudeSeatConfig::new(config_dir)`; single startup
   `tracing::info!` with the required wording; status JSON `claude_subscription` = `{seat_installed, config_dir}`;
   `a2b_claude_config_dir_installs_seat_flags` asserts `config_dir` only; the orphan-flag test became
   `claude_server_write_gate_flag_is_gone` (parse of `gents server --claude-write-approved` is `Err`);
   `install_claude_subscription_seat_from_server_flags` drops the `write_approved` assert.
9. `docs/backends.md` — table row (line 23), the §"Live Claude" paragraph, the server command blocks, and both failure
   rows rewritten per the decision; the `claude-login` line keeps its own `--claude-write-approved` and now says the
   server has no such flag.
10. `docs/design-notes/PR-STACK-claude-track-b-tools.md:96` and `SPEC-claude-a2c-tool-bridging.md:80,90,219,245`
    annotated "[Retired 2026-09-04: …]". Scope addition: the same annotation was added to
    `PR-STACK-claude-prod-hardening.md:99` and `SPEC-claude-a2a-unified-suite.md:164` (both mention the server flag
    and were not on the untouchable list), so the grep residue is only the login flag, the forbidden SPEC-a2b file,
    and the two negative-assertion tests. Historical text kept everywhere.
11. Untracked (not `git add`ed): `docs/superpowers/specs/2026-09-03-claude-single-wire-design.md` §2 diagram line →
    "seat installed → live; token unreadable → refuse closed"; §5 `gents serve` bullet → "`--claude-config-dir` only
    (the write gate was retired 2026-09-04)". `docs/superpowers/plans/2026-09-03-claude-single-wire.md` "Naming
    deviations" list gained the 2026-09-04 line.
12. `CLAUDE.md` — does not mention the flag; unchanged.

## TDD evidence (renamed tests)

The test assertion change and the runtime change were applied as one mechanical edit, so no separate red run was
recorded. The old assertion (`contains("--claude-write-approved")`) cannot pass on the new tree: that string no
longer exists anywhere in `crates/gents` (item 5 grep). Green run, from `task-10-gate-test.log`:

    test claude_subscription::tests::fixture_queue_serves_one_body_per_call_then_refuses ... ok
    test claude_subscription::tests::live_path_refuses_without_a_readable_seat ... ok

and from `task-10-gate-cli.log`:

    test cli::args::tests::claude_seat_help_names_the_config_dir_opt_in ... ok
    test cli::args::tests::server_parses_a2b_claude_seat_flags ... ok
    test commands::serve::shim_host_tests::claude_server_write_gate_flag_is_gone ... ok
    test commands::serve::shim_host_tests::a2b_claude_config_dir_installs_seat_flags ... ok

## Gates (tree = A+B; logs in this directory)

- `cargo test -p gents --no-fail-fast` → EXIT=0; results: 1943/366/44/57/107/8/27/0 passed, 0 failed
  (`task-10-gate-test.log`). `completion_retry_tape::deadline_tight_fails_cleanly ... ok` on the first run; no rerun needed.
- `env -u TMPDIR cargo test -p gents-cli` → EXIT=0; all `test result: ok`, 0 failed; no busy-port rerun needed
  (`task-10-gate-cli.log`).
- `cargo check --workspace --all-targets` → EXIT=0; warnings: only the two `GENTS_SKIP_*` build-script notices and the
  known `matchers.rs` `field \`description\` is never read` (`task-10-gate-check.log`).
- `rustup run 1.97.1 cargo fmt --all --check` → exit 0.

## Final grep: `git grep -n 'write_approved\|claude-write-approved' -- crates docs CLAUDE.md`

Residue beyond the allowed set:
- `crates/gents-cli/src/cli/args/tests.rs:887` and `crates/gents-cli/src/commands/serve.rs:1438` — negative assertions
  that the server flag is gone (the literal is needed to assert its absence).
- `docs/design-notes/SPEC-claude-a2b-in-process.md:36,51,128,141,173,178,185,202,223,231` — file explicitly off-limits
  for this task (pre-existing local modifications; left untouched).
Allowed set present: `args.rs:90,539` (ClaudeLoginArgs), `claude_login.rs` (10 lines), `backends.md:308-309` (login
line), and the bracketed "[Retired 2026-09-04 …]" annotations in `PR-STACK-claude-track-b-tools.md:96`,
`SPEC-claude-a2c-tool-bridging.md:80,90,219,245`, `PR-STACK-claude-prod-hardening.md:99`,
`SPEC-claude-a2a-unified-suite.md:164`.
