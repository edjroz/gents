//! Login-time child-process hygiene for `gents claude-login`
//! (`sanitize_child_env`) and the model-id default. No completer lives here
//! any more: every Claude turn goes over Messages HTTP (`claude_messages`).

use std::collections::HashMap;
use std::ffi::OsString;

/// Default client-facing model slug for ClaudeCliSubscription.
///
/// Full Claude model IDs only — not the old invented `claude-plan` seat label.
pub const DEFAULT_MODEL_ID: &str = "claude-sonnet-5";

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
}
