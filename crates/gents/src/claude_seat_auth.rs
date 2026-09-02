//! Process-local Claude seat credentials for C2 Messages HTTP.
//!
//! Reads `claudeAiOauth.accessToken` from `--claude-config-dir/.credentials.json`.
//! Never upserts `OAuthCredential`. Never logs or `Display`s the token.

use std::fmt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use thiserror::Error;

/// File the Claude CLI writes under `CLAUDE_CONFIG_DIR` (Linux and macOS
/// Keychain fallback).
pub const CREDENTIALS_FILE_NAME: &str = ".credentials.json";

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
/// function does not fall back to `~/.claude`.
pub fn read_seat_access_token(config_dir: &Path) -> Result<SeatAccessToken, SeatAuthError> {
    read_seat_access_token_at(config_dir, now_millis)
}

fn read_seat_access_token_at(
    config_dir: &Path,
    now_millis: fn() -> i64,
) -> Result<SeatAccessToken, SeatAuthError> {
    let path = credentials_path(config_dir);
    let raw = std::fs::read_to_string(&path).map_err(|_| SeatAuthError::MissingFile {
        path: path.display().to_string(),
    })?;
    let parsed: CredentialsFile =
        serde_json::from_str(&raw).map_err(|_| SeatAuthError::InvalidJson)?;
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
}
