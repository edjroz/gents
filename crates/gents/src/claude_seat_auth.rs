//! Process-local Claude seat credentials for C2 Messages HTTP.
//!
//! Reads `claudeAiOauth.accessToken` from `--claude-config-dir/.credentials.json`
//! first. On macOS, if that file is absent, reads the same JSON from the login
//! Keychain service `Claude Code-credentials-{sha256(config_dir)[:8]}` (account
//! `$USER`) via `security(1)`. Never upserts `OAuthCredential`. Never logs or
//! `Display`s the token.

use std::fmt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Utc};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use thiserror::Error;

/// File the Claude CLI writes under `CLAUDE_CONFIG_DIR` (Linux and macOS
/// Keychain fallback).
pub const CREDENTIALS_FILE_NAME: &str = ".credentials.json";

/// Keychain service prefix used by the Claude CLI on macOS.
pub const MACOS_KEYCHAIN_SERVICE_PREFIX: &str = "Claude Code-credentials";

/// Where a seat token was read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeatTokenSource {
    File,
    Keychain,
}

/// Access token read from the process seat. `Debug` is redacted; there is no
/// `Display` impl so it cannot leak through format strings by accident.
#[derive(Clone)]
pub struct SeatAccessToken {
    token: String,
    source: SeatTokenSource,
    expires_at: Option<DateTime<Utc>>,
}

impl SeatAccessToken {
    /// `Authorization` header value (`Bearer …`). Callers must not log this.
    pub fn authorization_value(&self) -> String {
        format!("Bearer {}", self.token)
    }

    pub fn source(&self) -> SeatTokenSource {
        self.source
    }

    pub fn expires_at(&self) -> Option<DateTime<Utc>> {
        self.expires_at
    }
}

impl fmt::Debug for SeatAccessToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SeatAccessToken")
            .field("source", &self.source)
            .field("expires_at", &self.expires_at)
            .finish_non_exhaustive()
    }
}

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

#[derive(Deserialize)]
struct CredentialsFile {
    #[serde(rename = "claudeAiOauth")]
    claude_ai_oauth: Option<ClaudeAiOauth>,
}

#[derive(Deserialize)]
struct ClaudeAiOauth {
    #[serde(rename = "accessToken")]
    access_token: Option<String>,
    #[serde(rename = "expiresAt")]
    expires_at: Option<i64>,
}

/// Read the seat access token from `{config_dir}/.credentials.json`.
///
/// `config_dir` is the explicit process seat (`--claude-config-dir`). This
/// function does not fall back to `~/.claude`. On macOS, a missing file falls
/// through to the Claude CLI Keychain item for this config dir.
pub fn read_seat_access_token(config_dir: &Path) -> Result<SeatAccessToken, SeatAuthError> {
    read_seat_access_token_at(config_dir, now_millis)
}

fn read_seat_access_token_at(
    config_dir: &Path,
    now_millis: fn() -> i64,
) -> Result<SeatAccessToken, SeatAuthError> {
    match read_credentials_file(config_dir) {
        Ok(raw) => parse_credentials(&raw, now_millis, SeatTokenSource::File),
        Err(file_err @ SeatAuthError::MissingFile { .. }) => {
            match read_macos_keychain_credentials(config_dir) {
                Ok(raw) => parse_credentials(&raw, now_millis, SeatTokenSource::Keychain),
                Err(SeatAuthError::KeychainNotFound { .. }) => Err(file_err),
                Err(other) => Err(other),
            }
        }
        Err(error) => Err(error),
    }
}

fn read_credentials_file(config_dir: &Path) -> Result<String, SeatAuthError> {
    let path = credentials_path(config_dir);
    std::fs::read_to_string(&path).map_err(|error| {
        let path = path.display().to_string();
        if error.kind() == std::io::ErrorKind::NotFound {
            SeatAuthError::MissingFile { path }
        } else {
            SeatAuthError::Io {
                path,
                message: error.to_string(),
            }
        }
    })
}

fn parse_credentials(
    raw: &str,
    now_millis: fn() -> i64,
    source: SeatTokenSource,
) -> Result<SeatAccessToken, SeatAuthError> {
    // Never echo the decode error: serde may quote file bytes.
    let parsed: CredentialsFile =
        serde_json::from_str(raw).map_err(|_| SeatAuthError::Malformed {
            reason: "credentials are not valid JSON".to_string(),
        })?;
    let oauth = parsed
        .claude_ai_oauth
        .ok_or_else(|| SeatAuthError::Malformed {
            reason: "no claudeAiOauth object".to_string(),
        })?;
    let expires_at = oauth.expires_at.filter(|millis| *millis > 0);
    if let Some(expires_at) = expires_at {
        if expires_at <= now_millis() {
            return Err(SeatAuthError::Expired {
                expires_at: DateTime::<Utc>::from_timestamp_millis(expires_at)
                    .map(|t| t.to_rfc3339())
                    .unwrap_or_else(|| expires_at.to_string()),
            });
        }
    }
    let token = oauth.access_token.unwrap_or_default().trim().to_string();
    if token.is_empty() {
        return Err(SeatAuthError::Malformed {
            reason: "no claudeAiOauth.accessToken".to_string(),
        });
    }
    Ok(SeatAccessToken {
        token,
        source,
        expires_at: expires_at.and_then(DateTime::<Utc>::from_timestamp_millis),
    })
}

pub fn credentials_path(config_dir: &Path) -> PathBuf {
    config_dir.join(CREDENTIALS_FILE_NAME)
}

/// Claude CLI Keychain service for this `--claude-config-dir`.
///
/// SHA-256 of the absolute path bytes (relative paths are resolved against
/// the current directory; no canonicalize, no trailing-slash normalization).
pub fn macos_keychain_service_name(config_dir: &Path) -> String {
    let absolute = if config_dir.is_absolute() {
        config_dir.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(config_dir))
            .unwrap_or_else(|_| config_dir.to_path_buf())
    };
    let digest = format!(
        "{:x}",
        Sha256::digest(absolute.to_string_lossy().as_bytes())
    );
    format!("{MACOS_KEYCHAIN_SERVICE_PREFIX}-{}", &digest[..8])
}

#[cfg(target_os = "macos")]
fn read_macos_keychain_credentials(config_dir: &Path) -> Result<String, SeatAuthError> {
    let service = macos_keychain_service_name(config_dir);
    let account = macos_keychain_account()?;
    // `security(1)` only: an unsigned debug `gents` is not on the item ACL,
    // so SecKeychainFindGenericPassword can block on a prompt that never
    // appears in a headless server. The CLI already returns for this
    // operator session.
    read_macos_keychain_via_security_cli(&service, &account)
}

#[cfg(not(target_os = "macos"))]
fn read_macos_keychain_credentials(config_dir: &Path) -> Result<String, SeatAuthError> {
    Err(SeatAuthError::KeychainNotFound {
        service: macos_keychain_service_name(config_dir),
    })
}

#[cfg(target_os = "macos")]
fn macos_keychain_account() -> Result<String, SeatAuthError> {
    std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or(SeatAuthError::KeychainAccountUnset)
}

/// `security(1)` exit status for `errSecItemNotFound`.
#[cfg(target_os = "macos")]
const SECURITY_CLI_ITEM_NOT_FOUND: i32 = 44;

#[cfg(target_os = "macos")]
fn read_macos_keychain_via_security_cli(
    service: &str,
    account: &str,
) -> Result<String, SeatAuthError> {
    let output = std::process::Command::new("security")
        .args(["find-generic-password", "-s", service, "-a", account, "-w"])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .map_err(|_| SeatAuthError::KeychainAccessDenied {
            service: service.to_string(),
        })?;
    if !output.status.success() {
        return Err(
            if output.status.code() == Some(SECURITY_CLI_ITEM_NOT_FOUND) {
                SeatAuthError::KeychainNotFound {
                    service: service.to_string(),
                }
            } else {
                SeatAuthError::KeychainAccessDenied {
                    service: service.to_string(),
                }
            },
        );
    }
    let raw = String::from_utf8(output.stdout).map_err(|_| SeatAuthError::Malformed {
        reason: "keychain payload is not UTF-8".to_string(),
    })?;
    Ok(raw.trim().to_string())
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_credentials(dir: &Path, expires_at_millis: i64) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(
            dir.join(".credentials.json"),
            format!(
                r#"{{"claudeAiOauth":{{"accessToken":"sk-ant-oat01-TESTTOKEN","expiresAt":{expires_at_millis}}}}}"#
            ),
        )
        .unwrap();
    }

    #[test]
    fn valid_file_reports_source_and_expiry() {
        let temp = tempfile::tempdir().unwrap();
        write_credentials(temp.path(), 4_102_444_800_000); // 2100-01-01
        let token = read_seat_access_token_at(temp.path(), || 1_000).expect("token");
        assert_eq!(token.source(), SeatTokenSource::File);
        assert_eq!(
            token.expires_at().map(|t| t.to_rfc3339()).as_deref(),
            Some("2100-01-01T00:00:00+00:00")
        );
        assert!(
            !format!("{token:?}").contains("TESTTOKEN"),
            "Debug must redact"
        );
        assert_eq!(token.authorization_value(), "Bearer sk-ant-oat01-TESTTOKEN");
    }

    #[test]
    fn expired_file_reports_expires_at() {
        let temp = tempfile::tempdir().unwrap();
        write_credentials(temp.path(), 1_000);
        let err = read_seat_access_token_at(temp.path(), || 2_000).expect_err("expired");
        assert!(matches!(err, SeatAuthError::Expired { .. }));
        assert!(err.to_string().contains("1970-01-01T00:00:01"), "{err}");
        assert!(!err.to_string().contains("TESTTOKEN"));
    }

    #[test]
    fn malformed_file_is_malformed_not_missing() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join(".credentials.json"), "{not json").unwrap();
        let err = read_seat_access_token_at(temp.path(), || 0).expect_err("malformed");
        assert!(matches!(err, SeatAuthError::Malformed { .. }), "{err:?}");
    }

    #[test]
    fn missing_access_token_field_is_malformed() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(
            temp.path().join(".credentials.json"),
            r#"{"claudeAiOauth":{"accessToken":"   "}}"#,
        )
        .unwrap();
        let err = read_seat_access_token_at(temp.path(), || 0).expect_err("blank");
        assert!(matches!(err, SeatAuthError::Malformed { .. }), "{err:?}");
        assert!(!err.to_string().contains("sk-ant"));
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
        let digest = format!(
            "{:x}",
            Sha256::digest(absolute.to_string_lossy().as_bytes())
        );
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
        let err = SeatAuthError::KeychainAccessDenied {
            service: "Claude Code-credentials-deadbeef".into(),
        };
        assert!(err.to_string().contains("Claude Code-credentials-deadbeef"));
        assert!(!err.to_string().contains("sk-ant"));
        assert!(!err.to_string().contains("Bearer"));
    }
}
