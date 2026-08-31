//! Experimental Path A loopback OpenAI Chat Completions adapter.
//!
//! `gents claude-proxy` makes Claude CLI look like `/v1/chat/completions` so
//! stock `OpenAiCompatible` can call a Max seat without storing oat in DefraDB.
//!
//! Default mode is canned (no Claude). Live Claude requires both
//! `PROXY_USE_CLAUDE=1` and `CLAUDE_WRITE_APPROVED=1`.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use axum::body::Body;
use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use gents::claude_completer::proxy::{
    flatten_messages, health_body, is_loopback_host, live_claude_allowed, models_list_body,
    non_stream_body, resolve_mode, resolve_request_model, sse_payload, stripped_env_vars,
    DEFAULT_MODEL_ID, PATH_A_MODEL_IDS,
};
use gents::claude_completer::{completer_argv, sanitize_child_env};
use serde_json::{json, Value};
use tokio::process::Command;
use tracing::{info, warn};

use crate::cli::args::ClaudeProxyArgs;

#[derive(Clone)]
struct ProxyState {
    model: String,
    canned_text: String,
    use_claude: bool,
    write_approved: bool,
    fake_completer: Option<PathBuf>,
    claude_bin: PathBuf,
    config_dir: PathBuf,
    workdir: PathBuf,
    log_dir: PathBuf,
}

pub(crate) async fn claude_proxy(args: ClaudeProxyArgs) -> Result<()> {
    if !is_loopback_host(&args.host) {
        bail!(
            "refusing non-loopback host `{}` (allowed: 127.0.0.1, localhost, ::1)",
            args.host
        );
    }
    if !args.config_dir.as_os_str().is_empty() {
        // required by clap; keep explicitness in help text
    }

    let config_dir = args.config_dir.canonicalize().unwrap_or(args.config_dir);
    let parent = config_dir
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let workdir = args
        .workdir
        .unwrap_or_else(|| parent.join("workdir"));
    let log_dir = args.log_dir.unwrap_or_else(|| parent.join("logs"));
    std::fs::create_dir_all(&workdir)
        .with_context(|| format!("create workdir {}", workdir.display()))?;
    std::fs::create_dir_all(&log_dir)
        .with_context(|| format!("create log_dir {}", log_dir.display()))?;

    let use_claude = std::env::var("PROXY_USE_CLAUDE").ok().as_deref() == Some("1");
    let write_approved = std::env::var("CLAUDE_WRITE_APPROVED").ok().as_deref() == Some("1");
    let fake = args.fake_completer.is_some();
    let mode = resolve_mode(use_claude, fake);

    let model = if args.model.trim().is_empty() {
        DEFAULT_MODEL_ID.to_string()
    } else {
        args.model
    };
    let state = Arc::new(ProxyState {
        model: model.clone(),
        canned_text: args.canned_text,
        use_claude,
        write_approved,
        fake_completer: args.fake_completer,
        claude_bin: args
            .claude_bin
            .unwrap_or_else(|| PathBuf::from("claude")),
        config_dir: config_dir.clone(),
        workdir,
        log_dir: log_dir.clone(),
    });

    let app = Router::new()
        .route("/", get(health))
        .route("/healthz", get(health))
        .route("/v1/models", get(models))
        .route("/models", get(models))
        .route("/v1/chat/completions", post(chat_completions))
        .route("/chat/completions", post(chat_completions))
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", args.host, args.port)
        .parse()
        .with_context(|| format!("parse bind address {}:{}", args.host, args.port))?;
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("bind {addr}"))?;
    info!(
        %addr,
        mode = mode.as_str(),
        model = %model,
        write_approved,
        config_dir = %config_dir.display(),
        log_dir = %log_dir.display(),
        "claude-proxy listening"
    );
    eprintln!(
        "claude-proxy listening on http://{} mode={} model={} write_approved={} config_dir={}",
        addr,
        mode.as_str(),
        model,
        write_approved,
        config_dir.display()
    );

    axum::serve(listener, app)
        .await
        .context("claude-proxy server error")?;
    Ok(())
}

async fn health(State(state): State<Arc<ProxyState>>) -> Json<Value> {
    let mode = resolve_mode(state.use_claude, state.fake_completer.is_some());
    Json(health_body(
        mode,
        &state.model,
        state.write_approved,
        state.fake_completer.is_some(),
    ))
}

async fn models(State(_state): State<Arc<ProxyState>>) -> Json<Value> {
    Json(models_list_body(PATH_A_MODEL_IDS))
}

async fn chat_completions(
    State(state): State<Arc<ProxyState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    let messages = body
        .get("messages")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let stream = body
        .get("stream")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let had_tools = body.get("tools").is_some() || body.get("tool_choice").is_some();
    let authorization_present = headers.get(header::AUTHORIZATION).is_some();
    let prompt = flatten_messages(&messages);
    let mode = resolve_mode(state.use_claude, state.fake_completer.is_some());
    let model = resolve_request_model(
        body.get("model").and_then(Value::as_str),
        &state.model,
    );

    let entry = json!({
        "ts": chrono_ts(),
        "model": model,
        "message_count": messages.len(),
        "stream": stream,
        "had_tools_fields": had_tools,
        "mode": mode.as_str(),
        "authorization_present": authorization_present,
        "prompt_chars": prompt.len(),
    });
    if let Err(err) = append_request_log(&state.log_dir, &entry) {
        warn!(error = %err, "failed to append proxy-requests.jsonl");
    }

    // tools / tool_choice are never forwarded to the completer.
    let text_result = if !state.use_claude {
        Ok(state.canned_text.clone())
    } else {
        complete_live(&state, &prompt, &model).await
    };

    match text_result {
        Ok(text) => completion_response(&model, &text, stream),
        Err(err) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": {"message": err.to_string()}})),
        )
            .into_response(),
    }
}

async fn complete_live(state: &ProxyState, prompt: &str, model: &str) -> Result<String> {
    if let Some(fake) = &state.fake_completer {
        let output = Command::new(fake)
            .arg(if prompt.is_empty() {
                "Reply with exactly: pong"
            } else {
                prompt
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .with_context(|| format!("spawn fake completer {}", fake.display()))?;
        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            bail!(
                "fake completer exit {}: {}",
                output.status.code().unwrap_or(-1),
                err.trim()
            );
        }
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if text.is_empty() {
            bail!("fake completer returned empty assistant text");
        }
        return Ok(text);
    }

    if !live_claude_allowed(state.write_approved) {
        bail!("live path refused: CLAUDE_WRITE_APPROVED=1 not set on proxy");
    }

    let prompt = if prompt.is_empty() {
        "Reply with exactly: pong"
    } else {
        prompt
    };
    let mut argv = completer_argv(prompt, Some(model));
    // Replace leading `claude` with configured binary path.
    if let Some(first) = argv.first_mut() {
        *first = state.claude_bin.as_os_str().to_os_string();
    }

    let mut cmd = Command::new(&argv[0]);
    cmd.args(&argv[1..])
        .current_dir(&state.workdir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_clear()
        .envs(sanitize_child_env(std::env::vars_os()))
        .env("CLAUDE_CONFIG_DIR", &state.config_dir)
        .env("CLAUDE_WRITE_APPROVED", "1");
    for key in stripped_env_vars() {
        cmd.env_remove(key);
    }

    let output = cmd
        .output()
        .await
        .with_context(|| format!("spawn {}", state.claude_bin.display()))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stderr.trim().is_empty() {
        // Completer context goes to stderr by design; keep it out of the HTTP body.
        eprintln!("[claude-proxy completer stderr]\n{}", stderr.trim());
    }
    if !output.status.success() {
        bail!(
            "completer exit {}: {}",
            output.status.code().unwrap_or(-1),
            stderr.trim().chars().take(500).collect::<String>()
        );
    }

    // Prefer stream-json parse when stdout looks like JSONL; else treat as plain text
    // (fake completers / future wrappers).
    let text = if stdout.lines().any(|l| l.trim_start().starts_with('{')) {
        gents::claude_completer::parse_stream_jsonl(&stdout)
            .map_err(|err| anyhow::anyhow!(err))?
    } else {
        stdout.trim().to_string()
    };
    if text.trim().is_empty() {
        bail!("completer returned empty assistant text");
    }
    Ok(text)
}

fn completion_response(model: &str, text: &str, stream: bool) -> Response {
    if stream {
        let payload = sse_payload(text, model);
        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/event-stream")
            .header(header::CACHE_CONTROL, "no-cache")
            .header(header::CONNECTION, "close")
            .body(Body::from(payload))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
    } else {
        Json(non_stream_body(text, model)).into_response()
    }
}

fn append_request_log(log_dir: &Path, entry: &Value) -> Result<()> {
    use std::io::Write;
    let path = log_dir.join("proxy-requests.jsonl");
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("open {}", path.display()))?;
    writeln!(file, "{entry}").context("write proxy request log")?;
    Ok(())
}

fn chrono_ts() -> String {
    // RFC3339-ish UTC without pulling chrono into this command module.
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secs}Z")
}

#[cfg(test)]
mod tests {
    use super::*;
    use gents::claude_completer::proxy::{DEFAULT_MODEL_ID, PATH_A_MODEL_IDS};
    use tokio::sync::oneshot;

    struct TempDirs {
        log_dir: tempfile::TempDir,
        workdir: tempfile::TempDir,
        config_dir: tempfile::TempDir,
    }

    fn make_state(
        temps: &TempDirs,
        use_claude: bool,
        write_approved: bool,
        fake_completer: Option<PathBuf>,
        canned: &str,
    ) -> Arc<ProxyState> {
        Arc::new(ProxyState {
            model: DEFAULT_MODEL_ID.to_string(),
            canned_text: canned.to_string(),
            use_claude,
            write_approved,
            fake_completer,
            claude_bin: PathBuf::from("claude"),
            config_dir: temps.config_dir.path().to_path_buf(),
            workdir: temps.workdir.path().to_path_buf(),
            log_dir: temps.log_dir.path().to_path_buf(),
        })
    }

    async fn serve(
        state: Arc<ProxyState>,
    ) -> (
        u16,
        oneshot::Sender<()>,
        tokio::task::JoinHandle<std::io::Result<()>>,
    ) {
        let app = Router::new()
            .route("/healthz", get(health))
            .route("/v1/models", get(models))
            .route("/v1/chat/completions", post(chat_completions))
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        let handle = tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let _ = shutdown_rx.await;
                })
                .await
        });
        // Tiny yield so accept loop is ready.
        tokio::task::yield_now().await;
        (port, shutdown_tx, handle)
    }

    #[tokio::test]
    async fn canned_models_and_sse_chat_completions() {
        let temps = TempDirs {
            log_dir: tempfile::tempdir().unwrap(),
            workdir: tempfile::tempdir().unwrap(),
            config_dir: tempfile::tempdir().unwrap(),
        };
        let state = make_state(&temps, false, false, None, "pong");
        let (port, shutdown_tx, handle) = serve(state).await;
        let client = reqwest::Client::new();

        let models: Value = client
            .get(format!("http://127.0.0.1:{port}/v1/models"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(models["data"].as_array().unwrap().len(), PATH_A_MODEL_IDS.len());
        assert_eq!(models["data"][0]["id"], PATH_A_MODEL_IDS[0]);
        assert_eq!(models["data"][1]["id"], "claude-sonnet-5");
        assert_eq!(models["data"][2]["id"], "claude-haiku-4-5-20251001");
        assert_eq!(models["data"][3]["id"], "claude-fable-5");
        assert_eq!(DEFAULT_MODEL_ID, "claude-sonnet-5");

        let health: Value = client
            .get(format!("http://127.0.0.1:{port}/healthz"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(health["mode"], "canned");

        let resp = client
            .post(format!("http://127.0.0.1:{port}/v1/chat/completions"))
            .json(&json!({
                "model": DEFAULT_MODEL_ID,
                "stream": true,
                "messages": [{"role":"user","content":"hi"}],
                "tools": [{"type":"function","function":{"name":"x"}}],
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = resp.text().await.unwrap();
        assert!(body.contains("pong"));
        assert!(body.contains("data: [DONE]"));

        let log = std::fs::read_to_string(temps.log_dir.path().join("proxy-requests.jsonl")).unwrap();
        assert!(log.contains("\"had_tools_fields\":true"));
        assert!(log.contains("\"mode\":\"canned\""));

        let _ = shutdown_tx.send(());
        let _ = handle.await;
    }

    #[tokio::test]
    async fn live_without_approval_returns_502() {
        let temps = TempDirs {
            log_dir: tempfile::tempdir().unwrap(),
            workdir: tempfile::tempdir().unwrap(),
            config_dir: tempfile::tempdir().unwrap(),
        };
        let state = make_state(&temps, true, false, None, "pong");
        let (port, shutdown_tx, handle) = serve(state).await;
        let client = reqwest::Client::new();
        let resp = client
            .post(format!("http://127.0.0.1:{port}/v1/chat/completions"))
            .json(&json!({
                "model": DEFAULT_MODEL_ID,
                "messages": [{"role":"user","content":"hi"}],
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_GATEWAY);
        let body: Value = resp.json().await.unwrap();
        assert!(
            body["error"]["message"]
                .as_str()
                .unwrap_or("")
                .contains("CLAUDE_WRITE_APPROVED"),
            "{body}"
        );
        let _ = shutdown_tx.send(());
        let _ = handle.await;
    }

    #[tokio::test]
    async fn fake_completer_live_path_returns_text() {
        let temps = TempDirs {
            log_dir: tempfile::tempdir().unwrap(),
            workdir: tempfile::tempdir().unwrap(),
            config_dir: tempfile::tempdir().unwrap(),
        };
        let fake = tempfile::NamedTempFile::new().unwrap();
        #[cfg(unix)]
        {
            use std::io::Write;
            use std::os::unix::fs::PermissionsExt;
            let mut f = fake.reopen().unwrap();
            writeln!(f, "#!/bin/sh\necho fake-pong").unwrap();
            std::fs::set_permissions(fake.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let state = make_state(
            &temps,
            true,
            true,
            Some(fake.path().to_path_buf()),
            "canned",
        );
        let (port, shutdown_tx, handle) = serve(state).await;
        let client = reqwest::Client::new();

        let health: Value = client
            .get(format!("http://127.0.0.1:{port}/healthz"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(health["mode"], "claude-fake");

        let resp = client
            .post(format!("http://127.0.0.1:{port}/v1/chat/completions"))
            .json(&json!({
                "model": DEFAULT_MODEL_ID,
                "stream": false,
                "messages": [{"role":"user","content":"hi"}],
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body: Value = resp.json().await.unwrap();
        assert_eq!(body["choices"][0]["message"]["content"], "fake-pong");

        let _ = shutdown_tx.send(());
        let _ = handle.await;
        drop(fake);
    }

    #[test]
    fn refuses_non_loopback_in_helper() {
        assert!(!is_loopback_host("0.0.0.0"));
    }
}
