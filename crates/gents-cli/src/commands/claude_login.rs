//! Thin wrapper around official Claude CLI login for Path A.
//!
//! Seat credentials stay in `--config-dir` / `CLAUDE_CONFIG_DIR`. This command
//! never upserts `OAuthCredential` into DefraDB.
//!
//! Live Anthropic login is write-gated: require `CLAUDE_WRITE_APPROVED=1`
//! unless `--dry-run` is set.

use std::path::PathBuf;
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

use crate::cli::args::{ClaudeAuthProbeArgs, ClaudeLoginArgs};
use crate::request_helpers::print_json;
use gents::claude_completer::sanitize_child_env;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ClaudeLoginPlan {
    pub config_dir: PathBuf,
    pub claude_bin: PathBuf,
    pub argv: Vec<String>,
    pub dry_run: bool,
    pub write_approved: bool,
}

pub(crate) async fn claude_login(args: ClaudeLoginArgs) -> Result<()> {
    let plan = plan_claude_login(&args)?;
    if plan.dry_run {
        print_json(&claude_login_plan_json(&plan))?;
        eprintln!(
            "dry-run: would exec {:?} with CLAUDE_CONFIG_DIR={} (no DefraDB oat write)",
            plan.argv,
            plan.config_dir.display()
        );
        return Ok(());
    }
    if !plan.write_approved {
        bail!(
            "refusing live Claude login: set CLAUDE_WRITE_APPROVED=1 after an explicit numbered write approval, or pass --dry-run"
        );
    }
    run_claude_login(&plan)?;
    // Re-probe so the operator sees the resulting seat without a second command.
    let probe = crate::commands::claude_auth_probe::run_claude_auth_probe(&ClaudeAuthProbeArgs {
        config_dir: plan.config_dir.clone(),
        claude_bin: Some(plan.claude_bin.clone()),
    })?;
    print_json(&json!({
        "login": "completed",
        "oauth_credential_written": false,
        "credential_store": "claude_config_dir",
        "probe": crate::commands::claude_auth_probe::claude_auth_probe_result_json(&probe),
    }))?;
    Ok(())
}

pub(crate) fn plan_claude_login(args: &ClaudeLoginArgs) -> Result<ClaudeLoginPlan> {
    if args.config_dir.as_os_str().is_empty() {
        bail!("--config-dir is required (no silent ~/.claude default)");
    }
    if args.console && args.sso {
        bail!("--console and --sso are mutually exclusive for Path A login wrapping");
    }
    let config_dir = args.config_dir.clone();
    let claude_bin = args
        .claude_bin
        .clone()
        .unwrap_or_else(|| PathBuf::from("claude"));

    let mut argv = vec![
        claude_bin.display().to_string(),
        "auth".to_string(),
        "login".to_string(),
    ];
    if args.console {
        argv.push("--console".to_string());
    } else {
        // Default Path A: Claude subscription seat (not Console API billing).
        argv.push("--claudeai".to_string());
    }
    if args.sso {
        argv.push("--sso".to_string());
    }
    if let Some(email) = &args.email {
        if email.trim().is_empty() {
            bail!("--email must not be empty when provided");
        }
        argv.push("--email".to_string());
        argv.push(email.clone());
    }

    Ok(ClaudeLoginPlan {
        config_dir,
        claude_bin,
        argv,
        dry_run: args.dry_run,
        write_approved: std::env::var("CLAUDE_WRITE_APPROVED").ok().as_deref() == Some("1"),
    })
}

fn run_claude_login(plan: &ClaudeLoginPlan) -> Result<()> {
    std::fs::create_dir_all(&plan.config_dir)
        .with_context(|| format!("create config-dir {}", plan.config_dir.display()))?;

    let mut cmd = Command::new(&plan.claude_bin);
    // Skip argv[0] (binary path) — Command::new already sets it.
    cmd.args(plan.argv.iter().skip(1))
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .env_clear()
        .envs(sanitize_child_env(std::env::vars_os()))
        .env("CLAUDE_CONFIG_DIR", &plan.config_dir)
        .env("CLAUDE_WRITE_APPROVED", "1");
    for key in gents::claude_completer::STRIPPED_ENV_VARS {
        cmd.env_remove(key);
    }

    let status = cmd
        .status()
        .with_context(|| format!("spawn {} auth login", plan.claude_bin.display()))?;
    if !status.success() {
        bail!(
            "claude auth login exit {}",
            status.code().unwrap_or(-1)
        );
    }
    Ok(())
}

pub(crate) fn claude_login_plan_json(plan: &ClaudeLoginPlan) -> Value {
    json!({
        "dry_run": plan.dry_run,
        "write_approved": plan.write_approved,
        "config_dir": plan.config_dir,
        "argv": plan.argv,
        "oauth_credential_written": false,
        "credential_store": "claude_config_dir",
        "note": "Path A login wraps official Claude CLI; seat stays in --config-dir, not DefraDB",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_args() -> ClaudeLoginArgs {
        ClaudeLoginArgs {
            config_dir: PathBuf::from("/tmp/claude-cfg"),
            claude_bin: None,
            dry_run: true,
            console: false,
            email: None,
            sso: false,
        }
    }

    #[test]
    fn dry_run_plan_defaults_to_claudeai() {
        let plan = plan_claude_login(&base_args()).expect("plan");
        assert!(plan.dry_run);
        assert_eq!(
            plan.argv,
            vec![
                "claude".to_string(),
                "auth".to_string(),
                "login".to_string(),
                "--claudeai".to_string()
            ]
        );
        let json = claude_login_plan_json(&plan);
        assert_eq!(json["oauth_credential_written"], false);
        assert_eq!(json["credential_store"], "claude_config_dir");
    }

    #[test]
    fn console_flag_replaces_claudeai() {
        let mut args = base_args();
        args.console = true;
        let plan = plan_claude_login(&args).expect("plan");
        assert!(plan.argv.iter().any(|a| a == "--console"));
        assert!(!plan.argv.iter().any(|a| a == "--claudeai"));
    }

    #[test]
    fn email_appended() {
        let mut args = base_args();
        args.email = Some("user@example.com".into());
        let plan = plan_claude_login(&args).expect("plan");
        assert!(plan.argv.windows(2).any(|w| w == ["--email", "user@example.com"]));
    }

    #[test]
    fn refuses_empty_config_dir() {
        let mut args = base_args();
        args.config_dir = PathBuf::from("");
        assert!(plan_claude_login(&args).is_err());
    }
}
