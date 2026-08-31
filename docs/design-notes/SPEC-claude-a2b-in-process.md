# SPEC: Claude A2b — in-process completer (draft)

**Date:** 2026-09-01  
**Status:** Draft for human review — **do not implement until open questions lock**  
**Parent:** [`claude-subscription-spike.md`](./claude-subscription-spike.md)  
**Depends on:** A2a complete (`SPEC-claude-a2a-unified-suite.md`)  
**Local handoff (not in git):** `.scratch/claude-spike/handoff/` (extend or add `claude-a2b-handoff.md` after locks)

## Problem

A2a hit the operator success bar:

```text
gents server   # DefraDB + Codex shim + managed Claude proxy (HTTP loopback)
gents codex --remote ws://127.0.0.1:9292/
```

But Claude still travels through an extra hop:

```text
owned loop → OpenAiCompatible ChatCompletions HTTP → claude-proxy → Claude CLI
```

That hop is useful for isolation and debug, and it is also:

- another failure domain (bind/port/healthz)
- duplicate request shaping (HTTP SSE ↔ CLI stream-json)
- a permanent “server babysits a fake OpenAI server” tax

## Goal

```text
owned loop / completion factory
  └─ Claude CLI completer directly (Path A seat in --config-dir)
```

Operator surface stays one command. `/model` still shows Grok + Claude. Claude remains subscription-backed, oat-free, text-only, write-gated.

**Rule of thumb:** A2a = “server babysits the proxy.” A2b = “server *is* the proxy.”

## Non-goals (unless explicitly unlocked)

- Claude↔gents tool bridging / MCP passthrough
- Harvesting `sk-ant-oat01` or storing Claude tokens in `OAuthCredential`
- Desktop UI
- Cross-home model federation
- Replacing Grok / Codex provider paths
- Removing standalone `gents claude-proxy` in the first A2b slice (debug retention preferred)

## Locked carry-forwards from Path A / A2a

| # | Contract | Carry? |
|---|---|---|
| 1 | Seat lives in explicit `--config-dir` / `CLAUDE_CONFIG_DIR` | Yes |
| 2 | No Claude `OAuthCredential` / oat in DefraDB | Yes |
| 3 | Text-only: `--tools ""` + fail closed on `tool_use` | Yes (until later milestone) |
| 4 | Numbered Claude write gate for billable calls | Yes |
| 5 | Forbid `claude --bare`; strip `ANTHROPIC_*` in child env | Yes |
| 6 | Full Claude model IDs only (`claude-opus-5`, `claude-sonnet-5`, `claude-haiku-4-5-20251001`, `claude-fable-5`) | Yes |
| 7 | Prod home unification (`~/.gents`) | Yes |
| 8 | Prefer `./target/debug/gents` during spike/branch work | Yes |

## Architecture options (open — pick one in A2b-0)

### Option B1 — Keep `OpenAiCompatible`, short-circuit loopback

Keep the Claude backend as `OpenAiCompatible` + Chat Completions pointing at `http://127.0.0.1:8787/v1`, but when `gents server --claude-proxy` (or a new flag) is on, the completion factory / HTTP client detects the managed local Claude endpoint and calls `claude_completer` directly instead of opening a socket.

```text
InferenceBackend (OpenAiCompatible → 127.0.0.1:8787/v1)
        │
        ├─ A2a today: real HTTP to managed proxy
        └─ A2b B1: short-circuit to in-process completer
```

**Pros:** no GraphQL/schema/`BackendProviderKind` change; existing Codex `/model` catalog unchanged; smallest doc/migration blast radius.  
**Cons:** endpoint URL becomes a lie/marker; healthz/proxy process semantics get weird; hidden special-case in the OpenAI path.

### Option B2 — New `BackendProviderKind` (first-class Claude subscription)

Add something like `ClaudeCliSubscription` / `AnthropicClaudeSubscription` that the completion factory maps straight to the Rust completer. Backend doc stores `config_dir`, model catalog, and gate-related settings instead of a fake OpenAI endpoint.

```text
InferenceBackend (ClaudeCliSubscription, config_dir=…)
        └─ completion_factory → claude_completer::run(...)
```

**Pros:** honest model; clean operator mental model; room for future Claude-specific fields.  
**Cons:** GraphQL/schema + possibly Lean/provider-input surface; migration from A2a OpenAiCompatible backend docs; larger review.

### Option B3 — Hybrid: new kind later, B1 now

Ship B1 as an internal optimization behind the existing managed-proxy flag, keep standalone proxy for debug, and only open B2 after real use proves the in-process path.

**Pros:** fastest path to “no HTTP child in the hot path.”  
**Cons:** two-step architecture; risk of B1 becoming permanent debt.

**Draft recommendation:** **B3 (B1 first, B2 later)** unless you want schema honesty immediately — then **B2**.

## Proposed component map (after locks)

| Piece | Role in A2b |
|---|---|
| `crates/gents/src/claude_completer/` | Shared CLI argv/env/parse/fail-closed (already exists) |
| `completion_factory` / owned loop seam | Dispatch Claude backends to completer (B1 short-circuit or B2 kind) |
| `gents server --claude-proxy` | Either becomes no-op/health shim, or is replaced by `--claude-config-dir` only |
| `gents claude-proxy` | Keep for debug / external clients (recommended) |
| `gents claude-login` / `claude-auth-probe` | Unchanged Path A seat tooling |
| Prod `InferenceBackend` Claude row | Migrated or reinterpreted per B1/B2 |

## Commands (expected)

```bash
# Build / focused tests (wasm skips as today)
GENTS_SKIP_LENS_BUILD=1 GENTS_SKIP_CALLBACK_WASM_BUILD=1 \
  cargo test -p gents claude_completer --lib
GENTS_SKIP_LENS_BUILD=1 GENTS_SKIP_CALLBACK_WASM_BUILD=1 \
  cargo test -p gents-cli --lib claude_proxy managed_claude_proxy

# Auth still Path A
./target/debug/gents claude-login --config-dir "$CLAUDE_CONFIG_DIR"
./target/debug/gents claude-auth-probe --config-dir "$CLAUDE_CONFIG_DIR"

# Server (exact flags TBD by B1/B2 lock)
./target/debug/gents server \
  --claude-config-dir "$CLAUDE_CONFIG_DIR" \
  # B1 may still accept --claude-proxy as compatibility alias
  # Live Claude still requires write gate env
```

## Testing strategy

- Unit: completer argv/env/parse fixtures (already green) remain the source of truth.
- Unit: completion-factory / short-circuit / new-kind dispatch with fake completer (no network).
- CLI: server startup without binding `:8787` when in-process path is selected (B1/B2 dependent).
- Regression: standalone `gents claude-proxy` canned/fake paths still pass.
- Gated live: one numbered Claude write proving owned-loop text pong with **no listener on the old proxy port** (or proxy only if explicitly kept for debug).
- Document harvest: `OAuthCredential` Claude count = 0; `AgentToolCall` = 0 on Claude turns.

## Boundaries

**Always**
- Keep write gate for billable Claude.
- Keep text-only / fail-closed on `tool_use` until a later milestone explicitly unlocks tools.
- Prefer focused tests + Herdr for long cargo builds.
- Update `docs/backends.md` when operator recipe changes.

**Ask first**
- New `BackendProviderKind` / GraphQL schema / Lean changes.
- Removing managed-proxy flags or standalone `claude-proxy`.
- Changing prod default behavior model.
- Enabling Claude tool bridging.
- Any oat / `OAuthCredential` design.

**Never**
- Silent Claude calls without numbered approval.
- `claude --bare`.
- Writing Claude tokens into DefraDB in A2b.
- Touching Lean request lifecycle just to fold the HTTP hop.

## Success criteria

- [ ] Claude completions no longer require a live HTTP proxy process for the normal prod path
- [ ] Prod Codex `/model` still lists Grok + Claude full IDs
- [ ] Owned-loop / Codex text-only Claude turn succeeds under write gate
- [ ] `OAuthCredential` for Claude remains 0
- [ ] Tool bridging still disabled; `tool_use` fails closed
- [ ] Standalone `gents claude-proxy` still works for debug **or** explicit decision documents its removal
- [ ] Docs/recipe updated; A2a marked historical relative to A2b
- [ ] Focused unit/CLI tests green; one gated live smoke filed

## Tasks (skeleton — expand only after A2b-0)

### A2b-0 Lock architecture

Human answers the open questions below. No code.

### A2b-1 Completer dispatch seam

Wire completion path to call `claude_completer` for the chosen option (B1/B2/B3). Fake-completer tests only.

### A2b-2 Server flag / lifecycle cleanup

Stop requiring a real `:8787` accept loop for the happy path; preserve debug proxy as decided.

### A2b-3 Backend doc / operator migration

Update prod recipe + `docs/backends.md`. If B2: migrate Claude `InferenceBackend` docs.

### A2b-4 GATED live verification

Numbered Claude write: in-process pong + oat=0 + tools stripped + no unexpected proxy dependency.

## Open questions — need human locks

1. **Provider seam:** B1 short-circuit, B2 new kind, or B3 (B1 now / B2 later)?  
   Draft lean: **B3**.
2. **Schema/Lean:** if B2, is a GraphQL/`BackendProviderKind` change acceptable in this branch, or does it need a separate PR after merge of A2a?
3. **Managed proxy flags:** keep `--claude-proxy` as alias, replace with `--claude-config-dir` only, or keep real HTTP child forever for debug while adding in-process hot path?
4. **Standalone `gents claude-proxy`:** keep indefinitely, deprecate, or delete after A2b green?
5. **Tools policy:** remain text-only for all of A2b, or allow a follow-on A2c for tool bridging?
6. **Write gate UX:** keep env double-gate only, or add an explicit server flag that still defaults refuse-closed?
7. **Merge sequencing:** merge/review A2a to main before A2b implementation, or continue on `spike/claude-subscription-plan`?

## Exit

A2b is done when the normal prod Claude path is in-process (no required HTTP child), Path A safety contracts still hold, and docs tell operators the new recipe. Further work (tool bridging, native Anthropic Messages, desktop) needs a new SPEC.