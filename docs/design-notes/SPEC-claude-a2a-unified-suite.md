# SPEC: Claude A2a — unified prod suite via managed proxy

**Date:** 2026-08-31  
**Status:** Approved in principle (human decisions locked); implementation not started  
**Parent:** [`claude-subscription-spike.md`](./claude-subscription-spike.md)  
**Depends on:** Path A packaging complete (`SPEC-claude-phase6-packaging.md`)  
**Local handoff (not in git):** `.scratch/claude-spike/handoff/claude-a2a-handoff.md`

## Problem

Path A works, but the operator surface is still three moving parts and two homes:

```text
prod:   gents server (:9191/:9292) + Grok backend
spike:  gents server (:9192/:9293) + Claude OpenAiCompatible backend
extra:  gents claude-proxy (:8787)
```

Codex `/model` only sees backends on the bound home, so a Claude Opus session cannot see Grok (and vice versa). That feels like incompatibility; it is isolation.

## Goal

```text
gents server   # DefraDB + Codex shim + managed Claude proxy
gents codex --remote ws://127.0.0.1:9292/
```

`/model` shows Grok + Claude Max (+ later local OpenAI-compatible backends) from **prod `~/.gents`**.

## Non-goals (A2a)

- In-process Claude completer (A2b)
- New `BackendProviderKind` / GraphQL schema / Lean changes
- Claude tool bridging
- Anthropic oat / `OAuthCredential` storage
- Cross-home model federation
- Desktop UI

## Architecture

### Keep

- Stock `OpenAiCompatible` + Chat Completions for Claude
- Path A seat: explicit `--config-dir`, no oat
- Text-only Claude: strip tools at proxy; completer `--tools ""`; fail closed on `tool_use`
- Existing `XaiGrokOAuth` backend unchanged
- Standalone `gents claude-proxy` remains for debug

### Add

1. **Prod catalog unification**  
   Register a Claude `InferenceBackend` beside Grok on prod home.
2. **Managed proxy lifecycle**  
   `gents server` optionally spawns/supervises `claude-proxy` as a child.
3. **Operator recipe**  
   One-command start + `/model` expectations documented in `docs/backends.md`.

```text
Before A2a:
  gents server  +  claude-proxy  +  codex

After A2a:
  gents server(manages proxy)  +  codex
```

## Managed proxy vs in-process

| | A2a managed proxy | A2b in-process |
|---|---|---|
| Claude adapter location | Child process supervised by server | Inside server/owned loop |
| Wire format | HTTP loopback Chat Completions | Direct completer call |
| Reuses Path A proxy | Yes | Partially (parser/env only) |
| Schema / provider kind | No | Likely yes |
| Speed to success bar | Fast | Slower |
| Process count (operator view) | 1 server + Codex client | 1 server + Codex client |

A2a is the approved next slice because it hits the success bar with minimal new seams.

## CLI / config sketch (implementation may rename)

```bash
gents server \
  --claude-proxy \
  --claude-proxy-port 8787 \
  --claude-config-dir "$CLAUDE_CONFIG_DIR"
```

Live Claude still requires an explicit write gate (env and/or carefully gated flag). Default must remain refuse-closed for billable calls.

Backend doc (prod):

```text
provider_kind: OpenAiCompatible
openai_wire_api: chat_completions
endpoint: http://127.0.0.1:8787/v1
api_key: not-used
models:
  - claude-opus-5
  - claude-sonnet-5
  - claude-haiku-4-5-20251001
  - claude-fable-5
```

## Tasks

### A2a-0 Stabilize Path A model catalog

- Commit remaining model-forwarding work if dirty
- Ensure docs match full Claude IDs + `--model` forwarding

### A2a-1 Register Claude backend on prod

- Add OpenAiCompatible Claude backend to `~/.gents`
- Preserve Grok backend
- Prove Codex `:9292` `/model` lists both families
- No Claude `OAuthCredential` rows

### A2a-2 Server-managed proxy

- Server flag(s) to spawn/supervise `claude-proxy`
- Required `--claude-config-dir` when enabled
- Clean shutdown of child
- Startup health check against proxy `/healthz`
- Keep standalone proxy command

### A2a-3 Docs / recipe

- Update `docs/backends.md` unified-suite section
- Point operators at prod path; mark spike home historical

### A2a-4 Gated live verification

Requires numbered Claude write approval:

- Only `gents server` started (managed proxy on)
- Codex `/model` shows Grok + Claude
- Claude text-only turn succeeds
- Grok turn still succeeds
- Evidence pack under `.scratch/` or designated log dir

## Success criteria

- [ ] Operator can start suite without manually launching `claude-proxy`
- [ ] Prod Codex `/model` lists Grok and Claude full IDs
- [ ] Switching models in Codex rewrites bound behavior `backend_id`/`model_name` correctly
- [ ] Claude path remains oat-free (`OAuthCredential` for Claude = 0)
- [ ] Claude remains text-only under Path A policy
- [ ] Live Claude calls still go through write gate
- [ ] No Lean/schema changes

## Exit into A2b

Only after A2a success bar is green in real use. A2b then removes the HTTP child and considers a first-class provider seam.
