# Task 5 report: Lean slice — system assembly, stream accumulation, witnesses, re-pointed fence

Status: DONE (intended red present; two pre-existing failures unchanged; one unrelated flake observed and filed as a concern).
Commit: ad97dd5a "spec(claude): model system assembly and SSE tool-block accumulation; fence the Messages parser" (branch spike/claude-b3-live-tools, on top of 8ae310d0).

## What changed per file

- crates/gents/proofs/Proofs/PromptAssembly/ClaudeMap.lean
  - MapError gains `overlappingBlock (id)`; errorName renders it as `overlappingBlock:<id>`.
  - New system-assembly model: `Msg`, `identity`, `splitSystem`, `systemBlocks`, `isSystem`, `toolsField` with theorems `systemBlocks_head`, `systemBlocks_tail_verbatim`, `splitSystem_partition`, `toolsField_empty`, `toolsField_nonempty`.
  - New stream-accumulation model: `StreamEvent`, `accumulate`, `Pending`, `StreamState`, `flush`, `step`, `runStream` with theorems `accumulate_ignores_start_when_streamed`, `accumulate_uses_start_when_no_deltas`, `runStream_text_only`, `runStream_deltas_win`, `runStream_start_input_without_deltas`, `runStream_overlap`, `runStream_duplicate`, `runStream_unterminated_flushes`.
- crates/gents/proofs/Proofs/Conformance/ContractCases/PromptAssembly.lean
  - `surfaceOf` is now `names.toFinset`.
  - New `PromptAssemblyClaudeBodyCase` + `promptAssemblyClaudeBodyCases` (6 cases) and `PromptAssemblyClaudeStreamCase` + `promptAssemblyClaudeStreamCases` (10 cases), computed by `systemBlocks`/`splitSystem`/`toolsField` and `runStream` respectively.
- crates/gents/proofs/Proofs/Conformance/Contracts/Json/PromptAssembly.lean: `promptAssemblyClaudeBodyCase(s)Json`, `promptAssemblyClaudeStreamCase(s)Json`.
- crates/gents/proofs/Proofs/Conformance/Contracts/Json/Snapshot.lean: keys `prompt_assembly_claude_body_cases`, `prompt_assembly_claude_stream_cases` after the map key.
- crates/gents/proofs/Proofs/Conformance/CoverageLedger.lean: consumer id renamed to `...generated_claude_map_cases_drive_the_messages_parser`; two new rows for `PromptAssemblyClaudeBodyCases` / `PromptAssemblyClaudeStreamCases`.
- crates/gents/src/lean_vocab_test/prompt_assembly.rs: `LeanPromptAssemblyClaudeBodyCase`, `LeanPromptAssemblyClaudeStreamCase`.
- crates/gents/src/lean_vocab_test/support.rs: two snapshot fields (`#[serde(default)]`) and two accessors.
- crates/gents/tests/conformance/coverage.rs: two emitted-set blocks.
- crates/gents/tests/support/conformance_consumers.rs: renamed entry + two new registry entries.
- crates/gents/tests/conformance/prompt_assembly.rs: imports switched from `claude_completer` to `claude_messages::{CLAUDE_CODE_IDENTITY, build_messages_body, parse_messages_sse}` + `rig::streaming::RawStreamingChoice` + `HashSet`; map driver re-pointed at `parse_messages_sse` (`claude_map_assistant_line` replaced by `sse_event`/`claude_map_blocks_as_sse`); new drivers `generated_claude_stream_cases_drive_the_messages_parser` (+ `claude_stream_events_as_sse`) and `generated_claude_body_cases_drive_the_body_builder`. File rustfmt'd.
- crates/gents/src/claude_messages.rs: test `identity_matches_lean_body_witness_head` in `mod tests`. (`parse_messages_sse`, `build_messages_body`, `CLAUDE_CODE_IDENTITY` were already `pub`.)

## Deviations from the brief's Lean text

1. `partial` is a Lean keyword. The `StreamEvent.delta` binder is `fragment` instead of `partial` (constructor name and tag string `delta:<...>` unchanged) in ClaudeMap.lean (`| delta (fragment : String)`, `step`'s `.delta fragment` arm) and in ContractCases `streamEventTag`.
2. `Except` has no `DecidableEq` instance in this Mathlib pin, so the six `native_decide` theorems failed to synthesize `Decidable`. Added a local instance `instDecidableEqExcept {ε α} [DecidableEq ε] [DecidableEq α] : DecidableEq (Except ε α)` (four-arm match using `Except.error.inj`/`Except.ok.inj` and `nofun`) immediately before `runStream_text_only`. The theorems then close with `native_decide` exactly as written (`native_decide` is already used in five other proof files).
3. `splitSystem_partition` closed with the brief's `simp` proof as written — no split needed.

## lake build

Tail of `.superpowers/sdd/2026-09-03-claude-single-wire/task-5-lake.log`:
```
✔ [1021/1022] Built Proofs
Build completed successfully.
EXIT=0
EXIT=0
```
Sorry scan: `grep -n sorry` over ClaudeMap.lean and ContractCases/PromptAssembly.lean → no matches (exit 1); `declaration uses 'sorry'` count in the build log: 0.

## Snapshot keys

`lake env lean --run Proofs/Conformance/Contracts.lean | grep -o '"prompt_assembly_claude_[a-z_]*_cases"'`:
```
"prompt_assembly_claude_map_cases"
"prompt_assembly_claude_body_cases"
"prompt_assembly_claude_stream_cases"
```

## Focused driver results (Step 11)

`cargo test -p gents --test conformance -- prompt_assembly::generated_claude`:
- generated_claude_map_cases_drive_the_messages_parser ... ok
- generated_claude_stream_cases_drive_the_messages_parser ... ok
- generated_claude_body_cases_drive_the_body_builder ... FAILED (intended red) — panics on the first system-row case:
  `system blocks (system-rows-follow-preamble)` left `[identity, "P"]`, right `[identity, "P", "S1", "S2"]`. The loop panics at the first failing case, so `system-rows-without-preamble` (same defect) is not reached; the four non-system-row cases precede it and pass.

`cargo test -p gents --lib claude_messages`: 15 passed, 0 failed, including `identity_matches_lean_body_witness_head ... ok`.

## Full gate (Step 12)

`cargo test -p gents` (`.superpowers/sdd/2026-09-03-claude-single-wire/task-5-gate-test.log`):
- lib: `test result: ok. 1967 passed; 0 failed; 2 ignored`
- conformance: `test result: FAILED. 363 passed; 3 failed; 0 ignored` — failing: `docs::rig_vocabulary_confined_to_the_seam` (pre-existing), `prompt_assembly::provider_invocations_are_confined_to_the_owned_loop_seam` (pre-existing), `prompt_assembly::generated_claude_body_cases_drive_the_body_builder` (intended red). EXIT=101.
- cargo stops after a failing binary, so the remaining binaries were run separately (`.superpowers/sdd/2026-09-03-claude-single-wire/task-5-gate-test-rest.log`, `--no-fail-fast`; `e2e_live` is feature-gated on `live-e2e` and was skipped):
  - e2e_lifecycle: `test result: ok. 44 passed`
  - e2e_runtime: `test result: FAILED. 56 passed; 1 failed; 1 ignored` — `completion_retry_tape::deadline_tight_fails_cleanly` ("deadline overshoot should prevent a second provider call", left 0 right 1). Reran in isolation twice: ok both times (1.5s each). Timing flake, unrelated to this slice.
  - e2e_subagent: `test result: ok. 107 passed`
  - e2e_triggers: `test result: ok. 8 passed`
  - misc: `test result: ok. 27 passed`

`cargo check --workspace --all-targets` (`.superpowers/sdd/2026-09-03-claude-single-wire/task-5-gate-check.log`): `Finished dev profile in 34.81s`, EXIT=0. Only warnings: the known `gents-cli` (lib test) `field description is never read`, plus the two GENTS_SKIP_* stub notices.

## Self-review

- Zero sorry; all brief-named Lean names exist with the brief's signatures (modulo binder rename).
- Ledger rows ↔ coverage.rs emitted set ↔ consumer registry are 1:1 for the three Claude witness sets; `structure::every_lean_model_has_a_declared_conformance_home` and the coverage tests pass in the gate.
- Witness/driver tag contracts match: `start:<id>:<name>:<input-or-empty>` uses `splitn(3, ':')` so JSON inputs containing `:` survive; stream `calls` compare canonical `serde_json::Value` renderings on both sides.
- No token printing, no println, lib.rs untouched, docs/ TODO.md tasks/ untouched, docs/superpowers not staged. Staged exactly the four paths; `docs/design-notes/SPEC-claude-a2b-in-process.md` (pre-existing modification) left unstaged.

## Concerns

1. `docs::rig_vocabulary_confined_to_the_seam` was already failing on `crates/gents/src/claude_messages.rs:127`; the brief-verbatim body driver adds two more marker hits (`use rig::completion::message::…` and `use rig::one_or_many::OneOrMany` at prompt_assembly.rs:562-563). Status unchanged, but whoever clears that fence must either allowlist the conformance driver or route the request construction through `llm::rig_compat`.
2. `e2e_runtime::completion_retry_tape::deadline_tight_fails_cleanly` flaked once in the full run (0 vs 1 provider calls), green on two isolated reruns. Per CLAUDE.md flaky tests are defects — worth filing; not touched here.
3. The body driver's intended red stops at the first failing case; Task 6 should see both `system-rows-follow-preamble` and `system-rows-without-preamble` turn green together.
