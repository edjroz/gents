# Todo: Claude subscription spike

Source plan: `tasks/plan.md`  
Specs: `docs/design-notes/claude-subscription-spike.md` + `SPEC-claude-phase*.md`

**Standing rule:** any task marked **GATED** requires a numbered Claude write
request and human `yes` before execution. Default = no Claude traffic.

---

## Task 1: Close Phase 0 toolchain ACs

**Description:** Finish Phase 0 from the already-captured
`.scratch/claude-spike/logs/toolchain.txt` and directory layout. No Claude `-p`.

**Acceptance criteria:**
- [x] Phase 0 SPEC checklist marked against real evidence (version, flag presence/absence)
- [x] GraphQL / gents-cli availability noted for Phase 3
- [x] Confirm `.scratch/` gitignored and layout complete

**Verification:**
- [x] Manual: read `toolchain.txt`; confirm `--tools`, no `--max-turns`, `--bare` documented as forbidden
- [x] Manual: `git check-ignore -v .scratch/claude-spike`

**Done:** `.scratch/claude-spike/logs/phase0-checklist.md` (2026-08-29). Claude writes: 0.

**Dependencies:** None  
**Files likely touched:**
- `.scratch/claude-spike/logs/phase0-checklist.md` (new)
- `docs/design-notes/SPEC-claude-phase0-toolchain.md` (checkboxes only if we track there)

**Estimated scope:** S

---

## Task 2: Harden spike env + refuse-by-default wrapper

**Description:** Ensure `env.sh` + `claude-completer.sh` always refuse real exec
without `--execute` **and** `CLAUDE_WRITE_APPROVED=1`. Fix any argv/`--tools ""`
edge cases.

**Acceptance criteria:**
- [x] `--dry-run` exits 0 and does not spawn `claude`
- [x] `--execute` without `CLAUDE_WRITE_APPROVED=1` exits non-zero
- [x] Documented unset var list matches Phase 1 SPEC

**Verification:**
- [x] Manual: run dry-run; run `--execute` without approval and confirm refuse
- [x] Manual: `ps`/`pgrep` shows no claude from dry-run

**Dependencies:** Task 1  
**Files likely touched:**
- `.scratch/claude-spike/bin/claude-completer.sh`
- `.scratch/claude-spike/bin/env.sh`

**Estimated scope:** S

**Done:** dry_exit=0; noapprove_exit=2; no `claude -p` processes. Logs: `task2-dry.txt`, `task2-noapprove.err`.

---

## Checkpoint: Foundation (after Tasks 1–2)

- [x] Phase 0 SPEC ACs satisfied
- [x] Completer refuses unapproved exec
- [x] Claude write count = 0 for these tasks
- [ ] Human review before Phase 1 smoke

---

## Task 3: Adapter parse / fail-closed (no Claude yet)

**Description:** Extend the completer wrapper to parse `stream-json` JSONL,
extract assistant text, and fail closed on `tool_use` / missing result — testable
with fixture JSONL files, without calling Claude.

**Acceptance criteria:**
- [x] Fixture: assistant-only JSONL → prints text, exit 0
- [x] Fixture: includes `tool_use` → exit non-zero + error
- [x] Fixture: empty/missing result → exit non-zero
- [x] Still dry-run by default

**Verification:**
- [x] Manual: feed fixtures via stdin or `--fixture` path
- [x] No network / no `claude` binary required for fixture mode

**Dependencies:** Task 2  
**Files likely touched:**
- `.scratch/claude-spike/bin/claude-completer.sh`
- `.scratch/claude-spike/bin/parse-stream-json.py`
- `.scratch/claude-spike/fixtures/*.jsonl`

**Estimated scope:** S–M

**Done:** ok→`pong`/0; tool_use→1; empty→1; dry-run still refuses. Claude writes: 0.

---

## Task 4: GATED — first completer smoke

**Description:** First live `claude -p` via adapter with prompt
`Reply with exactly: pong`. Requires **CLAUDE WRITE REQUEST #1**.

**Acceptance criteria:**
- [x] Write request #1/#1b/#1c approved by human before run
- [x] Assistant text contains `pong` (human terminal + log)
- [x] No `tool_use` content blocks in JSONL log (`tools: []` on init)
- [x] Log stored under `.scratch/claude-spike/logs/completer-*.jsonl`

**Verification:**
- [x] #1b: auth failure / `$0` under agent sandbox (keychain invisible)
- [x] #1c: human interactive shell with keychain → printed `pong`

**Dependencies:** Task 3 + human write approval #1  
**Files likely touched:**
- `.scratch/claude-spike/logs/` only

**Estimated scope:** S (execution), gated

**Status (2026-08-30):** #1 never reached Claude (env `-u` order). #1b auth-failed in agent sandbox (`total_cost_usd=0`). #2-login completed by human (`claude.ai` / Max). **#1c succeeded in human terminal: `pong`.** Log: `completer-20260830T001534Z.jsonl`. Note: live Claude execs that need keychain must run in a human-interactive shell until agent keychain access exists.
---

## Checkpoint: Completer (after Tasks 3–4)

- [x] Fixture fail-closed works
- [x] One approved live smoke succeeded (`pong` via #1c)
- [x] Human OK to start proxy build (Task 7 authorized)

---

## Task 5: Scaffold loopback proxy (no Claude)

**Description:** Create proxy project under `.scratch/claude-spike/proxy/` serving
`GET /v1/models` and a non-stream stub that does **not** call Claude (returns
canned text or 501 for completion until Task 7).

**Acceptance criteria:**
- [x] Listens on `127.0.0.1` only
- [x] `GET /v1/models` returns `claude-plan`
- [x] Process starts/stops via documented command (`bin/run-proxy.sh` / `proxy/server.py`)
- [x] No completer invocation from this task

**Verification:**
- [x] Manual: curl `/healthz`, `/v1/models`, non-stream + stream completions

**Dependencies:** Checkpoint Completer (or Task 3 if parallelizing carefully)  
**Files likely touched:**
- `.scratch/claude-spike/proxy/server.py`
- `.scratch/claude-spike/bin/run-proxy.sh`

**Estimated scope:** M

**Done:** canned proxy on `127.0.0.1:8787`; `PROXY_USE_CLAUDE=1` returns 501 until Task 7. Claude writes: 0.

---

## Task 6: SSE streaming Chat Completions (no Claude)

**Description:** Implement OpenAI SSE response shape for `stream: true`, initially
from canned tokens (no Claude) so rig/curl framing can be validated offline.

**Acceptance criteria:**
- [x] `POST /v1/chat/completions` with `"stream": true` yields `data: chat.completion.chunk` lines + `data: [DONE]`
- [x] Tolerates `stream_options.include_usage`
- [x] Non-stream JSON path works for curl smoke
- [x] Still no Claude call in canned mode

**Verification:**
- [x] Manual: curl SSE and confirm `[DONE]`
- [x] Request log records stream + tools-stripped flags

**Dependencies:** Task 5  
**Files likely touched:**
- `.scratch/claude-spike/proxy/server.py`

**Estimated scope:** M

**Done:** canned SSE verified; tools fields ignored (`had_tools_fields=true` still returns text). Claude writes: 0.

---

## Task 7: Wire proxy → completer + strip tools

**Description:** On completion requests, call Phase 1 adapter; flatten messages;
strip `tools`/`tool_choice`; never forward Anthropic env; write request log.
Keep a `PROXY_CANNED=1` or equivalent so default dev mode needs no Claude.

**Acceptance criteria:**
- [x] Canned mode default OR explicit `PROXY_USE_CLAUDE=1` required for live path
- [x] Live path still requires adapter’s `CLAUDE_WRITE_APPROVED=1`
- [x] `tools` / `tool_choice` never enable Claude tools
- [x] Request log: timestamp, model, message count, stream flag (no secrets)

**Verification:**
- [x] Manual: canned stream works without Claude
- [x] Manual: live path refuses without approval env (502)
- [x] Manual: `SPIKE_FAKE_COMPLETER` path proves wiring + SSE without Claude

**Dependencies:** Tasks 3, 6  
**Files likely touched:**
- `.scratch/claude-spike/proxy/server.py`
- `.scratch/claude-spike/bin/claude-completer.sh` (stderr-only context)
- `.scratch/claude-spike/bin/fake-completer.sh`
- `.scratch/claude-spike/bin/run-proxy.sh`

**Estimated scope:** M

**Done:** double gate (`PROXY_USE_CLAUDE` + `CLAUDE_WRITE_APPROVED`); tools stripped; fake-completer verifies live path. Claude writes this task: 0.

---

## Task 8: GATED — proxy live smoke

**Description:** curl (or tiny client) streaming request through proxy to Claude.
**CLAUDE WRITE REQUEST #2.**

**Acceptance criteria:**
- [x] Write request #2 approved
- [x] SSE completes with assistant text
- [x] Proxy + completer logs correlate
- [x] `/v1/models` still healthy

**Verification:**
- [x] Manual: human meter watch optional (CLI total_cost_usd≈0.02883; Phase 5 correlates Max meter)
- [x] Manual: confirm Authorization ignored / no ANTHROPIC key in child (Bearer unused accepted; mode=claude; tools=[])

**Done:** human two-terminal live smoke 2026-08-30T00:30:29Z — SSE `pong` + `[DONE]`;
completer `completer-20260830T003029Z.jsonl` (result=pong, tools=[], cost≈0.02883);
proxy request mode=claude; `/v1/models` + `/healthz` healthy. Evidence:
`.scratch/claude-spike/logs/write-request-2.md`. Claude writes this task: 1 (#2).

**Dependencies:** Task 7 + write approval #2  
**Files likely touched:**
- `.scratch/claude-spike/logs/`

**Estimated scope:** S, gated

---

## Checkpoint: Proxy (after Tasks 5–8)

- [x] Models + SSE contract green
- [x] Live smoke approved and logged
- [x] Human OK for gents init

---

## Task 9: Init throwaway gents home + backend

**Description:** `gents init` / backend config under `$GENTS_SPIKE_HOME` pointing
at proxy with `OpenAiCompatible` + `chat-completions` + dummy key + `claude-plan`.
No agent turn yet.

**Acceptance criteria:**
- [x] Home under `.scratch/claude-spike/gents-home` (or documented path)
- [x] Prod `~/.gents` untouched
- [x] Backend endpoint = loopback proxy `/v1`
- [x] Exact CLI command recorded in spike log

**Verification:**
- [x] Manual: init stdout recorded endpoint/provider/model; full `backend show` deferred until spike `gents server` (Task 10)
- [x] Manual: prod `~/.gents/init.json` still live principal DID / mtime Aug 29

**Done:** `gents init --home .scratch/claude-spike/gents-home ... --tool-package minimal`
→ status=initialized; agent_did=`did:key:z6MkgCE1AUd8uxQ6oEm3Phh54tftWiG7DfupDAUZpGvrwgu6`;
endpoint=`http://127.0.0.1:8787/v1`; provider=`OpenAiCompatible`; model=`claude-plan`;
tool_package=`Minimal` / ceiling=`MetaOnly`. Evidence:
`.scratch/claude-spike/logs/task9-init.md` (+ `phase3-init.md`).

**Dependencies:** Checkpoint Proxy  
**Files likely touched:**
- `.scratch/claude-spike/gents-home/**`
- `.scratch/claude-spike/logs/phase3-init.md`

**Estimated scope:** S–M

---

## Task 10: Text-only spike behavior

**Description:** Ensure spike agent behavior has no tool surface (empty tools /
no tool refs) so Phase 3 cannot confuse “no tool_calls” with failure.

**Acceptance criteria:**
- [x] Behavior used for spike has zero tools enabled
- [x] Documented how that was achieved (init flags / config commands)

**Verification:**
- [x] Manual: backend/behavior/tools show on spike GraphQL :9192; runtime log `tools=[]`

**Done:** `--tool-package minimal` + server `--tool-ceiling meta-only`, then
`gents config tools set --enable-context-budget false` (minimal still exposed
`context_budget`). Runtime rebuilt with `model=claude-plan tools=[]`. Backend
also confirmed `openai_wire_api=chat_completions`. Evidence:
`.scratch/claude-spike/logs/task10-text-only.md`. Spike server remains on
`:9192`/`:9293` for Task 11.

**Dependencies:** Task 9  
**Files likely touched:**
- `.scratch/claude-spike/gents-home/**`
- `.scratch/claude-spike/logs/phase3-init.md`

**Estimated scope:** S

---

## Task 11: GATED — one owned-loop text turn

**Description:** Run one gents agent turn through proxy→Claude.  
**CLAUDE WRITE REQUEST #3** (if multiple completions occur, each additional
completion needs its own approval or an explicit multi-call approval text).

**Acceptance criteria:**
- [x] Write request #3 approved before turn
- [x] Turn completes with assistant text
- [x] Proxy log shows the request
- [x] Adapter still saw no `tool_use`

**Verification:**
- [x] Manual: CLI total_cost_usd=0.032538 recorded; Phase 5 correlates Max meter
- [x] Manual: request `a4788ac7-…` ↔ proxy 01:42:45/46 ↔ completer `…T014246Z`

**Done:** Herdr pane `wP:p8` ran `gents chat … "Reply with exactly: pong"` → pane
`pong` / `TASK11_EXIT:0`; spike response content=`pong` status=complete;
completer `tools=[]` result=pong; proxy `had_tools_fields=false` (2 calls under
this approved turn: title gen + main). Evidence:
`.scratch/claude-spike/logs/write-request-3.md`.

**Dependencies:** Tasks 10, 8 + write approval #3  
**Files likely touched:**
- `.scratch/claude-spike/logs/`
- spike DefraDB docs (via runtime)

**Estimated scope:** M, gated

---

## Checkpoint: Integration (after Tasks 9–11)

- [x] Owned-loop text turn succeeded
- [x] Prod homes untouched
- [x] Human OK for document harvest (prefer no new Claude)

---

## Task 12: Document evidence pack

**Description:** Query spike GraphQL/home for AgentRequest, AgentMessage,
InferenceCall (or equivalent); write `phase4-evidence.md`. Prefer no new Claude.

**Acceptance criteria:**
- [x] Terminal successful AgentRequest cited
- [x] AgentMessage user+assistant cited
- [x] Inference audit doc cited
- [x] Explicit note: no Anthropic oat in DefraDB
- [x] Optional peer check done or explicitly skipped

**Verification:**
- [x] Manual: evidence file written from live spike GraphQL harvest
- [x] Manual: no Claude write request opened for Phase 4

**Done:** Harvested request `a4788ac7-…` from `:9192` — status/lifecycle
`completed`; user+assistant `AgentMessage` with assistant text `pong`; two
completed `InferenceCall`s on OpenAiCompatible/`chat_completions`/`not-used`;
`OAuthCredential`=0; `AgentToolCall`=0; peer check skipped (single-node).
Evidence: `.scratch/claude-spike/logs/phase4-evidence.md` (+ `phase4-raw.json`).

**Dependencies:** Task 11  
**Files likely touched:**
- `.scratch/claude-spike/logs/phase4-evidence.md`

**Estimated scope:** S–M

---

## Checkpoint: Documents (after Task 12)

- [x] Evidence pack complete
- [x] Ready for billing correlation

---

## Task 13: Billing Go/No-Go

**Description:** Human correlates every approved Claude write (#1–#N) with plan
meter / invoice. Agent prepares checklist only; human records verdict.

**Acceptance criteria:**
- [x] Checklist of all approved writes (id, time, purpose)
- [x] Human verdict: Go | No-Go | Inconclusive
- [x] Written to `.scratch/claude-spike/logs/phase5-verdict.md`
- [x] Extra Claude probes only if human approves new write requests

**Verification:**
- [x] Manual: human signed **Go** (plan/subscription meter, not Console API)
- [x] N/A Inconclusive probe list

**Done:** Human verdict **Go**. Checklist
`.scratch/claude-spike/logs/phase5-checklist.md`; verdict
`.scratch/claude-spike/logs/phase5-verdict.md`. Correlated #1c/#2/#3 (+ #3
title-gen sibling under same approval). No new Claude write opened for Phase 5.

**Dependencies:** Task 12  
**Files likely touched:**
- `.scratch/claude-spike/logs/phase5-verdict.md`

**Estimated scope:** S (human-led)

---

## Checkpoint: Spike decision (after Task 13)

- [x] Verdict filed (**Go**)
- [x] If Go → Phase 6 Path A SPEC/plan drafted (`SPEC-claude-phase6-packaging.md`)
- [x] If No-Go → N/A
- [x] All SPEC phase exits satisfied or explicitly waived by human
- [x] Human review of Phase 6 SPEC before implementation (Task 14)

---

# Phase 6: Path A packaging

Source SPEC: `docs/design-notes/SPEC-claude-phase6-packaging.md`  
Standing rule unchanged: **GATED** tasks need numbered Claude write approval.

---

## Task 14: Review / lock Path A open questions

**Description:** Human reviews Phase 6 SPEC and answers open questions before
any packaging code. No Claude traffic.

**Acceptance criteria:**
- [x] Confirm Path A locks (no oat, no new BackendProviderKind, text-only)
- [x] Decide proxy language → **Rust**
- [x] Decide CLI names → `claude-login` / `claude-auth-probe`
- [x] Decide `--config-dir` default → **require explicit**
- [x] Decide proxy packaging location → **`gents claude-proxy`**
- [x] Confirm A2 deferred; Path A used experimentally until stabilized

**Verification:**
- [x] Manual: answers recorded in SPEC open-questions section (locked 2026-08-30)

**Dependencies:** Task 13 (Go)  
**Files likely touched:**
- `docs/design-notes/SPEC-claude-phase6-packaging.md`
- `tasks/todo.md`

**Estimated scope:** S (human-led)

**Done note:** Experimental Path A: Rust local OpenAI adapter (`gents claude-proxy`),
keep exercising under write gate before any A2 native-provider work.

---

## Task 15: Completer library + fixtures

**Description:** Port Phase 1 parse/env/argv contracts into `crates/gents` with
checked-in stream-json fixtures. No live Claude.

**Acceptance criteria:**
- [x] Unit: assistant-only fixture → text
- [x] Unit: `tool_use` fixture → error
- [x] Unit: missing/empty result → error
- [x] Unit: child env sanitize strips Anthropic/cloud vars
- [x] No `claude --bare` path exists
- [x] In-tree `cargo test -p gents claude_completer` green
      (Herdr pane `wP:pA`; needed `GENTS_SKIP_LENS_BUILD=1`
      `GENTS_SKIP_CALLBACK_WASM_BUILD=1` because wasm32 target missing)

**Verification:**
- [x] Parser/env/argv tests pass
- [x] Herdr pane: `test result: ok. 6 passed; 0 failed`
- [x] No network / no Claude binary required for unit path

**Dependencies:** Task 14  
**Files likely touched:**
- `crates/gents/src/claude_completer/*` (new)
- fixture files under crate tests

**Estimated scope:** M

**Done note:** Task 15 complete. Herdr discovery works via socket API even
without `HERDR_ENV`; this agent pane is `wP:p3`, test pane `wP:pA`.

---

## Task 16: Productize loopback proxy

**Description:** Promote Phase 2 proxy into the agreed packaging location; wire
to completer lib or existing adapter with same SSE/tool-strip/gate behavior.

**Acceptance criteria:**
- [x] Loopback-only bind
- [x] `GET /v1/models` → `claude-plan`
- [x] SSE chat.completions + `[DONE]`
- [x] Tools stripped; Anthropic env not forwarded
- [x] Canned/fake path works without Claude
- [x] Live Claude path still requires write approval env

**Verification:**
- [x] Automated: `cargo test -p gents claude_completer --lib` → 13 passed (helpers + parse)
- [x] Automated: `cargo test -p gents-cli claude_proxy --lib` → 4 passed (canned SSE, 502 without approval, fake completer)
- [x] Manual/automated: fake-completer SSE smoke
- [x] Manual: unapproved live path refuses (502/error)

**Dependencies:** Task 15 (+ Task 14 proxy-language decision)  
**Files likely touched:**
- `crates/gents/src/claude_completer/proxy.rs`
- `crates/gents-cli/src/commands/claude_proxy.rs`
- `crates/gents-cli/src/cli/args.rs` / `commands/mod.rs` / `lib.rs`

**Estimated scope:** M

**Done:** Rust `gents claude-proxy` on Herdr wP:pA. Default canned; live needs `PROXY_USE_CLAUDE=1` + `CLAUDE_WRITE_APPROVED=1`. Explicit `--config-dir`. Claude writes: 0.

---

## Task 17: `claude-login` + `claude-auth-probe` — Track A

**Description:** Add CLI commands that wrap official Claude login/status. Must
**not** upsert Anthropic tokens into `OAuthCredential`.

**Acceptance criteria:**
- [x] `gents claude-login` invokes official CLI login flow (gated if network)
- [x] `gents claude-auth-probe` reports logged-in / authMethod / subscriptionType / config-dir
- [x] GraphQL/`OAuthCredential` writes for Claude tokens = 0
- [x] Help text warns seat lives in Claude config, not DefraDB

**Verification:**
- [x] Unit: arg/help tests (`claude_` module tests + clap parse test) — 12 passed
- [x] Manual probe against spike `CLAUDE_CONFIG_DIR` (read-only): `logged_in=true`, `auth_method=claude.ai`, `subscription_type=max`, `api_key_source=none`, `oauth_credential_written=false`
- [x] Login itself only after write-gate if it hits Anthropic (`CLAUDE_WRITE_APPROVED=1`; `--dry-run` ungated — verified)

**Dependencies:** Task 14  
**Parallel track:** A (code). May run beside Track B docs drafts.  
**Files likely touched:**
- `crates/gents-cli/src/commands/claude_login.rs`
- `crates/gents-cli/src/commands/claude_auth_probe.rs`
- `crates/gents-cli/src/cli/args.rs`
- `crates/gents-cli/src/cli/args/tests.rs`
- `crates/gents-cli/src/commands/mod.rs`
- `crates/gents-cli/src/lib.rs`

**Estimated scope:** M

**Status:** DONE. Units green (12); rebuilt `./target/debug/gents` help/probe/dry-run verified against spike seat.

---

## Task 18: Operator preset / recipe — Track B draft

**Description:** Document and/or add init/config helpers for the Path A recipe:
OpenAiCompatible → proxy `/v1`, dummy key, model `claude-plan`, text-only tools.

**Acceptance criteria:**
- [x] Exact operator commands recorded (init, disable tools, start proxy, chat)
- [x] Recipe uses isolated homes for smoke; warns about prod homes
- [x] No new provider kind required

**Verification:**
- [x] Flags cross-checked against rebuilt `./target/debug/gents claude-{login,auth-probe,proxy} --help` and `InitArgs`
- [ ] Manual dry run of commands against fake proxy (no Claude) where possible — deferred to Task 20 packaging smoke

**Dependencies:** Task 16 soft; finalize flags after Task 17  
**Parallel track:** B (docs-only; no `args.rs` edits)  
**Files likely touched:**
- `docs/design-notes/SPEC-claude-phase6-packaging.md` (operator recipe section)

**Estimated scope:** S–M

**Status:** DONE (docs). Operator recipe finalized against verified Task 17 flags.

---

## Task 19: backends.md + spike status — Track B draft

**Description:** Add Claude Max subscription row to `docs/backends.md` and mark
Phase 6 Path A status on the design note.

**Acceptance criteria:**
- [x] backends.md describes loopback completer path, billing = plan meter, no oat
- [x] Distinguishes from Console API-key Anthropic usage
- [x] Parent spike note links Phase 6 SPEC and packaging status

**Verification:**
- [x] Manual doc review against Task 17 help/probe output and Task 18 recipe

**Dependencies:** Task 18 soft for final wording; draftable now from SPEC  
**Parallel track:** B (docs-only)  
**Files likely touched:**
- `docs/backends.md`
- `docs/design-notes/claude-subscription-spike.md`

**Estimated scope:** S

**Status:** DONE (docs). Matrix row + Path A section in backends.md; spike status marked Phase 6 Path A partial / packaging next.

---

## Checkpoint: Packaging docs/code ready for live smoke (after Tasks 15–19)

- [x] Completer fixtures green
- [x] Proxy fake path green
- [x] Login/probe present; no oat writes (spike probe verified)
- [x] Docs/recipe present (Tasks 18–19)
- [x] Human OK before gated reproduction smoke

---

## Task 20: GATED — packaging reproduction smoke

**Description:** One approved end-to-end reproduction on isolated home/config:
probe → proxy → gents text turn → confirm documents + no oat. Requires
**CLAUDE WRITE REQUEST #4**.

**Acceptance criteria:**
- [x] Write request #4 approved before live Claude
- [x] Assistant text completes (e.g. `pong`)
- [x] `OAuthCredential` still 0 for Anthropic/Claude tokens
- [x] `AgentToolCall` = 0 / tools disabled
- [x] Evidence note under `.scratch/claude-spike/logs/` or packaging log path

**Verification:**
- [x] Manual: correlate proxy/completer logs with gents GraphQL harvest
- [x] Abort on `tool_use` or unexpected auth/API-key path

**Dependencies:** Tasks 15–19 + human write approval #4  
**Files likely touched:**
- logs only (plus todo checkboxes)

**Estimated scope:** S (execution), gated

**Status:** DONE. Write #4 approved; Rust `gents claude-proxy` on `:8787` + spike chat returned `pong` (`request_id=6ef9078d-…`). GraphQL: `OAuthCredential=0`, `AgentToolCall=0`, 2 completed InferenceCalls on OpenAiCompatible/`claude-plan`. Proxy log: 2× `mode=claude`, `had_tools_fields=false`. Evidence: `.scratch/claude-spike/logs/task20-packaging-evidence.md` (+ `task20-raw.json`).

---

## Checkpoint: Phase 6 Path A complete

- [x] Task 20 evidence filed
- [x] SPEC success criteria checked
- [x] A2 explicitly still deferred
- [ ] Ready for human merge/review decision (separate from this spike gate)

---

## Post-packaging: GATED — Codex shim → Claude Path A smoke

**Description:** Prove the personal goal path one step further: launch
`gents codex` against the spike Codex shim and complete a text-only turn on the
Claude Max subscription completer. Requires **CLAUDE WRITE REQUEST #5**.

**Acceptance criteria:**
- [x] Write request #5 approved before live Claude
- [x] Codex UI / owned-loop assistant text completes (`pong`)
- [x] Traffic path is shim `:9293` → OpenAiCompatible → Rust `claude-proxy` → Claude seat
- [x] `OAuthCredential` still 0
- [x] `AgentToolCall` = 0 / tools disabled
- [x] Evidence note under `.scratch/claude-spike/logs/`

**Verification:**
- [x] Manual: correlate Codex pane, proxy-requests.jsonl, and GraphQL harvest
- [x] Abort on `tool_use` or unexpected auth/API-key path

**Dependencies:** Task 20 + human write approval #5  
**Files likely touched:**
- logs / todo / plan only

**Estimated scope:** S (execution), gated

**Status:** DONE. Write #5 approved; `gents codex --remote ws://127.0.0.1:9293/ --no-alt-screen "Reply with exactly: pong"` returned `pong` (`request_id=0c978846-…`, session `76144f4d-…`). GraphQL: `OAuthCredential=0`, `AgentToolCall=0`, OpenAiCompatible/`claude-plan` InferenceCalls completed. Proxy: live `mode=claude`, `had_tools_fields=false`. Cleanup overshoot created one extra text turn (`a133b12c-…`) that correctly reported no tools; documented in evidence. Evidence: `.scratch/claude-spike/logs/write5-codex-evidence.md` (+ `write5-raw.json`, `write-request-5.md`).

---

## A2a unified prod suite (approved 2026-08-31)

**SPEC:** `docs/design-notes/SPEC-claude-a2a-unified-suite.md`  
**Local handoff (not in git):** `.scratch/claude-spike/handoff/claude-a2a-handoff.md`

**Success bar:** `gents server` (+ Codex) on prod; `/model` shows Grok + Claude full IDs; no manual `claude-proxy` process.

### Difference reminder
- **A2a managed proxy:** server spawns/supervises existing `gents claude-proxy` child (fast).
- **A2b in-process:** server calls Claude completer directly, no HTTP child (later).

- [x] **A2a-0** Commit/stabilize Path A model catalog leftovers (full IDs + `--model` forwarding + docs) — `6dd89732`
- [x] **A2a-1** Register Claude OpenAiCompatible backend on prod `~/.gents` beside Grok — catalog write done; `/model` re-prove after A2a-2 re-lands
- [x] **A2a-2** `gents server` manages Claude proxy lifecycle (`--claude-proxy`, required `--claude-config-dir`, healthz, clean shutdown)
  - Kept `36444a55`; hang fixed by `start_managed_claude_proxy` (spawn then healthz) + fail-path timeouts.
- [x] **A2a-3** Docs/recipe for unified prod suite (backends.md Path A + managed proxy); spike home remains historical until A2a-4 green
- [x] **A2a-4** **GATED** live verification — human-confirmed complete (2026-09-01). Claude text turns through managed proxy + Grok in unified prod suite; Path A contracts held (oat-free / text-only / write gate). Formal evidence pack may still be backfilled under `.scratch/claude-spike/logs/` if desired.

### Checkpoint: A2a complete
- [x] Operator can start suite without manually launching `claude-proxy`
- [x] Prod Codex `/model` lists Grok + Claude full IDs
- [x] Claude path remains oat-free / text-only under Path A policy
- [x] Managed proxy hang fix landed (`8bfe991c`)
- [ ] Human merge/review of A2a branch slice (optional; separate from A2b)

---

## A2b first-class in-process Claude provider (locked 2026-09-01)

**SPEC:** `docs/design-notes/SPEC-claude-a2b-in-process.md`  
**Status:** Locks accepted. Implementation may start at A2b-1.

### Locks
- **B2** new `BackendProviderKind` (working name `ClaudeCliSubscription`) — Claude is a different provider
- Server enablement: `--claude-config-dir` (not `--claude-proxy`)
- Delete standalone `gents claude-proxy` after cutover
- **Text-only in A2b**; tool bridging that mirrors OpenAI/Grok = **A2c** after A2b
- Write gate: refuse-closed **`--claude-write-approved` flag** (drop env double-gate from normal path)
- Continue on `spike/claude-subscription-plan`

- [x] **A2b-0** Lock architecture / write-gate / deletion / tools split
- [x] **A2b-1** Provider kind + in-process dispatch + `--claude-write-approved` (unit/CLI only; no live Claude)
- [x] **Interjection (before A2b-2):** Grok SSE / system-failure retry — `session fork` at last complete human user turn; strip `id: null` reasoning at the provider boundary so a dropped stream cannot brick the next turn. Fork retries the user prompt; it does **not** resume the tool loop. Report: `.scratch/claude-spike/handoff/grok-sse-drop-report.md`
- [x] **A2b-2** Migrate Claude `InferenceBackend` off OpenAiCompatible/:8787; update docs/recipe — verified 2026-09-01T21:35Z against live PID `96339` serving `./target/debug/gents` inode `44267548` (mtime Sep 1 17:14). `config backend list/show` → `ClaudeCliSubscription`, endpoint `claude-cli://subscription`, `openai_wire_api: null`. Commits: `665d8494`, `9154f490` (+ interjection `63ff2ff3`/`0b89e688`).
- [x] **A2b-3** Delete `claude-proxy` command + managed-proxy flags/path; align login gate to flag — verified 2026-09-01T22:15Z. Focused `claude_` unit tests 14/14 (`gents`) + 14/14 (`gents-cli`); rebuilt `./target/debug/gents` inode `44285772` (mtime Sep 1 18:14). `gents --help` has no `claude-proxy`; `gents claude-proxy` exits 2 unrecognized; `claude-login --help` documents refuse-closed `--claude-write-approved`; `server --help` keeps seat flags only (no `--claude-proxy*` / `--claude-model`).
- [x] **A2b-4** **GATED** live verification (numbered Claude write): in-process pong, no :8787, oat=0, tools=0 — verified 2026-09-02T17:20Z. Server PID `11716` serving `./target/debug/gents` inode `44344071` with `--claude-config-dir` + `--claude-write-approved`. New session `f21bccc9` request `4d39f8f0` → content `pong`. Both InferenceCalls on `:claude-backend`; `:8787` closed; `OAuthCredential` Claude=0; `AgentToolCall=0`; two `RenderedRequest` rows `capture_seam=process_cli`. Evidence: `.scratch/claude-spike/logs/a2b4-pong-evidence.md`. Capture seam still **uncommitted**.
- [x] **A2b-5** Draft A2c tool-bridging SPEC only (Lean starting point); no implementation — `docs/design-notes/SPEC-claude-a2c-tool-bridging.md`. Landing redesigned as its own track: `docs/design-notes/PR-STACK-claude-track-b-tools.md`.
- **Track A P1–P4 done** (text-only) on `spike/claude-p4-write-gate`. **Track B is separate**, purpose-sliced: B1 wire evidence, B2 fake owned-loop round-trip (Lean is how it lands), B3 live tool-capable seat, B4 spawn later. B1 and B2 do not block each other. Do not start B3 without a B1 wire. Do not treat A2c-0…4 as P5.

**A2b-1 done (2026-08-31):**
- `BackendProviderKind::ClaudeCliSubscription` wired through owned-loop / oneshot / openai_wire / completion_factory / CLI preset
- `crates/gents/src/claude_subscription.rs` Completer client + process seat (`--claude-config-dir`, refuse-closed `--claude-write-approved`, fake completer bypass)
- Fake-only tests green: `claude_subscription` (4) + CLI parse/seat install (5)
- No live Claude; proxy still present until A2b-3

**A2b-2 closeout evidence (2026-09-01):**
- Listener `:9191`/`:9292` = PID `96339` txt=`/Users/edjroz/Repos/source/gents/target/debug/gents` (same inode as rebuilt binary)
- No restart needed (old handoff PIDs `87593`/`87694` already gone)
- Claude backend show: `provider_kind=ClaudeCliSubscription`, `endpoint=claude-cli://subscription`, `openai_wire_api=null`, four full model IDs
- Zero live Claude traffic during verification

**A2b-3 closeout evidence (2026-09-01):**
- Deleted `gents claude-proxy` command + `claude_proxy.rs`
- Deleted managed-proxy flags/path from `serve.rs` / `args.rs` (`--claude-proxy*`, `--claude-model`)
- Deleted `crates/gents/src/claude_completer/proxy.rs`; kept `DEFAULT_MODEL_ID` / `PATH_A_MODEL_IDS` / `live_claude_allowed` on `claude_completer`
- Aligned `gents claude-login` live gate to refuse-closed `--claude-write-approved` (dry-run ungated)
- Docs: `docs/backends.md` + SPEC success checkbox for proxy deletion
- `GENTS_SKIP_LENS_BUILD=1 GENTS_SKIP_CALLBACK_WASM_BUILD=1 cargo check -p gents -p gents-cli` Finished ok
- `cargo test -p gents --lib claude_` → 14 passed (log: `.scratch/claude-spike/logs/a2b3-gents-claude-tests.log`)
- `cargo test -p gents-cli --lib claude_` → 14 passed (log: `.scratch/claude-spike/logs/a2b3-cli-claude-tests.log`)
- Help smoke on rebuilt binary inode `44285772`: no `claude-proxy` command/flags; login gate documented
- Zero live Claude traffic; running server PID `96339` still on pre-A2b-3 binary (no restart required for A2b-3)

**Not in A2b:** tool bridging, oat storage, desktop UI, keeping HTTP proxy for debug.

