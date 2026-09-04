# Track A + Track B review — 2026-09-03

Branch `spike/claude-b3-live-tools` vs `main` (merge-base `ca615b1c`): 41 commits, 72 files, +8726/−173.
Inputs: `/code-review high main...HEAD` (8 finder angles + verify pass, 33 candidates → 10 kept) and a
ponytail-review (over-engineering only) run as a Fable subagent. Read-only; no code changed.

## Correction to B3 evidence

`b3-live-identity-evidence.md` records PASS on `AgentToolCall ≥ 1`. The live `tool_use` arrived at Gents as
`"arguments": {}` (`b3-live-identity-timeline.json`, id `toolu_01R4PwSTHFYrNQ4qJgnD7227`) although the prompt
asked for directory `"."`. Cause is finding C1 below. The PASS stands for auth/routing (429 cleared, native
`tool_result` accepted); it does **not** demonstrate argument fidelity. Next live bar must assert the tool
call's arguments.

## Correctness findings (code review, severity order, all CONFIRMED)

| # | Where | Defect | Wire |
|---|---|---|---|
| C1 | `claude_messages.rs:223-226,250,317` | `content_block_start.input` (`{}`) is seeded into `input_json`, then `input_json_delta` fragments are appended → `{}{…}` → parse fails → `unwrap_or_else(json!({}))` dispatches the tool with **empty arguments, no error**. No SSE test emits an `input_json_delta`. **Confirmed live.** | HTTP |
| C2 | `claude_subscription.rs:583-591` | `flatten_completion_request` `other =>` arm silently drops `Message::System`; the loop delivers the behavior preamble only as a System row (`request.preamble` is never set, `loop_stream.rs:979-985`), so **text-only turns send Claude none of the behavior's system prompt** and use the hardcoded `--system-prompt "You are a text-only completer…"`. The two wires give different provider input for one transcript. | CLI |
| C3 | `compaction.rs:398` (commit `63ff2ff3`) | Unconditional 4th sanitize stage `strip_idless_reasoning` for **every provider**; regresses OpenRouter reasoning round-trip (`id: None` reasoning deltas). No Lean model (`Provider.lean:14` still says three stages); `tests/conformance/prompt_assembly.rs:105` mints `rs-lean-*` ids on every witness so the stage is never exercised by spec. | n/a (scope creep) |
| C4 | `compaction.rs:449` (same commit) | `provider_view` now yields fewer rows; legacy sessions with null `compacted_through_sequence` + stored count over-drain and lose live rows at the head of the active tail. | n/a (scope creep) |
| C5 | `claude_completer/mod.rs:45`, `claude_auth_probe.rs:102` | `&text[start..=end]` with `find('{')`/`rfind('}')`, no `end >= start` check → panic on reachable CLI output; release `panic = "abort"` → prober kills the server. | CLI |
| C6 | `backend_health.rs:240` | Health probes `claude auth status` only; tool turns need a readable unexpired oat (`read_seat_access_token`) with no refresh path → backend Healthy while every tool-bearing request fails. | seam |
| C7 | `claude_subscription.rs:319` | Whole flattened transcript as one argv element, stdin null → E2BIG once history passes ~128 KiB, session stuck; prompt visible in `ps`. | CLI |
| C8 | `claude_messages.rs:227,296` | No duplicate `tool_use` id check; a second `content_block_start` before `stop` overwrites `pending`. Lean ClaudeMap proves `duplicateId`, JSONL parser fails closed, but the conformance fence (`generated_claude_map_cases_drive_the_completer_parser`) drives `StreamJsonlState` — **the production tool parser is unfenced**. `line: 0` hardcoded. | HTTP |
| C9 | `backend_health.rs:258` | Claude backends never promoted on the document (`promote_document_on_unknown=false`, startup ratchet skipped) → a row born `unknown` is permanently unroutable. | seam |
| C10 | `claude_messages.rs:435` | `stream_messages` buffers the whole body, parses after EOF, wraps a finished Vec in `stream::iter` → never streams; fresh `ReqwestClient::new()` per request. | HTTP |

Verified-but-cut: `claude_seat_auth.rs:110` maps most fs/Keychain errors to `MissingFile`; `push_line` takes text
from non-assistant JSONL lines; `collect_backend_records` silently skips unparseable rows (PLAUSIBLE);
`CLAUDE_CODE_IDENTITY` (working tree) has no Lean/spec fence; runtime vs CLI `auth status` parsers disagree on
non-zero exit; production HTTP tool path has no owned-loop test (fakes route through CLI).

Refuted: `claim_and_capture_process_cli` Resend path; fork TOCTOU.

## Ponytail (over-engineering)

Full line-level list in the subagent report; totals:

| | two-wire kept | single-wire HTTP |
|---|---|---|
| removable lines | ~630 | **~2,400** (+~200 if rig anthropic reused) |
| scope creep (`0b89e688` fork retry, `63ff2ff3` reasoning strip) | 575 → own PRs | |

Lean already: `ClaudeMap.lean`, write gate, `security(1)` seat read, `completion_factory`/`context`/`oneshot`
arms, `backend_registry`. Not lean: the process-CLI wire (`spawn_completer`, `stream_child_stdout`,
`flatten_completion_request`, all of `claude_completer/mod.rs` JSONL + 5 fixtures, `CaptureSeam::ProcessCli`,
`--claude-bin/--claude-workdir/--claude-fake-completer/--claude-log-dir`, 3 fake-completer harnesses),
`ClaudeMessagesTransport`, the self-checking `MESSAGES_BODY_ALLOWED_KEYS` assert, Security.framework fallback,
`claude_auth_probe.rs`, `inference_backend.rs` clear-fields, ten copies of the seat literal in tests.
Only thing the `claude` binary still uniquely provides: OAuth login/refresh (`gents claude-login`).

Shape mismatch: Grok/Codex = wrap rig client in an `HttpClientExt` (header swap + body patch); Claude
implements `CompletionModel` itself. Recommendation: do not move onto rig anthropic (#438/#439 stage rig out);
trim `claude_messages.rs` to native pieces instead.

## Assessment

Seven of ten correctness defects live in the process-CLI wire or the seam between the two wires
(C2, C5, C6, C7, C8-fence, C9, plus the missing HTTP loop test). The two-wire shape was a workaround for
"HTTP 429s"; that premise is gone as of write request #7. Single-wire Messages HTTP deletes C2/C5/C7
outright, lets the health probe read the same token the wire uses (C6), and lets the ClaudeMap conformance
fence point at the only parser (C8). Remaining real work on HTTP: C1 (small fix + delta test), C8 dup-id,
C9 promotion, C10 incremental stream, identity block fence.

## Proposed order (each slice: approval → Fable subagent)

1. **C1 fix** + SSE test with `input_json_delta`; live re-run with an argument-asserting bar (numbered request).
2. **Split** `0b89e688` and `63ff2ff3` off this branch (own PRs, Lean-first; C3/C4 tracked there).
3. **Single-wire slice**: `stream()` always → Messages HTTP; delete CLI wire + flags + `ProcessCli` seam;
   re-point `generated_claude_map_cases_drive_the_completer_parser` at `claude_messages::mapped_tool_call`;
   fake-completer tests → SSE fixture. Keep `gents claude-login`, `sanitize_child_env`.
4. **C8 + C9 + C6**: dup-id fail-closed on HTTP; probe via `read_seat_access_token`/`expiresAt`; promote
   Claude documents like HTTP backends.
5. **C10**: incremental SSE; shared reqwest client. Then `Message::System` → `system[1]` + `cache_control`.
6. Ponytail residue (~630 lines) folded into the slices above.
