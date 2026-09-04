# Task 3 report: split scope creep onto their own branches and revert on the spike

Date: 2026-09-03. Repo `/Users/edjroz/Repos/source/gents`, branch `spike/claude-b3-live-tools`, starting HEAD `44908d95`.

Status: **DONE_WITH_CONCERNS** (concerns are procedural; every expected outcome of the brief holds).

## Step 1: tree state

`git status --porcelain` at start (exactly the known leftovers; nothing else):

```
 M docs/design-notes/SPEC-claude-a2b-in-process.md
?? TODO.md
?? docs/superpowers/plans/2026-09-03-claude-single-wire.md
?? docs/superpowers/specs/2026-09-03-claude-single-wire-design.md
?? tasks/a2b2-interjection-plan.md
```

`.scratch/wt-split` did not pre-exist. Main checkout stayed on `spike/claude-b3-live-tools` throughout.

## Step 2: side branches (throwaway worktree `.scratch/wt-split`, since removed)

Both cherry-picks applied cleanly (no conflicts). Worktree removed afterwards; `git worktree list` shows only the main checkout and the pre-existing `gents-session-fork-retry` worktree.

```
$ git log --oneline main..fix/session-fork-retry
602cc278 feat(session): fork retry at last human user turn        # <- 0b89e688, 7 files, +401/-13

$ git log --oneline main..fix/strip-idless-reasoning
0c23615a fix(prompt): strip id-less reasoning at provider sanitize # <- 63ff2ff3, 5 files, +134/-8
```

## Step 3: C3/C4 notes

Written verbatim from the brief to `.scratch/claude-spike/logs/creep-split-notes.md`.

## Step 4/5: reverts on the spike

```
2e39a4f2 Revert "feat(session): fork retry at last human user turn"          # reverts 0b89e688, 7 files, +13/-401
bcc830a3 Revert "fix(prompt): strip id-less reasoning at provider sanitize"  # reverts 63ff2ff3, 5 files, +8/-134
```

Both use git's default `--no-edit` revert messages.

**Deviation from the brief's expectation:** the second revert did *not* stop with a conflict; git auto-merged `crates/gents/tests/conformance/prompt_assembly.rs`. I verified the auto-merge produced exactly the Step 5 target, so no manual edit (and no `revert --continue`) was needed:

- Line 100 reads `"other" => AssistantContent::Reasoning(Reasoning::new(&reasoning_body(item.value))),`
- `rs-lean` minting: gone from the file.
- Kept from `fac2a94d`: `use gents::claude_completer::{CompleterParseError, StreamJsonlEvent, StreamJsonlState};` (L27), `fn generated_claude_map_cases_drive_the_completer_parser` (L322), `fn claude_map_assistant_line` (L388).
- `strip_idless_reasoning` no longer appears anywhere under `crates/gents/`.

The hunk `bcc830a3` applies to that file:

```diff
@@ -97,13 +97,7 @@ fn assistant_item(item: &LeanPromptAssemblyItem) -> AssistantContent {
         "text" => AssistantContent::Text(Text {
             text: text_body(item.value),
         }),
-        "other" => AssistantContent::Reasoning(
-            // Lean's `other` bucket does not model provider reasoning ids.
-            // Production strips id-less reasoning at the provider boundary, so
-            // the fence must mint a synthetic id or the case would be dropped
-            // before the Lean expected output is compared.
-            Reasoning::new(&reasoning_body(item.value)).with_id(format!("rs-lean-{}", item.value)),
-        ),
+        "other" => AssistantContent::Reasoning(Reasoning::new(&reasoning_body(item.value))),
         "call" => AssistantContent::ToolCall(tool_call(item.value)),
         other => panic!("generated content item has an unmodeled tag: {other}"),
```

No source file was edited by hand. Nothing under `docs/superpowers/`, `TODO.md`, `tasks/`, or the SPEC design note was staged.

## Step 6: gates

Environment: `GENTS_SKIP_LENS_BUILD=1 GENTS_SKIP_CALLBACK_WASM_BUILD=1 TMPDIR="$PWD/.scratch/tmp"`, elan on PATH.

### `lake build` (in `crates/gents/proofs`) — green

```
✔ [1020/1022] Built Proofs.Conformance.Contracts
✔ [1021/1022] Built Proofs
Build completed successfully.
```

### `cargo test -p gents`

A single `cargo test -p gents` invocation could not complete inside the 10-minute foreground limit: the `test` profile compile of the crate (lib test + 7 binaries, `opt-level = 1`) took 26m08s on its own. The compile was completed once (`cargo test -p gents --no-run`, "Finished `test` profile [optimized] target(s) in 26m 08s"), then every executable it listed was run individually with no recompilation. This is the same test set `cargo test -p gents` runs.

Per-binary `test result:` lines:

```
unittests src/lib.rs   test result: ok. 1965 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 115.74s
tests/conformance.rs   test result: FAILED. 362 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 217.28s
tests/misc.rs          test result: ok. 27 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 7.53s
tests/e2e_lifecycle.rs test result: ok. 44 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 62.51s
tests/e2e_runtime.rs   test result: ok. 57 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 41.33s
tests/e2e_subagent.rs  test result: ok. 107 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 75.01s
tests/e2e_triggers.rs  test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 61.66s
Doc-tests gents        test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

(`tests/e2e_live.rs` requires the `live-e2e` feature and is not part of the default set.)

Full failing-test list (exactly the two pre-existing failures named in the task ruling; nothing else failed):

```
docs::rig_vocabulary_confined_to_the_seam
prompt_assembly::provider_invocations_are_confined_to_the_owned_loop_seam
```

Failure excerpt (both point at `claude_messages.rs` / `claude_subscription.rs`, which the reverts do not touch):

```
---- docs::rig_vocabulary_confined_to_the_seam stdout ----
panicked at crates/gents/tests/conformance/docs.rs:174:5:
rig type vocabulary escaped the seam (CLAUDE.md claims one seam; either route through llm::rig_compat or update the allowlist AND the doc):
crates/gents/src/claude_messages.rs:125 ... :680 (11 sites)
crates/gents/src/claude_subscription.rs:19 ... :1350 (11 sites)

---- prompt_assembly::provider_invocations_are_confined_to_the_owned_loop_seam stdout ----
panicked at crates/gents/tests/conformance/prompt_assembly.rs:573:5:
assertion `left == right` failed: provider invocation escaped the owned-loop seam; every completion request must be born in run_loop_stream, which sanitizes loaded history at entry
  left: {"crates/gents/src/admission/client.rs", "crates/gents/src/agent/loop_stream.rs", "crates/gents/src/claude_subscription.rs"}
 right: {"crates/gents/src/admission/client.rs", "crates/gents/src/agent/loop_stream.rs"}
```

### `cargo check --workspace --all-targets` — green (exit 0)

```
    Checking gents-desktop-tauri v0.14.0 (/Users/edjroz/Repos/source/gents/apps/gents-desktop/src-tauri)
    Checking gents-fixture-host v0.14.0 (/Users/edjroz/Repos/source/gents/apps/fixture-host/src-tauri)
warning: `gents-cli` (lib test) generated 1 warning
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 25.55s
```

Zero `error` lines.

## Step 7: branch shape

```
$ git log --oneline -4
bcc830a3 Revert "fix(prompt): strip id-less reasoning at provider sanitize"
2e39a4f2 Revert "feat(session): fork retry at last human user turn"
44908d95 fix(claude): accumulate tool_use input from deltas, fail closed on dup and overlap
551c7969 feat(claude): Keychain seat auth and omit Messages sampling

$ git diff main --stat -- crates/gents/src/session/fork.rs crates/gents/src/compaction/history.rs
(empty — both identical to main)
```

Across all 12 reverted files, only `crates/gents-cli/src/cli/args.rs` (+99) and `crates/gents-cli/src/lib.rs` (+4) still differ from `main`; that residue is the spike's own `ClaudeLoginArgs` / `ClaudeAuthProbeArgs` CLI plumbing (commits `41205e25`, `d83f37d8`, `bca8cef2`, ...) and contains no `fork`/`retry` mentions. `prompt_assembly.rs` differs from `main` only by the `fac2a94d` ClaudeMap driver additions. Final `git status --porcelain` is unchanged from Step 1.

No further commit was made; the reverts are the commits.

## Concerns

1. **Brief expectation vs. outcome (Step 4/5):** the brief predicted a conflict in `prompt_assembly.rs`; git auto-merged it instead. Outcome verified identical to the prescribed resolution, so this is informational only.
2. **`cargo test -p gents` was run per-binary, not as one invocation.** Same executables, same results, but the raw single-command output the brief asked for does not exist. Cause: the test-profile compile alone is 26 minutes on this machine.
3. **CLAUDE.md warm-up advice does not apply on this workspace.** `Cargo.toml` defines `[profile.test]` (`opt-level = 1`, `debug = 0`, `lto = "off"`), so `cargo build --tests -p gents` (dev profile) produces artifacts `cargo test` (test profile) cannot reuse; I burned ~25 minutes discovering this. Not changed (out of scope); worth a follow-up to either drop the override or correct the doc.
4. The two conformance failures are pre-existing (from `3643bf7f`), tracked for Task 6, and untouched here.

## Artifacts

- Notes: `/Users/edjroz/Repos/source/gents/.scratch/claude-spike/logs/creep-split-notes.md`
- Raw logs (session scratchpad): `t-lib.log`, `t-conformance.log`, `t-misc.log`, `t-e2e_*.log`, `t-doc.log`, `cargo-check.log` under `/private/tmp/claude-501/-Users-edjroz-Repos-source-gents/556efe92-e944-46b4-b7ae-5c62ca8f465e/scratchpad/`
