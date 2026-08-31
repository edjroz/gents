# Implementation Plan: Claude subscription spike

## Overview

Prove whether a Claude Code / Claude subscription seat can back gents' owned
completion loop as a **loopback OpenAI Chat Completions completer** — without a
native Anthropic provider crate. Specs live in `docs/design-notes/`; this plan
turns them into ordered, verifiable tasks.

Parent: `docs/design-notes/claude-subscription-spike.md`

## Architecture Decisions

- **Option 1 only:** gents owns the loop + documents; Claude is a billed text completer.
- **Stock backend:** `OpenAiCompatible` + `ChatCompletions` + dummy API key → loopback proxy.
- **Billing gate late (Phase 5):** correlate after gents-shaped traffic exists.
- **Claude write gate:** no `claude` / proxy→Claude / gents-turn-via-proxy without an explicit numbered human approval for that call.
- **Text-only spike:** no tool bridging; `--tools ""` + reject `tool_use`.
- **Artifacts:** `.scratch/claude-spike/` (gitignored). Prod homes untouched.
- **Phase 6 Path A unlocked** by Phase 5 Go: CLI-login + loopback completer; no oat in DefraDB; no new `BackendProviderKind` in v1.
- **Not a Lean change:** plumbing/integration only.

## Dependency graph

```text
Phase 0 toolchain/isolation
    │
    ├── Phase 1 completer adapter (script; Claude gated)
    │       │
    │       └── Phase 2 OpenAI SSE proxy (build without Claude;
    │               Claude smoke gated)
    │               │
    │               └── Phase 3 gents throwaway home + one text turn (gated)
    │                       │
    │                       └── Phase 4 DefraDB document evidence (prefer no Claude)
    │                               │
    │                               └── Phase 5 billing Go/No-Go (human meter)
    │                                       │
    │                                       └── Phase 6 packaging (only on Go)
```

## Task List

### Phase 0: Foundation
- [ ] Task 1: Close Phase 0 toolchain ACs from captured help log
- [ ] Task 2: Harden spike env + refuse-by-default completer wrapper

### Checkpoint: Foundation
- [ ] Phase 0 SPEC ACs checked
- [ ] Completer `--dry-run` refuses exec
- [ ] Claude write count still 0 unless human approved otherwise

### Phase 1: Completer adapter
- [ ] Task 3: Finish adapter parse/fail-closed (still dry-run default)
- [ ] Task 4: **GATED** first completer smoke (`pong`) — only after write request #1

### Checkpoint: Completer
- [ ] Approved smoke produced text, no `tool_use`
- [ ] Log under `.scratch/claude-spike/logs/`

### Phase 2: Proxy
- [ ] Task 5: Scaffold loopback proxy (`/v1/models` + non-stream stub) — no Claude
- [ ] Task 6: Implement SSE `chat.completions` streaming path — no Claude until smoke
- [ ] Task 7: Wire proxy → completer; strip tools; request log
- [ ] Task 8: **GATED** proxy smoke (curl stream) — write request #2

### Checkpoint: Proxy
- [ ] `/v1/models` returns `claude-plan`
- [ ] SSE ends with `data: [DONE]` on approved smoke
- [ ] Tools stripped; Anthropic env not forwarded

### Phase 3: Gents integration
- [ ] Task 9: Init throwaway `$GENTS_SPIKE_HOME` + OpenAiCompatible backend
- [ ] Task 10: Configure text-only / no-tools spike behavior
- [ ] Task 11: **GATED** one owned-loop text turn — write request #3

### Checkpoint: Integration
- [ ] Turn completed; proxy log correlates
- [ ] Prod homes untouched

### Phase 4: Documents
- [x] Task 12: Collect AgentRequest / AgentMessage / InferenceCall evidence pack

### Checkpoint: Documents
- [x] `phase4-evidence.md` written
- [x] No oat in DefraDB

### Phase 5: Billing
- [x] Task 13: Correlate approved writes vs plan meter → verdict file

### Checkpoint: Complete (spike decision)
- [x] `phase5-verdict.md` = **Go**
- [x] Phase 6 Path A SPEC drafted (`SPEC-claude-phase6-packaging.md`) — awaiting human review before code

### Phase 6: Path A packaging (docs-first; code after SPEC approval)
- [x] Task 14: Human review / lock Path A open questions
- [x] Task 15: Rust completer lib + fixtures (`claude-completer-lib`) — 6/6 via Herdr `wP:pA`
- [x] Task 16: Productize loopback proxy (`claude-loopback-proxy` → `gents claude-proxy`)
- [x] Task 17: `gents claude-login` + `claude-auth-probe` (no oat) — **Track A DONE** (12 unit tests; spike probe + dry-run verified)
- [x] Task 18: Operator preset / recipe (init + text-only behavior) — **Track B DONE** (SPEC recipe finalized vs verified flags)
- [x] Task 19: `docs/backends.md` + spike status update — **Track B DONE**
- [x] Task 20: **GATED** packaging reproduction smoke (write request #4) — evidence in `task20-packaging-evidence.md`
- [x] Post-packaging: **GATED** Codex shim → Claude Path A smoke (write request #5) — evidence in `write5-codex-evidence.md`

Path A packaging smoke complete under write #4. Codex text-only smoke complete under write #5 (`pong` via `:9293` shim). Multi-model Claude catalog + `--model` forwarding landed (full IDs; default `claude-sonnet-5`).

## Next: A2a unified prod suite (approved 2026-08-31)

SPEC: `docs/design-notes/SPEC-claude-a2a-unified-suite.md`  
Local handoff (not in git): `.scratch/claude-spike/handoff/claude-a2a-handoff.md`

Locked:
- Unify into prod `~/.gents`
- Fast fold = managed child proxy under `gents server`
- Success bar = one command + `/model` shows Grok + Claude
- In-process completer = A2b (later)
- Claude remains text-only / oat-free in A2a

### Checkpoint: A2a-2 hang fix re-landed

- Kept `36444a55` in history; hang was healthz-before-spawn
- `start_managed_claude_proxy` now spawns accept loop then healthz; fail-path joins time-bounded
- Focused tests green: managed_claude_proxy_* (3), start_managed_spawns_before_healthz, claude_proxy:: (5), server_parses_claude_proxy_flags
- Remaining: A2a-4 gated live smoke (Claude write approval)


### Checkpoint: Path A packaged
- [x] Fixture fail-closed in-tree
- [x] Login/probe do not write `OAuthCredential` tokens
- [x] Documented OpenAiCompatible → proxy → Claude path works
- [x] backends.md row landed
- [x] No Lean/schema change
- [x] Codex shim text-only turn against Claude Path A works (`gents codex --remote ws://127.0.0.1:9293/`)

## Risks and Mitigations

| Risk | Impact | Mitigation |
| --- | --- | --- |
| Accidental Claude usage burns plan / confuses meter | High | Write gate + `CLAUDE_WRITE_APPROVED=1` + `--dry-run` default |
| `--bare` or API key flips billing to Console | High | Forbid `--bare`; strip `ANTHROPIC_*` in child env |
| Proxy non-SSE breaks rig (`stream: true`) | High | Task 6 before gents turn; curl SSE check |
| Tool bridging mistaken for spike failure | Med | Text-only behavior; strip tools at proxy |
| Meter UI lag / ambient Claude IDE usage | Med | Correlate timestamps; list every approved write |
| `gents init` flags drift vs SPEC | Low | Record exact CLI used in spike log |
| Phase 6 accidentally copies Grok oat-into-DefraDB | High | Path A lock: no `OAuthCredential` upsert; probe CLI seat only |
| Scope creep into native provider kind / desktop | Med | Deferred to A2; Ask-first boundary in Phase 6 SPEC |

## Open Questions

- Phase 6 Path A open questions in `SPEC-claude-phase6-packaging.md` (proxy language, command names, config-dir default, proxy location, defer A2).
- None remaining for Phases 0–5.

## Task list target

Checklist: `tasks/todo.md`
