# CLAUDE WRITE REQUEST #11 — health on an expired seat, then promotion after restore

Date: 2026-09-04
Branch: `spike/claude-b3-live-tools` @ 9a92c586
Agent: did:key:z6MkpEq4KsC2oibBmHe6faw1246CU5gmtVJJEYf3wvo1yKiQ:default (model `claude-sonnet-5`, unchanged)
Approval: #11 approved — user /goal directive in session 0195Uzn9EziXQPWV4e8WdKHy (2026-09-03): "proceed with them as we've done … If we hit 429s you can stop"

## Hypothesis
Health is a token read, not a request. With the spike seat's OAuth token expired, the health
prober marks the Claude backend Degraded on the first failing cycle and Unhealthy (routing veto)
after K=3 consecutive failures, with an error carrying `expired at` and the
`run gents claude-login --config-dir` hint — without spawning a `claude` process and without any
Claude HTTP request. The replicated `InferenceBackend` document is only ever promoted by the
prober (never demoted), so its `probe_status` stays as-is. After the seat is restored, a document
whose `probe_status` is `unknown` is promoted to `healthy` on the first passing cycle.

## Change under test
Commit `9a92c586` — Keychain seat auth (`gents claude-login` stores the seat in the macOS
Keychain; the server reads it via `--claude-config-dir`) and the health prober's seat-token read.

## Scope
### 11a (this run, expired seat)
1. Server WITHOUT `--claude-write-approved`:
   `--home ~/.gents --tool-root <repo> --tool-ceiling readwrite --claude-config-dir <repo>/.scratch/claude-spike/claude-config`
2. No chat turn. Wait >= 3 probe cycles (`probe_interval` default 60 s; foreground `sleep 200`).
3. Read server stderr `backend probe` lines for the Claude backend; `ps` for `claude` processes;
   `gents query --collection InferenceBackend` for the Claude row.
4. Stop server.

### 11b (after the user refreshes the seat — NOT this run)
1. User runs `gents claude-login --config-dir <repo>/.scratch/claude-spike/claude-config` (agent never does).
2. Set the Claude `InferenceBackend` document's `probe_status` to `unknown` via `gents backend …`
   or a GraphQL mutation with `escape_graphql_string`-safe literals (record the exact command).
3. Start the server as in 11a; wait one probe cycle; stop.

## Success bar
### 11a
- server stderr shows the backend-probe warning crossing the routing threshold (K=3) for the
  Claude backend, with `error=` containing `expired at` and `run gents claude-login --config-dir`
- no `claude` process spawned (`ps -eo command | grep -c '[c]laude '` = 0)
- no Claude HTTP: `RenderedRequest` row count before == after; no `live Claude Messages HTTP send` log line
- no HTTP 4xx / 429 / rate_limit in server stderr
- token scan clean (`grep -l 'sk-ant\|Bearer '` over `b3-live-health-expired-*`)
- server stopped, `:9191`/`:8787` closed after
### 11b
- with a fresh seat and the document's `probe_status` set to `unknown`, one probe cycle promotes it
  to `healthy` without hand-editing; stderr shows `promoted shared document unknown -> healthy`

## Stop rule
Any 4xx/429: stop, write evidence, no retry. FAIL stops the plan for a user decision.

## Preflight
- no `gents server` listening on :9191; no listener on :8787
- `./target/debug/gents` rebuilt from `9a92c586` (inode recorded)

### Preflight results (2026-09-04, 11a)
- Build: `cargo build --bin gents` → `Finished `dev` profile [unoptimized + debuginfo] target(s) in 1m 25s` (1 pre-existing warning in `gents-cli`)
- Binary inode: `45228558 ./target/debug/gents` (built from `9a92c586`)
- `lsof -nP -iTCP:9191 -sTCP:LISTEN` → no output (exit 1)
- `lsof -nP -iTCP:8787 -sTCP:LISTEN` → no output (exit 1)

11b executed 2026-09-04 — see b3-live-health-restored-evidence.md
