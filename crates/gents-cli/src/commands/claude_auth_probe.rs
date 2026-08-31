//! Read-only Claude CLI seat probe for Path A (no DefraDB oat).
//!
//! Reports whether the explicit `--config-dir` Claude seat is logged in and
//! what subscription/auth method it claims. Never upserts `OAuthCredential`.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::cli::args::ClaudeAuthProbeArgs;
use crate::request_helpers::print_json;
use gents::claude_completer::sanitize_child_env;

/// Structured probe result matching SPEC-claude-phase6-packaging.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ClaudeAuthProbeResult {
    pub logged_in: bool,
    pub auth_method: Option<String>,
    pub subscription_type: Option<String>,
    pub config_dir: PathBuf,
    pub api_key_source: Option<String>,
    pub api_provider: Option<String>,
    pub email: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ClaudeAuthStatusJson {
    #[serde(default, rename = "loggedIn")]
    logged_in: bool,
    #[serde(default, rename = "authMethod")]
    auth_method: Option<String>,
    #[serde(default, rename = "subscriptionType")]
    subscription_type: Option<String>,
    #[serde(default, rename = "apiProvider")]
    api_provider: Option<String>,
    #[serde(default)]
    email: Option<String>,
}

pub(crate) async fn claude_auth_probe(args: ClaudeAuthProbeArgs) -> Result<()> {
    let result = run_claude_auth_probe(&args)?;
    print_json(&claude_auth_probe_result_json(&result))?;
    Ok(())
}

pub(crate) fn run_claude_auth_probe(args: &ClaudeAuthProbeArgs) -> Result<ClaudeAuthProbeResult> {
    if args.config_dir.as_os_str().is_empty() {
        bail!("--config-dir is required (no silent ~/.claude default)");
    }
    let config_dir = args
        .config_dir
        .canonicalize()
        .unwrap_or_else(|_| args.config_dir.clone());
    let claude_bin = args
        .claude_bin
        .clone()
        .unwrap_or_else(|| PathBuf::from("claude"));

    let status = invoke_claude_auth_status(&claude_bin, &config_dir)?;
    Ok(probe_result_from_status(config_dir, status))
}

fn invoke_claude_auth_status(claude_bin: &Path, config_dir: &Path) -> Result<ClaudeAuthStatusJson> {
    let mut cmd = Command::new(claude_bin);
    cmd.args(["auth", "status", "--json"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_clear()
        .envs(sanitize_child_env(std::env::vars_os()))
        .env("CLAUDE_CONFIG_DIR", config_dir);
    for key in gents::claude_completer::STRIPPED_ENV_VARS {
        cmd.env_remove(key);
    }

    let output = cmd
        .output()
        .with_context(|| format!("spawn {} auth status", claude_bin.display()))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        bail!(
            "claude auth status exit {}: {}",
            output.status.code().unwrap_or(-1),
            first_nonempty(stderr.trim(), stdout.trim())
        );
    }
    parse_claude_auth_status_json(stdout.trim())
        .with_context(|| format!("parse claude auth status JSON from {}", config_dir.display()))
}

fn parse_claude_auth_status_json(text: &str) -> Result<ClaudeAuthStatusJson> {
    // Tolerate leading/trailing noise; take the outermost JSON object.
    let start = text
        .find('{')
        .ok_or_else(|| anyhow::anyhow!("no JSON object in auth status output"))?;
    let end = text
        .rfind('}')
        .ok_or_else(|| anyhow::anyhow!("unterminated JSON object in auth status output"))?;
    let slice = &text[start..=end];
    serde_json::from_str(slice).context("decode Claude auth status JSON")
}

fn probe_result_from_status(
    config_dir: PathBuf,
    status: ClaudeAuthStatusJson,
) -> ClaudeAuthProbeResult {
    // Path A Max seats should not be on an API-key source. Claude's status JSON
    // does not currently expose an apiKeySource field; report "none" when the
    // seat looks like a claude.ai subscription login.
    let api_key_source = if status.logged_in
        && status
            .auth_method
            .as_deref()
            .is_some_and(|m| m.eq_ignore_ascii_case("claude.ai"))
    {
        Some("none".to_string())
    } else if status.logged_in {
        Some("unknown".to_string())
    } else {
        Some("none".to_string())
    };

    ClaudeAuthProbeResult {
        logged_in: status.logged_in,
        auth_method: status.auth_method,
        subscription_type: status.subscription_type,
        config_dir,
        api_key_source,
        api_provider: status.api_provider,
        email: status.email,
    }
}

pub(crate) fn claude_auth_probe_result_json(result: &ClaudeAuthProbeResult) -> Value {
    json!({
        "logged_in": result.logged_in,
        "auth_method": result.auth_method,
        "subscription_type": result.subscription_type,
        "config_dir": result.config_dir,
        "api_key_source": result.api_key_source,
        "api_provider": result.api_provider,
        "email": result.email,
        "credential_store": "claude_config_dir",
        "oauth_credential_written": false,
        "note": "Path A seat lives in CLAUDE_CONFIG_DIR / --config-dir; gents does not store Anthropic oat in DefraDB",
    })
}

fn first_nonempty<'a>(a: &'a str, b: &'a str) -> &'a str {
    if !a.is_empty() {
        a
    } else {
        b
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_spike_auth_status_shape() {
        let raw = r#"{
  "loggedIn": true,
  "authMethod": "claude.ai",
  "apiProvider": "firstParty",
  "analyticsDisabled": false,
  "projectsDirectory": "/tmp/projects",
  "email": "user@example.com",
  "orgId": "org",
  "orgName": "Org",
  "subscriptionType": "max"
}"#;
        let status = parse_claude_auth_status_json(raw).expect("parse");
        let result = probe_result_from_status(PathBuf::from("/tmp/cfg"), status);
        assert!(result.logged_in);
        assert_eq!(result.auth_method.as_deref(), Some("claude.ai"));
        assert_eq!(result.subscription_type.as_deref(), Some("max"));
        assert_eq!(result.api_key_source.as_deref(), Some("none"));
        assert_eq!(result.email.as_deref(), Some("user@example.com"));
        let json = claude_auth_probe_result_json(&result);
        assert_eq!(json["oauth_credential_written"], false);
        assert_eq!(json["credential_store"], "claude_config_dir");
    }

    #[test]
    fn logged_out_reports_none_api_key_source() {
        let status = parse_claude_auth_status_json(r#"{"loggedIn":false}"#).unwrap();
        let result = probe_result_from_status(PathBuf::from("/tmp/cfg"), status);
        assert!(!result.logged_in);
        assert_eq!(result.api_key_source.as_deref(), Some("none"));
    }

    #[test]
    fn tolerates_leading_noise_around_json() {
        let raw = "note: ok\n{\"loggedIn\":true,\"authMethod\":\"claude.ai\",\"subscriptionType\":\"max\"}\n";
        let status = parse_claude_auth_status_json(raw).expect("parse");
        assert!(status.logged_in);
    }
}
