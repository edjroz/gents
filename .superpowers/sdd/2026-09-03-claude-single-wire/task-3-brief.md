### Task 3: Split scope creep onto their own branches and revert on the spike

Git-only task, no source edits beyond one conflict resolution. Still runs in a Fable 5.1 subagent after approval, because the revert touches tracked source.

**Files:**
- Branches created: `fix/session-fork-retry` (← `0b89e688`, 7 files: `gents-cli/src/cli/args.rs`, `commands/codex_shim/thread_routes.rs`, `commands/session.rs`, `gents-cli/src/lib.rs`, `gents/src/session.rs`, `gents/src/session/fork.rs`, `gents/tests/e2e_runtime/fork_invariants.rs`), `fix/strip-idless-reasoning` (← `63ff2ff3`, 5 files: `proofs/Proofs/PromptAssembly/Content.lean`, `compaction.rs`, `compaction/history.rs`, `compaction/tests.rs`, `tests/conformance/prompt_assembly.rs`)
- Modify on spike (via `git revert`): the same 12 files; manual resolution in `crates/gents/tests/conformance/prompt_assembly.rs` (~L100-106)

**Interfaces:**
- Produces: the spike no longer contains `strip_idless_reasoning` (C3/C4) or the fork-retry flag; Task 5's driver edits in `tests/conformance/prompt_assembly.rs` start from the reverted `"other"` arm.

- [ ] **Step 1: Confirm the tree is clean apart from the known leftovers**

```bash
git status --porcelain
```
Expected: only ` M docs/design-notes/SPEC-claude-a2b-in-process.md`, `?? TODO.md`, `?? docs/superpowers/specs/…`, `?? docs/superpowers/plans/…`, `?? tasks/a2b2-interjection-plan.md`. Anything else: stop and report.

- [ ] **Step 2: Create the side branches in a throwaway git worktree**

A plain `git worktree add` is acceptable here because nothing is compiled in it (the `make worktree` rule exists to warm `target/`; these branches are validated by CI when their PRs open).

```bash
git worktree add .scratch/wt-split main
git -C .scratch/wt-split checkout -b fix/session-fork-retry
git -C .scratch/wt-split cherry-pick 0b89e688
git -C .scratch/wt-split checkout -b fix/strip-idless-reasoning main
git -C .scratch/wt-split cherry-pick 63ff2ff3
git -C .scratch/wt-split log --oneline main..fix/session-fork-retry main..fix/strip-idless-reasoning
git worktree remove .scratch/wt-split
```
Expected: two branches, one commit each, no conflicts (neither commit overlaps the other or anything on `main`).

- [ ] **Step 3: Record C3/C4 on the reasoning branch's commit trailer**

```bash
git branch --edit-description fix/strip-idless-reasoning
```
is interactive and unavailable; instead write `.scratch/claude-spike/logs/creep-split-notes.md`:

```markdown
# Creep split — 2026-09-03
- fix/session-fork-retry ← 0b89e688 (fork retry at last human user turn). Needs a Lean model of the cut point before review.
- fix/strip-idless-reasoning ← 63ff2ff3. Review findings C3 (unconditional 4th sanitize stage, Provider.lean still says three stages; conformance driver mints rs-lean-* ids so the stage is never exercised by spec) and C4 (provider_view over-drains legacy sessions with null compacted_through_sequence) must be addressed there, Lean-first.
```

- [ ] **Step 4: Revert both commits on the spike**

```bash
git revert --no-edit 0b89e688
git revert --no-edit 63ff2ff3
```
Expected: the first revert applies cleanly. The second stops with a conflict in `crates/gents/tests/conformance/prompt_assembly.rs` (the file was later touched by `fac2a94d`, which added the ClaudeMap driver).

- [ ] **Step 5: Resolve the conflict**

Keep everything `fac2a94d` added (the `generated_claude_map_cases_drive_the_completer_parser` test, `claude_map_assistant_line`, the `claude_completer` import). In the witness-to-Rust conversion (the `"other"` arm around L100-106), drop the id minting so the arm reads:

```rust
            "other" => AssistantContent::Reasoning(Reasoning::new(&reasoning_body(item.value))),
```

instead of

```rust
            "other" => AssistantContent::Reasoning(
                Reasoning::new(&reasoning_body(item.value))
                    .with_id(format!("rs-lean-{}", item.value)),
            ),
```

Then:
```bash
git add crates/gents/tests/conformance/prompt_assembly.rs
git -c core.editor=true revert --continue
```

- [ ] **Step 6: Gates**

```bash
export GENTS_SKIP_LENS_BUILD=1 GENTS_SKIP_CALLBACK_WASM_BUILD=1 TMPDIR="$PWD/.scratch/tmp"
(cd crates/gents/proofs && lake build 2>&1 | tail -3)
cargo test -p gents 2>&1 | tail -30
cargo check --workspace --all-targets 2>&1 | tail -5
```
Expected: `lake build` green (Content.lean is back to `main`'s text). All Rust tests green: the conformance sanitize cases no longer need minted ids because the stripping stage is gone.

- [ ] **Step 7: Verify the branch shape**

```bash
git log --oneline -4
git diff main --stat -- crates/gents/src/session/fork.rs crates/gents/src/compaction/history.rs
```
Expected: two revert commits on top; both diffs empty against `main`.

No further commit: the reverts are the commits.

