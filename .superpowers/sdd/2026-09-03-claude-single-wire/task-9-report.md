# Task 9 report: PR stack carved off `main` (re-run)

Status: DONE. Four stacked branches, each green on its own gates. Nothing pushed, no PRs opened, tag and worktree left in place.

- Spike tip / tag: `spike/claude-b3-live-tools` = `spike-final-2026-09-04` = `9a92c586dd1ed35b2e0e75312c8a8d009dd833c6` (tag pre-existing; not re-tagged).
- Base: `main` = `ca615b1c59999f52bf061ecb53c46b6ab8ffda03`.
- Carve worktree: `/Users/edjroz/Repos/source/gents/.scratch/wt-carve` (left in place, now checked out on `claude/pr4-cli-docs`, clean). It had stray staged files from the aborted attempt; it was `reset --hard main` + `clean -fd` before carving (carve tree only; the main checkout was never touched).
- Main checkout: untouched (`HEAD` still `9a92c586`; pre-existing dirty `docs/design-notes/SPEC-claude-a2b-in-process.md`, `TODO.md`, `docs/superpowers/**`, `tasks/a2b2-interjection-plan.md` left as found).

## Branches

| PR | Branch | Head | Parent |
|---|---|---|---|
| PR1 | `claude/pr1-protocol-seat-health` | `1f2c2a59906f94710caa6c3cff7b14a18be57409` | `main` `ca615b1c` |
| PR2 | `claude/pr2-messages-wire` | `29fba211fee78636cf1d9288b198b3688ac3ea86` | PR1 `1f2c2a59` |
| PR3 | `claude/pr3-lean-fence` | `69c27f83e35e819ece87da4883fe1bb845f245cc` | PR2 `29fba211` |
| PR4 | `claude/pr4-cli-docs` | `0e29b25fef2d35899457401ee3295a66e2d6ecfa` | PR3 `69c27f83` |

## PR1 shape (9.x fallback, deeper than named)

The brief's fallback ("PR1 = protocol + seat auth + backend_provider + inference_backend; health moves to PR2") does not compile either: `backend_provider.rs` adds `BackendProviderKind::ClaudeCliSubscription`, and the exhaustive matches in `agent/runtime/context.rs` and `oneshot.rs` (no wildcard arm on `main`) must then gain arms that construct `crate::claude_subscription::ClaudeSubscriptionClient`, which pulls in `claude_subscription.rs` -> `claude_messages.rs`. So PR1 is the smallest self-compiling protocol/seat slice and `backend_provider.rs`, `backend_health.rs`, `backend_registry.rs(+tests)`, `openai_wire.rs` all moved to PR2. `crates/gents/src/config_client/inference_backend.rs`, `crates/gents/Cargo.toml`, `Cargo.lock` are not in the spike diff at all (nothing to carve). This was decided from the match sites before burning a compile cycle; PR1 passed on its first gate run.

The PR1 commit subject was adjusted to `feat(claude): protocol vocabulary and seat auth` because the brief's subject ("... health probe and promotion") would have been false for this shape; the body records the fallback. PR2-PR4 use the brief's subjects verbatim.

## Path -> PR table (authoritative list: `git diff --stat main..spike-final-2026-09-04`, 60 paths)

| Path | PR |
|---|---|
| `.gitignore` (adds `.scratch/`) | PR1 |
| `crates/gents-protocol/src/rendered_request.rs` | PR1 |
| `crates/gents/src/rendered_request/transport.rs` (1-line fingerprint arm; exhaustive match on the new `RenderedRequestSource` variant) | PR1 |
| `crates/gents/src/claude_seat_auth.rs` | PR1 |
| `crates/gents/src/lib.rs` | PR1 (`pub mod claude_seat_auth;` only) + PR2 (the other three `pub mod claude_*` lines); vs `main` the file differs by exactly the four lines |
| `crates/gents/src/agent/loop_stream/tests/claude.rs`, `.../tests/mod.rs` | PR2 |
| `crates/gents/src/agent/runtime/context.rs` | PR2 |
| `crates/gents/src/backend_health.rs` | PR2 (moved from PR1) |
| `crates/gents/src/backend_provider.rs` | PR2 (moved from PR1) |
| `crates/gents/src/backend_registry.rs`, `backend_registry/tests.rs` | PR2 (moved from PR1) |
| `crates/gents/src/claude_completer/mod.rs` | PR2 |
| `crates/gents/src/claude_messages.rs` | PR2 |
| `crates/gents/src/claude_messages/tests.rs` | PR2 (all but `identity_matches_lean_body_witness_head`) + PR3 (that test) |
| `crates/gents/src/claude_subscription.rs`, `claude_subscription/tests.rs` | PR2 |
| `crates/gents/src/completion_factory.rs`, `completion_factory/tests.rs` | PR2 |
| `crates/gents/src/llm/rig_compat.rs` | PR2 |
| `crates/gents/src/oneshot.rs` | PR2 |
| `crates/gents/src/openai_wire.rs` | PR2 |
| `crates/gents/proofs/**` (8 files) | PR3 |
| `crates/gents/src/lean_vocab_test/prompt_assembly.rs`, `support.rs` | PR3 |
| `crates/gents/tests/conformance/coverage.rs`, `prompt_assembly.rs` | PR3 |
| `crates/gents/tests/support/conformance_consumers.rs` | PR3 |
| `crates/gents-cli/**` (9 files) | PR4 |
| `docs/backends.md`, `docs/design-notes/*` (13 files), `CLAUDE.md` | PR4 |
| `tasks/plan.md`, `tasks/todo.md` | EXCLUDED (spike only) |

Per-branch `git diff --stat` vs parent: PR1 5 files (+413/-5); PR2 18 files (+2374/-63); PR3 14 files (+939/-7); PR4 23 files (+2417/-12).

## Re-carve moves

1. PR1 (pre-emptive, from match-site inspection): `backend_provider.rs`, `backend_health.rs`, `backend_registry.rs(+tests)`, `openai_wire.rs` -> PR2; `rendered_request/transport.rs` <- PR1. One gate run, green.
2. PR2 attempt 1 failed `cargo check` (`task-9-pr2-check.log` first run, EXIT=101): `claude_messages/tests.rs::identity_matches_lean_body_witness_head` calls `lean_vocab_test::lean_prompt_assembly_claude_body_cases()` (defined in PR3's `support.rs`) and asserts the witness set is non-empty, which needs the PR3 Lean cases (the loader runs `lake build`). Move: that one test (12 lines, doc comment + fn) held back from PR2 and restored in PR3 by taking the full spike file. PR2 attempt 2 green. (Logs were overwritten by the second run; the first-run failure is recorded here.)
3. PR3, PR4: no re-carves.

## Gates (all run inside `.scratch/wt-carve`; logs under `/Users/edjroz/Repos/source/gents/.superpowers/sdd/2026-09-03-claude-single-wire/`)

| Branch | Gate | Result | Log |
|---|---|---|---|
| PR1 | `cargo check --workspace --all-targets` | EXIT=0 | `task-9-pr1-check.log` |
| PR1 | `cargo test -p gents -p gents-protocol --no-fail-fast` | EXIT=0; 1906+363+44+57+107+8+27+143 passed, 0 failed | `task-9-pr1-test.log` |
| PR2 | `cargo check --workspace --all-targets` | EXIT=0 (attempt 2) | `task-9-pr2-check.log` |
| PR2 | `cargo test -p gents -p gents-protocol --no-fail-fast` | EXIT=0; 1942+363+... passed, 0 failed | `task-9-pr2-test.log` |
| PR3 | `lake build` (proofs) | EXIT=0, "Build completed successfully", no sorry/errors | `task-9-pr3-lake.log` |
| PR3 | `cargo check --workspace --all-targets` | EXIT=0 | `task-9-pr3-check.log` |
| PR3 | `cargo test -p gents -p gents-protocol --no-fail-fast` | EXIT=0; 1943+366+... passed, 0 failed | `task-9-pr3-test.log` |
| PR4 | `cargo check --workspace --all-targets` | EXIT=0 | `task-9-pr4-check.log` |
| PR4 | `env -u TMPDIR cargo test -p gents-cli --no-fail-fast` | EXIT=0; 666+31+28+5+3+27+16+37+14+13 passed, 0 failed | `task-9-pr4-cli-test.log` |
| PR4 | `cargo test -p gents -p gents-protocol --no-fail-fast` | EXIT=0; 1943+366+... passed, 0 failed | `task-9-pr4-test.log` |
| PR4 | `rustup run 1.97.1 cargo fmt --all --check` | EXIT=0 | `task-9-pr4-fmt.log` |

No flaky reruns were needed (`deadline_tight_fails_cleanly` and the codex-shim port test passed every run). The only warning in every check log is a pre-existing `gents-cli` dead-code notice (`commands/demo/secscan/matchers.rs:28` `description` never read), present on `main`.

## Step 6

```
$ git diff spike-final-2026-09-04 claude/pr4-cli-docs --stat
 tasks/plan.md | 173 --------------
 tasks/todo.md | 749 ----------------------------------------------------------
 2 files changed, 922 deletions(-)
```

Exactly the two excluded files, as expected.

## Concerns

- PR1 shape is smaller than the 9.x fallback named (no `backend_provider.rs`); reason above. PR2 is correspondingly the largest branch (+2374). If the user wants `backend_provider.rs` in PR1, it needs stub arms in `context.rs`/`oneshot.rs` (new code, out of scope).
- PR1 commit subject deviates from the brief's text (see above) to stay truthful.
- The `claude_messages/tests.rs` split means PR2's copy of that file is not byte-identical to the spike until PR3; the full stack is.
- An untracked `docs/superpowers/plans/2026-09-03-claude-single-wire.md` now appears in the main checkout's `git status`; it was not created by this task (only `docs/superpowers/specs/...` was listed at start) and is excluded from every PR regardless.
