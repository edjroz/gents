//! Process-local Claude seat credentials for C2 Messages HTTP.
//!
//! Reads `claudeAiOauth.accessToken` from `--claude-config-dir/.credentials.json`
//! first. On macOS, if that file is absent, reads the same JSON from the login
//! Keychain service `Claude Code-credentials-{sha256(config_dir)[:8]}` (account
//! `$USER`). Never upserts `OAuthCredential`. Never logs or `Display`s the token.

use std::fmt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use sha2::{Digest, Sha256};
use thiserror::Error;

/// File the Claude CLI writes under `CLAUDE_CONFIG_DIR` (Linux and macOS
/// Keychain fallback).
pub const CREDENTIALS_FILE_NAME: &str = ".credentials.json";

/// Keychain service prefix used by the Claude CLI on macOS.
pub const MACOS_KEYCHAIN_SERVICE_PREFIX: &str = "Claude Code-credentials";

/// Access token read from the process seat. `Debug` is redacted; there is no
/// `Display` impl so it cannot leak through format strings by accident.
#[derive(Clone)]
pub struct SeatAccessToken(String);

impl SeatAccessToken {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// `Authorization` header value (`Bearer …`). Callers must not log this.
    pub fn authorization_value(&self) -> String {
        format!("Bearer {}", self.0)
    }
}

impl fmt::Debug for SeatAccessToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SeatAccessToken(redacted)")
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SeatAuthError {
    #[error("Claude seat credentials file missing: {path}")]
    MissingFile { path: String },
    #[error("Claude seat credentials file is not valid JSON")]
    InvalidJson,
    #[error("Claude seat credentials file has no claudeAiOauth.accessToken")]
    MissingAccessToken,
    #[error("Claude seat OAuth access token is expired; re-run gents claude-login")]
    Expired,
    #[error("Claude seat Keychain access denied for service {service}")]
    KeychainAccessDenied { service: String },
    #[error("Claude seat Keychain item missing for service {service}")]
    KeychainMissing { service: String },
    #[error("Claude seat Keychain account is unset (USER)")]
    KeychainAccountUnset,
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
        Ok(raw) => parse_credentials(&raw, now_millis),
        Err(file_err @ SeatAuthError::MissingFile { .. }) => {
            match read_macos_keychain_credentials(config_dir) {
                Ok(raw) => parse_credentials(&raw, now_millis),
                Err(SeatAuthError::KeychainAccessDenied { service }) => {
                    Err(SeatAuthError::KeychainAccessDenied { service })
                }
                Err(SeatAuthError::KeychainAccountUnset) => {
                    Err(SeatAuthError::KeychainAccountUnset)
                }
                Err(_) => Err(file_err),
            }
        }
        Err(error) => Err(error),
    }
}

fn read_credentials_file(config_dir: &Path) -> Result<String, SeatAuthError> {
    let path = credentials_path(config_dir);
    std::fs::read_to_string(&path).map_err(|_| SeatAuthError::MissingFile {
        path: path.display().to_string(),
    })
}

fn parse_credentials(raw: &str, now_millis: fn() -> i64) -> Result<SeatAccessToken, SeatAuthError> {
    let parsed: CredentialsFile =
        serde_json::from_str(raw).map_err(|_| SeatAuthError::InvalidJson)?;
    let oauth = parsed
        .claude_ai_oauth
        .ok_or(SeatAuthError::MissingAccessToken)?;
    if let Some(expires_at) = oauth.expires_at {
        if expires_at > 0 && expires_at <= now_millis() {
            return Err(SeatAuthError::Expired);
        }
    }
    let token = oauth.access_token.unwrap_or_default().trim().to_string();
    if token.is_empty() {
        return Err(SeatAuthError::MissingAccessToken);
    }
    Ok(SeatAccessToken(token))
}

pub fn credentials_path(config_dir: &Path) -> PathBuf {
    config_dir.join(CREDENTIALS_FILE_NAME)
}

/// Claude CLI Keychain service for this `--claude-config-dir`.
///
/// SHA-256 of the path bytes as given (no canonicalize, no trailing slash).
pub fn macos_keychain_service_name(config_dir: &Path) -> String {
    let digest = format!(
        "{:x}",
        Sha256::digest(config_dir.as_os_str().as_encoded_bytes())
    );
    format!("{MACOS_KEYCHAIN_SERVICE_PREFIX}-{}", &digest[..8])
}

fn read_macos_keychain_credentials(config_dir: &Path) -> Result<String, SeatAuthError> {
    #[cfg(target_os = "macos")]
    {
        let service = macos_keychain_service_name(config_dir);
        let account = macos_keychain_account()?;
        // Prefer `security(1)`: an unsigned debug `gents` is not on the
        // item ACL, so SecKeychainFindGenericPassword can block on a prompt
        // that never appears in a headless server. The CLI already returns
        // for this operator session.
        match read_macos_keychain_via_security_cli(&service, &account) {
            Ok(raw) => Ok(raw),
            Err(cli_err) => match read_macos_keychain_via_framework(&service, &account) {
                Ok(raw) => Ok(raw),
                Err(_) => Err(cli_err),
            },
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err(SeatAuthError::MissingFile {
            path: credentials_path(config_dir).display().to_string(),
        })
    }
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

#[cfg(target_os = "macos")]
const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;

#[cfg(target_os = "macos")]
fn read_macos_keychain_via_framework(
    service: &str,
    account: &str,
) -> Result<String, SeatAuthError> {
    let keychain =
        security_framework::os::macos::keychain::SecKeychain::default().map_err(|_| {
            SeatAuthError::KeychainAccessDenied {
                service: service.to_string(),
            }
        })?;
    match keychain.find_generic_password(service, account) {
        Ok((password, _)) => String::from_utf8(password.as_ref().to_vec())
            .map_err(|_| SeatAuthError::InvalidJson)
            .map(|raw| raw.trim().to_string()),
        Err(error) if error.code() == ERR_SEC_ITEM_NOT_FOUND => {
            Err(SeatAuthError::KeychainMissing {
                service: service.to_string(),
            })
        }
        Err(_) => Err(SeatAuthError::KeychainAccessDenied {
            service: service.to_string(),
        }),
    }
}

#[cfg(target_os = "macos")]
fn read_macos_keychain_via_security_cli(
    service: &str,
    account: &str,
) -> Result<String, SeatAuthError> {
    let output = std::process::Command::new("security")
        .args(["find-generic-password", "-s", service, "-a", account, "-w"])
        .stderr(std::process::Stdio::null())
        .output()
        .map_err(|_| SeatAuthError::KeychainAccessDenied {
            service: service.to_string(),
        })?;
    if !output.status.success() {
        return Err(SeatAuthError::KeychainMissing {
            service: service.to_string(),
        });
    }
    let raw = String::from_utf8(output.stdout).map_err(|_| SeatAuthError::InvalidJson)?;
    let trimmed = raw.trim().to_string();
    if trimmed.is_empty() {
        return Err(SeatAuthError::MissingAccessToken);
    }
    Ok(trimmed)
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
    use std::fs;

    fn temp_dir(label: &str) -> PathBuf {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../.scratch/claude-spike/tmp")
            .join(label);
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("temp dir");
        root
    }

    fn write_credentials(dir: &Path, body: &str) {
        fs::write(credentials_path(dir), body).expect("write credentials");
    }

    #[test]
    fn reads_access_token_from_credentials_file() {
        let dir = temp_dir("seat-auth-ok");
        write_credentials(
            &dir,
            r#"{"claudeAiOauth":{"accessToken":"sk-ant-oat01-test-token","expiresAt":9999999999999}}"#,
        );
        let token = read_seat_access_token(&dir).expect("token");
        assert_eq!(token.as_str(), "sk-ant-oat01-test-token");
        assert_eq!(
            token.authorization_value(),
            "Bearer sk-ant-oat01-test-token"
        );
        let debug = format!("{token:?}");
        assert_eq!(debug, "SeatAccessToken(redacted)");
        assert!(
            !debug.contains("sk-ant-oat01-test-token"),
            "Debug must not include the token: {debug}"
        );
    }

    #[test]
    fn missing_file_is_an_error_without_a_token() {
        let dir = temp_dir("seat-auth-missing");
        let err = read_seat_access_token(&dir).expect_err("missing");
        let msg = err.to_string();
        assert!(msg.contains(".credentials.json"), "{msg}");
        assert!(!msg.contains("sk-ant-oat01"));
    }

    #[test]
    fn missing_access_token_field_fail_closes() {
        let dir = temp_dir("seat-auth-no-token");
        write_credentials(&dir, r#"{"claudeAiOauth":{}}"#);
        let err = read_seat_access_token(&dir).expect_err("no token");
        assert_eq!(err, SeatAuthError::MissingAccessToken);
        assert!(!err.to_string().contains("sk-ant-oat01"));
    }

    #[test]
    fn invalid_json_does_not_echo_file_bytes() {
        let dir = temp_dir("seat-auth-bad-json");
        write_credentials(
            &dir,
            r#"{"claudeAiOauth":{"accessToken":"sk-ant-oat01-secret""#,
        );
        let err = read_seat_access_token(&dir).expect_err("invalid json");
        assert_eq!(err, SeatAuthError::InvalidJson);
        assert!(
            !err.to_string().contains("sk-ant-oat01-secret"),
            "{}",
            err.to_string()
        );
    }

    #[test]
    fn expired_token_fail_closes() {
        let dir = temp_dir("seat-auth-expired");
        write_credentials(
            &dir,
            r#"{"claudeAiOauth":{"accessToken":"sk-ant-oat01-expired","expiresAt":1}}"#,
        );
        let err = read_seat_access_token_at(&dir, || 2).expect_err("expired");
        assert_eq!(err, SeatAuthError::Expired);
        assert!(!err.to_string().contains("sk-ant-oat01-expired"));
    }

    #[test]
    fn empty_access_token_is_missing() {
        let dir = temp_dir("seat-auth-blank");
        write_credentials(&dir, r#"{"claudeAiOauth":{"accessToken":"   "}}"#);
        let err = read_seat_access_token(&dir).expect_err("blank");
        assert_eq!(err, SeatAuthError::MissingAccessToken);
    }

    #[test]
    fn macos_keychain_service_is_sha256_prefix_of_config_dir() {
        let dir = Path::new("/Users/edjroz/Repos/source/gents/.scratch/claude-spike/claude-config");
        assert_eq!(
            macos_keychain_service_name(dir),
            "Claude Code-credentials-6b2dc9d7"
        );
        assert_ne!(
            macos_keychain_service_name(Path::new(
                "/Users/edjroz/Repos/source/gents/.scratch/claude-spike/claude-config/"
            )),
            "Claude Code-credentials-6b2dc9d7",
            "trailing slash must not silently match"
        );
    }

    #[test]
    fn keychain_error_display_does_not_include_secrets() {
        let denied = SeatAuthError::KeychainAccessDenied {
            service: "Claude Code-credentials-6b2dc9d7".into(),
        };
        let msg = denied.to_string();
        assert!(msg.contains("Claude Code-credentials-6b2dc9d7"), "{msg}");
        assert!(!msg.contains("sk-ant-oat01"));
        assert!(!msg.contains("Bearer"));
    }
}
