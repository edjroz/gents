# B3 live #11a: health on an expired seat — **PASS** (no spawn, no HTTP, routing veto after K=3)

**When:** 2026-09-04T07:50:38Z (server up) → 07:52:48Z (threshold crossed) → 07:54:01Z (server stopped); ~3.4 min
**Write request:** #11 (`write-request-11.md`), 11a half, approved by user /goal directive
**Branch:** `spike/claude-b3-live-tools` @ `9a92c586` (no source edits; `.scratch/` untracked)
**Binary:** `./target/debug/gents` inode `45228558` (rebuilt from `9a92c586`, 1m 25s)
**Server:** PID `55359` `--home ~/.gents --tool-root <repo> --tool-ceiling readwrite --claude-config-dir <repo>/.scratch/claude-spike/claude-config` (NO `--claude-write-approved`); stopped after the run (`kill` exit 0, pid gone in 2 s, `:9191` and `:8787` closed)
**Chat turns:** none. **Claude HTTP requests:** none. **Billing:** nothing.
**Seat:** spike seat in the macOS Keychain, OAuth access token expired at `2026-09-04T00:55:22.769+00:00` (per the prober's error text; the token itself was never read, printed, or touched by this run)

## Change under test

Commit `9a92c586` — Keychain seat auth and the health prober's seat-token read. Exercised: the prober read the expired seat on each cycle, failed closed, and after K=3 consecutive failures vetoed routing to the Claude backend; the reconcile then marked the behavior unavailable. No `claude` process, no HTTP.

## What happened

- 07:50:38 server up; 07:50:49 behavior `…:default` built (`model=claude-sonnet-5`).
- Probe cycles at ~t=0, ~60 s, ~120 s (`probe_interval` default 60 s): cycles 1–2 are silent at WARN (Degraded is not logged as a transition line); cycle 3 at 07:52:48.713 logged:
  `WARN gents::backend_health: backend probe: measured health crossed the routing threshold backend_id=…:claude-backend endpoint=claude-cli://subscription previous_state=degraded next_state=unhealthy failure_count=3 error="Claude seat OAuth access token expired at 2026-09-04T00:55:22.769+00:00; run gents claude-login --config-dir /Users/edjroz/Repos/source/gents/.scratch/claude-spike/claude-config"`
- 07:52:48.713 `control_watcher: backend measured-health transition detected; scheduling reconcile`
- 07:52:55.341 `runtime reconcile applied generation=2 … removed_behaviors=1 … unavailable_changed=true`, then
  `WARN … behavior unavailable after runtime reconcile … reason=… backend …:claude-backend is measured unhealthy by the local prober (document probe_status=healthy is operator intent; routing resumes on the next successful probe)`
- The replicated `InferenceBackend` document stayed `probe_status=healthy`, `last_probe=null` (prober only promotes, never demotes) — as predicted.
- Server stopped 07:54:01.

## Bar (11a)

| Check | Result |
|---|---|
| stderr shows the backend-probe warning crossing the routing threshold for the Claude backend | **PASS** — line 13, `previous_state=degraded next_state=unhealthy failure_count=3` |
| `error=` contains `expired at` | **PASS** — `…access token expired at 2026-09-04T00:55:22.769+00:00…` |
| `error=` contains `run gents claude-login --config-dir` | **PASS** — hint carries the absolute spike config dir |
| no `claude` process spawned | **PASS** — `pgrep -P <server pid>` = 0 children (before and after the threshold). Note: a raw `ps -eo command \| grep -c '[c]laude '` reports 9 on this machine, all unrelated (this Claude Code session, its daemon/bg-pty helpers, and shell snapshots with `claude` in the path — ppids 1/61032/83908/etc., none under 55359) |
| no Claude HTTP: `RenderedRequest` rows before == after | **PASS** — 1000 / 1000 (both at the query's page cap, so also checked: rows with `created_at >= 2026-09-04T07:50:38Z` = **0**, `b3-live-health-expired-rows-since-start.json`) |
| no `live Claude Messages HTTP send` log line | **PASS** — 0 |
| no HTTP 4xx / 429 / rate_limit in server stderr | **PASS** — `grep -ciE` = 0 |
| Token scan (`grep -l 'sk-ant\|Bearer '` over `b3-live-health-expired-*`) | **clean** — no files listed, exit 1 |
| Server stopped, `:9191`/`:8787` closed | **PASS** |
| `InferenceBackend` document not demoted | as predicted — `probe_status=healthy`, `last_probe=null` (`b3-live-health-expired-backend.json`) |

**Verdict: PASS (11a).** Health is a token read: an expired seat produces Unhealthy after K=3 with an actionable hint, no spawn, no HTTP, nothing billed.

## 11b — pending

Pending the user refreshing the spike seat (`gents claude-login --config-dir <repo>/.scratch/claude-spike/claude-config`; the agent never runs this). Then: set the Claude `InferenceBackend` document's `probe_status` to `unknown` (record the exact `gents backend …` command or `escape_graphql_string`-safe mutation), start the server as above, wait one probe cycle, and expect stderr `promoted shared document unknown -> healthy` with the document reading `healthy` without hand-editing.

## Anomalies

- Threshold crossed ~130 s after start (cycles at t≈0/60/120 s), earlier than the 200 s wait budget; the wait loop exited on the threshold line at 102 s of waiting.
- Cycle-1 Degraded and cycle-2 transitions are not visible at the default log level; only the routing-threshold crossing is logged at WARN. Not a defect for this bar, but worth noting for observability.
- stderr line 17 is an ERROR from my own mis-fielded query (`last_probe_at`; the field is `last_probe`) — self-inflicted, harmless, retried correctly.
- `gents query --limit` is capped at 1000 rows server-side (before-count and after-count both 1000), so the time-filtered check is the load-bearing one.

Raw: `b3-live-health-expired-backend.json`, `b3-live-health-expired-rows-{before,after}.txt`, `b3-live-health-expired-rows-since-start.json`
Logs: `b3-live-health-expired-server.{stdout,stderr}.log`, `b3-live-health-expired-server.pid`, `b3-live-health-expired-{start,end}.ts`
