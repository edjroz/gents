# Claude subscription backend: OAuth credential parity

**Date:** 2026-09-04
**Status:** design approved in-session (decisions 1–8 accepted by the user); not committed (working file)
**Builds on:** `docs/superpowers/specs/2026-09-03-claude-single-wire-design.md` (single Messages wire, shipped on
`spike/claude-b3-live-tools` and carved as PRs 1–4). This is PR 5. Development is deferred to the user's desktop;
this spec and its plan are the hand-off.

## 1. Why

The Grok and ChatGPT-Codex backends authenticate with an `OAuthCredential` document per agent DID: gents runs the
login, stores the tokens, and refreshes them through a single-flight bearer. The Claude backend authenticates with a
host-local Claude Code seat (`--claude-config-dir`, credentials file or macOS Keychain), read on every request, never
refreshed by gents. That asymmetry costs: a per-host server flag, a Keychain reader, no fleet replication of the
credential, no expiry-driven refresh (an expired seat is a hard stop until a human re-logs in), no runnability gate,
no `gents diagnose` auth check, and a login-time dependency on the `claude` binary.

Decisions taken in-session (2026-09-04):

| Decision | Choice |
|---|---|
| Whose login | Separate gents-owned login per agent DID (A). The user's own Claude Code seat is untouched; no shared refresh-token rotation |
| Login transport | Loopback PKCE listener by default; Anthropic's manual-paste redirect as the SSH-safe fallback (`--manual`). No device code exists for this client |
| Scopes | The CLI's subscription set: `user:profile user:inference user:sessions:claude_code user:mcp_servers user:file_upload` |
| Refresh | `OAuthRefreshKind::Claude` on the existing single-flight `DbCredentialBearer`; JSON body; rotation optional; owner-only |
| Runtime | Claude client gets node + agent DID like Codex/Grok; per-request bearer from the credential; Messages wire untouched; `--claude-config-dir` and the Keychain reader deleted |
| Health | One document-expiry probe shared by all three OAuth kinds (new for Grok/Codex; replaces Claude's seat read) |
| Provider kind | `ClaudeCliSubscription` / `claude-cli://subscription` keep their persisted names; doc comment updated |
| Lean | No new model; the credential lifecycle has no Lean fence for any provider today (recorded as a follow-up) |
| Sequencing | Spec → plan → subagent execution as PR 5 on top of PRs 1–4, not folded into them |

Risk posture (carried from the single-wire spec and extended): the backend already depends on the undocumented
identity routing (`system[0]`); this design adds dependence on Claude Code's public OAuth client id, authorize and
token endpoints, and scope names, all read from the shipped CLI bundle. They are client constants, not secrets, and
may change without notice. Mitigation: they live in one constants module with provenance comments, exactly as
`XAI_OAUTH_CLIENT_ID` does, and every live probe records the bundle version they were read from.

## 2. Target architecture

```text
gents claude-login --agent-did <did>                 (PKCE loopback | --manual paste)
   └─ authorize: https://claude.com/cai/oauth/authorize  client_id=9d1c250a-… scope=<subscription set>
   └─ token:     https://platform.claude.com/v1/oauth/token  (JSON body, grant_type=authorization_code)
   └─ upsert OAuthCredential { provider: "claude-subscription", agent_did, access_token, refresh_token,
                               access_token_expires_at, last_refresh, enabled }

owned loop → ClaudeCliSubscription.stream(request)
   │  client = ClaudeSubscriptionClient::new(node, agent_did)         (built in context.rs / oneshot.rs arms)
   │  bearer = shared_bearer(credential_id, DbCredentialBearer{ OAuthRefreshKind::Claude, CLAUDE_OAUTH_PRODUCT })
   │  token  = bearer.current_bearer().await   (cache → 5-min skew → single-flight refresh → persist)
   └─ POST https://api.anthropic.com/v1/messages   (unchanged body, headers, identity block, SSE parser)
```

- `claude_seat_auth.rs`, `ClaudeSeatConfig`, `install_process_seat`, `require_process_seat`,
  `probe_process_seat_health`, `probe_seat_detail`, `seat_detail`, `gents server --claude-config-dir`, the
  Keychain `security(1)` reader, and `claude_completer::{STRIPPED_ENV_VARS, sanitize_child_env}` are removed.
  The `claude` binary is no longer a dependency of any kind.
- `claude_messages.rs` keeps `build_messages_body*`, `MessagesSseState`, `stream_sse_body`, `SeatTransport`
  (renamed `MessagesTransport`), and the fixture queue. `stream_messages` gains a `bearer: &dyn BearerSource`
  parameter and loses the seat lookup; the Authorization header is built from `bearer.current_bearer()`.
- `is_agent_scoped_oauth()` includes `ClaudeCliSubscription`; `skips_fleet_http_probe()` collapses to
  `is_agent_scoped_oauth()`.

### Removed

`claude_seat_auth.rs` (whole file), `ClaudeSeatConfig` + process-seat slot, `probe_process_seat_health`,
`probe_seat_detail`, `seat_detail`, `--claude-config-dir` on `gents server` (+ status JSON `claude_subscription`
object), `ClaudeLoginArgs.{config_dir, claude_bin, dry_run, claude_write_approved, email}` and the
`claude auth login` subprocess, `claude_completer/mod.rs` (whole module; `DEFAULT_MODEL_ID` moves to
`claude_subscription.rs`), the `gents diagnose` "seat-token probe" note, `docs/backends.md` seat/Keychain prose.

## 3. Components

### 3a. `claude_oauth.rs` (new, `crates/gents/src`)

Constants with provenance comments (read from Claude Code 2.1.260 bundle, 2026-09-04):

```rust
pub const CLAUDE_OAUTH_PROVIDER: &str = "claude-subscription";
pub const CLAUDE_OAUTH_CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
pub const CLAUDE_OAUTH_AUTHORIZE_URL: &str = "https://claude.com/cai/oauth/authorize";
pub const CLAUDE_OAUTH_TOKEN_URL: &str = "https://platform.claude.com/v1/oauth/token";
pub const CLAUDE_OAUTH_MANUAL_REDIRECT_URL: &str = "https://platform.claude.com/oauth/code/callback";
pub const CLAUDE_OAUTH_SCOPES: &str =
    "user:profile user:inference user:sessions:claude_code user:mcp_servers user:file_upload";
pub const CLAUDE_OAUTH_TOKEN_URL_OVERRIDE_ENV: &str = "GENTS_CLAUDE_OAUTH_TOKEN_URL";
pub const CLAUDE_OAUTH_PRODUCT: OAuthProduct = OAuthProduct { name: "Claude subscription", login_command: "claude-login", .. };
```

`normalize_provider(&str) -> String` (mirrors `chatgpt_codex::normalize_provider`).

### 3b. Login flow (`crates/gents-claude-login`, new crate)

A sibling of `crates/gents-chatgpt-login` with the same dependencies (`tiny_http`, `url`, `rand`, `sha2`, `base64`, `webbrowser`, `reqwest`); the `gents` crate has none of the listener/URL dependencies and should not grow them for a login-time flow.

- `run_loopback_login(http, open_browser) -> Result<ClaudeLoginTokens>`: bind `127.0.0.1` on an ephemeral port
  (Claude Code uses a random port; the redirect is `http://localhost:{port}/callback`), PKCE S256 verifier +
  challenge, random `state`, print the authorize URL (`code=true`, `client_id`, `response_type=code`,
  `redirect_uri`, `scope`, `code_challenge`, `code_challenge_method=S256`, `state`), open the browser when asked,
  accept one `GET /callback?code=…&state=…`, verify `state`, exchange the code.
- `run_manual_login(http) -> Result<ClaudeLoginTokens>`: same authorize URL with `redirect_uri` =
  `CLAUDE_OAUTH_MANUAL_REDIRECT_URL`; the user pastes the code shown on Anthropic's success page (the CLI's
  "code#state" form: split on `#`, verify `state`).
- `exchange_code(http, code, verifier, redirect_uri, state) -> ClaudeLoginTokens`: `POST` JSON
  `{grant_type: "authorization_code", code, redirect_uri, client_id, code_verifier, state}`; response fields
  `access_token`, `refresh_token`, `expires_in` (seconds), optional `scope`. Non-200 → error with status only.
- `credential_from_login_tokens(agent_did, provider, tokens, now) -> OAuthCredential`: `expires_at = now +
  expires_in` (fallback 60 min), `id_token/account_id/chatgpt_plan_type = None`, `is_fedramp = false`,
  `last_refresh = Some(now)`, `enabled = true`.
- Errors never carry token bytes; transport errors are redacted like `gents-chatgpt-login::redacted_transport_error`.

### 3c. Refresh (`crates/gents/src/claude_oauth_refresh.rs`, new)

```rust
pub async fn refresh_claude_token(refresh_token: &str, http: &reqwest::Client)
    -> Result<RefreshedTokens, OAuthAuthProblem>
```
`POST CLAUDE_OAUTH_TOKEN_URL` (env override honoured) with JSON `{grant_type: "refresh_token", refresh_token,
client_id, scope: CLAUDE_OAUTH_SCOPES}`. 200 → `RefreshedTokens { access_token, refresh_token: response.or(old),
expires_at: now + expires_in, id_token: None }`. 401 or 400 `invalid_grant` → `OAuthAuthProblem::Expired`; 403 →
`NotEntitled`; other → transport error (status only). `OAuthRefreshKind::Claude` dispatches here from
`DbCredentialBearer::refresh_tokens`.

### 3d. Runtime client (`claude_subscription.rs`, rewritten)

```rust
pub struct ClaudeSubscriptionClient { node: Arc<EmbeddedNode>, agent_did: String }
impl ClaudeSubscriptionClient {
    pub async fn build(node: Arc<EmbeddedNode>, agent_did: &str) -> Result<Self>   // looks the credential up once, fails with classify_claude_auth_error(Missing)
}
pub struct ClaudeSubscriptionModel { model: String, bearer: Arc<DbCredentialBearer>, http: ReqwestClient }
```
`completion_model()` resolves `shared_bearer(credential_id, …)` with `OAuthRefreshKind::Claude` and
`CLAUDE_OAUTH_PRODUCT`. `stream()` / `completion()` call `claude_messages::stream_messages(model, request,
surface, bearer.as_ref(), &http)`. A `401` from Messages HTTP calls `bearer.invalidate()` once and returns the
error (the loop's retry policy re-drives; the second attempt refreshes). `403` is not a bearer rejection (as Grok).
`classify_claude_auth_error(agent_did, provider, problem) -> String` mirrors `classify_xai_auth_error` with the
`claude-login` hint.

`context.rs` and `oneshot.rs` `ClaudeCliSubscription` arms: `ClaudeSubscriptionClient::build(node.clone(),
behavior.agent_did()).await` under the same startup timeout as Codex.

### 3e. CLI (`crates/gents-cli`)

- `gents claude-login [--agent-did <did>] [--manual] [--no-browser] [--provider <name>]`: runs 3b, upserts the
  credential through `ConfigAccess::execute`, prints `{login: completed, credential_doc_id, agent_did, provider,
  access_token: "<redacted>", refresh_token: "<redacted>", access_token_expires_at}`.
- `gents server`: no Claude flags.
- `gents diagnose`: `checks.claude_auth` shaped like `checks.xai_auth` (credential present, `token_is_fresh`,
  guidance from `classify_claude_auth_error`); backends discovery note becomes "OAuth credential; no HTTP model
  discovery".
- Snapshot runnability gate (`document_view/snapshot.rs`): third arm — a `ClaudeCliSubscription` behavior with no
  enabled `OAuthCredential` for `claude-subscription` is not runnable; message names `gents claude-login
  --agent-did <did>`.

### 3f. Health (`backend_health.rs`)

For every backend whose kind `is_agent_scoped_oauth()`, the cycle runs a document-expiry probe instead of skipping:
read the newest enabled `OAuthCredential` for the backend's agent DID and the kind's provider; `token_is_fresh`
→ `ProbeSuccess` (detail `source=document expires_at=<rfc3339>`); stale or missing → `ProbeFail` with the
provider's `classify_*_auth_error(... Expired | Missing)` text. Recording and promotion go through the shared
`record_probe_event` (promotion of `unknown` on first success, as today). The probe never refreshes: refresh is the
owner bearer's job on the request path. The agent DID for a fleet backend document is the deployment's principal;
where a backend serves several agents, probe the deployment's own DID (documented limitation).

## 4. Data and persistence

- `OAuthCredential` schema unchanged. Claude rows: `provider = "claude-subscription"`, `credential_id =
  "claude-subscription:<agent_did>"`, `id_token/account_id/chatgpt_plan_type = null`, `is_fedramp = false`.
- Replication filter for fleets: the existing agent-scoped `OAuthCredential:agent_did=<did>` rule covers Claude
  rows with no change.
- Owner-only refresh: `DbCredentialBearer::with_cache(.., is_owner = true, ..)` on the process that built the
  client, exactly as Codex/Grok; owner election remains the tracked fleet follow-up it already is.
- Persisted vocabulary unchanged: `ClaudeCliSubscription`, `claude-cli://subscription`,
  `RenderedRequestSource::ClaudeCliSubscription`.

## 5. Error handling

| Situation | Behaviour |
|---|---|
| No credential for the agent | Behavior not runnable at reconcile (snapshot gate); `completion_model()` fails closed with the login hint if reached |
| Token stale within 5 min | Bearer refreshes single-flight before the send; refreshed row persisted with retry |
| Refresh 401/400 invalid_grant | `Expired` → send fails with the login hint; health goes ProbeFail on the next cycle |
| Messages HTTP 401 | `bearer.invalidate()` once; error returned; loop retry re-sends with a refreshed token |
| Messages HTTP 429 / 4xx | Unchanged from the single-wire spec (status + bounded body prefix) |
| Persist failure after refresh | Served from memory with an error log, as Codex/Grok today |

No token reaches a log, capture, error string, `Debug`, or CLI output; login output redacts all tokens.

## 6. Testing

Gates: `cargo test -p gents` (full package), `cargo test -p gents-cli`, `cargo check --workspace --all-targets`,
`cargo fmt --check`. No Lean change, so `lake build` is unaffected; the lean-vocab snapshot still builds.

- `claude_oauth_login`: PKCE challenge derivation; authorize URL query set (golden); loopback callback with a
  wrong `state` rejected; manual code `code#state` parsing; code exchange against a local mock token endpoint
  (JSON body asserted; tokens never in error text).
- `claude_oauth_refresh`: 200 with and without rotated `refresh_token`; 401 and 400 `invalid_grant` → `Expired`;
  403 → `NotEntitled`; env override honoured.
- `claude_subscription`: `build` fails closed without a credential (message contains `gents claude-login`);
  fixture-driven stream with a seeded credential document on an embedded test node; stale credential triggers one
  refresh against the mock endpoint before the send; `401` invalidates once.
- `claude_messages`: unchanged tests; `stream_messages` takes a bearer stub in tests.
- Owned-loop test (`agent/loop_stream/tests/claude.rs`): seeded credential document instead of `install_fake_seat`.
- Health: fresh / stale / missing credential → success / fail with hint / fail with hint; promotion of `unknown`
  on first success; the probe never calls the refresh endpoint (mock asserts zero hits).
- CLI: `claude-login` arg parsing; login output redaction; `diagnose` `claude_auth` shapes; snapshot gate.
- Conformance: the seam scans stay green (no rig vocabulary; no provider invocation outside the loop seam).

Live probes (numbered, on the desktop; evidence under `.scratch/claude-spike/logs/`):

| # | Bar |
|---|---|
| 13 | `gents claude-login --manual` against the real authorize page: credential row written, tokens redacted in output, `expires_at` ≈ +8 h |
| 14 | One tool turn over the wire with the credential bearer: same bar as #10b; `cached_input_tokens` on turn 2 |
| 15 | Force the credential stale (edit `access_token_expires_at` to the past on the document): next turn refreshes once (mock-free: real endpoint), row rotated, turn completes; health reports fresh |
| 16 | `gents diagnose` shows `claude_auth.ok=true`; delete the credential → not runnable, `diagnose` guidance names `claude-login` |

## 7. Sequencing (plan tasks)

1. Constants + provider + refresh kind + refresh client (`claude_oauth.rs`, `claude_oauth_refresh.rs`, `oauth_credential.rs` arms) with mock-endpoint tests.
2. Login flow (`claude_oauth_login.rs`) + `gents claude-login` rewrite + snapshot runnability gate + `diagnose` check.
3. Runtime client on the bearer (`claude_subscription.rs`, `claude_messages::stream_messages` bearer parameter, `context.rs` / `oneshot.rs` arms, owned-loop test) and deletion of the seat model, `--claude-config-dir`, Keychain reader, `claude_completer`.
4. Shared OAuth document-expiry health probe for the three kinds; `backend_provider` predicates; tests.
5. Docs (`backends.md` Claude section rewritten onto the credential model; design-note annotations; `CLAUDE.md` sentence) and live probes #13–#16.

## 8. Constraints carried through

- No token in any log, capture, evidence file, error string, or CLI output.
- Owner-only refresh; replicas never refresh.
- `crates/gents/src/lib.rs` not rustformatted; import-order churn out of the PR.
- `docs/design-notes/SPEC-claude-a2b-in-process.md`, `TODO.md`, `tasks/` untouched.
- Default behavior model stays `claude-sonnet-5`.
- Development runs on the user's desktop under the standing approval rules; this session produces only the spec and plan.

## 9. Out of scope (tracked follow-ups)

- A Lean model of the credential lifecycle (single-writer refresh, rotation non-loss) for all three OAuth kinds.
- Owner election for fleet refresh (pre-existing follow-up).
- Renaming `ClaudeCliSubscription` / `claude-cli://subscription`.
- Importing an existing Claude Code seat (decision B, rejected).
- Multi-agent backends: which DID the health probe uses beyond the deployment principal.
