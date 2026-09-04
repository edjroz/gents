//! Login-time child-process hygiene for `gents claude-login`
//! (`sanitize_child_env`) and the model-id default. No completer lives here
//! any more: every Claude turn goes over Messages HTTP (`claude_messages`).

use std::collections::HashMap;
use std::ffi::OsString;

use serde_json::Value;

/// Default client-facing model slug for ClaudeCliSubscription.
///
/// Full Claude model IDs only — not the old invented `claude-plan` seat label.
pub const DEFAULT_MODEL_ID: &str = "claude-sonnet-5";

/// Read `loggedIn` from `claude auth status --json` stdout (read-only probe).
///
/// Tolerates leading/trailing noise around the JSON object, matching the CLI
/// `claude-auth-probe` parser. Missing `loggedIn` is an error, not `false`.
pub fn parse_auth_status_logged_in(text: &str) -> Result<bool, String> {
    let start = text
        .find('{')
        .ok_or_else(|| "no JSON object in auth status output".to_string())?;
    let end = text
        .rfind('}')
        .ok_or_else(|| "unterminated JSON object in auth status output".to_string())?;
    let value: Value = serde_json::from_str(&text[start..=end])
        .map_err(|error| format!("decode Claude auth status JSON: {error}"))?;
    value
        .get("loggedIn")
        .and_then(Value::as_bool)
        .ok_or_else(|| "loggedIn missing from auth status JSON".to_string())
}

/// Environment variable names that must not reach the Claude CLI child.
///
/// These flip the seat onto API-key / cloud-provider billing paths. Keep in
/// sync with the spike completer (`.scratch/claude-spike/bin/claude-completer.sh`).
pub const STRIPPED_ENV_VARS: &[&str] = &[
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_API_KEY_OLD",
    "CLAUDE_CODE_OAUTH_TOKEN",
    "AWS_BEARER_TOKEN_BEDROCK",
    "ANTHROPIC_BEDROCK_BASE_URL",
    "ANTHROPIC_VERTEX_PROJECT_ID",
    "CLOUD_ML_REGION",
    "ANTHROPIC_FOUNDRY_API_KEY",
];

/// Remove Anthropic/cloud override vars from an inherited environment map.
pub fn sanitize_child_env<K, V>(
    env: impl IntoIterator<Item = (K, V)>,
) -> HashMap<OsString, OsString>
where
    K: Into<OsString>,
    V: Into<OsString>,
{
    let strip: std::collections::HashSet<&str> = STRIPPED_ENV_VARS.iter().copied().collect();
    env.into_iter()
        .map(|(k, v)| (k.into(), v.into()))
        .filter(|(k, _)| k.to_str().map(|s| !strip.contains(s)).unwrap_or(true))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::{OsStr, OsString};

    #[test]
    fn sanitize_child_env_strips_anthropic_and_cloud_vars() {
        let inherited = [
            ("PATH", "/usr/bin"),
            ("ANTHROPIC_API_KEY", "sk-ant-api03-secret"),
            ("CLAUDE_CODE_OAUTH_TOKEN", "oat-secret"),
            ("AWS_BEARER_TOKEN_BEDROCK", "bedrock"),
            ("CLAUDE_CONFIG_DIR", "/tmp/claude-config"),
            ("HOME", "/tmp"),
        ];
        let cleaned = sanitize_child_env(inherited);
        assert_eq!(
            cleaned.get(OsStr::new("PATH")).map(OsString::as_os_str),
            Some(OsStr::new("/usr/bin"))
        );
        assert_eq!(
            cleaned
                .get(OsStr::new("CLAUDE_CONFIG_DIR"))
                .map(OsString::as_os_str),
            Some(OsStr::new("/tmp/claude-config"))
        );
        for key in STRIPPED_ENV_VARS {
            assert!(
                !cleaned.contains_key(OsStr::new(key)),
                "expected {key} stripped"
            );
        }
    }

    #[test]
    fn parse_auth_status_logged_in_reads_logged_in() {
        assert!(
            parse_auth_status_logged_in(r#"{"loggedIn":true,"authMethod":"claude.ai"}"#)
                .expect("parse")
        );
        assert!(!parse_auth_status_logged_in(r#"{"loggedIn":false}"#).expect("parse"));
        assert!(parse_auth_status_logged_in("note\n{\"loggedIn\":true}\n").expect("noise"));
        assert!(parse_auth_status_logged_in("no json").is_err());
        assert!(parse_auth_status_logged_in(r#"{"authMethod":"claude.ai"}"#).is_err());
    }
}
