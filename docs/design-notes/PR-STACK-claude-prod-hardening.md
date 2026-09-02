# PR stack: Claude in-process provider → production

**Date:** 2026-09-02  
**Branch:** `spike/claude-subscription-plan` (HEAD includes `9d94eff7` process-CLI capture)  
**Not this stack:** fork-retry (own worktree `feat/session-fork-retry`); restoring Grok as prod default (ask first).

Live **text** pong is green. That is not production parity with Grok / ChatGPT Codex.

## The largest divergence (tools)

Grok (`XaiGrokOAuth`) and ChatGPT Codex already run **function tools inside `run_loop_stream`**: behavior surface → provider `tool_call` → `AgentToolCall` → `dispatch_tool` in gents → `tool_result` on the next turn.

Claude A2b **ignores** that surface (`--tools ""`, fail-closed on any `tool_use`). A Claude-backed default behavior still *has* a `tool_selection_id`; the Completer drops it. So Claude cannot bash, MCP, skills, or spawn — the thing that makes Grok/Codex usable as coding agents.

That is a larger gap than buffering stdout, empty usage, or the write flag. Path A text-only was a **viability fence**, not the destination. Closing it is **A2c** (`docs/design-notes/SPEC-claude-a2c-tool-bridging.md`, draft). Gents must still **execute** tools; Claude only **requests** them. Enabling Claude Code’s own Bash/Task is out.

A2c changes provider-input and tool legality → **Lean → conformance → Rust**. It cannot sneak into P1–P4.

Track A is still text-only and **ends at P4**. Tool parity is its own track: [`PR-STACK-claude-track-b-tools.md`](./PR-STACK-claude-track-b-tools.md). Do not Graphite-stack A2c as P5. P1’s incremental JSONL reader is the parser Track B will extend (today it fail-closes on `tool_use`; B2 maps **gents** names and still fail-closes on Claude-native `Bash`).

```text
Track A — transport / ops (text-only)          DONE on spike/claude-p4-write-gate
P1  honest stream-json
 └─ P2  usage from result events
     └─ P3  process-local seat health
         └─ P4  write-gate productization

Track B — own stack, own purposes              see PR-STACK-claude-track-b-tools.md
B1  wire evidence          (declare without execute)
B2  owned-loop round-trip  (fake completer; Lean is how it lands)
B3  live tool-capable seat
B4  spawn / subagent       (later; not v1)
```

P3 does not technically depend on P1/P2 (health never touches stdout). Keep it stacked so review order matches “streaming first.” If we need to land health while streaming is in flight, P3 can be rebased onto spike HEAD instead.

**Always (Track A):** fake-completer tests first; no live Claude without numbered write approval; persist-before-send stays before spawn; no oat in DefraDB; no `--bare`.  
**Track A only:** `--tools ""` + fail-closed on `tool_use`. Track B replaces that on tool-capable turns (B3), not here.

---

## P1 — Honest stream-json (tokens as they arrive)

**Problem.** `complete_text` uses `Command::output()`, waits for process exit, then `parse_stream_jsonl` on the whole buffer. `stream()` then fakes two rig events (`Message(full_text)`, `FinalResponse`). Codex/TUI sees one blob. Compaction/title pay the same latency.

**Change.**

- Spawn Claude with piped stdout. Read **line by line** (JSONL).
- On `assistant` / `content_block_delta` / `text` blocks: yield `RawStreamingChoice::Message` (or text delta) immediately.
- On `tool_use`: fail closed (same `CompleterParseError::ToolUse`); do not yield later text.
- On `result` with `is_error=true`: fail closed.
- After exit: if no text and no result text → `EmptyAssistantText`.
- Keep `claim_and_capture_process_cli` **before** spawn.
- Fake completer: emit JSONL incrementally (sleep between lines in a fixture script) so a unit test proves a Message event arrives before process exit.
- Keep buffered `parse_stream_jsonl` as the oracle for “concatenated text equals today’s parser,” or split it into an incremental state machine used by both.

**Not in P1:** usage mapping (P2); changing argv; mapping `tool_use` into `AgentToolCall` (Track B / B2); HTTP. P1 still fail-closes on `tool_use` so A2b text-only holds until Track B.

**Verify:** existing fixtures still fail-closed; new test that a two-line JSONL fake completer yields the first text chunk before the process exits; `cargo test -p gents --lib claude_`.

---

## P2 — Usage from `result` events

**Depends on P1** so the incremental reader already surfaces the terminal `result` object.

**Problem.** `CompletionResponse.usage` is `Usage::new()` (zeros). Aggregate-budget / PromptAssembly treat missing usage as fail-closed `Missing`. Claude Code `result` events typically carry token counts (`usage.input_tokens` / `output_tokens`, names to pin from a captured live `result` line — do not guess in the PR; add a fixture).

**Change.**

- Parse usage off the `result` JSONL object (fixture-pinned field names).
- Thread into rig `Usage` on `FinalResponse` / completion response.
- If the CLI omits usage, keep today’s empty Usage (still `Missing`) — do not invent numbers.
- Charge path: owned loop already consumes `Usage`; no Lean change unless we add a new fail-closed class (we should not).

**Verify:** fixture with a `result.usage` object → non-zero input/output; fixture without usage → zeros; one loop-stream unit if easy.

---

## P3 — Process-local seat health

**Problem.** `skips_fleet_http_probe()` is correct (placeholder is not `/models`). Consequence: Claude is never measured-unhealthy. A logged-out or missing CLI still looks `probe_status: healthy` on the document.

**Change.** Keep the replicated document probe-status as **operator intent** (do not stamp `unhealthy` from N runtimes). Add a **process-local** measurement:

- Reuse `gents claude-auth-probe` semantics (`claude auth status` under `CLAUDE_CONFIG_DIR`, read-only, not billable).
- Feed `BackendHealthMap` the way HTTP probers do for other kinds (K=3 demote, one success promote — existing BackendHealth model).
- Never write Claude tokens. Never spawn `-p` for health.
- If `--claude-config-dir` is unset, Claude backends stay unavailable (`process seat not installed`) as today.

**Verify:** unit test with fake `claude auth status` stdout `loggedIn=true` / `false`; health map demotes without mutating InferenceBackend GraphQL; `skips_fleet_http_probe` still true.

**Not in P3:** changing `is_agent_scoped_oauth()`; HTTP discovery.

---

## P4 — Write-gate productization

**Problem.** `--claude-write-approved` is a spike kill switch. This home currently runs with it **always on**, so every default-behavior turn is billable. SPEC still requires numbered human approval before live writes.

**Change (keep refuse-closed).**

- Live spawn still requires the flag. No silent default-on.
- Operator docs (`docs/backends.md`, server `--help`): flag means “this process may bill Claude”; recipes must not imply it is the normal prod default.
- Login remains flag-gated for live `claude login`; `--dry-run` ungated.
- Optional code: log at info when a live Completer spawn happens (so a forgotten flag is obvious in traces). Do **not** persist oat.

**Ask first before this PR lands in prod home:** restore default behavior to Grok so the unified suite is not Claude-by-accident. That is a config write, not this PR’s diff, but the PR description should say so.

**Verify:** refuse-closed unit tests unchanged; help text; no env double-gate (`PROXY_USE_CLAUDE` stays gone).

---

## Track B — moved

Tool parity is not P5. It is a separate purpose-sliced stack:

**[`PR-STACK-claude-track-b-tools.md`](./PR-STACK-claude-track-b-tools.md)**

Parent architecture SPEC remains [`SPEC-claude-a2c-tool-bridging.md`](./SPEC-claude-a2c-tool-bridging.md). The old A2c-0…4 list (human locks → Lean → conformance → map → live) is retired; those were methodology stages of one feature, not purposes.

## Still out of stack

- Fork-retry / id-null sanitize — `feat/session-fork-retry` worktree.
- Renaming `ClaudeCliSubscription`.
- Desktop UI.
- Letting Claude Code execute tools on gents’ behalf.

## Landing

Track A: Graphite (`gt stack`) if available, else `git` stacked branches `spike/claude-p1-stream` … `p4-write-gate` based on `spike/claude-subscription-plan`. Focused `claude_` tests per PR; live text pong only after P1, with numbered write approval.

Track B: own branches off P4 HEAD. See [`PR-STACK-claude-track-b-tools.md`](./PR-STACK-claude-track-b-tools.md). Do not mix A2c into P1–P4.
