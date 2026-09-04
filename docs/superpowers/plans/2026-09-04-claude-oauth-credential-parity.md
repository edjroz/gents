# Claude OAuth Credential Parity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Authenticate the Claude subscription backend with a gents-owned `OAuthCredential` per agent DID (login by gents, refresh by gents), the same model the Grok and ChatGPT-Codex backends use, and delete the host-local Claude Code seat.

**Architecture:** A new `claude_oauth` constants module and `claude_oauth_refresh` client add a third `OAuthRefreshKind` to the existing single-flight `DbCredentialBearer`. `gents claude-login` runs the PKCE loopback flow itself (manual-paste fallback) and upserts the credential through the existing mutation. `ClaudeSubscriptionClient` is built with node and agent DID like the Codex and Grok arms, and `claude_messages::stream_messages` takes a `BearerSource` instead of reading a seat. A shared document-expiry health probe replaces the seat read for all three OAuth kinds. The Messages wire, body assembly, SSE parser, and Lean models do not change.

**Tech Stack:** Rust (`crates/gents`, `crates/gents-cli`), reqwest, `tiny_http` loopback server pattern from `crates/gents-chatgpt-login`, sha2 + base64 (PKCE), serde_json, DefraDB GraphQL via `ConfigAccess`.

**Spec:** `docs/superpowers/specs/2026-09-04-claude-oauth-credential-parity-design.md` (decisions 1–8 accepted 2026-09-04; not committed). Builds on `docs/superpowers/specs/2026-09-03-claude-single-wire-design.md` and the PR stack `claude/pr1…pr4`.

**Branch:** `spike/claude-b3-live-tools` tip (after Task 10) → new branch `claude/pr5-oauth-parity` off `claude/pr4-cli-docs`. Executed on the user's desktop; this plan is the hand-off.

## Global Constraints

Copied from spec §8 ("Constraints carried through"):

- No token in any log, capture, evidence file, error string, or CLI output.
- Owner-only refresh; replicas never refresh.
- `crates/gents/src/lib.rs` not rustformatted; import-order churn out of the PR.
- `docs/design-notes/SPEC-claude-a2b-in-process.md`, `TODO.md`, `tasks/` untouched.
- Default behavior model stays `claude-sonnet-5`.
- Development runs on the user's desktop under the standing approval rules; this session produces only the spec and plan.

Project rules that bind every task:

- Every code change: approved first, executed in a Fable 5.1 subagent at low effort (standing rule).
- Gates: `cargo test -p gents` (full package), `cargo test -p gents-cli`, `cargo check --workspace --all-targets`, `rustup run 1.97.1 cargo fmt --all --check`. Run long gates as one background job with an `EXIT=` log line and wait on the pid; never kill a compile at a timeout. Run `gents-cli` tests with the system `TMPDIR`.
- `tracing`, never `println`. `graphql::escape_graphql_string()` for anything interpolated into GraphQL. Never emit `[]` in a DefraDB mutation.
- The seam scans `provider_invocations_are_confined_to_the_owned_loop_seam` and `docs::rig_vocabulary_confined_to_the_seam` stay green: no `.stream(` / `.completion(` outside the loop seam in production files; no `rig::completion::message::` / `rig::completion::Message` / `rig::one_or_many` outside the allowlist.
- Build env: `export GENTS_SKIP_LENS_BUILD=1 GENTS_SKIP_CALLBACK_WASM_BUILD=1 TMPDIR="$PWD/.scratch/tmp"; export PATH="$HOME/.elan/bin:$PATH"`.
- Live probes #13–#16 are numbered write requests, one per bar, refuse on 429, tokens never printed.

## Decisions recorded for the plan (not in the spec)

- The loopback listener is implemented in `crates/gents` (not by extending `gents-chatgpt-login`), copying that crate's `bind_server` / PKCE / callback shape with Claude's URL parameters. Reason: the ChatGPT crate hard-codes its authorize query (`originator`, `codex_cli_simplified_flow`) and issuer layout; parametrising it would touch a shipped login path for no gain.
- `DEFAULT_MODEL_ID` moves from `claude_completer/mod.rs` to `claude_subscription.rs`; `claude_completer` is deleted with the `claude` binary dependency.
- The health probe for OAuth kinds reads the credential document directly (`lookup_oauth_credential`), never through the bearer, so it cannot trigger a refresh.
- `stream_messages` keeps its signature shape but takes `bearer: &dyn BearerSource` and `http: &ReqwestClient` explicitly; the process-seat slot is deleted, so tests build a `StaticBearer` stub.

## File map

| File | Responsibility after this plan |
|---|---|
| `crates/gents/src/claude_oauth.rs` (new) | Provider name, client id, URLs, scopes, `CLAUDE_OAUTH_PRODUCT`, `normalize_provider`, `classify_claude_auth_error` |
| `crates/gents/src/claude_oauth_refresh.rs` (new) | `refresh_claude_token` against the token endpoint (JSON body) |
| `crates/gents/src/claude_oauth_login.rs` (new) | PKCE loopback + manual-paste login, code exchange, `credential_from_login_tokens` |
| `crates/gents/src/oauth_credential.rs` | `OAuthRefreshKind::Claude` arm |
| `crates/gents/src/claude_subscription.rs` | `ClaudeSubscriptionClient::build(node, agent_did)`, bearer-backed model, `DEFAULT_MODEL_ID` |
| `crates/gents/src/claude_messages.rs` | `stream_messages(model, request, surface, bearer, http)`; `MessagesTransport` |
| `crates/gents/src/agent/runtime/context.rs`, `oneshot.rs` | Claude arm builds the client with node + agent DID under the startup timeout |
| `crates/gents/src/agent/document_view/snapshot.rs` | Runnability gate for Claude credentials |
| `crates/gents/src/backend_health.rs`, `backend_provider.rs` | Shared OAuth document-expiry probe; `is_agent_scoped_oauth` includes Claude |
| `crates/gents-cli/src/commands/claude_login.rs`, `cli/args.rs` | First-party `gents claude-login` |
| `crates/gents-cli/src/commands/diagnose/{mod.rs,backends.rs}` | `checks.claude_auth`; discovery note |
| `crates/gents-cli/src/commands/serve.rs` | No Claude flags |
| Deleted | `claude_seat_auth.rs`, `claude_completer/`, seat slot, `--claude-config-dir` |

---

### Task 1: Constants, product, refresh kind, and the refresh client

**Files:**
- Create: `crates/gents/src/claude_oauth.rs`
- Create: `crates/gents/src/claude_oauth_refresh.rs`
- Modify: `crates/gents/src/oauth_credential.rs:47-52` (`OAuthRefreshKind`), `:658-670` (`refresh_tokens` dispatch)
- Modify: `crates/gents/src/lib.rs` (two `pub mod` lines next to the other `claude_*` modules; nothing else in that file)
- Test: inline `mod tests` in both new files

**Interfaces:**
- Consumes: `oauth_credential::{OAuthProduct, OAuthAuthProblem, RefreshedTokens, classify_oauth_auth_error, oauth_credential_id, OAuthCredential}`.
- Produces (used by Tasks 2–4):
  - `claude_oauth::{CLAUDE_OAUTH_PROVIDER, CLAUDE_OAUTH_CLIENT_ID, CLAUDE_OAUTH_AUTHORIZE_URL, CLAUDE_OAUTH_TOKEN_URL, CLAUDE_OAUTH_MANUAL_REDIRECT_URL, CLAUDE_OAUTH_SCOPES, CLAUDE_OAUTH_TOKEN_URL_OVERRIDE_ENV, CLAUDE_OAUTH_PRODUCT}`
  - `claude_oauth::normalize_provider(&str) -> String`, `classify_claude_auth_error(agent_did: &str, provider: &str, problem: &OAuthAuthProblem) -> String`
  - `claude_oauth::ClaudeLoginTokens { access_token: String, refresh_token: String, expires_in: Option<i64>, scope: Option<String> }` and `credential_from_login_tokens(agent_did, provider, &ClaudeLoginTokens, now) -> OAuthCredential`
  - `claude_oauth_refresh::refresh_claude_token(refresh_token: &str, http: &reqwest::Client) -> Result<RefreshedTokens, OAuthAuthProblem>`
  - `OAuthRefreshKind::Claude`

- [ ] **Step 1: Write the failing tests**

`crates/gents/src/claude_oauth.rs` (tests section):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::oauth_credential::OAuthAuthProblem;

    #[test]
    fn client_id_is_public_uuid_shape() {
        assert_eq!(CLAUDE_OAUTH_CLIENT_ID.len(), 36);
        assert_eq!(CLAUDE_OAUTH_CLIENT_ID.matches('-').count(), 4);
    }

    #[test]
    fn scopes_are_the_subscription_set() {
        let scopes: Vec<&str> = CLAUDE_OAUTH_SCOPES.split(' ').collect();
        assert_eq!(
            scopes,
            ["user:profile", "user:inference", "user:sessions:claude_code", "user:mcp_servers", "user:file_upload"]
        );
        assert!(!CLAUDE_OAUTH_SCOPES.contains("org:create_api_key"));
    }

    #[test]
    fn normalize_provider_defaults_and_trims() {
        assert_eq!(normalize_provider(""), CLAUDE_OAUTH_PROVIDER);
        assert_eq!(normalize_provider("  "), CLAUDE_OAUTH_PROVIDER);
        assert_eq!(normalize_provider(" claude-subscription "), "claude-subscription");
    }

    #[test]
    fn missing_credential_guidance_names_claude_login() {
        let text = classify_claude_auth_error("did:key:z6MkTest", CLAUDE_OAUTH_PROVIDER, &OAuthAuthProblem::Missing);
        assert!(text.contains("gents claude-login --agent-did did:key:z6MkTest"), "{text}");
        assert!(text.contains("Claude subscription backend"), "{text}");
    }

    #[test]
    fn credential_from_login_tokens_uses_expires_in_and_fills_claude_shape() {
        let now = chrono::Utc::now();
        let tokens = ClaudeLoginTokens {
            access_token: "access-TEST".into(),
            refresh_token: "refresh-TEST".into(),
            expires_in: Some(28800),
            scope: Some(CLAUDE_OAUTH_SCOPES.into()),
        };
        let credential = credential_from_login_tokens("did:key:z6MkTest", CLAUDE_OAUTH_PROVIDER, &tokens, now);
        assert_eq!(credential.credential_id, "claude-subscription:did:key:z6MkTest");
        assert_eq!(credential.access_token_expires_at, now + chrono::Duration::seconds(28800));
        assert_eq!(credential.id_token, None);
        assert_eq!(credential.account_id, None);
        assert_eq!(credential.chatgpt_plan_type, None);
        assert!(!credential.is_fedramp);
        assert_eq!(credential.last_refresh, Some(now));
        assert!(credential.enabled);
    }

    #[test]
    fn credential_from_login_tokens_falls_back_to_one_hour() {
        let now = chrono::Utc::now();
        let tokens = ClaudeLoginTokens { access_token: "a".into(), refresh_token: "r".into(), expires_in: None, scope: None };
        let credential = credential_from_login_tokens("did:key:z6MkTest", CLAUDE_OAUTH_PROVIDER, &tokens, now);
        assert_eq!(credential.access_token_expires_at, now + chrono::Duration::hours(1));
    }
}
```

`crates/gents/src/claude_oauth_refresh.rs` (tests section). The one-shot server is a plain `TcpListener` so the test needs no new dependency. Define it once as `#[cfg(test)] pub(crate) mod test_support` at the bottom of `crates/gents/src/oauth_credential.rs` (`pub(crate) async fn one_shot_token_server(status: u16, body: &'static str) -> (String, tokio::task::JoinHandle<String>)`, body exactly as below) and `use crate::oauth_credential::test_support::one_shot_token_server;` from every test module that needs it (Tasks 1, 3, 4):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// Accepts exactly one HTTP request, returns its body, and answers with `status` + `body`.
    async fn one_shot_token_server(status: u16, body: &'static str) -> (String, tokio::task::JoinHandle<String>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let url = format!("http://{}/v1/oauth/token", listener.local_addr().unwrap());
        let handle = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let mut buf = Vec::new();
            let mut chunk = [0u8; 4096];
            loop {
                let n = socket.read(&mut chunk).await.expect("read");
                buf.extend_from_slice(&chunk[..n]);
                let text = String::from_utf8_lossy(&buf);
                if let Some(idx) = text.find("\r\n\r\n") {
                    let headers = &text[..idx];
                    let content_length: usize = headers
                        .lines()
                        .find_map(|line| line.strip_prefix("content-length: ").or_else(|| line.strip_prefix("Content-Length: ")))
                        .and_then(|value| value.trim().parse().ok())
                        .unwrap_or(0);
                    if buf.len() >= idx + 4 + content_length { break; }
                }
                if n == 0 { break; }
            }
            let request = String::from_utf8_lossy(&buf).into_owned();
            let response = format!(
                "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            socket.write_all(response.as_bytes()).await.expect("write");
            socket.shutdown().await.ok();
            request
        });
        (url, handle)
    }

    async fn refresh_against(status: u16, body: &'static str) -> (Result<RefreshedTokens, OAuthAuthProblem>, String) {
        let (url, handle) = one_shot_token_server(status, body).await;
        let http = reqwest::Client::new();
        let result = refresh_claude_token_at(&url, "refresh-OLD", &http).await;
        (result, handle.await.expect("server"))
    }

    #[tokio::test]
    async fn refresh_posts_json_with_client_id_and_scope() {
        let (result, request) = refresh_against(200, r#"{"access_token":"access-NEW","refresh_token":"refresh-NEW","expires_in":28800}"#).await;
        let refreshed = result.expect("refreshed");
        assert!(request.starts_with("POST /v1/oauth/token"), "{request}");
        assert!(request.contains("content-type: application/json"), "{request}");
        let body: serde_json::Value = serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!(body["grant_type"], "refresh_token");
        assert_eq!(body["refresh_token"], "refresh-OLD");
        assert_eq!(body["client_id"], crate::claude_oauth::CLAUDE_OAUTH_CLIENT_ID);
        assert_eq!(body["scope"], crate::claude_oauth::CLAUDE_OAUTH_SCOPES);
        assert_eq!(refreshed.access_token, "access-NEW");
        assert_eq!(refreshed.refresh_token, "refresh-NEW");
        assert!(refreshed.access_token_expires_at > chrono::Utc::now() + chrono::Duration::hours(7));
        assert_eq!(refreshed.id_token, None);
    }

    #[tokio::test]
    async fn refresh_keeps_old_refresh_token_when_not_rotated() {
        let (result, _) = refresh_against(200, r#"{"access_token":"access-NEW","expires_in":3600}"#).await;
        assert_eq!(result.expect("refreshed").refresh_token, "refresh-OLD");
    }

    #[tokio::test]
    async fn refresh_401_is_expired() {
        let (result, _) = refresh_against(401, r#"{"error":"invalid_token"}"#).await;
        assert_eq!(result.unwrap_err(), OAuthAuthProblem::Expired);
    }

    #[tokio::test]
    async fn refresh_400_invalid_grant_is_expired_but_other_400_is_other() {
        let (result, _) = refresh_against(400, r#"{"error":"invalid_grant","error_description":"revoked"}"#).await;
        assert_eq!(result.unwrap_err(), OAuthAuthProblem::Expired);
        let (result, _) = refresh_against(400, r#"{"error":"invalid_request"}"#).await;
        assert!(matches!(result.unwrap_err(), OAuthAuthProblem::Other(text) if text.contains("400") && text.contains("invalid_request")));
    }

    #[tokio::test]
    async fn refresh_403_is_not_entitled() {
        let (result, _) = refresh_against(403, r#"{"error":"forbidden"}"#).await;
        assert_eq!(result.unwrap_err(), OAuthAuthProblem::NotEntitled);
    }

    #[tokio::test]
    async fn refresh_error_text_never_carries_tokens() {
        let (result, _) = refresh_against(500, r#"{"error":"server_error","error_description":"boom"}"#).await;
        let text = match result.unwrap_err() { OAuthAuthProblem::Other(text) => text, other => panic!("{other:?}") };
        assert!(!text.contains("refresh-OLD"), "{text}");
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cargo test -p gents --lib claude_oauth 2>&1 | tail -5
```
Expected: compile errors (`claude_oauth` module does not exist).

- [ ] **Step 3: Implement `claude_oauth.rs`**

```rust
//! Claude subscription OAuth: public client constants, the credential
//! provider name, and operator-facing error copy.
//!
//! The client id, endpoints, and scopes are Claude Code's public OAuth client
//! constants, read from the Claude Code 2.1.260 bundle on 2026-09-04 (see
//! `docs/superpowers/specs/2026-09-04-claude-oauth-credential-parity-design.md`
//! §1). They are not secrets; they can change without notice, which is why
//! they live here and nowhere else.

use chrono::{DateTime, Duration, Utc};

use crate::oauth_credential::{
    classify_oauth_auth_error, oauth_credential_id, OAuthAuthProblem, OAuthCredential, OAuthProduct,
};

pub const CLAUDE_OAUTH_PROVIDER: &str = "claude-subscription";
pub const CLAUDE_OAUTH_CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
pub const CLAUDE_OAUTH_AUTHORIZE_URL: &str = "https://claude.com/cai/oauth/authorize";
pub const CLAUDE_OAUTH_TOKEN_URL: &str = "https://platform.claude.com/v1/oauth/token";
pub const CLAUDE_OAUTH_MANUAL_REDIRECT_URL: &str = "https://platform.claude.com/oauth/code/callback";
/// The subscription (claude.ai) scope set Claude Code requests. Not the
/// API-key-creation scope.
pub const CLAUDE_OAUTH_SCOPES: &str =
    "user:profile user:inference user:sessions:claude_code user:mcp_servers user:file_upload";
pub const CLAUDE_OAUTH_TOKEN_URL_OVERRIDE_ENV: &str = "GENTS_CLAUDE_OAUTH_TOKEN_URL";

pub const CLAUDE_OAUTH_PRODUCT: OAuthProduct = OAuthProduct {
    name: "Claude",
    backend_label: "Claude subscription backend",
    login_command: "claude-login",
    not_entitled_guidance: "Check that the Claude plan includes Claude Code (Pro/Max), or use an API-key backend.",
};

pub fn normalize_provider(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        CLAUDE_OAUTH_PROVIDER.to_string()
    } else {
        trimmed.to_string()
    }
}

pub fn classify_claude_auth_error(agent_did: &str, provider: &str, problem: &OAuthAuthProblem) -> String {
    classify_oauth_auth_error(&CLAUDE_OAUTH_PRODUCT, agent_did, provider, problem)
}

/// Tokens returned by the authorization-code exchange. `expires_in` is seconds.
#[derive(Debug, Clone)]
pub struct ClaudeLoginTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: Option<i64>,
    pub scope: Option<String>,
}

pub fn credential_from_login_tokens(
    agent_did: impl Into<String>,
    provider: impl Into<String>,
    tokens: &ClaudeLoginTokens,
    now: DateTime<Utc>,
) -> OAuthCredential {
    let agent_did = agent_did.into();
    let provider = provider.into();
    let access_token_expires_at = tokens
        .expires_in
        .filter(|seconds| *seconds > 0)
        .map(|seconds| now + Duration::seconds(seconds))
        .unwrap_or_else(|| now + Duration::hours(1));
    OAuthCredential {
        doc_id: None,
        credential_id: oauth_credential_id(&agent_did, &provider),
        agent_did,
        provider,
        access_token: tokens.access_token.clone(),
        refresh_token: tokens.refresh_token.clone(),
        id_token: None,
        account_id: None,
        chatgpt_plan_type: None,
        is_fedramp: false,
        access_token_expires_at,
        last_refresh: Some(now),
        enabled: true,
    }
}
```

- [ ] **Step 4: Implement `claude_oauth_refresh.rs`**

```rust
//! Claude subscription OAuth refresh against Anthropic's token endpoint.
//!
//! JSON body (like ChatGPT, unlike xAI's form encoding); `scope` is re-sent on
//! refresh as Claude Code does; refresh-token rotation is optional.

use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::claude_oauth::{CLAUDE_OAUTH_CLIENT_ID, CLAUDE_OAUTH_SCOPES, CLAUDE_OAUTH_TOKEN_URL, CLAUDE_OAUTH_TOKEN_URL_OVERRIDE_ENV};
use crate::oauth_credential::{OAuthAuthProblem, RefreshedTokens};

#[derive(Serialize)]
struct RefreshRequest<'a> {
    grant_type: &'static str,
    refresh_token: &'a str,
    client_id: &'static str,
    scope: &'static str,
}

#[derive(Deserialize)]
struct RefreshResponse {
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
}

pub async fn refresh_claude_token(
    refresh_token: &str,
    http: &reqwest::Client,
) -> Result<RefreshedTokens, OAuthAuthProblem> {
    let endpoint = std::env::var(CLAUDE_OAUTH_TOKEN_URL_OVERRIDE_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| CLAUDE_OAUTH_TOKEN_URL.to_string());
    refresh_claude_token_at(&endpoint, refresh_token, http).await
}

pub(crate) async fn refresh_claude_token_at(
    endpoint: &str,
    refresh_token: &str,
    http: &reqwest::Client,
) -> Result<RefreshedTokens, OAuthAuthProblem> {
    let request = RefreshRequest {
        grant_type: "refresh_token",
        refresh_token,
        client_id: CLAUDE_OAUTH_CLIENT_ID,
        scope: CLAUDE_OAUTH_SCOPES,
    };
    let response = http
        .post(endpoint)
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await
        .map_err(|error| OAuthAuthProblem::Other(format!("Claude token refresh request failed: {}", transport_kind(&error))))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(OAuthAuthProblem::Expired);
        }
        if status == reqwest::StatusCode::BAD_REQUEST && error_code(&body).as_deref() == Some("invalid_grant") {
            return Err(OAuthAuthProblem::Expired);
        }
        if status == reqwest::StatusCode::FORBIDDEN {
            return Err(OAuthAuthProblem::NotEntitled);
        }
        return Err(OAuthAuthProblem::Other(format!(
            "Claude token refresh failed with HTTP {status}: {}",
            parse_error_message(&body)
        )));
    }

    let refreshed = response
        .json::<RefreshResponse>()
        .await
        .map_err(|error| OAuthAuthProblem::Other(format!("decoding Claude token refresh response: {error}")))?;
    let access_token = refreshed
        .access_token
        .ok_or_else(|| OAuthAuthProblem::Other("Claude token refresh response omitted access_token".to_string()))?;
    let access_token_expires_at = refreshed
        .expires_in
        .filter(|seconds| *seconds > 0)
        .map(|seconds| Utc::now() + Duration::seconds(seconds))
        .unwrap_or_else(|| Utc::now() + Duration::hours(1));

    Ok(RefreshedTokens {
        access_token,
        refresh_token: refreshed.refresh_token.unwrap_or_else(|| refresh_token.to_string()),
        id_token: None,
        account_id: None,
        is_fedramp: false,
        plan_type: None,
        access_token_expires_at,
    })
}

fn transport_kind(error: &reqwest::Error) -> &'static str {
    if error.is_timeout() { "timed out" } else if error.is_connect() { "could not connect" } else { "failed" }
}

fn error_code(body: &str) -> Option<String> {
    serde_json::from_str::<Value>(body).ok()?.get("error")?.as_str().map(str::to_string)
}

fn parse_error_message(body: &str) -> String {
    let Ok(value) = serde_json::from_str::<Value>(body) else { return body.to_string(); };
    value
        .get("error_description")
        .and_then(Value::as_str)
        .or_else(|| value.get("error").and_then(|error| error.as_str().or_else(|| error.get("message").and_then(Value::as_str))))
        .or_else(|| value.get("message").and_then(Value::as_str))
        .unwrap_or(body)
        .to_string()
}
```

- [ ] **Step 5: Wire the refresh kind**

`oauth_credential.rs`: add `Claude` to `OAuthRefreshKind`; in `refresh_tokens` add
```rust
            OAuthRefreshKind::Claude => {
                crate::claude_oauth_refresh::refresh_claude_token(refresh_token, &self.http).await
            }
```
`lib.rs`: `pub mod claude_oauth;` and `pub mod claude_oauth_refresh;` beside `pub mod claude_messages;` (edit only those two lines).

- [ ] **Step 6: Run the tests to verify they pass**

```bash
cargo test -p gents --lib claude_oauth 2>&1 | tail -15
```
Expected: 6 `claude_oauth::tests` + 6 `claude_oauth_refresh::tests` PASS.

- [ ] **Step 7: Gates and commit**

```bash
cargo test -p gents 2>&1 | grep -E 'test result|FAILED' | head
cargo check --workspace --all-targets 2>&1 | tail -3
git add crates/gents/src/claude_oauth.rs crates/gents/src/claude_oauth_refresh.rs crates/gents/src/oauth_credential.rs crates/gents/src/lib.rs
git commit -m "feat(claude): OAuth client constants, provider, and refresh kind for the subscription credential

Adds the Claude subscription provider (claude-subscription), Claude Code's
public OAuth client constants with provenance, the operator error copy, and
a JSON refresh client wired as OAuthRefreshKind::Claude on the shared
single-flight bearer. No runtime path uses it yet."
```

### Task 2: First-party `gents claude-login` (new crate), runnability gate, and `diagnose` check

**Files:**
- Create: `crates/gents-claude-login/Cargo.toml`, `crates/gents-claude-login/src/lib.rs`
- Modify: workspace `Cargo.toml` `[workspace] members` (add the crate) and `[workspace.dependencies]` (`gents-claude-login = { path = "crates/gents-claude-login" }`); `crates/gents-cli/Cargo.toml` `[dependencies]` (`gents-claude-login.workspace = true`)
- Rewrite: `crates/gents-cli/src/commands/claude_login.rs`
- Modify: `crates/gents-cli/src/cli/args.rs` (`ClaudeLoginArgs`, the `claude-login` `#[command]`), `crates/gents-cli/src/cli/args/tests.rs` (`claude_login_*` tests)
- Modify: `crates/gents/src/agent/document_view/snapshot.rs:125-146` (third gate arm)
- Modify: `crates/gents-cli/src/commands/diagnose/mod.rs:195-238` (add `checks.claude_auth` after `xai_auth`)
- Test: crate `mod tests`; `claude_login.rs` `mod tests`; snapshot gate test next to the existing Codex/Grok gate tests; diagnose JSON shape test if the module has one for `xai_auth` (mirror it)

**Interfaces:**
- Consumes: Task 1 constants and `credential_from_login_tokens`; `gents::oauth_credential::oauth_credential_upsert_mutation`; `ConfigAccess::execute`; `resolve_agent_did`, `resolve_config_access`, `print_json` (as `codex_login.rs` uses them).
- Produces: `gents_claude_login::{LoginOptions, LoginTokens, LoginServer, run_loopback_login, run_manual_login, exchange_code}`; CLI `gents claude-login [--home] [--graphql] [--agent-did] [--provider claude-subscription] [--manual] [--no-browser] [--client-id] [--token-url]`; `checks.claude_auth` JSON; snapshot gate text `run \`gents claude-login --agent-did {agent_did}\``.

- [ ] **Step 1: Create the crate skeleton and failing tests**

`crates/gents-claude-login/Cargo.toml`:

```toml
[package]
name = "gents-claude-login"
version.workspace = true
edition.workspace = true
license.workspace = true
publish = false

[dependencies]
base64.workspace = true
rand.workspace = true
reqwest.workspace = true
serde.workspace = true
serde_json.workspace = true
sha2.workspace = true
tiny_http = "0.12"
tokio.workspace = true
tracing.workspace = true
url = "2"
webbrowser = "1"
```
(Copy `version`/`edition`/`license` keys exactly as `crates/gents-chatgpt-login/Cargo.toml` declares them.)

Tests at the bottom of `src/lib.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn options() -> LoginOptions {
        LoginOptions { open_browser: false, force_state: Some("state-1".into()), ..LoginOptions::default() }
    }

    #[test]
    fn authorize_url_carries_claude_code_query_shape() {
        let pkce = generate_pkce();
        let url = build_authorize_url(&options(), "http://localhost:4242/callback", &pkce, "state-1").unwrap();
        let parsed = url::Url::parse(&url).unwrap();
        assert_eq!(parsed.origin().ascii_serialization(), "https://claude.com");
        assert_eq!(parsed.path(), "/cai/oauth/authorize");
        let q: std::collections::HashMap<_, _> = parsed.query_pairs().into_owned().collect();
        assert_eq!(q["code"], "true");
        assert_eq!(q["client_id"], CLIENT_ID);
        assert_eq!(q["response_type"], "code");
        assert_eq!(q["redirect_uri"], "http://localhost:4242/callback");
        assert_eq!(q["scope"], SCOPES);
        assert_eq!(q["code_challenge"], pkce.challenge);
        assert_eq!(q["code_challenge_method"], "S256");
        assert_eq!(q["state"], "state-1");
    }

    #[test]
    fn pkce_challenge_is_s256_of_verifier() {
        let pkce = generate_pkce();
        let expected = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(pkce.verifier.as_bytes()));
        assert_eq!(pkce.challenge, expected);
        assert!(pkce.verifier.len() >= 43);
    }

    #[test]
    fn manual_code_parsing_requires_matching_state() {
        assert_eq!(parse_manual_code("abc#state-1", "state-1").unwrap(), "abc");
        assert!(parse_manual_code("abc#state-2", "state-1").is_err());
        assert!(parse_manual_code("abc", "state-1").is_err());
        assert!(parse_manual_code("", "state-1").is_err());
    }

    #[tokio::test]
    async fn callback_with_wrong_state_is_rejected_and_keeps_waiting() {
        let pkce = generate_pkce();
        let outcome = handle_callback_request("/callback?code=abc&state=wrong", &options(), "http://localhost:1/callback", &pkce, "state-1").await;
        let (status, _, completed) = outcome.into_parts();
        assert_eq!(status, 400);
        assert!(completed.is_none());
    }

    #[tokio::test]
    async fn exchange_posts_json_with_state_and_verifier() {
        let (url, handle) = one_shot_server(200, r#"{"access_token":"access-NEW","refresh_token":"refresh-NEW","expires_in":28800,"scope":"user:inference"}"#).await;
        let opts = LoginOptions { token_url: url, ..options() };
        let pkce = generate_pkce();
        let tokens = exchange_code(&opts, "http://localhost:1/callback", &pkce, "code-1", "state-1").await.unwrap();
        let request = handle.await.unwrap();
        let body: serde_json::Value = serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!(body["grant_type"], "authorization_code");
        assert_eq!(body["code"], "code-1");
        assert_eq!(body["redirect_uri"], "http://localhost:1/callback");
        assert_eq!(body["client_id"], CLIENT_ID);
        assert_eq!(body["code_verifier"], pkce.verifier);
        assert_eq!(body["state"], "state-1");
        assert!(request.contains("content-type: application/json"), "{request}");
        assert_eq!(tokens.access_token, "access-NEW");
        assert_eq!(tokens.refresh_token, "refresh-NEW");
        assert_eq!(tokens.expires_in, Some(28800));
    }

    #[tokio::test]
    async fn exchange_error_never_echoes_the_code() {
        let (url, _handle) = one_shot_server(401, r#"{"error":"invalid_grant"}"#).await;
        let opts = LoginOptions { token_url: url, ..options() };
        let pkce = generate_pkce();
        let err = exchange_code(&opts, "http://localhost:1/callback", &pkce, "code-SECRET", "state-1").await.unwrap_err();
        assert!(!err.to_string().contains("code-SECRET"), "{err}");
        assert!(err.to_string().contains("401"), "{err}");
    }

    /// One-shot HTTP responder (no extra dependency): accepts one request, returns it, answers with `status` + `body`.
    async fn one_shot_server(status: u16, body: &'static str) -> (String, tokio::task::JoinHandle<String>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let url = format!("http://{}/v1/oauth/token", listener.local_addr().unwrap());
        let handle = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let mut buf = Vec::new();
            let mut chunk = [0u8; 4096];
            loop {
                let n = socket.read(&mut chunk).await.expect("read");
                buf.extend_from_slice(&chunk[..n]);
                let text = String::from_utf8_lossy(&buf);
                if let Some(idx) = text.find("\r\n\r\n") {
                    let content_length: usize = text[..idx]
                        .lines()
                        .find_map(|line| line.strip_prefix("content-length: ").or_else(|| line.strip_prefix("Content-Length: ")))
                        .and_then(|value| value.trim().parse().ok())
                        .unwrap_or(0);
                    if buf.len() >= idx + 4 + content_length { break; }
                }
                if n == 0 { break; }
            }
            let request = String::from_utf8_lossy(&buf).into_owned();
            let response = format!(
                "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            socket.write_all(response.as_bytes()).await.expect("write");
            socket.shutdown().await.ok();
            request
        });
        (url, handle)
    }
}
```

- [ ] **Step 2: Run the crate tests to verify they fail**

```bash
cargo test -p gents-claude-login 2>&1 | tail -5
```
Expected: compile errors (no items yet).

- [ ] **Step 3: Implement `crates/gents-claude-login/src/lib.rs`**

```rust
//! Claude subscription OAuth login (PKCE) used by `gents claude-login`.
//!
//! Owns only the browser/loopback and manual-paste code exchanges. Tokens go
//! back to the caller for immediate persistence in DefraDB. Client constants
//! mirror `gents::claude_oauth`; keep both in sync (a gents unit test pins it).

use std::collections::HashMap;
use std::io;
use std::sync::Arc;
use std::thread;

use base64::Engine;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tiny_http::{Header, Response, Server, StatusCode as TinyStatusCode};
use tokio::sync::{mpsc, Notify};
use url::Url;

pub const CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
pub const AUTHORIZE_URL: &str = "https://claude.com/cai/oauth/authorize";
pub const TOKEN_URL: &str = "https://platform.claude.com/v1/oauth/token";
pub const MANUAL_REDIRECT_URL: &str = "https://platform.claude.com/oauth/code/callback";
pub const SCOPES: &str = "user:profile user:inference user:sessions:claude_code user:mcp_servers user:file_upload";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoginOptions {
    pub client_id: String,
    pub authorize_url: String,
    pub token_url: String,
    pub open_browser: bool,
    /// Test hook for deterministic callback-state checks.
    pub force_state: Option<String>,
}

impl Default for LoginOptions {
    fn default() -> Self {
        Self {
            client_id: CLIENT_ID.to_string(),
            authorize_url: AUTHORIZE_URL.to_string(),
            token_url: TOKEN_URL.to_string(),
            open_browser: true,
            force_state: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoginTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: Option<i64>,
    pub scope: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ShutdownHandle { notify: Arc<Notify> }
impl ShutdownHandle { pub fn shutdown(&self) { self.notify.notify_one(); } }

pub struct LoginServer {
    pub auth_url: String,
    pub actual_port: u16,
    task: tokio::task::JoinHandle<io::Result<LoginTokens>>,
    shutdown: ShutdownHandle,
}

impl LoginServer {
    pub async fn block_until_done(self) -> io::Result<LoginTokens> {
        self.task.await.map_err(|error| io::Error::other(format!("login callback task failed: {error}")))?
    }
    pub fn cancel_handle(&self) -> ShutdownHandle { self.shutdown.clone() }
}

/// Loopback PKCE login: binds an ephemeral localhost port, prints/opens the
/// authorize URL, accepts one `/callback`, exchanges the code.
pub fn run_loopback_login(options: LoginOptions) -> io::Result<LoginServer> {
    let pkce = generate_pkce();
    let state = options.force_state.clone().unwrap_or_else(generate_state);
    let server = Server::http("127.0.0.1:0").map_err(io::Error::other)?;
    let actual_port = server
        .server_addr()
        .to_ip()
        .map(|address| address.port())
        .ok_or_else(|| io::Error::other("login callback server did not expose an IP port"))?;
    let server = Arc::new(server);
    let redirect_uri = format!("http://localhost:{actual_port}/callback");
    let auth_url = build_authorize_url(&options, &redirect_uri, &pkce, &state)?;

    if options.open_browser {
        if let Err(error) = webbrowser::open(&auth_url) {
            tracing::warn!(%error, "could not open the Claude login URL in a browser");
        }
    }

    let (sender, mut receiver) = mpsc::channel(8);
    let receive_server = server.clone();
    thread::spawn(move || {
        while let Ok(request) = receive_server.recv() {
            if sender.blocking_send(request).is_err() { break; }
        }
    });

    let notify = Arc::new(Notify::new());
    let task_notify = notify.clone();
    let task_server = server.clone();
    let task = tokio::spawn(async move {
        let result = loop {
            tokio::select! {
                _ = task_notify.notified() => break Err(io::Error::other("Claude login was cancelled")),
                request = receiver.recv() => {
                    let Some(request) = request else { break Err(io::Error::other("Claude login callback server stopped")); };
                    let outcome = handle_callback_request(request.url(), &options, &redirect_uri, &pkce, &state).await;
                    let (status, body, completed) = outcome.into_parts();
                    let response = text_response(status, body);
                    let _ = tokio::task::spawn_blocking(move || request.respond(response)).await;
                    if let Some(result) = completed { break result; }
                }
            }
        };
        task_server.unblock();
        result
    });

    Ok(LoginServer { auth_url, actual_port, task, shutdown: ShutdownHandle { notify } })
}

/// Manual-paste login for hosts without a browser: prints the authorize URL
/// (redirect = Anthropic's code page), the caller supplies the pasted
/// `code#state` string, we exchange it.
pub async fn run_manual_login(
    options: LoginOptions,
    read_pasted: impl FnOnce(&str) -> io::Result<String>,
) -> io::Result<LoginTokens> {
    let pkce = generate_pkce();
    let state = options.force_state.clone().unwrap_or_else(generate_state);
    let auth_url = build_authorize_url(&options, MANUAL_REDIRECT_URL, &pkce, &state)?;
    let pasted = read_pasted(&auth_url)?;
    let code = parse_manual_code(pasted.trim(), &state)?;
    exchange_code(&options, MANUAL_REDIRECT_URL, &pkce, &code, &state).await
}

pub(crate) fn parse_manual_code(pasted: &str, expected_state: &str) -> io::Result<String> {
    let (code, state) = pasted
        .split_once('#')
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "expected the pasted value to look like code#state"))?;
    if state != expected_state {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "OAuth state mismatch; restart sign-in"));
    }
    if code.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "authorization code was empty"));
    }
    Ok(code.to_string())
}

enum CallbackOutcome {
    Continue { status: u16, body: String },
    Complete { status: u16, body: String, result: io::Result<LoginTokens> },
}

impl CallbackOutcome {
    fn into_parts(self) -> (u16, String, Option<io::Result<LoginTokens>>) {
        match self {
            Self::Continue { status, body } => (status, body, None),
            Self::Complete { status, body, result } => (status, body, Some(result)),
        }
    }
}

async fn handle_callback_request(
    request_target: &str,
    options: &LoginOptions,
    redirect_uri: &str,
    pkce: &PkceCodes,
    expected_state: &str,
) -> CallbackOutcome {
    let Ok(parsed) = Url::parse(&format!("http://localhost{request_target}")) else {
        return CallbackOutcome::Continue { status: 400, body: "Invalid callback request.".into() };
    };
    if parsed.path() != "/callback" {
        return CallbackOutcome::Continue { status: 404, body: "Not found.".into() };
    }
    let params: HashMap<String, String> = parsed.query_pairs().into_owned().collect();
    if params.get("state").map(String::as_str) != Some(expected_state) {
        tracing::warn!("rejected Claude OAuth callback with mismatched state");
        return CallbackOutcome::Continue { status: 400, body: "OAuth state mismatch. Return to the terminal and retry sign-in.".into() };
    }
    if let Some(error) = params.get("error") {
        let message = params.get("error_description").map(String::as_str).filter(|v| !v.trim().is_empty()).unwrap_or(error);
        return CallbackOutcome::Complete {
            status: 400,
            body: "Claude sign-in was not completed. Return to Gents for details.".into(),
            result: Err(io::Error::new(io::ErrorKind::PermissionDenied, format!("Claude authorization failed: {message}"))),
        };
    }
    let Some(code) = params.get("code").map(String::as_str).filter(|v| !v.is_empty()) else {
        return CallbackOutcome::Continue { status: 400, body: "The callback omitted its authorization code.".into() };
    };
    match exchange_code(options, redirect_uri, pkce, code, expected_state).await {
        Ok(tokens) => CallbackOutcome::Complete { status: 200, body: "Claude sign-in complete. You may close this window.".into(), result: Ok(tokens) },
        Err(error) => CallbackOutcome::Complete { status: 502, body: "Claude token exchange failed. Return to Gents for details.".into(), result: Err(error) },
    }
}

fn text_response(status: u16, body: String) -> Response<std::io::Cursor<Vec<u8>>> {
    let mut response = Response::from_string(body).with_status_code(TinyStatusCode(status));
    if let Ok(header) = Header::from_bytes("Content-Type", "text/plain; charset=utf-8") {
        response.add_header(header);
    }
    response
}

#[derive(Clone, Debug)]
pub(crate) struct PkceCodes { pub(crate) verifier: String, pub(crate) challenge: String }

pub(crate) fn generate_pkce() -> PkceCodes {
    let mut bytes = [0u8; 64];
    rand::rng().fill_bytes(&mut bytes);
    let verifier = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    PkceCodes { verifier, challenge }
}

fn generate_state() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

pub(crate) fn build_authorize_url(options: &LoginOptions, redirect_uri: &str, pkce: &PkceCodes, state: &str) -> io::Result<String> {
    let mut url = Url::parse(&options.authorize_url).map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    url.query_pairs_mut()
        .append_pair("code", "true")
        .append_pair("client_id", &options.client_id)
        .append_pair("response_type", "code")
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("scope", SCOPES)
        .append_pair("code_challenge", &pkce.challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", state);
    Ok(url.into())
}

#[derive(Serialize)]
struct ExchangeRequest<'a> {
    grant_type: &'static str,
    code: &'a str,
    redirect_uri: &'a str,
    client_id: &'a str,
    code_verifier: &'a str,
    state: &'a str,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: String,
    #[serde(default)]
    expires_in: Option<i64>,
    #[serde(default)]
    scope: Option<String>,
}

pub(crate) async fn exchange_code(
    options: &LoginOptions,
    redirect_uri: &str,
    pkce: &PkceCodes,
    code: &str,
    state: &str,
) -> io::Result<LoginTokens> {
    let request = ExchangeRequest { grant_type: "authorization_code", code, redirect_uri, client_id: &options.client_id, code_verifier: &pkce.verifier, state };
    let response = reqwest::Client::new()
        .post(&options.token_url)
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await
        .map_err(redacted_transport_error)?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        let detail = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_string))
            .unwrap_or_else(|| "no error code".to_string());
        return Err(io::Error::other(format!("Claude token endpoint returned HTTP {status}: {detail}")));
    }
    let tokens = response
        .json::<TokenResponse>()
        .await
        .map_err(|error| io::Error::other(format!("decoding Claude OAuth tokens: {error}")))?;
    Ok(LoginTokens { access_token: tokens.access_token, refresh_token: tokens.refresh_token, expires_in: tokens.expires_in, scope: tokens.scope })
}

fn redacted_transport_error(error: reqwest::Error) -> io::Error {
    let kind = if error.is_timeout() { "timed out" } else if error.is_connect() { "could not connect" } else { "failed" };
    io::Error::other(format!("Claude token exchange {kind}"))
}
```

Add a pin test in `crates/gents/src/claude_oauth.rs` tests once `gents-cli` links both crates (see Step 5): the constants in the two crates must be byte-equal (`gents_claude_login::CLIENT_ID == CLAUDE_OAUTH_CLIENT_ID`, same for the three URLs and `SCOPES`). Put that test in `gents-cli` (`commands/claude_login.rs` tests), which depends on both crates.

- [ ] **Step 4: Rewrite the CLI command**

`args.rs`:

```rust
    #[command(
        name = "claude-login",
        about = "Sign in with the Claude subscription (OAuth) and store credentials in DefraDB",
        after_help = "Default: opens the browser and listens on a localhost callback. Use --manual on hosts without a browser: open the printed URL anywhere, then paste the code shown on Anthropic's page."
    )]
    ClaudeLogin(ClaudeLoginArgs),
```

```rust
#[derive(clap::Args)]
pub(crate) struct ClaudeLoginArgs {
    #[arg(long, help = "Agent home directory. Defaults to ~/.gents")]
    pub(crate) home: Option<PathBuf>,
    #[arg(long, help = "GraphQL endpoint for the target gents node")]
    pub(crate) graphql: Option<String>,
    #[arg(long, help = "Agent DID that owns the OAuthCredential document")]
    pub(crate) agent_did: Option<String>,
    #[arg(long, default_value = "claude-subscription")]
    pub(crate) provider: String,
    #[arg(long, default_value_t = false, help = "Manual-paste login (no localhost callback, no browser)")]
    pub(crate) manual: bool,
    #[arg(long, default_value_t = false, help = "Print the login URL instead of opening a browser")]
    pub(crate) no_browser: bool,
    #[arg(long, help = "OAuth client ID override for testing")]
    pub(crate) client_id: Option<String>,
    #[arg(long, help = "OAuth token endpoint override for testing")]
    pub(crate) token_url: Option<String>,
}
```

`commands/claude_login.rs`:

```rust
//! First-party Claude subscription OAuth login. Tokens are stored as an
//! `OAuthCredential` document for the agent DID, exactly like `codex-login`
//! and `grok-login`; the `claude` binary is not involved.

use std::io::{self, BufRead, Write};

use anyhow::{Context, Result};
use gents_claude_login::{run_loopback_login, run_manual_login, LoginOptions};
use serde_json::{json, Value};

use crate::cli::args::ClaudeLoginArgs;
use crate::config_writes::ConfigAccess;
use crate::{print_json, resolve_agent_did, resolve_config_access};

pub(crate) struct ClaudeLoginOptions {
    pub(crate) provider: String,
    pub(crate) manual: bool,
    pub(crate) open_browser: bool,
    pub(crate) client_id: Option<String>,
    pub(crate) token_url: Option<String>,
}

pub(crate) struct ClaudeLoginOutcome {
    pub(crate) doc_id: String,
    pub(crate) credential: gents::oauth_credential::OAuthCredential,
}

pub(crate) async fn claude_login(args: ClaudeLoginArgs) -> Result<()> {
    let (access, home_dir) = resolve_config_access(args.home.as_deref(), args.graphql.as_deref()).await?;
    let agent_did = resolve_agent_did(Some(&home_dir), args.agent_did.as_deref())?;
    let outcome = run_claude_login(
        &access,
        &agent_did,
        &ClaudeLoginOptions {
            provider: args.provider,
            manual: args.manual,
            open_browser: !args.no_browser,
            client_id: args.client_id,
            token_url: args.token_url,
        },
    )
    .await?;
    print_json(&claude_login_result_json(&outcome))?;
    Ok(())
}

pub(crate) async fn run_claude_login(access: &ConfigAccess, agent_did: &str, opts: &ClaudeLoginOptions) -> Result<ClaudeLoginOutcome> {
    let provider = gents::claude_oauth::normalize_provider(&opts.provider);
    let mut login_options = LoginOptions { open_browser: opts.open_browser, ..LoginOptions::default() };
    if let Some(client_id) = opts.client_id.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
        login_options.client_id = client_id.to_string();
    }
    if let Some(token_url) = opts.token_url.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
        login_options.token_url = token_url.to_string();
    }

    let tokens = if opts.manual {
        run_manual_login(login_options, |auth_url| {
            eprintln!("Open this URL to sign in with Claude:\n{auth_url}\n\nPaste the code shown on the success page (code#state) and press Enter:");
            io::stderr().flush()?;
            let mut line = String::new();
            io::stdin().lock().read_line(&mut line)?;
            Ok(line)
        })
        .await
        .context("Claude manual login failed")?
    } else {
        let server = run_loopback_login(login_options).context("starting Claude login callback server")?;
        eprintln!("Open this URL to sign in with Claude:\n{}", server.auth_url);
        server.block_until_done().await.context("Claude browser login failed")?
    };

    let login_tokens = gents::claude_oauth::ClaudeLoginTokens {
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token,
        expires_in: tokens.expires_in,
        scope: tokens.scope,
    };
    let credential = gents::claude_oauth::credential_from_login_tokens(agent_did, &provider, &login_tokens, chrono::Utc::now());
    let mutation = gents::oauth_credential::oauth_credential_upsert_mutation(&credential);
    let response = access.execute(&mutation).await?;
    let doc_id = gents_protocol::graphql::extract_mutation_doc_id(&response, "OAuthCredential")?;
    Ok(ClaudeLoginOutcome { doc_id, credential })
}

pub(crate) fn claude_login_result_json(outcome: &ClaudeLoginOutcome) -> Value {
    let credential = &outcome.credential;
    json!({
        "login": "completed",
        "doc_id": outcome.doc_id,
        "credential_id": credential.credential_id,
        "agent_did": credential.agent_did,
        "provider": credential.provider,
        "access_token_expires_at": credential.access_token_expires_at,
        "last_refresh": credential.last_refresh,
        "enabled": credential.enabled,
        "access_token": "<redacted>",
        "refresh_token": "<redacted>",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_and_runtime_constants_match() {
        assert_eq!(gents_claude_login::CLIENT_ID, gents::claude_oauth::CLAUDE_OAUTH_CLIENT_ID);
        assert_eq!(gents_claude_login::AUTHORIZE_URL, gents::claude_oauth::CLAUDE_OAUTH_AUTHORIZE_URL);
        assert_eq!(gents_claude_login::TOKEN_URL, gents::claude_oauth::CLAUDE_OAUTH_TOKEN_URL);
        assert_eq!(gents_claude_login::MANUAL_REDIRECT_URL, gents::claude_oauth::CLAUDE_OAUTH_MANUAL_REDIRECT_URL);
        assert_eq!(gents_claude_login::SCOPES, gents::claude_oauth::CLAUDE_OAUTH_SCOPES);
    }

    #[test]
    fn result_json_redacts_tokens() {
        let credential = gents::claude_oauth::credential_from_login_tokens(
            "did:key:z6MkTest",
            "claude-subscription",
            &gents::claude_oauth::ClaudeLoginTokens { access_token: "access-SECRET".into(), refresh_token: "refresh-SECRET".into(), expires_in: Some(60), scope: None },
            chrono::Utc::now(),
        );
        let json = claude_login_result_json(&ClaudeLoginOutcome { doc_id: "bae-1".into(), credential });
        let text = json.to_string();
        assert!(!text.contains("SECRET"), "{text}");
        assert_eq!(json["access_token"], "<redacted>");
        assert_eq!(json["login"], "completed");
    }
}
```

Delete the old `ClaudeLoginPlan`, `plan_claude_login`, `run_claude_login` subprocess code and their tests; remove the `gents::claude_completer` imports. In `args/tests.rs`, replace `claude_login_parses_and_requires_config_dir` and `claude_login_rejects_removed_console_and_sso_flags` with:

```rust
#[test]
fn claude_login_parses_oauth_flags_and_rejects_seat_flags() {
    let cli = Cli::try_parse_from(["gents", "claude-login", "--agent-did", "did:key:z6MkTest", "--manual", "--no-browser"]).expect("parse");
    let Command::ClaudeLogin(args) = cli.command else { panic!("expected claude-login") };
    assert_eq!(args.agent_did.as_deref(), Some("did:key:z6MkTest"));
    assert!(args.manual && args.no_browser);
    assert_eq!(args.provider, "claude-subscription");
    for removed in ["--config-dir", "--claude-bin", "--dry-run", "--claude-write-approved", "--email"] {
        assert!(Cli::try_parse_from(["gents", "claude-login", removed, "x"]).is_err(), "{removed} must be gone");
    }
}
```
(Use the same `Cli`/`Command` names the neighbouring tests use.)

- [ ] **Step 5: Snapshot runnability gate and `diagnose`**

`document_view/snapshot.rs`, after the `XaiGrokOAuth` arm:

```rust
            if backend.provider_kind == crate::backend_provider::BackendProviderKind::ClaudeCliSubscription
                && !view.has_enabled_oauth_credential(crate::claude_oauth::CLAUDE_OAUTH_PROVIDER)
            {
                let agent_did = view.principal.value.agent_did.as_str();
                anyhow::bail!(
                    "behavior {} ClaudeCliSubscription backend {} has no enabled OAuthCredential for agent \
                     {agent_did}; run `gents claude-login --agent-did {agent_did}`",
                    behavior.behavior_id,
                    backend.backend_id,
                );
            }
```
Add a test beside the existing Grok gate test that a Claude behavior without a credential is reported not runnable with that message, and runnable once an enabled `claude-subscription` credential record is present in the view (mirror the Grok test's fixture construction exactly).

`diagnose/mod.rs`, after `xai_auth_check`:

```rust
    let claude_provider = gents::claude_oauth::CLAUDE_OAUTH_PROVIDER;
    let claude_auth_check = match crate::commands::grok_auth_probe::load_oauth_credential(&access, &agent_did, claude_provider).await {
        Ok(Some(credential)) if gents::oauth_credential::token_is_fresh(credential.access_token_expires_at) => json!({
            "ok": true,
            "credential_id": credential.credential_id,
            "provider": credential.provider,
            "expires_at": credential.access_token_expires_at,
        }),
        Ok(Some(credential)) => json!({
            "ok": false,
            "credential_id": credential.credential_id,
            "provider": credential.provider,
            "expires_at": credential.access_token_expires_at,
            "guidance": gents::claude_oauth::classify_claude_auth_error(&agent_did, claude_provider, &gents::oauth_credential::OAuthAuthProblem::Expired),
        }),
        Ok(None) => json!({
            "ok": false,
            "provider": claude_provider,
            "guidance": gents::claude_oauth::classify_claude_auth_error(&agent_did, claude_provider, &gents::oauth_credential::OAuthAuthProblem::Missing),
        }),
        Err(error) => json!({ "ok": false, "provider": claude_provider, "error": error.to_string() }),
    };
```
and add `"claude_auth": claude_auth_check` to the `checks` object next to `"xai_auth"`.

- [ ] **Step 6: Run tests**

```bash
cargo test -p gents-claude-login 2>&1 | tail -8
cargo test -p gents-cli --lib claude_login 2>&1 | tail -6
cargo test -p gents --lib document_view 2>&1 | tail -6
```
Expected: all PASS.

- [ ] **Step 7: Gates and commit**

```bash
cargo test -p gents 2>&1 | grep -E 'test result|FAILED' | head
env -u TMPDIR cargo test -p gents-cli 2>&1 | grep -E 'test result|FAILED' | head
cargo check --workspace --all-targets 2>&1 | tail -3
git add Cargo.toml Cargo.lock crates/gents-claude-login crates/gents-cli/Cargo.toml crates/gents-cli/src crates/gents/src/agent/document_view crates/gents/src/claude_oauth.rs
git commit -m "feat(claude): first-party claude-login writes an OAuthCredential; runnability gate; diagnose check

gents claude-login now runs the Claude subscription PKCE flow itself
(loopback callback by default, --manual paste fallback) and upserts an
OAuthCredential for the agent DID, like codex-login and grok-login. A
Claude behavior without an enabled credential is not runnable, and gents
diagnose reports checks.claude_auth. The claude binary is no longer
involved in login."
```
Note: at the end of this task the runtime still uses the seat (`--claude-config-dir`); both paths coexist until Task 4.

### Task 3: Shared OAuth document-expiry health probe; `is_agent_scoped_oauth` includes Claude

**Files:**
- Modify: `crates/gents/src/backend_provider.rs:102-115`
- Modify: `crates/gents/src/backend_health.rs:223-300` (`probe_backends_cycle`), `:361-393` (`run_backend_probe_cycle`), Claude tests `:688+`
- Modify: the caller of `run_backend_probe_cycle` in `crates/gents-cli/src/commands/serve.rs` (pass the server principal DID)
- Modify: `crates/gents-cli/src/commands/diagnose/backends.rs:178,235` (note text → "OAuth credential backend; health is the credential-expiry probe")
- Test: `backend_health.rs` `mod tests`

**Interfaces:**
- Consumes: `oauth_credential::{lookup_oauth_credential, token_is_fresh, OAuthAuthProblem}`; `claude_oauth::{CLAUDE_OAUTH_PROVIDER, classify_claude_auth_error}`; `chatgpt_codex::{CHATGPT_CODEX_PROVIDER, classify_chatgpt_auth_error}`; `xai_grok_oauth::{XAI_OAUTH_PROVIDER, classify_xai_auth_error}`.
- Produces: `probe_backends_cycle(client, backends, now, health_map, options, oauth: Option<OAuthProbeContext<'_>>)` where `pub struct OAuthProbeContext<'a> { pub node: &'a EmbeddedNode, pub principal_did: &'a str }`; `run_backend_probe_cycle(node, client, health_map, options, principal_did: &str)`; `BackendProviderKind::oauth_provider(self) -> Option<&'static str>` and `oauth_auth_guidance(self, agent_did, provider, &OAuthAuthProblem) -> String`; Claude removed from `probe_backends_cycle`'s special branch.

- [ ] **Step 1: Write the failing tests** (replace the four Claude seat tests in `backend_health.rs`; keep `claude_backend()`; add `oauth_backend(kind)` helpers)

```rust
    fn oauth_backend(kind: crate::backend_provider::BackendProviderKind, id: &str, endpoint: &str) -> InferenceBackend {
        let mut backend = backend(id, endpoint.to_string(), "healthy");
        backend.provider_kind = kind;
        backend
    }

    async fn seed_credential(node: &EmbeddedNode, agent_did: &str, provider: &str, expires_at: DateTime<Utc>) {
        let credential = crate::oauth_credential::OAuthCredential {
            doc_id: None,
            credential_id: crate::oauth_credential::oauth_credential_id(agent_did, provider),
            agent_did: agent_did.to_string(),
            provider: provider.to_string(),
            access_token: "access-TEST".into(),
            refresh_token: "refresh-TEST".into(),
            id_token: None, account_id: None, chatgpt_plan_type: None, is_fedramp: false,
            access_token_expires_at: expires_at,
            last_refresh: Some(Utc::now()),
            enabled: true,
        };
        crate::oauth_credential::upsert_oauth_credential(node, &credential).await.expect("seed credential");
    }

    #[tokio::test]
    async fn oauth_kinds_probe_the_credential_document_fresh_is_healthy_and_promotes() {
        let node = test_node().await;   // the embedded-node helper oauth_credential::tests uses
        let did = "did:key:z6MkProbe";
        seed_credential(&node, did, crate::claude_oauth::CLAUDE_OAUTH_PROVIDER, Utc::now() + chrono::Duration::hours(8)).await;
        let (options, client, health_map) = (probe_options(), reqwest::Client::new(), BackendHealthMap::new());
        let mut claude = claude_backend();
        claude.probe_status = "unknown".to_string();
        let outcome = probe_backends_cycle(&client, &[claude], Utc::now(), &health_map, &options, Some(OAuthProbeContext { node: &node, principal_did: did })).await;
        assert_eq!(outcome.promotable, vec!["claude".to_string()]);
        let snap = health_map.get("claude").await.expect("entry");
        assert_eq!(snap.state, BackendHealthState::Healthy);
        assert!(snap.last_error.is_none());
    }

    #[tokio::test]
    async fn oauth_kinds_stale_credential_demotes_after_k_with_login_hint() {
        let node = test_node().await;
        let did = "did:key:z6MkProbe";
        seed_credential(&node, did, crate::xai_grok_oauth::XAI_OAUTH_PROVIDER, Utc::now() - chrono::Duration::minutes(1)).await;
        let (options, client, health_map) = (probe_options(), reqwest::Client::new(), BackendHealthMap::new());
        let grok = oauth_backend(crate::backend_provider::BackendProviderKind::XaiGrokOAuth, "grok", "https://cli-chat-proxy.grok.com/v1");
        for cycle in 1..=3u32 {
            let outcome = probe_backends_cycle(&client, std::slice::from_ref(&grok), Utc::now(), &health_map, &options, Some(OAuthProbeContext { node: &node, principal_did: did })).await;
            assert!(outcome.promotable.is_empty());
            let snap = health_map.get("grok").await.expect("entry");
            assert_eq!(snap.failure_count, cycle);
            let err = snap.last_error.clone().unwrap_or_default();
            assert!(err.contains("expired or revoked") && err.contains("gents grok-login --agent-did"), "{err}");
            if cycle == 3 { assert_eq!(snap.state, BackendHealthState::Unhealthy); }
        }
    }

    #[tokio::test]
    async fn oauth_kinds_missing_credential_fails_with_login_hint() {
        let node = test_node().await;
        let (options, client, health_map) = (probe_options(), reqwest::Client::new(), BackendHealthMap::new());
        let codex = oauth_backend(crate::backend_provider::BackendProviderKind::ChatGptCodex, "codex", "https://chatgpt.com/backend-api/codex");
        probe_backends_cycle(&client, &[codex], Utc::now(), &health_map, &options, Some(OAuthProbeContext { node: &node, principal_did: "did:key:z6MkNobody" })).await;
        let snap = health_map.get("codex").await.expect("entry");
        assert_eq!(snap.state, BackendHealthState::Degraded);
        assert!(snap.last_error.clone().unwrap_or_default().contains("gents codex-login --agent-did did:key:z6MkNobody"));
    }

    #[tokio::test]
    async fn oauth_kinds_are_skipped_without_a_probe_context() {
        let (options, client, health_map) = (probe_options(), reqwest::Client::new(), BackendHealthMap::new());
        let outcome = probe_backends_cycle(&client, &[claude_backend()], Utc::now(), &health_map, &options, None).await;
        assert!(outcome.promotable.is_empty());
        assert!(health_map.get("claude").await.is_none(), "no measurement without a node");
    }

    #[test]
    fn provider_kind_oauth_provider_names() {
        use crate::backend_provider::BackendProviderKind as K;
        assert_eq!(K::ClaudeCliSubscription.oauth_provider(), Some("claude-subscription"));
        assert_eq!(K::XaiGrokOAuth.oauth_provider(), Some("xai-oauth"));
        assert_eq!(K::ChatGptCodex.oauth_provider(), Some("chatgpt-codex"));
        assert_eq!(K::OpenAiCompatible.oauth_provider(), None);
        assert!(K::ClaudeCliSubscription.is_agent_scoped_oauth());
    }
```
Put `seed_credential` and `test_node` in the same `oauth_credential::test_support` module as Task 1's one-shot server, and `use` them here. `test_node()` builds an in-memory `EmbeddedNode` with the schemas loaded the way `oauth_credential.rs`'s existing tests do (`grep -n 'EmbeddedNode' crates/gents/src/oauth_credential.rs` shows the constructor they call); move that construction into `test_support::test_node()` so all three tasks share one helper. The provider string constants are `CHATGPT_CODEX_PROVIDER = "chatgpt-codex"` and `XAI_OAUTH_PROVIDER = "xai-oauth"` (verify against the source and adjust the literals if they differ).

- [ ] **Step 2: Run to verify failure**

```bash
cargo test -p gents --lib backend_health 2>&1 | tail -6
```
Expected: compile errors (`OAuthProbeContext`, `oauth_provider` missing; old signature).

- [ ] **Step 3: Implement**

`backend_provider.rs`:

```rust
    pub fn is_agent_scoped_oauth(self) -> bool {
        matches!(self, Self::ChatGptCodex | Self::XaiGrokOAuth | Self::ClaudeCliSubscription)
    }

    /// Backends that must not be fleet HTTP-probed: every agent-scoped OAuth
    /// kind. Their health is the credential-expiry probe instead.
    pub fn skips_fleet_http_probe(self) -> bool {
        self.is_agent_scoped_oauth()
    }

    /// The `OAuthCredential.provider` value an agent-scoped kind authenticates with.
    pub fn oauth_provider(self) -> Option<&'static str> {
        match self {
            Self::ChatGptCodex => Some(crate::chatgpt_codex::CHATGPT_CODEX_PROVIDER),
            Self::XaiGrokOAuth => Some(crate::xai_grok_oauth::XAI_OAUTH_PROVIDER),
            Self::ClaudeCliSubscription => Some(crate::claude_oauth::CLAUDE_OAUTH_PROVIDER),
            _ => None,
        }
    }

    pub fn oauth_auth_guidance(self, agent_did: &str, provider: &str, problem: &crate::oauth_credential::OAuthAuthProblem) -> String {
        match self {
            Self::ChatGptCodex => crate::oauth_credential::classify_chatgpt_auth_error(agent_did, provider, problem),
            Self::XaiGrokOAuth => crate::xai_grok_oauth::classify_xai_auth_error(agent_did, provider, problem),
            Self::ClaudeCliSubscription => crate::claude_oauth::classify_claude_auth_error(agent_did, provider, problem),
            _ => format!("{self} does not use OAuth credentials"),
        }
    }
```
Update the `ClaudeCliSubscription` doc comment: "Claude subscription over Messages HTTP, authenticated with an agent-scoped `OAuthCredential` (`claude-subscription`) written by `gents claude-login`."

`backend_health.rs`:

```rust
/// Node + principal used to read agent-scoped OAuth credentials during a probe cycle.
pub struct OAuthProbeContext<'a> {
    pub node: &'a EmbeddedNode,
    pub principal_did: &'a str,
}

async fn probe_oauth_credential(
    context: &OAuthProbeContext<'_>,
    backend: &InferenceBackend,
) -> (ProbeEvent, Option<String>) {
    let Some(provider) = backend.provider_kind.oauth_provider() else {
        return (ProbeEvent::ProbeFail, Some("backend kind has no OAuth provider".to_string()));
    };
    match crate::oauth_credential::lookup_oauth_credential(context.node, context.principal_did, provider).await {
        Ok(Some(credential)) if crate::oauth_credential::token_is_fresh(credential.access_token_expires_at) => {
            tracing::debug!(backend_id = %backend.backend_id, provider, expires_at = %credential.access_token_expires_at.to_rfc3339(), "oauth credential probe ok");
            (ProbeEvent::ProbeSuccess, None)
        }
        Ok(Some(_)) => (
            ProbeEvent::ProbeFail,
            Some(backend.provider_kind.oauth_auth_guidance(context.principal_did, provider, &crate::oauth_credential::OAuthAuthProblem::Expired)),
        ),
        Ok(None) => (
            ProbeEvent::ProbeFail,
            Some(backend.provider_kind.oauth_auth_guidance(context.principal_did, provider, &crate::oauth_credential::OAuthAuthProblem::Missing)),
        ),
        Err(error) => (ProbeEvent::ProbeFail, Some(format!("reading OAuthCredential: {error:#}"))),
    }
}
```
In `probe_backends_cycle`, replace the `ClaudeCliSubscription` branch and the `skips_fleet_http_probe` `continue` with:

```rust
        if backend.provider_kind.is_agent_scoped_oauth() {
            let Some(context) = oauth.as_ref() else { continue; };
            probed_ids.insert(backend.backend_id.clone());
            let (event, error_text) = probe_oauth_credential(context, backend).await;
            record_probe_event(backend, event, error_text, now, health_map, options, &mut outcome).await;
            continue;
        }
```
and add the `oauth: Option<OAuthProbeContext<'_>>` parameter. `run_backend_probe_cycle(node, client, health_map, options, principal_did: &str)` passes `Some(OAuthProbeContext { node, principal_did })`. In `serve.rs`, pass `identity.did()` (the string the status JSON already prints as `agent_did`) at the call site of `run_backend_probe_cycle`; update every other caller/test of `probe_backends_cycle` to pass `None`. `backend_registry.rs:397` log text: "startup backend probe: skipping agent-scoped OAuth backend (health is the credential-expiry probe)". `diagnose/backends.rs` note constant: `"OAuth credential backend: discovery not applicable; health is the credential-expiry probe"`, and gate it on `is_agent_scoped_oauth()` for all three kinds (the note is no longer Claude-specific; adjust the existing test accordingly).

- [ ] **Step 4: Run tests, gates, commit**

```bash
cargo test -p gents --lib backend_health 2>&1 | tail -10
cargo test -p gents 2>&1 | grep -E 'test result|FAILED' | head
env -u TMPDIR cargo test -p gents-cli 2>&1 | grep -E 'test result|FAILED' | head
cargo check --workspace --all-targets 2>&1 | tail -3
git add crates/gents/src/backend_provider.rs crates/gents/src/backend_health.rs crates/gents/src/backend_registry.rs crates/gents-cli/src/commands/serve.rs crates/gents-cli/src/commands/diagnose/backends.rs
git commit -m "feat(health): probe agent-scoped OAuth credentials for expiry; Claude joins the OAuth kinds

Grok, Codex and Claude backends are now probed each cycle by reading the
principal's OAuthCredential document: fresh is healthy (and promotes an
unknown document), stale or missing fails with the provider's login hint.
The probe never refreshes. Claude's seat-file probe is retired here; the
seat itself goes in the next task."
```
Note: between Task 3 and Task 4 the Claude branch's health is document-based while its sends still use the seat. That window is acceptable on the development branch; it must not be released.

### Task 4: Runtime client on the credential bearer; delete the seat

**Files:**
- Rewrite: `crates/gents/src/claude_subscription.rs` (client, model, `DEFAULT_MODEL_ID`, test helpers)
- Modify: `crates/gents/src/claude_messages.rs:576-645` (`stream_messages` signature; `SeatTransport` → `MessagesTransport`), its test module, `crates/gents/src/claude_subscription/tests.rs`
- Modify: `crates/gents/src/agent/runtime/context.rs:341-356`, `crates/gents/src/oneshot.rs:222-236`
- Rewrite: `crates/gents/src/agent/loop_stream/tests/claude.rs` (seeded credential instead of the seat)
- Delete: `crates/gents/src/claude_seat_auth.rs`, `crates/gents/src/claude_completer/` (whole module)
- Modify: `crates/gents/src/lib.rs` (remove the two deleted `pub mod` lines; nothing else)
- Modify: `crates/gents-cli/src/cli/args.rs` (`ServeArgs.claude_config_dir` removed), `cli/args/tests.rs`, `crates/gents-cli/src/commands/serve.rs` (`install_claude_subscription_seat`, status JSON `claude_subscription` object, seat tests)
- Modify: `crates/gents/src/backend_health.rs` tests that still reference `install_process_seat` (none should remain after Task 3; verify)

**Interfaces:**
- Consumes: Task 1 (`CLAUDE_OAUTH_PROVIDER`, `CLAUDE_OAUTH_PRODUCT`, `classify_claude_auth_error`, `OAuthRefreshKind::Claude`), `oauth_credential::{lookup_oauth_credential, shared_bearer, DbCredentialBearer, BearerSource, OAuthAuthProblem}`.
- Produces:
  - `claude_subscription::ClaudeSubscriptionClient<S: BearerSource = DbCredentialBearer> { bearer: Arc<S>, http: ReqwestClient }`, `ClaudeSubscriptionClient::build(node: Arc<EmbeddedNode>, agent_did: &str) -> anyhow::Result<ClaudeSubscriptionClient>`, `#[cfg(test)] ClaudeSubscriptionClient::<StaticBearer>::with_bearer(Arc<StaticBearer>)`, `claude_subscription::DEFAULT_MODEL_ID`, `#[cfg(test)] StaticBearer::new(token) ` (implements `BearerSource`, counts `invalidate` calls).
  - `claude_messages::stream_messages<S: BearerSource>(model: &str, request: &CompletionRequest, surface: HashSet<String>, bearer: &S, http: &ReqwestClient) -> Result<impl Stream<…>, CompletionError>`; on a `401` from the transport it calls `bearer.invalidate().await` once before returning the error.
  - `#[cfg(test)] claude_messages::lock_fixtures_for_test()` (renamed from `claude_subscription::lock_process_seat_for_test`).

- [ ] **Step 1: Write the failing tests**

`claude_subscription/tests.rs` (replace the seat-based tests; keep the fixture-driven stream/usage tests, adapted):

```rust
    fn test_client() -> ClaudeSubscriptionClient<StaticBearer> {
        ClaudeSubscriptionClient::with_bearer(Arc::new(StaticBearer::new("access-TEST")))
    }

    #[tokio::test]
    async fn fixture_stream_maps_tool_use_without_touching_the_bearer() {
        let _guard = crate::claude_messages::lock_fixtures_for_test();
        crate::claude_messages::install_messages_sse_fixtures(vec![
            crate::claude_messages::sse_fixture_tool_use("toolu_1", "echo", "{\"text\":\"hi\"}"),
        ]);
        let client = test_client();
        let model = client.completion_model("claude-sonnet-5");
        let mut stream = model.stream(echo_tool_request()).await.expect("stream");
        // … same assertions as today's messages_http_fixture_maps_gents_tool_use_with_arguments …
        assert_eq!(client.bearer.bearer_calls(), 0, "fixtures never consult the bearer");
    }

    #[tokio::test]
    async fn live_path_without_fixture_asks_the_bearer_and_fails_closed_on_bearer_error() {
        let _guard = crate::claude_messages::lock_fixtures_for_test();
        let client = ClaudeSubscriptionClient::with_bearer(Arc::new(StaticBearer::failing("no credential")));
        let model = client.completion_model("claude-sonnet-5");
        let err = model.completion(ping_request()).await.expect_err("bearer error");
        assert!(err.to_string().contains("Claude Messages bearer: no credential"), "{err}");
        assert_eq!(client.bearer.bearer_calls(), 1);
    }

    #[tokio::test]
    async fn build_without_a_credential_fails_closed_with_the_login_hint() {
        let node = test_node().await;
        let err = ClaudeSubscriptionClient::build(Arc::new(node), "did:key:z6MkNobody").await.expect_err("missing");
        assert!(err.to_string().contains("gents claude-login --agent-did did:key:z6MkNobody"), "{err:#}");
    }

    #[tokio::test]
    async fn build_with_a_seeded_credential_yields_a_shared_bearer() {
        let node = Arc::new(test_node().await);
        seed_credential(&node, "did:key:z6MkSeeded", crate::claude_oauth::CLAUDE_OAUTH_PROVIDER, chrono::Utc::now() + chrono::Duration::hours(8)).await;
        let client = ClaudeSubscriptionClient::build(node.clone(), "did:key:z6MkSeeded").await.expect("client");
        let again = ClaudeSubscriptionClient::build(node, "did:key:z6MkSeeded").await.expect("client");
        assert!(Arc::ptr_eq(&client.bearer, &again.bearer), "one bearer per credential id");
        assert_eq!(client.bearer.current_bearer().await.expect("token"), "access-TEST");
    }
```
`seed_credential` and `test_node` come from `oauth_credential::test_support` (Task 3).

`claude_messages/tests.rs` additions:

```rust
    #[tokio::test]
    async fn transport_401_invalidates_the_bearer_once() {
        let _guard = lock_fixtures_for_test();
        let (url, _handle) = one_shot_server(401, r#"{"type":"error","error":{"type":"authentication_error","message":"bad token"}}"#).await;
        let bearer = StaticBearer::new("access-STALE");
        let err = stream_messages_at(&url, "claude-sonnet-5", &echo_request(), HashSet::new(), &bearer, &ReqwestClient::new()).await.expect_err("401");
        assert!(err.to_string().contains("401"), "{err}");
        assert_eq!(bearer.invalidations(), 1);
    }
```
`stream_messages_at(uri, …)` is a `pub(crate)` variant taking the URI so the test can point at the one-shot server; `stream_messages` calls it with `MESSAGES_URI`. `one_shot_server` here is `oauth_credential::test_support::one_shot_token_server` (Task 1).

`agent/loop_stream/tests/claude.rs`: replace `install_fake_seat` / `lock_process_seat_for_test` with `lock_fixtures_for_test()` and `ClaudeSubscriptionClient::with_bearer(Arc::new(StaticBearer::new("access-TEST")))`; everything else (fixtures, assertions on `AgentToolCall.args`) unchanged.

- [ ] **Step 2: Run to verify failure**

```bash
cargo test -p gents --lib claude_ 2>&1 | tail -6
```
Expected: compile errors (`with_bearer`, `StaticBearer`, `lock_fixtures_for_test`, new `stream_messages` arity).

- [ ] **Step 3: Implement `claude_subscription.rs`**

```rust
//! Claude subscription backend: one Messages HTTP wire (`claude_messages`),
//! authenticated with the agent's `OAuthCredential` (`claude-subscription`)
//! through the shared single-flight bearer. Login is `gents claude-login`;
//! refresh is `claude_oauth_refresh`; the `claude` binary is not involved.

use std::collections::HashSet;
use std::sync::Arc;

use anyhow::{Context, Result};
use futures::StreamExt;
use rig::client::CompletionClient;
use rig::completion::{CompletionError, CompletionModel, CompletionRequest, CompletionResponse, GetTokenUsage, Usage};
use rig::http_client::ReqwestClient;
use rig::streaming::{RawStreamingChoice, StreamingCompletionResponse};
use rig::OneOrMany;
use serde::{Deserialize, Serialize};

use crate::claude_oauth::{classify_claude_auth_error, CLAUDE_OAUTH_PRODUCT, CLAUDE_OAUTH_PROVIDER};
use crate::oauth_credential::{lookup_oauth_credential, shared_bearer, BearerSource, DbCredentialBearer, OAuthAuthProblem, OAuthRefreshKind};
use defra_node::EmbeddedNode;

pub const DEFAULT_BACKEND_ENDPOINT: &str = "claude-cli://subscription";
/// Default client-facing model slug for ClaudeCliSubscription.
pub const DEFAULT_MODEL_ID: &str = "claude-sonnet-5";

pub fn default_backend_endpoint() -> &'static str { DEFAULT_BACKEND_ENDPOINT }
pub fn default_model_name() -> &'static str { DEFAULT_MODEL_ID }

#[derive(Debug, Clone)]
pub struct ClaudeSubscriptionClient<S: BearerSource = DbCredentialBearer> {
    pub(crate) bearer: Arc<S>,
    pub(crate) http: ReqwestClient,
}

impl ClaudeSubscriptionClient<DbCredentialBearer> {
    /// Look the agent's Claude credential up once and bind the shared bearer.
    /// Fails closed with the `claude-login` hint when no enabled credential exists.
    pub async fn build(node: Arc<EmbeddedNode>, agent_did: &str) -> Result<Self> {
        let provider = CLAUDE_OAUTH_PROVIDER;
        let credential = lookup_oauth_credential(node.as_ref(), agent_did, provider)
            .await
            .with_context(|| format!("loading OAuthCredential for agent {agent_did}"))?
            .ok_or_else(|| anyhow::anyhow!(classify_claude_auth_error(agent_did, provider, &OAuthAuthProblem::Missing)))?;
        let credential_id = credential.credential_id.clone();
        let bearer = shared_bearer(&credential_id, || {
            DbCredentialBearer::with_cache(node, agent_did, provider, credential_id.clone(), true, Some(credential.clone()), OAuthRefreshKind::Claude, CLAUDE_OAUTH_PRODUCT)
        });
        Ok(Self { bearer, http: ReqwestClient::new() })
    }
}

impl<S: BearerSource> ClaudeSubscriptionClient<S> {
    #[cfg(test)]
    pub(crate) fn with_bearer(bearer: Arc<S>) -> Self {
        Self { bearer, http: ReqwestClient::new() }
    }
}

impl<S: BearerSource + 'static> CompletionClient for ClaudeSubscriptionClient<S> {
    type CompletionModel = ClaudeSubscriptionModel<S>;
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct ClaudeStreamResponse { pub usage: Option<Usage> }
impl GetTokenUsage for ClaudeStreamResponse { fn token_usage(&self) -> Option<Usage> { self.usage } }

#[derive(Debug, Clone)]
pub struct ClaudeSubscriptionModel<S: BearerSource = DbCredentialBearer> {
    model: String,
    bearer: Arc<S>,
    http: ReqwestClient,
}

fn surface_of(request: &CompletionRequest) -> HashSet<String> {
    request.tools.iter().map(|tool| tool.name.clone()).collect()
}

impl<S: BearerSource + 'static> CompletionModel for ClaudeSubscriptionModel<S> {
    type Response = ();
    type StreamingResponse = ClaudeStreamResponse;
    type Client = ClaudeSubscriptionClient<S>;

    fn make(client: &Self::Client, model: impl Into<String>) -> Self {
        Self { model: model.into(), bearer: client.bearer.clone(), http: client.http.clone() }
    }

    /// Text-only fold over the same stream `stream` returns (the owned loop
    /// uses `stream()`); calls `stream_messages` directly so provider
    /// invocations stay inside the loop seam.
    async fn completion(&self, request: CompletionRequest) -> Result<CompletionResponse<Self::Response>, CompletionError> {
        let surface = surface_of(&request);
        let stream = crate::claude_messages::stream_messages(&self.model, &request, surface, self.bearer.as_ref(), &self.http).await?;
        futures::pin_mut!(stream);
        let mut text = String::new();
        let mut usage = Usage::new();
        while let Some(item) = stream.next().await {
            match item? {
                RawStreamingChoice::Message(chunk) => text.push_str(&chunk),
                RawStreamingChoice::FinalResponse(raw) => { if let Some(reported) = raw.token_usage() { usage = reported; } break; }
                _ => {}
            }
        }
        if text.trim().is_empty() {
            return Err(CompletionError::ProviderError("Claude Messages returned empty assistant text".to_string()));
        }
        Ok(CompletionResponse { choice: OneOrMany::one(rig::completion::AssistantContent::text(text)), usage, raw_response: (), message_id: None })
    }

    async fn stream(&self, request: CompletionRequest) -> Result<StreamingCompletionResponse<Self::StreamingResponse>, CompletionError> {
        let surface = surface_of(&request);
        let stream = crate::claude_messages::stream_messages(&self.model, &request, surface, self.bearer.as_ref(), &self.http).await?;
        Ok(StreamingCompletionResponse::stream(Box::pin(stream)))
    }
}

/// Test bearer: a fixed token, or a fixed error; counts calls and invalidations.
#[cfg(test)]
pub(crate) struct StaticBearer {
    token: Result<String, String>,
    calls: std::sync::atomic::AtomicUsize,
    invalidations: std::sync::atomic::AtomicUsize,
}

#[cfg(test)]
impl StaticBearer {
    pub(crate) fn new(token: &str) -> Self { Self { token: Ok(token.to_string()), calls: Default::default(), invalidations: Default::default() } }
    pub(crate) fn failing(message: &str) -> Self { Self { token: Err(message.to_string()), calls: Default::default(), invalidations: Default::default() } }
    pub(crate) fn bearer_calls(&self) -> usize { self.calls.load(std::sync::atomic::Ordering::SeqCst) }
    pub(crate) fn invalidations(&self) -> usize { self.invalidations.load(std::sync::atomic::Ordering::SeqCst) }
}

#[cfg(test)]
impl BearerSource for StaticBearer {
    async fn current_bearer(&self) -> anyhow::Result<String> {
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.token.clone().map_err(|message| anyhow::anyhow!(message))
    }
    async fn invalidate(&self) {
        self.invalidations.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
}

#[cfg(test)]
#[path = "claude_subscription/tests.rs"]
mod tests;
```
(`BearerSource` uses return-position `impl Future`; implementing it with `async fn` in an impl block is accepted by the compiler for this trait shape — mirror whatever form `DbCredentialBearer`'s impl uses if the compiler objects.)

- [ ] **Step 4: Implement the `stream_messages` change**

Replace the signature and the seat lines in `claude_messages.rs`:

```rust
pub async fn stream_messages<S: BearerSource>(
    model: &str,
    request: &CompletionRequest,
    surface: HashSet<String>,
    bearer: &S,
    http: &ReqwestClient,
) -> Result<impl futures::Stream<Item = Result<RawStreamingChoice<ClaudeStreamResponse>, CompletionError>>, CompletionError> {
    stream_messages_at(MESSAGES_URI, model, request, surface, bearer, http).await
}

pub(crate) async fn stream_messages_at<S: BearerSource>(
    uri: &str,
    model: &str,
    request: &CompletionRequest,
    surface: HashSet<String>,
    bearer: &S,
    http: &ReqwestClient,
) -> Result<impl futures::Stream<Item = Result<RawStreamingChoice<ClaudeStreamResponse>, CompletionError>>, CompletionError> {
    #[cfg(test)]
    let fixture = take_messages_sse_fixture();
    #[cfg(not(test))]
    let fixture: Option<String> = None;
    let body = build_messages_body(model, request);
    let body_bytes = serde_json::to_vec(&body).map_err(|error| CompletionError::ProviderError(format!("encode Claude Messages body: {error}")))?;

    let mut builder = Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .header("anthropic-version", ANTHROPIC_VERSION)
        .header("anthropic-beta", OAUTH_BETA);
    if fixture.is_none() {
        let token = bearer.current_bearer().await.map_err(|error| CompletionError::ProviderError(format!("Claude Messages bearer: {error}")))?;
        let mut authorization = HeaderValue::from_str(&format!("Bearer {token}")).map_err(|error| CompletionError::ProviderError(format!("Claude Messages auth header: {error}")))?;
        authorization.set_sensitive(true);
        builder = builder.header("authorization", authorization);
        tracing::info!(model = %model, "live Claude Messages HTTP send (this process bills the Claude subscription)");
    }
    let http_request = builder.body(Bytes::from(body_bytes)).map_err(|error| CompletionError::ProviderError(format!("Claude Messages request: {error}")))?;

    let client = RenderedRequestCapturingHttpClient::new(MessagesTransport { fixture, live: http.clone() });
    let response = match client.send_streaming(http_request).await {
        Ok(response) => response,
        Err(http_client::Error::InvalidStatusCodeWithMessage(status, message)) => {
            if status.as_u16() == 401 {
                bearer.invalidate().await;
            }
            return Err(non_success_error(status, None, &body_prefix(message.as_bytes())));
        }
        Err(error) => return Err(error.into()),
    };
    // … unchanged from here (status/request-id/non-2xx branch, stream_sse_body) …
}
```
Rename `SeatTransport` → `MessagesTransport` (same body). Remove the `claude_seat_auth` and `ClaudeSeatConfig` imports. Move `lock_process_seat_for_test` here as `#[cfg(test)] pub(crate) fn lock_fixtures_for_test()` (same body: takes the static mutex, clears the fixture queue).

- [ ] **Step 5: Runtime arms and deletions**

`context.rs` Claude arm:

```rust
            BackendProviderKind::ClaudeCliSubscription => {
                let client = tokio::time::timeout(
                    self.startup_readiness.build_timeout,
                    crate::claude_subscription::ClaudeSubscriptionClient::build(self.node.clone(), behavior.agent_did()),
                )
                .await
                .map_err(|_| anyhow::anyhow!("timed out after {:?} building the Claude subscription completion client", self.startup_readiness.build_timeout))
                .and_then(|result| result)
                .with_context(|| format!("building Claude subscription completion client for behavior {}", behavior.behavior_id))?;
                Box::pin(self.run_behavior_with_client(behavior, request_rx, shutdown, prompt_builder, preamble, loop_tools.clone(), background_tool_registry, tool_surface.approval_required_tools().to_vec(), tool_surface.output_obligations(), client)).await
            }
```
`oneshot.rs` Claude arm: `ClaudeSubscriptionClient::build(node.clone(), behavior.agent_did()).await.with_context(...)?` then `run_oneshot_with_completion_client(...)` as the Codex arm does.

Delete `crates/gents/src/claude_seat_auth.rs` and `crates/gents/src/claude_completer/` (`git rm -r`); remove their `pub mod` lines from `lib.rs`; `git grep -n 'claude_seat_auth\|claude_completer\|install_process_seat\|require_process_seat\|probe_process_seat_health\|probe_seat_detail\|ClaudeSeatConfig\|install_fake_seat\|lock_process_seat_for_test' -- crates` must be empty.

CLI: remove `ServeArgs.claude_config_dir` and its `#[arg]`; delete `install_claude_subscription_seat` and its call; delete the `claude_subscription` status-JSON object; delete the seat tests in `serve.rs` and `args/tests.rs::server_parses_a2b_claude_seat_flags` (+ the help-text negative test, or repoint it to assert `--claude-config-dir` is gone). `git grep -n 'claude_config_dir\|claude-config-dir' -- crates` must be empty.

- [ ] **Step 6: Tests, gates, commit**

```bash
cargo test -p gents --lib claude_ 2>&1 | tail -10
cargo test -p gents --lib agent::loop_stream::tests::claude 2>&1 | tail -4
cargo test -p gents --test conformance -- prompt_assembly::provider_invocations docs::rig_vocabulary 2>&1 | tail -4
cargo test -p gents 2>&1 | grep -E 'test result|FAILED' | head
env -u TMPDIR cargo test -p gents-cli 2>&1 | grep -E 'test result|FAILED' | head
cargo check --workspace --all-targets 2>&1 | tail -3
git add -A crates/gents crates/gents-cli
git status --porcelain | grep -v '^??'
git commit -m "feat(claude): authenticate the Messages wire with the agent's OAuthCredential; delete the seat

ClaudeSubscriptionClient::build looks the agent's claude-subscription
credential up and binds the shared single-flight bearer; stream_messages
takes the bearer and a shared client, and invalidates the bearer once on
a 401 so the loop's retry re-sends with a refreshed token. The host-local
seat (--claude-config-dir, credentials file, Keychain reader), the
process-seat slot, and the claude_completer residue are gone; the claude
binary is no longer a dependency."
```

### Task 5: Docs, live probes #13–#16, and the PR 5 gate

**Files:**
- Modify: `docs/backends.md` (Claude section onto the credential model: setup via `gents claude-login`, credential storage = same document model as Codex/Grok, fleet/replication paragraph, failure table, remove every `--claude-config-dir` / Keychain / seat sentence), `docs/design-notes/SPEC-claude-a2c-tool-bridging.md` (C2 lock: credential model, one dated paragraph), `docs/design-notes/PR-STACK-claude-track-b-tools.md` (status: PR 5 parity), `CLAUDE.md` (the arm's-length sentence becomes: "Claude subscription seats are reached over Anthropic Messages HTTP with an agent-scoped `OAuthCredential` written by `gents claude-login` and refreshed by gents; the `claude` binary is not a dependency.")
- Create: `.scratch/claude-spike/logs/write-request-13.md` … `write-request-16.md` and their evidence files

**Interfaces:** none new; this task closes the spec's §6 live bars.

- [ ] **Step 1: Docs**

Rewrite the four locations above. `git grep -n 'claude-config-dir\|Keychain\|\.credentials\.json\|claude_completer\|seat token' -- docs/backends.md CLAUDE.md docs/design-notes/PR-STACK-claude-track-b-tools.md` must return only dated historical annotations (`[Retired …]` / "previously"). Commit: `docs(claude): document the OAuth credential model for the Claude backend`.

- [ ] **Step 2: Live #13 — login**

Request file in the #10b shape. Run `gents claude-login --manual --agent-did <the default agent DID> --home ~/.gents` (manual avoids a browser dependency on the desktop; loopback may be exercised instead if a browser is available — record which). Bar: JSON output shows `login: completed`, `access_token: "<redacted>"`, `access_token_expires_at` ≈ now + 8 h; `gents query --collection OAuthCredential --field credential_id --field provider --field access_token_expires_at --field enabled --filter '{"provider":{"_eq":"claude-subscription"}}'` shows one row; a token scan of the evidence files is clean.

- [ ] **Step 3: Live #14 — tool turn on the credential bearer**

Start `gents server --home ~/.gents --tool-root <repo> --tool-ceiling readwrite` (no Claude flags exist any more); one chat turn with the #10b prompt; bar identical to #10b (`system[]` order, cache breakpoints, `tools` per scope, `cached_input_tokens` on the second call, `AgentToolCall.args.path == "."`, response `listed`, no 4xx/429); plus `gents diagnose` → `checks.claude_auth.ok == true`.

- [ ] **Step 4: Live #15 — refresh on a stale credential**

With the server stopped, set the credential stale: `update_OAuthCredential(filter: {credential_id: {_eq: "claude-subscription:<did>"}}, input: {access_token_expires_at: "2020-01-01T00:00:00Z"})` via curl on the running server's GraphQL (start the server first, mutate, then chat). One chat turn (`Reply with exactly: pong`). Bar: the turn completes; the credential row now has `access_token_expires_at` in the future and a new `last_refresh`; server stderr contains no `401`; the health snapshot (next cycle) is healthy. Stop on any 4xx/429.

- [ ] **Step 5: Live #16 — diagnose and runnability**

`gents diagnose` shows `claude_auth.ok=true`. Then set the credential `enabled: false` via GraphQL, restart the server: the Claude behavior is reported not runnable with the `gents claude-login --agent-did` hint in the startup readiness output; `gents diagnose` shows `claude_auth.ok=false` with the same guidance. Re-enable the credential and confirm the behavior is runnable again.

- [ ] **Step 6: PR 5 gate**

```bash
cargo test -p gents 2>&1 | grep -E 'test result|FAILED'
env -u TMPDIR cargo test -p gents-cli 2>&1 | grep -E 'test result|FAILED'
cargo test -p gents-claude-login -p gents-protocol 2>&1 | grep -E 'test result|FAILED'
cargo check --workspace --all-targets 2>&1 | tail -3
rustup run 1.97.1 cargo fmt --all --check
(cd crates/gents/proofs && lake build 2>&1 | tail -2)
```
All green; then `claude/pr5-oauth-parity` is ready to open on top of `claude/pr4-cli-docs`.
