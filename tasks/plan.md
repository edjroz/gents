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
- **Phase 6 deferred** until Phase 5 Go.
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
- [ ] Phase 6 only if Go (new plan then) — unlocked; not started

## Risks and Mitigations

| Risk | Impact | Mitigation |
| --- | --- | --- |
| Accidental Claude usage burns plan / confuses meter | High | Write gate + `CLAUDE_WRITE_APPROVED=1` + `--dry-run` default |
| `--bare` or API key flips billing to Console | High | Forbid `--bare`; strip `ANTHROPIC_*` in child env |
| Proxy non-SSE breaks rig (`stream: true`) | High | Task 6 before gents turn; curl SSE check |
| Tool bridging mistaken for spike failure | Med | Text-only behavior; strip tools at proxy |
| Meter UI lag / ambient Claude IDE usage | Med | Correlate timestamps; list every approved write |
| `gents init` flags drift vs SPEC | Low | Record exact CLI used in spike log |

## Open Questions

- None blocking Phases 0–5 (defaults locked in parent SPEC).
- Phase 6 packaging path (A/B) only if Phase 5 Go.

## Task list target

Checklist: `tasks/todo.md`
