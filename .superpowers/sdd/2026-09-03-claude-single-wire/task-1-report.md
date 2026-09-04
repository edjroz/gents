# Task 1 report: C1 fix — tool_use input accumulation, duplicate ids, overlapping blocks

Status: DONE_WITH_CONCERNS (concerns are pre-existing, out of this task's scope; see below)

Commit: `44908d95` fix(claude): accumulate tool_use input from deltas, fail closed on dup and overlap
Branch: `spike/claude-b3-live-tools` (worked in place, no worktree, no branch switch)

## What was implemented

`crates/gents/src/claude_completer/mod.rs`
- `CompleterParseError` gains `OverlappingToolUse { id: String }` with Display
  `fail-closed: overlapping tool_use block {id}`, placed after `DuplicateToolUseId`.
  No existing Display string changed.

`crates/gents/src/claude_messages.rs`
- `PendingTool` now carries `start_input: Option<String>` (serialized
  `content_block_start.input` when it is an object) and `deltas: String`
  (concatenated `input_json_delta.partial_json`), replacing the single
  `input_json` field that seeded `{}` and appended deltas onto it.
- `PendingTool::arguments_json()` implements the Lean `ClaudeMap.accumulate`
  rule: deltas win when any arrived; else start input; else `{}`.
- `mapped_tool_call(tool, surface, seen_ids)` now:
  1. rejects missing id/name (`MalformedToolUse`, unchanged),
  2. rejects an id already flushed (`DuplicateToolUseId`) via `seen_ids.insert`,
  3. rejects names off the surface (`ToolUse`, unchanged),
  4. parses the accumulated JSON and returns `MalformedToolUse` with message
     `tool_use {id} input is not JSON: {serde error}` instead of silently
     substituting `{}`.
- `parse_messages_sse`: `seen_ids: HashSet<String>` lives next to `pending`; a
  `content_block_start` of type `tool_use` while `pending.is_some()` returns
  `OverlappingToolUse { id }` (written as `if pending.is_some()`, no placeholder
  binding, per the ambiguity resolution); both `mapped_tool_call` call sites
  (on `content_block_stop` and the trailing flush) thread `&mut seen_ids`.
- Tests (in `mod tests`, after `sse_maps_echo_and_rejects_bash`): helpers
  `sse_tool_use_block` and `tool_call_arguments`, plus the six tests from the
  brief, verbatim. Four long lines were wrapped to the form `cargo fmt --check`
  demands; content identical.
- The commit also carries the previously uncommitted identity-block change
  (`CLAUDE_CODE_IDENTITY` as `system[0]` in `build_messages_body`) and its
  three tests, as the brief's Step 7 requires.

## TDD evidence

RED
```
cargo test -p gents --lib claude_messages::tests::sse_ -- --nocapture
test claude_messages::tests::sse_tool_use_with_no_input_at_all_is_empty_object ... ok
test claude_messages::tests::sse_tool_use_without_deltas_uses_start_input ... ok
test claude_messages::tests::sse_maps_echo_and_rejects_bash ... ok
test claude_messages::tests::sse_duplicate_tool_use_id_fails_closed ... FAILED
test claude_messages::tests::sse_tool_use_deltas_yield_exact_arguments ... FAILED
test claude_messages::tests::sse_tool_use_with_unparseable_input_fails_closed ... FAILED
test claude_messages::tests::sse_overlapping_tool_use_block_fails_closed ... FAILED
  assertion `left == right` failed
    left: Object {}
   right: Object {"text": String("hi")}
test result: FAILED. 3 passed; 4 failed; 0 ignored; 0 measured; 1964 filtered out
```
Matches the brief's prediction exactly (deltas test yields `{}`; the three
fail-closed tests return `Ok`).

GREEN
```
cargo test -p gents --lib claude_messages -- --nocapture
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 1958 filtered out; finished in 0.01s
```
All 13 `claude_messages::tests::*` pass, including the six new ones and
`sse_maps_echo_and_rejects_bash`.

## Full gates

Environment for every command:
`GENTS_SKIP_LENS_BUILD=1 GENTS_SKIP_CALLBACK_WASM_BUILD=1 TMPDIR="$PWD/.scratch/tmp"`,
plus `PATH="$HOME/.elan/bin:$PATH"` for the test run (see note 1).

`cargo check --workspace --all-targets` — GREEN
```
warning: `gents-cli` (lib test) generated 1 warning
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 24m 33s
EXIT=0
```
The one warning is pre-existing and untouched by this task:
`field description is never read` at
`crates/gents-cli/src/commands/demo/secscan/matchers.rs:28:9`. No warnings in
either touched file.

`cargo test -p gents` — lib GREEN, conformance 2 pre-existing failures
```
test result: ok. 1969 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 119.50s   (lib)
test result: FAILED. 362 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 220.05s  (conformance)
error: test failed, to rerun pass `-p gents --test conformance`
```
Failing tests:
- `docs::rig_vocabulary_confined_to_the_seam` — flags
  `crates/gents/src/claude_messages.rs:125,129,134,139,159,163,168,183,560`
  and `crates/gents/src/claude_subscription.rs` (rig vocabulary outside the
  `llm::rig_compat` allowlist).
- `prompt_assembly::provider_invocations_are_confined_to_the_owned_loop_seam` —
  `left` has `crates/gents/src/claude_subscription.rs` in addition to the two
  expected seam files.

Attribution (both are pure source-tree string scans, so they are
deterministic on HEAD):
- No added line in this task's diff contains any of the scanned markers
  (`git diff HEAD | grep '^+' | grep -E '<markers>'` → none).
- `git grep` on HEAD (before this commit) already lists `claude_messages.rs`
  and `claude_subscription.rs` outside both allowlists.
- Both violations were introduced by `3643bf7f feat(claude): send tool-capable
  turns on Messages HTTP`, already on this branch.

Per the user's mid-task direction ("if those are standing from before that's
something that can be fixed as a follow up and not part of this lifecycle"),
the commit was made and these two failures are recorded here as a follow-up
for the coordinator, not as a blocker for Task 1.

Note 1 — first run of the full suite showed 69 lib failures, all
`while holding exclusive Lean contract load lock ... failed to build Lean
conformance contract target ... No such file or directory (os error 2)`. Root
cause: `lake` lives in `~/.elan/bin`, which is not on this shell's PATH; one
failed load poisons the shared lock and cascades. Re-running with
`~/.elan/bin` on PATH gave the 1969/0 lib result above. Later tasks should
prepend `$HOME/.elan/bin` to PATH for the test gate.

Formatting: `cargo fmt -p gents --check` reports only three diffs in
`claude_messages.rs` (lines 16, 488, 513), all present in HEAD before this
task; my new lines are rustfmt-clean. `crates/gents/src/lib.rs` was not
formatted or touched.

## Files changed

- `/Users/edjroz/Repos/source/gents/crates/gents/src/claude_messages.rs`
- `/Users/edjroz/Repos/source/gents/crates/gents/src/claude_completer/mod.rs`

Not staged, per constraints: `docs/design-notes/SPEC-claude-a2b-in-process.md`,
`TODO.md`, `tasks/a2b2-interjection-plan.md`, `docs/superpowers/**`.

## Self-review

- Diff matches the brief's code verbatim except: the `let _ = open;`
  placeholder is replaced by `if pending.is_some()` (per the ambiguity
  resolution), and four test lines are wrapped for rustfmt.
- All four frozen Display strings are intact; only `OverlappingToolUse` was
  added, positioned after `DuplicateToolUseId`.
- `seen_ids` is checked after the missing-id/name guard and before the surface
  check, so a duplicate of an off-surface name reports the duplicate (the
  brief's ordering).
- Diff contains no `println`, no `sk-ant`, no `Bearer`; nothing logs a token.
- Identity-block hunks from the prior session are unchanged and included.

## Concerns

1. Two pre-existing conformance failures (`rig_vocabulary_confined_to_the_seam`,
   `provider_invocations_are_confined_to_the_owned_loop_seam`) from `3643bf7f`
   remain on the branch. They will fail the full gate for every subsequent
   task until either the allowlists (and CLAUDE.md's "one seam" claim) are
   updated or the Messages HTTP path is routed through `llm::rig_compat`.
   Follow-up, outside this lifecycle per the user.
2. The test gate requires `~/.elan/bin` on PATH; the default agent shell does
   not have it, which produces a misleading 69-failure cascade.
3. `MalformedToolUse.line` is hard-coded to `0` on the SSE path (as the brief
   specifies; Task 6 drops the field).
