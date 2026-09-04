### Task 9: Carve the PR stack off `main`

No new code. Four branches, each compiling and green on its own, carved by path from the final spike tree (spec §8).

**Files:** branches `claude/pr1-protocol-seat-health`, `claude/pr2-messages-wire`, `claude/pr3-lean-fence`, `claude/pr4-cli-docs`.

- [ ] **Step 1: Snapshot the final tree**

```bash
git tag spike-final-2026-09-03
git worktree add .scratch/wt-carve main
```

- [ ] **Step 2: PR1 — protocol vocabulary, seat auth, health, promotion**

```bash
cd .scratch/wt-carve && git checkout -b claude/pr1-protocol-seat-health main
git checkout spike-final-2026-09-03 -- \
  crates/gents-protocol/src/rendered_request.rs \
  crates/gents/src/backend_provider.rs \
  crates/gents/src/claude_seat_auth.rs \
  crates/gents/src/backend_health.rs \
  crates/gents/src/backend_registry.rs \
  crates/gents/src/config_client/inference_backend.rs \
  crates/gents/Cargo.toml Cargo.lock
```
Then `git checkout spike-final-2026-09-03 -- <path>` for any file `cargo check --workspace --all-targets` reports as missing a symbol (`claude_subscription.rs` will be needed because `backend_health.rs` calls `probe_process_seat_health`; when it is, take `claude_subscription.rs`, `claude_messages.rs`, `claude_completer/mod.rs`, `lib.rs` module lines and the `completion_factory` / `agent/runtime/context` / `oneshot` arms together and move them into PR2's scope instead — the rule is: PR1 must compile without `claude_messages.rs`; if it cannot, PR1 = protocol + seat auth + backend_provider only and health moves to PR2). Run:

```bash
export GENTS_SKIP_LENS_BUILD=1 GENTS_SKIP_CALLBACK_WASM_BUILD=1 TMPDIR="$PWD/.scratch/tmp"
cargo check --workspace --all-targets 2>&1 | tail -3 && cargo test -p gents -p gents-protocol 2>&1 | grep -E 'test result|FAILED' | head
git commit -am "feat(claude): protocol vocabulary, seat auth, health probe and promotion"
```
(This worktree is cold; expect the ~40 min first build. Run it in the background with `run_in_background` and continue carving.)

- [ ] **Step 3: PR2 — Messages wire and owned-loop test** (stacks on PR1)

```bash
git checkout -b claude/pr2-messages-wire claude/pr1-protocol-seat-health
git checkout spike-final-2026-09-03 -- \
  crates/gents/src/claude_messages.rs crates/gents/src/claude_subscription.rs \
  crates/gents/src/claude_completer crates/gents/src/lib.rs \
  crates/gents/src/rendered_request crates/gents/src/agent/loop_stream/tests/claude.rs \
  crates/gents/src/completion_factory.rs crates/gents/src/agent/runtime/context.rs crates/gents/src/oneshot.rs
git diff --cached --stat
```
Verify `lib.rs` differs from `main` only by the four `pub mod claude_*` lines (the no-rustfmt rule): `git diff main -- crates/gents/src/lib.rs`. If it shows more, hand-apply only those lines. Gates as in Step 2; commit "feat(claude): single Messages HTTP wire, streaming parser, owned-loop test".

- [ ] **Step 4: PR3 — Lean fence** (stacks on PR2)

```bash
git checkout -b claude/pr3-lean-fence claude/pr2-messages-wire
git checkout spike-final-2026-09-03 -- crates/gents/proofs crates/gents/src/lean_vocab_test crates/gents/tests/conformance crates/gents/tests/support/conformance_consumers.rs
(cd crates/gents/proofs && lake build 2>&1 | tail -2)
cargo test -p gents 2>&1 | grep -E 'test result|FAILED' | head
git commit -am "spec(claude): ClaudeMap system assembly and stream accumulation; conformance witnesses"
```

- [ ] **Step 5: PR4 — CLI and docs** (stacks on PR3)

```bash
git checkout -b claude/pr4-cli-docs claude/pr3-lean-fence
git checkout spike-final-2026-09-03 -- crates/gents-cli docs/design-notes/PR-STACK-claude-track-b-tools.md docs/design-notes/SPEC-claude-a2c-tool-bridging.md CLAUDE.md
git rm -q --cached docs/superpowers 2>/dev/null; true
cargo test -p gents-cli 2>&1 | grep -E 'test result|FAILED' | head
cargo check --workspace --all-targets 2>&1 | tail -3
git commit -am "feat(cli): claude-login trim, serve seat flags, single-wire docs"
```

- [ ] **Step 6: Confirm the stack equals the spike**

```bash
git diff spike-final-2026-09-03 claude/pr4-cli-docs --stat
```
Expected: empty except the untracked spec/plan/TODO/tasks leftovers (which are not in either tree) and `docs/design-notes/SPEC-claude-a2b-in-process.md` (uncommitted on the spike). Report the four branch names and the two side branches from Task 3 to the user; opening the PRs is the user's call.

#### 9.x Corrections (controller rulings before dispatch)

- Use the warm-worktree bootstrap instead of a bare `git worktree add` in Step 1: `make worktree BRANCH=claude/pr1-protocol-seat-health DIR=.scratch/wt-carve BASE=main` (clones `target/` and `proofs/.lake` via APFS clonefile). Then in `.scratch/wt-carve` create the remaining branches with `git checkout -b <name> <parent>` as the steps say. Because `[profile.test]` builds separate artifacts, run `cargo test -p <crate> --no-run` first if you want to see the warm-up cost before the real gates.
- Gate per PR (each branch must pass on its own): `cargo check --workspace --all-targets` and `cargo test -p gents -p gents-protocol` for PR1–PR3; PR4 additionally `env -u TMPDIR cargo test -p gents-cli`; PR3 additionally `lake build` in the worktree's `crates/gents/proofs`. Run each long gate as ONE background shell job writing to `.superpowers/sdd/2026-09-03-claude-single-wire/task-9-<pr>-<gate>.log` with an `EXIT=` line and wait with foreground `until ! kill -0 <pid>` loops under 9 minutes each; never kill a compile.
- PR1 decision rule from Step 2 stands: if PR1 cannot compile without `claude_messages.rs` (because `backend_health.rs` → `claude_subscription::probe_process_seat_health`), then PR1 = protocol vocabulary + `backend_provider.rs` + `claude_seat_auth.rs` + `inference_backend.rs` only, and health/promotion (`backend_health.rs`, `backend_registry.rs`) move to PR2. Record which shape you ended with.
- Do NOT push any branch and do NOT open PRs; report the branch names, their head SHAs, and each gate result. Leave the `.scratch/wt-carve` worktree in place (the user may want to inspect it); say so in the report.
- The tag `spike-final-2026-09-03` in Step 1 should be created on the current HEAD (the Task 8 commit) — name it `spike-final-2026-09-04` since the date rolled over.
