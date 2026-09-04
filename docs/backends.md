# Backends

This page is the committed backend support matrix for Gents. It tracks
which providers are supported, which wire API each provider uses, what request
and response shaping Gents applies, and whether a provider has an offline
wire fixture replay fence.

The runtime owns provider-input assembly before any provider-specific client is
called. Provider-specific shaping should stay small, explicit, and tested at the
HTTP seam because live provider bugs tend to appear in headers, unsupported
parameters, response content types, and tool schema details rather than in the
agent loop itself.

## Support Matrix

| Provider kind | Wire API | Auth | Streaming | Tools | Reasoning | Request shaping | Response shaping | Fixture status |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `OpenAiCompatible` | OpenAI Responses by default; Chat Completions fallback for compatible local servers | API key or local/no-auth | SSE | Function tools through rig | Responses inherit the model default; local Chat Completions sends `chat_template_kwargs.enable_thinking` | Adds cache-scope `user` when available; does not assume every Responses model accepts `reasoning.effort` | Standard rig OpenAI handling | Planned by #545 |
| `ChatGptCodex` | ChatGPT Codex Responses endpoint | `OAuthCredential` document, refreshed by owner runtime | SSE | Function tools, forced `strict: false` to match Codex CLI | `reasoning.effort` currently fixed at `medium` | Strips unsupported `max_output_tokens`, `temperature`, `top_p`; injects instructions/store/stream defaults; adds Codex `version` and `Accept` headers | Adds missing `Content-Type: text/event-stream` only when the backend omits it; synthesizes completion body from SSE for non-streaming probes | Unit-pinned in #530; replay corpus planned by #545 |
| `XaiGrokOAuth` | Grok CLI subscription proxy (`cli-chat-proxy.grok.com`); Responses by default, `openai_wire: chat_completions` honored (the proxy serves both; the official client picks per model) | `OAuthCredential` document (`provider=xai-oauth`), refreshed by owner runtime | SSE | Function tools through rig | Not forced (several Grok models reject `reasoning.effort`) | Sets `store: false` when absent (Responses); injects Grok-CLI identity headers (`x-xai-token-auth`, `x-authenticateresponse`, `x-grok-client-*`, User-Agent) + bearer on every wire | Adds missing SSE `Content-Type` when omitted | Unit tests for headers/bearer/wire; live replay planned by #545 |
| `OpenRouter` | Chat Completions | API key | SSE | Function tools through rig | Provider-dependent | Adds OpenRouter provider preference `require_parameters: true` | Standard rig OpenRouter handling | Planned by #545 |
| local OpenAI-compatible servers | Responses or Chat Completions depending on server support | Usually none/local key | SSE varies by server | Function tools when server supports them | Reasoning parser support varies; Chat Completions sends `enable_thinking` for vLLM-style servers | Same as `OpenAiCompatible`; operators may need Chat Completions fallback for servers without `/v1/responses` | Standard rig OpenAI handling | Planned by #545 |
| `ClaudeCliSubscription` (Path A → A2b experimental) | In-process Claude CLI completer (no HTTP `/models`; endpoint placeholder `claude-cli://subscription`) | Claude CLI seat in process-local `--claude-config-dir` (**no** DefraDB `OAuthCredential` / oat) | Stream-json → owned loop SSE | Text-only: completer `--tools ""` + fail-closed on `tool_use`; tools never forwarded | N/A (text completer) | Full Claude model IDs on `InferenceBackend.models[]`; Anthropic/cloud env stripped from child; refuse-closed without `--claude-write-approved`; process-local seat-token read for health (not fleet HTTP) | Completer stdout → owned completion/stream path | Unit/fake completer fixtures; live under Claude write gate |

## Probe lifecycle and health (#640)

Backend availability composes two signals, and they deliberately live in
different places:

- **Operator/bootstrap intent** — the fleet-replicated `InferenceBackend`
  document's `enabled` and `probe_status`. The startup ratchet promotes
  `unknown → healthy` for reachable backends and stamps `last_probe`; the
  scheduled prober keeps that promotion recurring with fresh `last_probe`, and
  `gents config backend set --probe-status ...` remains the manual
  override. Nothing ever writes `unhealthy` here: reachability is
  observer-relative, and 16 runtimes stomping one document would replicate
  churn and conflicting opinions.
- **Measured health** — each runtime's scheduled prober (default: every 60s,
  10s timeout) probes the models endpoint of every enabled, probeable backend
  and keeps an in-memory `BackendHealthMap`. Hysteresis is K=3 consecutive
  failures to demote to `unhealthy`, one success to promote back (formal
  model: `crates/gents/proofs/Proofs/BackendHealth/`). Backends that
  `skips_fleet_http_probe()` are never HTTP-probed (no `/models` round-trip).
  Agent-scoped OAuth (`ChatGptCodex`, `XaiGrokOAuth`) is therefore never
  demoted by this prober — the document status governs them. `ClaudeCliSubscription`
  is also skipped for HTTP, but this runtime still runs a **process-local**
  seat-token read (`probe_process_seat_health`; missing or expired token counts
  as failure; K=3 demotes in `BackendHealthMap` only — the replicated document
  is not stamped `unhealthy`; a document born `unknown` is promoted to
  `healthy` on the first passing cycle).

Effective availability is `intent AND NOT measured-unhealthy`: a measured
demotion removes the backend from admission and marks dependent behaviors
unavailable within `probe_interval × K + reconcile debounce`, and one
successful probe restores routing. Measured state resets on restart (a dead
backend is doc-available again for up to K probe intervals until re-demoted).

The `gents_backend_probe_status{backend_id,status}` metric reports the
MEASURED state with value 1 iff healthy — it genuinely reads 0 during an
outage — and `gents_backend_last_probe_seconds` reports probe freshness.
Both fall back to document values for backends the prober has no opinion on.

## Wire Fixture Policy

Provider fixture replay is tracked in #545. Recorded fixtures live under
`crates/gents/tests/fixtures/providers/` and must be safe to commit.

Rules:

- No access tokens, refresh tokens, API keys, account ids, or bearer strings in
  fixtures.
- Redaction happens before writing fixtures to disk.
- Fixture replay should assert every recorded request is consumed exactly once.
- Fixture refresh is a live/operator action; CI should replay committed fixtures
  offline.

The fixture directory has a regression test that scans committed fixture files
for common credential patterns. The scanner is intentionally conservative: if a
new provider introduces another credential shape, add it to the scanner before
committing fixtures.

## ChatGPT subscription (ChatGptCodex, OAuth)

Use a ChatGPT/Codex subscription instead of an API key. The credential is stored
as an `OAuthCredential` DefraDB document scoped by `agent_did` and provider
(`chatgpt-codex`), not in `~/.codex`.

### Setup

1. Configure a backend with `provider_kind = ChatGptCodex`.
2. Sign in and write the credential document:

   ```sh
   gents codex-login --agent-did did:key:...
   ```

   Add `--device-auth` for headless login, and `--graphql` to write to a running
   node instead of the local home.
3. Verify:

   ```sh
   gents codex-auth-probe --agent-did did:key:...
   ```

### Models

- **Default:** the `chatgpt-codex` preset defaults the model to **`gpt-5.5`**, so
  `gents init --backend-preset chatgpt-codex` works without `--model-name`.
- **Use plain `gpt-5.x` slugs, not `-codex` variants.** A ChatGPT subscription serves
  models like `gpt-5.5`; the `-codex` variants (`gpt-5.2-codex`, …) return
  *"not supported when using Codex with a ChatGPT account"*.
- **List your account's models:**

  ```sh
  gents config backend discover-models \
    --graphql <url> --backend-id <id> --agent-did did:key:...
  ```

  The returned set is what the account can actually use — it is gated server-side by
  plan and by the advertised Codex client version (see below). An empty list usually
  means a stale client version.
- **Change the model:** pass `--model-name <slug>` to `init`, or update the behavior
  with `gents config behavior set --backend-id <id> --model-name <slug>`.
- **Client version gate.** The backend gates model availability on the Codex client
  version Gents advertises (currently `0.138.0`, on both the request `version`
  header and the `/models` `client_version` query param). If a newer floor is required,
  set `GENTS_CHATGPT_CODEX_CLIENT_VERSION` — one knob moves it everywhere.
- **Reasoning effort** is currently fixed at `medium`; per-behavior effort selection
  (e.g. `xhigh`) is tracked in #540.

### Wire-shaping guarantees

The ChatGPT Codex path is stricter than hosted OpenAI Responses in several
places. Regression tests pin these details:

- unsupported top-level params are stripped: `max_output_tokens`, `temperature`,
  `top_p`
- function tools are sent as `strict: false`
- the Codex client version is sourced from one accessor and used for both the
  request `version` header and `/models?client_version=...`
- `Accept: text/event-stream, application/json` is sent
- a missing SSE `Content-Type` is filled as `text/event-stream`, while a
  backend-supplied content type is preserved

### Credential storage

- `gents codex-login` uses Codex's OAuth flow with an ephemeral in-memory
  store, then writes the resulting access token, refresh token, id token, account
  id, plan, FedRAMP flag, and expiry into `OAuthCredential`.
- The runtime reads `OAuthCredential` for the behavior's `agent_did`; it does not
  read `CODEX_HOME` or `~/.codex` for ChatGPT backend auth.
- v1 stores token fields as plaintext document fields, matching the current
  `InferenceBackend.api_key` precedent. Filtered replication must scope the
  credential to the owning `agent_did`; encrypted token fields are the next slice.

### Fleet / remote

- OAuth refresh rotates the refresh token, so only the owner node for the
  `(agent_did, behavior)` should refresh and write the document. Replicas can use
  the current access token and receive the rotated document through replication.
- Owner election across nodes is not yet wired: every runtime currently builds
  the bearer as the owner, so the single-writer guarantee relies on the routing
  model placing each `(agent_did, behavior)` on exactly one deployment. Do not
  replicate an `OAuthCredential` to a second node that also runs the same
  `(agent_did, behavior)` until owner derivation lands (a later slice).
- When replicating credentials, use an agent-scoped filter such as
  `OAuthCredential:agent_did=did:key:...`; do not include `OAuthCredential` in an
  unfiltered config replicator.
- The single-node/local demo path treats the local runtime as the owner.
- A remote frontend (`gents codex`) does not need local ChatGPT credentials;
  the server-side runtime uses the replicated `OAuthCredential` document.

### Token refresh

- The ChatGPT HTTP client asks a `DbCredentialBearer` for a bearer before every
  request. All clients for one `credential_id` share a single bearer (cache and
  refresh lock) per process, so the rotating refresh token has exactly one
  in-process writer.
- If the access token is near expiry and this runtime is the owner, it posts the
  refresh token to OpenAI's token endpoint, writes the rotated tokens back to
  `OAuthCredential`, then sends the request.
- If the provider rejects a live request with 401/403, the bearer is invalidated
  so the next request forces a refresh rather than replaying a clock-fresh but
  server-revoked token. Runtime errors still tell the operator to rerun
  `gents codex-login` when a refresh cannot recover.

### Diagnostics

- `gents codex-auth-probe` reads the credential document (read-only; it
  never refreshes — the owning runtime is the single refresh writer, so a second
  writer would trip the provider's reuse-detection), probes `/models`, and prints
  account, plan, expiry, and reachable models.
- `gents diagnose` reports `checks.chatgpt_auth` as structured JSON with
  `credential_id` and `expires_at`, or an actionable `gents codex-login`
  guidance string when the document is missing or expired.

## Grok subscription (XaiGrokOAuth, OAuth)

Use a SuperGrok or eligible X Premium+ subscription instead of minting a
`console.x.ai` API key. The credential is an `OAuthCredential` document scoped
by `agent_did` and provider (`xai-oauth`), parallel to ChatGPT Codex.

Spike facts (auth endpoints, public client id, proxy headers) live in
[`docs/design-notes/xai-grok-oauth-spike.md`](design-notes/xai-grok-oauth-spike.md).

### Setup

1. Configure a backend with `provider_kind = XaiGrokOAuth` (preset `xai-oauth`
   / `grok-oauth`). Default endpoint:
   `https://cli-chat-proxy.grok.com/v1`. Default model: `grok-4.5`.
2. Sign in (device-code; works over SSH without a loopback callback):

   ```sh
   gents grok-login --agent-did did:key:...
   ```

   Aliases: `gents xai-login`. Use `--graphql` to write to a running node.
3. Verify (read-only; does not refresh):

   ```sh
   gents grok-auth-probe --agent-did did:key:...
   ```

Model discovery and the probe query the proxy's `/models-v2` catalog (the
path the official Grok CLI uses); entries are identified by `model` /
`modelId`. The wire API defaults to Responses; if a model turns out to be
Chat-Completions-only, set `openai_wire: chat_completions` on the backend
document — unlike `ChatGptCodex`, the setting is honored for this provider.

### Endpoint choice

| Path | Base URL | Auth |
| --- | --- | --- |
| **Subscription OAuth (this provider)** | `https://cli-chat-proxy.grok.com/v1` | SuperGrok / X Premium+ OAuth bearer + Grok-CLI identity headers |
| **API key (existing OpenAI-compatible)** | `https://api.x.ai/v1` | `XAI_API_KEY` on a generic `OpenAiCompatible` backend |

Do **not** send a subscription OAuth bearer to `api.x.ai` expecting free quota —
that surface commonly returns **402** spending-limit / **403** tier errors for
subscription tokens. Use the CLI chat proxy for OAuth.

### Failure modes

| Symptom | Meaning | Fix |
| --- | --- | --- |
| Login succeeds, inference **401** | Expired / revoked grant | `gents grok-login` again |
| Login succeeds, inference or refresh **403** tier / permission | Account not entitled to OAuth API | Not fixed by re-login; use `XAI_API_KEY` + `api.x.ai`, or check SuperGrok tier |
| Inference **402** on `api.x.ai` with OAuth token | Wrong base URL for subscription | Point the backend at `cli-chat-proxy.grok.com` |
| Proxy **402/426** without client headers | Client not recognized as CLI | Gents injects Grok-CLI identity headers automatically |

### Credential storage & fleet

Same document model and owner-only refresh rules as ChatGPT Codex
(`OAuthCredential`, agent-scoped filter, rotating refresh token). ChatGPT-only
fields (`chatgpt_plan_type`, `is_fedramp`) stay null/false for Grok.

### Diagnostics

- `gents grok-auth-probe` / `xai-auth-probe` is read-only.
- `gents diagnose` reports `checks.xai_auth` parallel to `checks.chatgpt_auth`.

## Claude Max subscription (`ClaudeCliSubscription`, no oat)

Use a Claude Max / Claude.ai subscription seat as a **gents-owned text
completer** without a native Anthropic Messages provider and without storing
Anthropic OAuth tokens in DefraDB.

This is **not** Anthropic Console API-key billing. The seat lives in an explicit
Claude CLI `--config-dir` installed into the server process via
`--claude-config-dir`. A2b dispatches `ClaudeCliSubscription` backends
in-process to the Claude CLI completer (placeholder endpoint
`claude-cli://subscription`). Full Claude model IDs are advertised from
`InferenceBackend.models[]` (default behavior model `claude-sonnet-5`; catalog
also lists `claude-opus-5`, `claude-haiku-4-5-20251001`, `claude-fable-5`).
Live Claude requires `--claude-write-approved` (refuse-closed otherwise).
That flag means **this process may bill the Claude subscription**. It is **off
by default** and is **not** a production default. Numbered human write approval
is still required before setting it. Do not leave a prod `gents server`
running with the flag unless you intend every Claude-backed turn to spend.

Design notes:

- [`docs/design-notes/claude-subscription-spike.md`](design-notes/claude-subscription-spike.md)
- [`docs/design-notes/SPEC-claude-a2b-in-process.md`](design-notes/SPEC-claude-a2b-in-process.md)
- Historical Path A packaging (loopback proxy era):
  [`docs/design-notes/SPEC-claude-phase6-packaging.md`](design-notes/SPEC-claude-phase6-packaging.md)

### Setup (isolated smoke homes only)

Do **not** use prod `~/.gents` or a personal `~/.claude` for packaging smokes.

1. Login / probe the Claude CLI seat (no DefraDB write):

   ```sh
   gents claude-login --config-dir "$CLAUDE_CONFIG_DIR" --dry-run
   # Live login needs numbered Claude write approval + --claude-write-approved
   gents claude-login --config-dir "$CLAUDE_CONFIG_DIR" --claude-write-approved
   # After login the command prints a `seat` object ({ok, detail}); the server's
   # periodic health cycle reports the same detail on the backend document.
   ```

2. Create / migrate a Claude backend (no `:8787`, no dummy API key):

   ```sh
   gents config backend create \
     --backend-preset claude-cli-subscription \
     --name "Claude Max CLI subscription" \
     --model-name claude-sonnet-5

   # Expand catalog to the four full IDs (CLI update does not rewrite models[]):
   # GraphQL upsert_InferenceBackend with models: [claude-opus-5, claude-sonnet-5,
   # claude-haiku-4-5-20251001, claude-fable-5]
   gents config behavior set --model-name claude-sonnet-5
   ```

   Migrating an older A2a `OpenAiCompatible` + `http://127.0.0.1:8787/v1` row:

   ```sh
   gents config backend set \
     --backend-id "$CLAUDE_BACKEND_ID" \
     --backend-preset claude-cli-subscription \
     --name "Claude Max CLI subscription"
   # Then set models[] via GraphQL as above; clear openai_wire_api / api_key.
   # `gents config backend set --backend-preset claude-cli-subscription`
   # clears sticky openai_wire_api on update.
   ```

3. Start the server with the process seat (no managed HTTP proxy required):

   ```sh
   # wiring / fake (no Claude spend)
   gents server --claude-config-dir "$CLAUDE_CONFIG_DIR"

   # live Claude (requires numbered write approval)
   gents server \
     --claude-config-dir "$CLAUDE_CONFIG_DIR" \
     --claude-write-approved
   ```

### Endpoint / billing choice

| Path | Endpoint / transport | Auth / billing |
| --- | --- | --- |
| **Claude Max A2b (this recipe)** | `ClaudeCliSubscription` + `claude-cli://subscription` (in-process CLI) | Claude.ai subscription seat in `--claude-config-dir`; plan meter; **no oat in DefraDB** |
| **Anthropic Console API key** | Anthropic API / Console-billed usage | API key / Console billing — **out of Path A/A2b scope**; do not confuse with Max seat |

### Failure modes

| Symptom | Meaning | Fix |
| --- | --- | --- |
| Probe `logged_in=false` | Seat missing / wrong `--config-dir` | Re-run login against the intended config dir (write-gated) |
| Completer refuse / missing seat | Server started without `--claude-config-dir` | Restart with `--claude-config-dir` pointing at the seat |
| Live Claude refused | Write gate closed | Pass `--claude-write-approved` only after numbered approval |
| Unexpected Claude spend | Server started with `--claude-write-approved` and a Claude-backed behavior | Restart **without** the flag; keep default behavior on Grok |
| Completer errors on `tool_use` | Tools leaked into Claude path | Keep text-only; do not enable Claude tools |
| Expecting DefraDB Claude credential | Wrong mental model (Grok/Codex-shaped) | Path A/A2b never upserts `OAuthCredential` for Claude |
| Fleet probe demotes Claude | Old binary still HTTP-probes the placeholder | Rebuild/restart A2b+; `ClaudeCliSubscription` skips fleet HTTP probes |

### Credential storage

Seat truth lives only in Claude CLI config under `--claude-config-dir`.
`gents claude-login` prints a `seat` object (`ok`, `detail`) after login and
explicitly sets `oauth_credential_written=false`; there is no separate probe
command — the server's health cycle reports the same seat detail. There is no Claude refresh writer in gents for
Path A/A2b.

### Unified prod suite (A2b in-process)

Fold Claude into the **prod** home so Codex `/model` lists Grok **and** Claude
together:

1. Register a `ClaudeCliSubscription` backend on prod `~/.gents` with endpoint
   `claude-cli://subscription` and the four full Claude model IDs. Keep the
   existing Grok/`XaiGrokOAuth` backend. Do **not** create a Claude
   `OAuthCredential`. Do **not** point Claude at `http://127.0.0.1:8787/v1`.
2. Start the server with the process seat. Default (no spend):

   ```sh
   gents server --claude-config-dir "$CLAUDE_CONFIG_DIR"
   ```

   Live Claude is opt-in after numbered write approval. Do **not** treat
   `--claude-write-approved` as the usual prod command line:

   ```sh
   gents server --claude-config-dir "$CLAUDE_CONFIG_DIR" --claude-write-approved
   ```

   Keep the **default behavior** on Grok unless you explicitly point a
   behavior at the Claude backend.

3. Chat with one surface:

   ```sh
   gents codex --remote ws://127.0.0.1:9292/
   ```

`/model` should show Grok models and the Claude Max IDs. Claude remains
text-only under A2b. There is no HTTP `claude-proxy` / `:8787` path anymore —
use `--claude-config-dir` + `ClaudeCliSubscription` only.
