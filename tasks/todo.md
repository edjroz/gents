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
- [ ] Write request #2 approved
- [ ] SSE completes with assistant text
- [ ] Proxy + completer logs correlate
- [ ] `/v1/models` still healthy

**Verification:**
- [ ] Manual: human meter watch optional
- [ ] Manual: confirm Authorization ignored / no ANTHROPIC key in child

**Dependencies:** Task 7 + write approval #2  
**Files likely touched:**
- `.scratch/claude-spike/logs/`

**Estimated scope:** S, gated

---

## Checkpoint: Proxy (after Tasks 5–8)

- [ ] Models + SSE contract green
- [ ] Live smoke approved and logged
- [ ] Human OK for gents init

---

## Task 9: Init throwaway gents home + backend

**Description:** `gents init` / backend config under `$GENTS_SPIKE_HOME` pointing
at proxy with `OpenAiCompatible` + `chat-completions` + dummy key + `claude-plan`.
No agent turn yet.

**Acceptance criteria:**
- [ ] Home under `.scratch/claude-spike/gents-home` (or documented path)
- [ ] Prod `~/.gents` untouched
- [ ] Backend endpoint = loopback proxy `/v1`
- [ ] Exact CLI command recorded in spike log

**Verification:**
- [ ] Manual: `gents config backend show` (or GraphQL) against spike home
- [ ] Manual: confirm prod home mtime unchanged if feasible

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
- [ ] Behavior used for spike has zero tools enabled
- [ ] Documented how that was achieved (init flags / config commands)

**Verification:**
- [ ] Manual: behavior show / GraphQL tool list empty for spike agent

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
- [ ] Write request #3 approved before turn
- [ ] Turn completes with assistant text
- [ ] Proxy log shows the request
- [ ] Adapter still saw no `tool_use`

**Verification:**
- [ ] Manual: human meter watch recommended
- [ ] Manual: correlate gents request id ↔ proxy log ↔ completer log

**Dependencies:** Tasks 10, 8 + write approval #3  
**Files likely touched:**
- `.scratch/claude-spike/logs/`
- spike DefraDB docs (via runtime)

**Estimated scope:** M, gated

---

## Checkpoint: Integration (after Tasks 9–11)

- [ ] Owned-loop text turn succeeded
- [ ] Prod homes untouched
- [ ] Human OK for document harvest (prefer no new Claude)

---

## Task 12: Document evidence pack

**Description:** Query spike GraphQL/home for AgentRequest, AgentMessage,
InferenceCall (or equivalent); write `phase4-evidence.md`. Prefer no new Claude.

**Acceptance criteria:**
- [ ] Terminal successful AgentRequest cited
- [ ] AgentMessage user+assistant cited
- [ ] Inference audit doc cited
- [ ] Explicit note: no Anthropic oat in DefraDB
- [ ] Optional peer check done or explicitly skipped

**Verification:**
- [ ] Manual: evidence file review
- [ ] Manual: no Claude write request opened unless evidence missing

**Dependencies:** Task 11  
**Files likely touched:**
- `.scratch/claude-spike/logs/phase4-evidence.md`

**Estimated scope:** S–M

---

## Checkpoint: Documents (after Task 12)

- [ ] Evidence pack complete
- [ ] Ready for billing correlation

---

## Task 13: Billing Go/No-Go

**Description:** Human correlates every approved Claude write (#1–#N) with plan
meter / invoice. Agent prepares checklist only; human records verdict.

**Acceptance criteria:**
- [ ] Checklist of all approved writes (id, time, purpose)
- [ ] Human verdict: Go | No-Go | Inconclusive
- [ ] Written to `.scratch/claude-spike/logs/phase5-verdict.md`
- [ ] Extra Claude probes only if human approves new write requests

**Verification:**
- [ ] Manual: human signs verdict
- [ ] If Inconclusive: listed next approved probe

**Dependencies:** Task 12  
**Files likely touched:**
- `.scratch/claude-spike/logs/phase5-verdict.md`

**Estimated scope:** S (human-led)

---

## Checkpoint: Spike decision (after Task 13)

- [ ] Verdict filed
- [ ] If Go → new plan for Phase 6 packaging (do not invent tasks here yet)
- [ ] If No-Go → stop; keep adapter/proxy as negative evidence
- [ ] All SPEC phase exits satisfied or explicitly waived by human
