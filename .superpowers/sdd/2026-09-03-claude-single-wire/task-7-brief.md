### Task 7: Seat auth without Security.framework; health probes the token; promotion → live #11

**Files:**
- Modify: `crates/gents/src/claude_seat_auth.rs:45-61` (enum), `:86-106` (`read_seat_access_token_at`), `:115-131` (`parse_credentials`), `:148-235` (Keychain), `:337-362` (tests)
- Modify: `crates/gents/Cargo.toml:83-85` (delete the macOS `security-framework` table)
- Modify: `crates/gents/src/claude_subscription.rs` (`ClaudeSeatConfig` drops `claude_bin`; `probe_process_seat_health` → token read; delete `probe_seat_auth_status`)
- Modify: `crates/gents/src/claude_completer/mod.rs` (delete `parse_auth_status_logged_in` if `gents-cli` does not import it — `claude_auth_probe.rs` has its own parser; verify with `git grep parse_auth_status_logged_in`)
- Modify: `crates/gents/src/backend_health.rs:233-262,304-359,688-856`
- Modify: `crates/gents-cli/src/commands/serve.rs` (`ClaudeSeatConfig::new` loses the third argument; `--claude-bin` on `server` is removed here since nothing reads it), `crates/gents-cli/src/cli/args.rs` (`ServeArgs.claude_bin`), `args/tests.rs`

**Interfaces:**
- Consumes: `read_seat_access_token(&Path) -> Result<SeatAccessToken, SeatAuthError>`.
- Produces:
  - `SeatAuthError::{MissingFile { path: String }, Io { path: String, message: String }, Malformed { reason: String }, KeychainNotFound { service: String }, KeychainAccessDenied { service: String }, KeychainAccountUnset, Expired { expires_at: String }}`.
  - `SeatAccessToken::source() -> SeatTokenSource` (`File | Keychain`) and `SeatAccessToken::expires_at() -> Option<DateTime<Utc>>`; `Debug` redacted.
  - `claude_subscription::probe_process_seat_health() -> Result<String, String>` (Ok = healthy detail `source=file|keychain expires_at=<rfc3339|none>`; Err = Display of the error plus the login hint for `Expired`/`MissingFile`).
  - `backend_health::record_probe_event` without `promote_document_on_unknown`; Claude backends promote `unknown → healthy` like HTTP backends.
  - `ClaudeSeatConfig::new(config_dir: PathBuf, write_approved: bool)`.

- [ ] **Step 1: Failing seat-auth tests** (replace the Keychain tests in `claude_seat_auth.rs` `mod tests`)

```rust
    fn write_credentials(dir: &Path, expires_at_millis: i64) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(
            dir.join(".credentials.json"),
            format!(r#"{{"claudeAiOauth":{{"accessToken":"sk-ant-oat01-TESTTOKEN","expiresAt":{expires_at_millis}}}}}"#),
        )
        .unwrap();
    }

    #[test]
    fn valid_file_reports_source_and_expiry() {
        let temp = tempfile::tempdir().unwrap();
        write_credentials(temp.path(), 4_102_444_800_000); // 2100-01-01
        let token = read_seat_access_token_at(temp.path(), || 1_000).expect("token");
        assert_eq!(token.source(), SeatTokenSource::File);
        assert_eq!(token.expires_at().map(|t| t.to_rfc3339()).as_deref(), Some("2100-01-01T00:00:00+00:00"));
        assert!(!format!("{token:?}").contains("TESTTOKEN"), "Debug must redact");
    }

    #[test]
    fn expired_file_reports_expires_at() {
        let temp = tempfile::tempdir().unwrap();
        write_credentials(temp.path(), 1_000);
        let err = read_seat_access_token_at(temp.path(), || 2_000).expect_err("expired");
        assert!(matches!(err, SeatAuthError::Expired { .. }));
        assert!(err.to_string().contains("1970-01-01T00:00:01"), "{err}");
    }

    #[test]
    fn malformed_file_is_malformed_not_missing() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join(".credentials.json"), "{not json").unwrap();
        let err = read_seat_access_token_at(temp.path(), || 0).expect_err("malformed");
        assert!(matches!(err, SeatAuthError::Malformed { .. }), "{err:?}");
    }

    #[test]
    fn missing_file_and_missing_keychain_item_is_missing_file() {
        let temp = tempfile::tempdir().unwrap();
        let err = read_seat_access_token_at(temp.path(), || 0).expect_err("missing");
        // On macOS the Keychain lookup for this fresh dir yields KeychainNotFound,
        // which folds back to MissingFile so the operator hint names the path.
        assert!(matches!(err, SeatAuthError::MissingFile { .. }), "{err:?}");
        assert!(err.to_string().contains(&temp.path().display().to_string()));
    }

    #[test]
    fn macos_keychain_service_is_sha256_prefix_of_absolute_config_dir() {
        let dir = Path::new("relative/claude-config");
        let absolute = std::env::current_dir().unwrap().join(dir);
        let digest = hex::encode(sha2::Sha256::digest(absolute.to_string_lossy().as_bytes()));
        assert_eq!(
            macos_keychain_service_name(dir),
            format!("Claude Code-credentials-{}", &digest[..8])
        );
        assert_ne!(
            macos_keychain_service_name(Path::new("relative/claude-config/")),
            macos_keychain_service_name(dir),
            "trailing slash must not silently match"
        );
    }

    #[test]
    fn keychain_error_display_does_not_include_secrets() {
        let err = SeatAuthError::KeychainAccessDenied { service: "Claude Code-credentials-deadbeef".into() };
        assert!(!err.to_string().contains("sk-ant"));
    }
```

Use whichever hashing crates `macos_keychain_service_name` already uses (`sha2` + `hex` are in the file today; if `hex` is absent, format with `{:x}`). If `macos_keychain_service_name` is `#[cfg(target_os = "macos")]`, gate the two Keychain tests the same way.

- [ ] **Step 2: Run to verify failure**

```bash
cargo test -p gents --lib claude_seat_auth 2>&1 | tail -15
```
Expected: compile errors for `source()`, `expires_at()`, `Expired { .. }`, `Malformed`.

- [ ] **Step 3: Implement**

Enum:

```rust
#[derive(Debug, Error, PartialEq, Eq)]
pub enum SeatAuthError {
    #[error("Claude seat credentials file missing: {path}")]
    MissingFile { path: String },
    #[error("Claude seat credentials file unreadable: {path}: {message}")]
    Io { path: String, message: String },
    #[error("Claude seat credentials malformed: {reason}")]
    Malformed { reason: String },
    #[error("Claude seat Keychain item not found for service {service}")]
    KeychainNotFound { service: String },
    #[error("Claude seat Keychain access denied for service {service}")]
    KeychainAccessDenied { service: String },
    #[error("Claude seat Keychain account is unset (USER)")]
    KeychainAccountUnset,
    #[error("Claude seat OAuth access token expired at {expires_at}")]
    Expired { expires_at: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeatTokenSource {
    File,
    Keychain,
}

pub struct SeatAccessToken {
    token: String,
    source: SeatTokenSource,
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl SeatAccessToken {
    pub fn authorization_value(&self) -> String { format!("Bearer {}", self.token) }
    pub fn source(&self) -> SeatTokenSource { self.source }
    pub fn expires_at(&self) -> Option<chrono::DateTime<chrono::Utc>> { self.expires_at }
}

impl fmt::Debug for SeatAccessToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SeatAccessToken")
            .field("source", &self.source)
            .field("expires_at", &self.expires_at)
            .finish_non_exhaustive()
    }
}
```

Reader:

```rust
fn read_seat_access_token_at(config_dir: &Path, now_millis: fn() -> i64) -> Result<SeatAccessToken, SeatAuthError> {
    match read_credentials_file(config_dir) {
        Ok(raw) => parse_credentials(&raw, now_millis, SeatTokenSource::File),
        Err(file_err @ SeatAuthError::MissingFile { .. }) => match read_macos_keychain_credentials(config_dir) {
            Ok(raw) => parse_credentials(&raw, now_millis, SeatTokenSource::Keychain),
            Err(SeatAuthError::KeychainNotFound { .. }) => Err(file_err),
            Err(other) => Err(other),
        },
        Err(error) => Err(error),
    }
}
```

`read_credentials_file`: `NotFound` → `MissingFile { path }`, other io errors → `Io { path, message }`. `parse_credentials(raw, now_millis, source)`: JSON failure or missing `claudeAiOauth.accessToken` → `Malformed { reason }`; expiry → `Expired { expires_at: chrono::DateTime::<Utc>::from_timestamp_millis(expires_at).map(|t| t.to_rfc3339()).unwrap_or_else(|| expires_at.to_string()) }`; success carries `expires_at` as a `DateTime<Utc>` when present. Keychain: delete `read_macos_keychain_via_framework` and `ERR_SEC_ITEM_NOT_FOUND`; `read_macos_keychain_credentials` calls `read_macos_keychain_via_security_cli` directly; the CLI wrapper maps exit status 44 (`errSecItemNotFound`) → `KeychainNotFound`, other non-zero → `KeychainAccessDenied`, non-UTF-8 output → `Malformed { reason: "keychain payload is not UTF-8" }`. Non-macOS: `read_macos_keychain_credentials` returns `KeychainNotFound { service }`. Delete `crates/gents/Cargo.toml` L83-85.

- [ ] **Step 4: Seat and health**

`claude_subscription.rs`: `ClaudeSeatConfig { config_dir, write_approved, http }`, `ClaudeSeatConfig::new(config_dir, write_approved)`; delete `probe_seat_auth_status`; replace `probe_process_seat_health`:

```rust
/// Health probe = the same token read the wire performs. No spawn, no refresh.
pub fn probe_process_seat_health() -> Result<String, String> {
    let seat = process_seat().ok_or_else(|| "process seat not installed".to_string())?;
    match crate::claude_seat_auth::read_seat_access_token(&seat.config_dir) {
        Ok(token) => Ok(format!(
            "source={} expires_at={}",
            match token.source() {
                crate::claude_seat_auth::SeatTokenSource::File => "file",
                crate::claude_seat_auth::SeatTokenSource::Keychain => "keychain",
            },
            token.expires_at().map(|t| t.to_rfc3339()).unwrap_or_else(|| "none".to_string())
        )),
        Err(error @ (crate::claude_seat_auth::SeatAuthError::Expired { .. }
        | crate::claude_seat_auth::SeatAuthError::MissingFile { .. })) => Err(format!(
            "{error}; run gents claude-login --config-dir {}",
            seat.config_dir.display()
        )),
        Err(error) => Err(error.to_string()),
    }
}
```

(`gents claude-login` takes `--config-dir`, not `--claude-config-dir`; the spec's hint text is corrected here.)

`backend_health.rs` Claude branch:

```rust
        if backend.provider_kind == crate::backend_provider::BackendProviderKind::ClaudeCliSubscription {
            probed_ids.insert(backend.backend_id.clone());
            let (event, error_text) = match crate::claude_subscription::probe_process_seat_health() {
                Ok(detail) => {
                    tracing::debug!(backend_id = %backend.backend_id, detail = %detail, "claude seat probe ok");
                    (ProbeEvent::ProbeSuccess, None)
                }
                Err(reason) => (ProbeEvent::ProbeFail, Some(reason)),
            };
            record_probe_event(backend, event, error_text, now, health_map, options, &mut outcome).await;
            continue;
        }
```

`record_probe_event`: drop the `promote_document_on_unknown` parameter and the `#[allow(clippy::too_many_arguments)]`; the promotion condition becomes `event == ProbeEvent::ProbeSuccess && backend.probe_status == UNKNOWN_PROBE_STATUS`. Update the HTTP-branch call site to the seven-argument form.

- [ ] **Step 5: Health tests** (replace `write_auth_status_fake`, `install_claude_seat`, and the four Claude tests)

```rust
    fn install_claude_seat_with_credentials(expires_at_millis: Option<i64>) -> tempfile::TempDir {
        let temp = tempfile::tempdir().expect("seat dir");
        let config_dir = temp.path().join("claude-config");
        std::fs::create_dir_all(&config_dir).unwrap();
        if let Some(expires) = expires_at_millis {
            std::fs::write(
                config_dir.join(".credentials.json"),
                format!(r#"{{"claudeAiOauth":{{"accessToken":"sk-ant-oat01-TEST","expiresAt":{expires}}}}}"#),
            )
            .unwrap();
        }
        crate::claude_subscription::install_process_seat(Some(
            crate::claude_subscription::ClaudeSeatConfig::new(config_dir, false),
        ));
        temp
    }

    #[tokio::test]
    async fn cycle_does_not_http_probe_claude_cli_subscription_backends() {
        let _guard = crate::claude_subscription::lock_process_seat_for_test();
        crate::claude_subscription::install_process_seat(None);
        let (options, client, health_map) = (probe_options(), reqwest::Client::new(), BackendHealthMap::new());
        let mut claude = claude_backend();
        claude.endpoint = "http://127.0.0.1:1/v1".to_string();
        let outcome = probe_backends_cycle(&client, &[claude], Utc::now(), &health_map, &options).await;
        assert!(outcome.promotable.is_empty());
        let snap = health_map.get("claude").await.expect("measured entry");
        assert_eq!(snap.state, BackendHealthState::Degraded);
        assert!(snap.last_error.as_deref().is_some_and(|e| e.contains("process seat not installed")), "{:?}", snap.last_error);
    }

    #[tokio::test]
    async fn cycle_demotes_claude_after_k_expired_probes_with_login_hint() {
        let _guard = crate::claude_subscription::lock_process_seat_for_test();
        let _seat = install_claude_seat_with_credentials(Some(1_000));
        let (options, client, health_map) = (probe_options(), reqwest::Client::new(), BackendHealthMap::new());
        let backends = vec![claude_backend()];
        for cycle in 1..=3u32 {
            let outcome = probe_backends_cycle(&client, &backends, Utc::now(), &health_map, &options).await;
            assert!(outcome.promotable.is_empty());
            let snap = health_map.get("claude").await.expect("entry");
            assert_eq!(snap.failure_count, cycle);
            let err = snap.last_error.clone().unwrap_or_default();
            assert!(err.contains("expired at") && err.contains("gents claude-login --config-dir"), "{err}");
            if cycle < 3 {
                assert_eq!(snap.state, BackendHealthState::Degraded);
            } else {
                assert_eq!(snap.state, BackendHealthState::Unhealthy);
                assert_eq!(outcome.flipped, vec!["claude".to_string()]);
            }
        }
        crate::claude_subscription::install_process_seat(None);
    }

    #[tokio::test]
    async fn cycle_marks_claude_healthy_and_promotes_unknown_document() {
        let _guard = crate::claude_subscription::lock_process_seat_for_test();
        let _seat = install_claude_seat_with_credentials(Some(4_102_444_800_000));
        let (options, client, health_map) = (probe_options(), reqwest::Client::new(), BackendHealthMap::new());
        let mut claude = claude_backend();
        claude.probe_status = "unknown".to_string();
        let outcome = probe_backends_cycle(&client, &[claude], Utc::now(), &health_map, &options).await;
        assert_eq!(outcome.promotable, vec!["claude".to_string()], "C9: unknown Claude document promotes on first pass");
        let snap = health_map.get("claude").await.expect("entry");
        assert_eq!(snap.state, BackendHealthState::Healthy);
        assert!(snap.last_error.is_none());
        crate::claude_subscription::install_process_seat(None);
    }

    #[tokio::test]
    async fn cycle_fails_claude_when_credentials_are_missing() {
        let _guard = crate::claude_subscription::lock_process_seat_for_test();
        let _seat = install_claude_seat_with_credentials(None);
        let (options, client, health_map) = (probe_options(), reqwest::Client::new(), BackendHealthMap::new());
        probe_backends_cycle(&client, &[claude_backend()], Utc::now(), &health_map, &options).await;
        let snap = health_map.get("claude").await.expect("entry");
        assert_eq!(snap.state, BackendHealthState::Degraded);
        let err = snap.last_error.clone().unwrap_or_default();
        assert!(err.contains("credentials file missing") && err.contains("gents claude-login --config-dir"), "{err}");
        crate::claude_subscription::install_process_seat(None);
    }
```

The missing-credentials test depends on the Keychain also lacking an item for the tempdir's digest, which is true for a fresh tempdir on any machine.

- [ ] **Step 6: CLI follow-through**

`serve.rs`: `ClaudeSeatConfig::new(config_dir, args.claude_write_approved)`; remove `ServeArgs.claude_bin` and its mention in `server_parses_a2b_claude_seat_flags`. `git grep -n 'claude_bin' crates/gents-cli/src/commands/serve.rs` must print nothing.

- [ ] **Step 7: Gates and commit**

```bash
cargo test -p gents 2>&1 | grep -E 'test result|FAILED|panicked' | head
cargo test -p gents-cli 2>&1 | grep -E 'test result|FAILED' | head
cargo check --workspace --all-targets 2>&1 | tail -3
git grep -n 'security_framework\|security-framework\|ERR_SEC_ITEM_NOT_FOUND\|probe_seat_auth_status\|parse_auth_status_logged_in' -- crates ; echo "residue exit=$?"
git add crates/gents/Cargo.toml Cargo.lock crates/gents/src crates/gents-cli/src
git commit -m "feat(claude): health probes the seat token; promote unknown Claude documents

The probe now performs the same read the wire performs (credentials file,
then security(1) Keychain) and reports source and expiry; expired or
missing seats carry the claude-login hint. record_probe_event no longer
special-cases Claude, so a document born unknown promotes to healthy on
its first passing cycle (C9, C6). Security.framework fallback and the
claude auth status spawn are gone; the error enum names each failure."
```

- [ ] **Step 8: Live #11**

Write `.scratch/claude-spike/logs/write-request-11.md`. Bar (spec §6 #11), three server starts with no chat turns:
1. Start with `--claude-config-dir <tempdir with an expired .credentials.json>` (write the file with `expiresAt: 1000` in a fresh dir under `.scratch/claude-spike/tmp/expired-seat`); after one probe interval, `gents query --collection InferenceBackend --field backend_id --field probe_status` plus the server stderr line must show `Unhealthy`/degraded detail containing `gents claude-login --config-dir`. Stop.
2. Start with the real spike config dir and a backend document whose `probe_status` is `unknown` (set it via the existing `gents` backend update path — `gents backend set` or a GraphQL mutation on `InferenceBackend` using `escape_graphql_string`-safe literal; record the mutation in the evidence). After one probe cycle the document reads `healthy` without hand-editing. Stop.
3. Token scan on all `b3-live-health-*` files.
This run sends no Messages request, but it reads the seat and writes an `InferenceBackend` document, so it stays a numbered approval. Ask "Approve #11?" and wait.

