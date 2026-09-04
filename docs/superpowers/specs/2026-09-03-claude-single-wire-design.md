# Claude subscription backend: single-wire Messages HTTP

**Date:** 2026-09-03
**Status:** design approved in-session; not committed (working file)
**Branch under change:** `spike/claude-b3-live-tools` → PR stack off `main`
**Supersedes:** the two-wire Completer described in `docs/design-notes/PR-STACK-claude-track-b-tools.md`
and the C2 lock in `docs/design-notes/SPEC-claude-a2c-tool-bridging.md` where they conflict.

## 1. Why

Track A (text) and Track B1–B3 (tools) shipped a Completer with two wires: `claude -p --tools ""`
(process CLI) for empty-surface turns, and `POST /v1/messages` (Messages HTTP) for tool-capable
turns. The split existed because Messages HTTP 429'd on the seat's OAuth token. Write request #7
(2026-09-03) showed the 429 was identity-based routing: with `You are Claude Code, Anthropic's
official CLI for Claude.` as `system[0]`, Messages HTTP works for tool turns on the same token.

The review of the branch (`.scratch/claude-spike/logs/review-track-ab-2026-09-03.md`) found ten
confirmed defects; seven live in the process-CLI wire or the seam between the wires (system prompt
dropped on text turns, argv E2BIG, panic-to-abort in the prober, health probing a different
credential than the wire uses, the Lean fence pointing at the parser production does not use,
documents never promoted). The ponytail pass measured ~2,400 removable lines if the CLI wire goes.

Decisions taken in-session:

| Decision | Choice |
|---|---|
| Risk posture | HTTP-only; accept dependence on the undocumented identity routing |
| Finish line | Clean PR stack off `main`; unrelated commits split to their own branches |
| Lean depth | Model body assembly and tool-block accumulation; drive the HTTP parser from witnesses |
| Token expiry | Health-gate only; gents never refreshes or writes the seat |
| v1 scope | Incremental streaming, `system[]` routing + cache breakpoints, drop `claude-auth-probe`; B4 out |
| Method | Fix forward on the spike (live-verifiable at every step), then carve the final tree by path |

## 2. Target architecture

```text
owned loop → ClaudeCliSubscription.stream(request)
   │  seat = ClaudeSeatConfig { config_dir, write_approved, http: Arc<reqwest::Client> }
   │  seat installed → live; token unreadable → refuse closed
   │  token = claude_seat_auth::read_seat_access_token(config_dir)   file → Keychain → expiresAt
   └─ POST https://api.anthropic.com/v1/messages   (always; empty surface or not)
        headers: Authorization: Bearer <oat>, anthropic-version: 2023-06-01,
                 anthropic-beta: oauth-2025-04-20
        body:    model, max_tokens, stream: true, system[], messages[], tools?   (key absent when empty)
        SSE → incremental parse → RawStreamingChoice { Message | ToolCall | FinalResponse }
```

- `BackendProviderKind::ClaudeCliSubscription` and the URI `claude-cli://subscription` keep their
  names: they are persisted vocabulary. Accepted spellings shrink to `claude_cli_subscription` and
  `claude-cli-subscription`. The kind's doc says: subscription seat over Messages HTTP; the `claude`
  binary is a login-time dependency only.
- The `claude` binary is not a runtime dependency. `sanitize_child_env` / `STRIPPED_ENV_VARS` remain
  only where `gents claude-login` spawns it.
- One test seam: `install_messages_sse_fixture(...)` on the seat, fed through the same line-splitting
  path as a live body. The fake-completer seam, `--claude-fake-completer`, and the shell-script fakes
  are removed.
- One capture seam: `RenderedRequestCapturingHttpClient` (persist-before-send unchanged).
  `CaptureSeam::ProcessCli` and `ProvenanceManifest::captured_only_at` leave `gents-protocol`;
  `RenderedRequestSource::ClaudeCliSubscription` stays.
- rig's anthropic provider is not reused (rig removal is staged, #438/#439). `claude_messages.rs`
  stays native and loses `ClaudeMessagesTransport` and the self-checking `MESSAGES_BODY_ALLOWED_KEYS`
  assertion; the body shape becomes a conformance case.

### Removed

`spawn_completer`, `stream_child_stdout`, `flatten_completion_request`,
`capture_claude_cli_request` / `claim_and_capture_process_cli`, all JSONL parsing in
`claude_completer/mod.rs` (`StreamJsonlEvent`, `StreamJsonlState`, `parse_stream_jsonl`,
`content_blocks`, `completer_argv`, five fixtures), `PATH_A_MODEL_IDS`, `live_claude_allowed`,
`parse_auth_status_logged_in`, `probe_process_seat_health`'s `claude auth status` spawn,
`ClaudeSeatConfig.log_dir`, `--claude-bin`, `--claude-workdir`, `--claude-log-dir`,
`--claude-fake-completer`, `gents claude-auth-probe`, `validate_claude_seat_args`, the Security.framework
Keychain fallback, `inference_backend.rs` clear-fields machinery, `claude-max-cli` spellings.
Defects C2, C5, C7 disappear with this list.

## 3. Lean model and conformance fence

File: `crates/gents/proofs/Proofs/PromptAssembly/ClaudeMap.lean` (extended, not replaced). Zero `sorry`.

### 3a. System assembly

- `splitSystem : List Msg → List String × List Msg` — pulls `System` rows out of the transcript in
  order; every other message is untouched.
- `identity : String` — the Claude Code identity block, a vocab constant checked against Rust's
  `CLAUDE_CODE_IDENTITY` by the lean-vocab test.
- `systemBlocks (preamble : Option String) (rows : List String) : List String :=
  identity :: (preamble.toList ++ rows)`.
- Theorems: `systemBlocks_head` (`system[0]` is always the identity), `systemBlocks_tail_verbatim`
  (preamble then System rows, order and text preserved), `splitSystem_partition` (system rows ++
  remaining = input up to the split; the remaining list contains no `System` row).

### 3b. Tool-block accumulation

- Events for one block: `start id name (input : Option Json) | delta partial | stop`.
- `accumulate (start : Option Json) (deltas : List String) : String` — `String.join deltas` when
  `deltas ≠ []`, otherwise the serialized start input. Theorem `accumulate_ignores_start_when_streamed`:
  the `{}` ++ deltas shape (defect C1) is unrepresentable.
- Parser state `Idle | InTool`. `start` in `InTool` → `MapError.overlappingBlock`; `stop` in `Idle`
  is ignored; end-of-stream in `InTool` flushes the block. `duplicateId` reuses the existing
  `mapToolUse` seen-set; unmapped names fail closed as today.

### 3c. Tools key

`toolsField : List Tool → Option (List Tool)` is `none` on `[]`; the wire never carries `tools: []`.

### Conformance

- Witnesses generated in `Conformance/ContractCases/PromptAssembly.lean` next to the existing
  ClaudeMap cases; `CoverageLedger` gains `systemAssembly`, `accumulate`, `overlappingBlock`,
  `emptyTools`. `surfaceOf` uses `toFinset`.
- `tests/conformance/prompt_assembly.rs::generated_claude_map_cases_drive_the_completer_parser` is
  re-pointed from `StreamJsonlState` to `claude_messages::parse_messages_sse`; the driver renders
  each witness's events as SSE `data:` lines. A sibling test drives `build_messages_body` for the
  system-array and tools-omission cases.
- Not modeled (Rust tests instead): SSE byte framing, usage extraction, headers, streaming
  incrementality.

## 4. Body and streaming

### Body (`build_messages_body`)

```json
{ "model": "...", "max_tokens": N, "stream": true,
  "system":   [ {identity}, {preamble}?, {System row}* ],    // last block: cache_control ephemeral
  "messages": [ non-system rows ],                          // last content block of last message: cache_control ephemeral
  "tools":    [ ... ]                                       // key absent when the surface is empty
}
```

- `Message::System` rows leave `messages` (no `system: …` user block) and follow identity + preamble.
- Two cache breakpoints: last `system` block (tools + system prefix) and the last content block of the
  final message (moving breakpoint for the tool_result continuation). Whether the OAuth beta honours
  caching is unknown; live probe #10 records `cached_input_tokens` either way.
- No sampling keys (`temperature`, `top_p`, `top_k`, `additional_params`); Sonnet 5 rejects them.
  `max_tokens` default unchanged.

### Streaming

- `MessagesSseState { pending, seen_ids, usage, surface }` with
  `push_line(&str) -> Result<Vec<RawStreamingChoice>, CompletionError>` and
  `finish() -> Result<Vec<RawStreamingChoice>, CompletionError>` (flushes an unterminated block,
  emits `FinalResponse` if none seen). Single parser. `parse_messages_sse(&str)` remains as the
  all-lines wrapper used by tests and the conformance driver.
- `stream_messages` wraps the response body with `try_unfold`: buffer bytes, split on `\n`, feed
  complete lines, yield events as they arrive. Text deltas reach the owned loop and the live-response
  projection during generation.
- SSE `event: error` / `{"type":"error"}` → `CompletionError::ProviderError` with type + message and
  the `request-id` header value; never the request headers. Overlapping block, duplicate id, unmapped
  tool → fail closed mid-stream.
- One `reqwest::Client` per seat (`Arc`), wrapped by `RenderedRequestCapturingHttpClient`.
  Fixture check sits at the top of `stream_messages` and uses the same line-splitting path.

## 5. Seat, health, lifecycle

### Token read (`claude_seat_auth.rs`)

`.credentials.json` → macOS Keychain via `security(1)`
(`Claude Code-credentials-{sha256(absolute config dir)[:8]}`, account `$USER`). No Security.framework
fallback. Error variants: `MissingFile`, `Io`, `Malformed`, `KeychainNotFound`,
`KeychainAccessDenied`, `KeychainAccountUnset`, `Expired { expires_at }`. `Debug` redacted; the token
is never logged. The digest test computes its expectation from a relative path.

### Health (C6)

The probe is `read_seat_access_token(config_dir)`. `Ok` → `Healthy`, detail
`source=file|keychain expires_at=<rfc3339>`. `Err(e)` → `Unhealthy`, detail is `e`'s display;
`Expired` / `MissingFile` include `run gents claude-login --config-dir <dir>`. No expiry margin.

### Promotion (C9)

The Claude branch of the health cycle produces the same `(status, detail)` as the HTTP branch and
falls through to the shared recording code; `record_probe_event`'s extraction and
`promote_document_on_unknown` are removed. A Claude `InferenceBackend` born `unknown` is promoted to
`healthy` on its first passing cycle. `skips_fleet_http_probe` stays `true` and no longer implies
"never promote".

### Send time

`stream()` reads the token per request. Non-2xx → `ProviderError` with status + `request-id` (through rig's reqwest transport the pre-checked non-2xx path carries no headers; the error then carries status plus a bounded body prefix — follow-up: transport-level status check); the
loop's retry policy applies; the next probe cycle flips health. Follow-up if traces show cost:
cache the token until `expires_at`.

### CLI surface

- `gents claude-login`: unchanged except `--console` / `--sso` removed.
- `gents claude-auth-probe`: removed.
- `gents serve`: `--claude-config-dir` only (the write gate was retired 2026-09-04).
  Status JSON keeps `claude_seat`, drops `claude_fake_completer`.
- `TODO.md` `/usage` seat item stays a follow-up.

## 6. Testing

Gate on every slice: `cargo test -p gents` (full package suite) and
`cargo check --workspace --all-targets`; Lean slices also `lake build` with zero `sorry` and the
lean-vocab test.

- Conformance witnesses drive `parse_messages_sse` and `build_messages_body` (§3).
- `agent/loop_stream/tests/claude.rs` rewritten onto the SSE fixture: `tool_use` with an
  `input_json_delta` → `AgentToolCall` with the exact arguments → `tool_result` continuation → text.
- Chunk-boundary test: the fixture split at arbitrary byte offsets yields identical events; the
  first `Message` event is observable before the fixture is exhausted.
- Health tests: credentials file valid / expired / missing / malformed → expected status and detail;
  `unknown` document promoted after a passing cycle.
- Hygiene: one `install_fake_seat`, one `ping_request`, one SSE fixture helper (`pub(crate)`);
  tempdirs only; no hardcoded home directory.

### Live probes

Each is a numbered write request approved in-session, one turn, refuse-closed
(`--claude-write-approved`), evidence under `.scratch/claude-spike/logs/`, stop on any 4xx/429
(no retries), tokens never printed, default behavior model stays `claude-sonnet-5`, server stopped after.

| # | When | Bar |
|---|---|---|
| 8 | after the C1 fix, before the cut | `list_files` on `"."`: `AgentToolCall` arguments equal the requested path argument; response `listed` |
| 9 | before deleting the CLI wire | text-only turn over HTTP with the empty surface (`tools` absent), `stop_reason: end_turn`, no 400/429; title generation also over HTTP. If this fails, the cut stops |
| 10 | after body + streaming | capture shows `system[1]` and no `system:` user block; `cached_input_tokens` on turn 2 recorded; timeline shows text deltas before `message_stop` |
| 11 | after health | expired/missing seat → Unhealthy with the `claude-login` detail; restored seat → document promoted to `healthy` without hand-editing |

## 7. Sequencing

On `spike/claude-b3-live-tools`; each slice is proposed, approved, executed by a Fable 5.1 subagent,
gated, and evidenced.

1. C1 fix (input accumulation) + delta test → live #8.
2. Split creep: `fix/session-fork-retry` ← `0b89e688`, `fix/strip-idless-reasoning` ← `63ff2ff3`,
   both off `main`; revert both on the spike. C3/C4 are tracked on those branches with the note that
   they need a Lean model before review.
3. Live #9 (text-only over HTTP) via a one-line temporary routing switch, before any deletion.
4. Lean slice (§3): model, witnesses, re-pointed fence — red against today's Rust where it should be.
5. Single-wire cut (§2) + body + streaming (§4) as one slice; the step-4 fence defines its green.
   Live #10.
6. Seat, health, promotion (§5). Live #11.
7. CLI/serve trimming, docs, ponytail residue.

## 8. PR stack

Carved from the final tree by path, off `main`; each PR compiles and passes on its own.

| PR | Contents |
|---|---|
| 1 | `gents-protocol` vocabulary (`RenderedRequestSource::ClaudeCliSubscription`; `CaptureSeam` without `ProcessCli`), `BackendProviderKind::ClaudeCliSubscription` (two spellings), `claude_seat_auth.rs`, health probe + promotion, `inference_backend.rs` simplification |
| 2 | `claude_messages.rs` Completer (body, streaming parser, write gate, capture), `ClaudeCliSubscription` shell, `completion_factory` / `agent/runtime/context` / `oneshot` arms, owned-loop test |
| 3 | `ClaudeMap.lean` extensions, conformance witnesses, Rust driver (stacks on PR2 because the driver compiles against its parser; the Lean work is still done first on the spike, per §7 step 4 — PR order is a review-compilability concern, not a development-order one) |
| 4 | `gents-cli`: `claude-login`, serve flags, status JSON, `resolve_helpers`; docs |
| side | `fix/session-fork-retry`, `fix/strip-idless-reasoning` — own Lean-first reviews |

### Docs (PR4)

- `docs/design-notes/PR-STACK-claude-track-b-tools.md`: B3 done, two-wire retired, B4 still later.
- `docs/design-notes/SPEC-claude-a2c-tool-bridging.md`: C2 lock records identity block, Keychain,
  single wire; open questions #1, #3, #5, #9 closed.
- `crates/gents/proofs/README.md`: ClaudeMap map entry lists the new theorems.
- `CLAUDE.md`, under "External code is held at arm's length": Claude is reached over Messages HTTP
  with the seat's token; the `claude` binary is a login-time dependency only.
- `.scratch/claude-spike/logs/b3-live-identity-evidence.md`: argument-fidelity correction (already noted).

## 9. Constraints carried through

- No token in any log, capture, evidence file, or chat output (`sk-ant`, `Bearer` values).
- Gents never writes the seat; Claude `OAuthCredential` rows stay 0.
- No rustfmt of `crates/gents/src/lib.rs`; import-order churn kept out of the PRs.
- Leftover `SPEC-claude-a2b-in-process.md`, `TODO.md`, `tasks/a2b2-interjection-plan.md` untouched.
- Grok is not restored as the default backend; default behavior model stays `claude-sonnet-5`.
- Any code change: approved first, executed in a Fable 5.1 subagent.

## 10. Out of scope (tracked follow-ups)

- B4 spawn/subagent bridge.
- OAuth refresh by gents.
- Token caching until `expires_at`.
- `/usage` showing Claude seat usage (`TODO.md`).
- Reusing rig's anthropic provider (moot once #438/#439 land).
- Renaming `ClaudeCliSubscription` / `claude-cli://subscription`.
