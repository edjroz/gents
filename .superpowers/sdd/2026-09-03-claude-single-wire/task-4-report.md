# Task 4 report — code half (Steps 1–5 only)

Branch `spike/claude-b3-live-tools`, base `bcc830a3`, commit `8ae310d0`
`feat(claude): route every turn over Messages HTTP; omit tools when empty`

Hard stop honored: no write-request file, no server, no `gents chat`, nothing sent to Anthropic.

## What changed

- `crates/gents/src/claude_messages.rs`
  - `build_messages_body`: the body is now built without a `tools` key and `body["tools"] = Value::Array(tools)` is set only when the surface is non-empty (Lean `ClaudeMap.toolsField`; Task 5 witness `emptyTools`). Nothing else in the body changed; the `messages_body_has_only_allowed_keys` debug_assert still guards it.
  - `mod tests`: appended `messages_body_omits_tools_key_when_surface_is_empty` after the `sse_*` tests, verbatim from the brief.
- `crates/gents/src/claude_subscription.rs`
  - `ClaudeSubscriptionModel::stream`: routing predicate `!request.tools.is_empty() && seat.fake_completer.is_none()` -> `seat.fake_completer.is_none()`. Every real turn (text-only included, so title generation too) now goes over Messages HTTP; the fake-completer branch survives until Task 6.

## TDD evidence

RED — `cargo test -p gents --lib claude_messages::tests::messages_body_omits_tools_key_when_surface_is_empty` (log: `task-4-red.log`)
```
test claude_messages::tests::messages_body_omits_tools_key_when_surface_is_empty ... FAILED
thread '...' panicked at crates/gents/src/claude_messages.rs:842:9   (body carried "tools":[])
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 1967 filtered out
EXIT=101
```

GREEN — `cargo test -p gents --lib claude_` (log: `task-4-green.log`)
```
test claude_messages::tests::messages_body_omits_tools_key_when_surface_is_empty ... ok
test claude_subscription::tests::live_path_refuses_without_write_approval ... ok
test result: ok. 61 passed; 0 failed; 0 ignored; 0 measured; 1907 filtered out; finished in 4.04s
EXIT=0
```

## Gates

`cargo test -p gents` (log: `task-4-gate-test.log`)
```
Running unittests src/lib.rs
test result: ok. 1966 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 126.80s
Running tests/conformance.rs
test result: FAILED. 362 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 242.82s
EXIT=101
```
Failing tests (exactly the two expected pre-existing ones):
- `docs::rig_vocabulary_confined_to_the_seam` — reports rig vocabulary at `claude_messages.rs:127/131/136` (the SSE/`RawStreamingChoice` code, not touched by this diff; only shifted +2 lines by the body edit).
- `prompt_assembly::provider_invocations_are_confined_to_the_owned_loop_seam` — `claude_subscription.rs` appears in the provider-invocation set; the Messages call site existed before this task, the predicate change only widened when it runs.

Because cargo stops after the first failing test binary, the targets after `conformance` never ran in that invocation. I ran them separately (log: `task-4-gate-test-rest.log`), `cargo test -p gents --no-fail-fast --test e2e_lifecycle --test e2e_runtime --test e2e_subagent --test e2e_triggers --test misc`:
```
e2e_lifecycle: test result: ok. 44 passed; 0 failed; 0 ignored
e2e_runtime:   test result: ok. 57 passed; 0 failed; 1 ignored
e2e_subagent:  test result: ok. 107 passed; 0 failed; 0 ignored
e2e_triggers:  test result: ok. 8 passed; 0 failed; 0 ignored
misc:          test result: ok. 27 passed; 0 failed; 0 ignored
EXIT=0
```
`e2e_live` requires the `live-e2e` feature and is not part of the plain `cargo test -p gents` gate; not run (it would also be a live call, which is out of scope here).

`cargo check --workspace --all-targets` (log: `task-4-gate-check.log`) — tail:
```
    Checking gents-desktop-tauri v0.14.0 (...)
    Checking gents-fixture-host v0.14.0 (...)
warning: `gents-cli` (lib test) generated 1 warning     <- known: field `description` never read, commands/demo/secscan/matchers.rs:28
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 16.88s
EXIT=0
```

## Files changed (committed)

- `/Users/edjroz/Repos/source/gents/crates/gents/src/claude_messages.rs` (+15/-2)
- `/Users/edjroz/Repos/source/gents/crates/gents/src/claude_subscription.rs` (+1/-1)

Not touched / not staged: `docs/`, `TODO.md`, `tasks/`, `docs/superpowers/`, `crates/gents/src/lib.rs`.

## Self-review

- Code matches the brief verbatim; `Value` was already imported in `claude_messages.rs`, so no import changes.
- Formatting of the new lines matches the surrounding style. (`rustfmt --check` reports only pre-existing diffs elsewhere in both files and in `agent/loop_stream/tests/mod.rs` — import ordering and trait-bound indentation — so I did not run `cargo fmt`, per the lib.rs constraint.)
- Token scan over every `task-4-*.log`: no `sk-ant` / `Bearer ` matches. `.credentials.json` not read.
- `live_path_refuses_without_write_approval` now exercises the Messages-path refusal, as the brief predicted, and still passes.

## Concerns

- None blocking. The two conformance failures are the expected pre-existing ones; no new failures anywhere. The live half (Step 6+, write request #9) is untouched and needs its own approval.
