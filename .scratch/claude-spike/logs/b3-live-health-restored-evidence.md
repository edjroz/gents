# B3 live #11b: promotion after seat restore — **PASS** on the document (unknown -> healthy in one cycle, no spawn, no HTTP); promotion log line not visible at the default log level

**When:** 2026-09-04T08:01:56Z (server up) → 08:02:12Z (mutation) → 08:03:03.96Z (promoted) → 08:04:42Z (server stopped); ~2.8 min
**Write request:** #11 (`write-request-11.md`), 11b half, approved by user /goal directive
**Branch:** `spike/claude-b3-live-tools` @ `9a92c586` (no source edits; `.scratch/` untracked)
**Binary:** `./target/debug/gents` inode `45229261` (not rebuilt)
**Server:** PID `57284` `--home ~/.gents --tool-root <repo> --tool-ceiling readwrite --claude-config-dir <repo>/.scratch/claude-spike/claude-config` (NO `--claude-write-approved`); stopped after the run (`kill` exit 0, pid gone in 2 s, `:9191` and `:8787` closed)
**Chat turns:** none. **Claude HTTP requests:** none. **Billing:** nothing.
**Seat:** spike seat in the macOS Keychain, refreshed by the user minutes before (#10b passed on it). The token was read only by the prober; never printed or touched by this run.

## Procedure deviations

- There is no `gents backend` subcommand in this build (`gents backend show` → "unrecognized subcommand"). The before/after documents were recorded with `gents query --collection InferenceBackend` against the running server instead (same fields the prober touches plus `enabled`/`endpoint`/`provider_kind`).
- The document write went through the preferred path (anonymous GraphQL mutation to the running server); the `backend set` fallback was not needed.

## The one document write (the mutation)

```
curl -s http://127.0.0.1:9191/api/v0/graphql -H 'content-type: application/json' -d '{"query":"mutation { update_InferenceBackend(filter: { backend_id: { _eq: \"did:key:z6MkpEq4KsC2oibBmHe6faw1246CU5gmtVJJEYf3wvo1yKiQ:claude-backend\" } }, input: { probe_status: \"unknown\" }) { _docID probe_status } }"}'
```
Response (`b3-live-health-restored-mutation.json`):
```
{"data":{"update_InferenceBackend":[{"_docID":"bae-a714b38e-9793-5a38-bdf6-68a95a079718","probe_status":"unknown"}]}}
```
The backend id literal contains only `did:key:`, base58 and `:claude-backend` characters — nothing needing `escape_graphql_string`. Verified immediately after (`b3-live-health-restored-backend-set-unknown.json`): `probe_status=unknown`, `last_probe=null`.

## Before / after

| | `probe_status` | `last_probe` | file |
|---|---|---|---|
| before (08:02:0xZ, server up) | `healthy` | `null` | `b3-live-health-restored-backend-before.json` |
| after mutation (08:02:12Z) | `unknown` | `null` | `b3-live-health-restored-backend-set-unknown.json` |
| after one probe cycle (08:03:2xZ) | `healthy` | `2026-09-04T08:03:03.963955Z` | `b3-live-health-restored-backend-after.json` |

Other fields unchanged throughout: `enabled=true`, `endpoint=claude-cli://subscription`, `provider_kind=ClaudeCliSubscription`.

## What happened (stderr, `b3-live-health-restored-server.stderr.log`)

- 08:01:56 server started; 08:02:05 `gents ready runnable_behaviors=1 unavailable_behaviors=0` (probe cycle 1 at ~08:02:04 saw the document still `healthy`).
- 08:02:12.580 `control_watcher: runtime control update detected … doc_id=bae-a714b38e-…` — my mutation.
- 08:02:19.270 `runtime reconcile applied generation=2 … removed_behaviors=1 … unavailable_changed=true` + `WARN … behavior unavailable after runtime reconcile … reason=… backend …:claude-backend is unavailable (enabled=true probe_status=unknown)` — the runtime treats `unknown` as not-yet-routable (operator intent), as designed.
- 08:03:03.963 (per `last_probe`) probe cycle 2 passed and the prober wrote `probe_status=healthy` + `last_probe`; 08:03:04.033 `control_watcher: runtime control update detected … doc_id=bae-a714b38e-…` — the prober's write (the only two control updates in the whole run are mine and the prober's).
- 08:03:10.692 `runtime reconcile applied generation=3 added_behaviors=1 … unavailable_changed=true`; behavior `…:default` rebuilt (`model=claude-sonnet-5`) and back online.
- 08:04:42 server stopped.

## Bar (11b)

| Check | Result |
|---|---|
| with a fresh seat and `probe_status=unknown`, one probe cycle promotes the document to `healthy` without hand-editing | **PASS** — `unknown` at 08:02:12Z → `healthy` with `last_probe=2026-09-04T08:03:03.963955Z` on the next cycle (~51 s later); the only writer besides my mutation was the prober (control-watcher shows exactly two updates on the doc id) |
| stderr shows `promoted shared document unknown -> healthy` | **NOT VISIBLE** — 0 `backend probe` lines in stderr. The line is emitted at INFO from `gents::backend_health` (`backend_health.rs:373`), but the server's `DEFAULT_LOG_FILTER` (`crates/gents-cli/src/lib.rs:57`) is `warn` for every target not listed, and `gents::backend_health` is not listed; no `RUST_LOG` was set. The promotion itself is proven by the document and the control-watcher update; the log line is an observability gap (same family as 11a's silent Degraded transition), not a prober defect |
| no threshold warning for the Claude backend | **PASS** — the only WARN in the run is the expected `behavior unavailable … probe_status=unknown` at generation 2 |
| no `claude` process spawned | **PASS** — `pgrep -P 57284 \| grep -c claude` = 0 (checked after the cycle and again just before stop) |
| no Claude HTTP: no `live Claude Messages HTTP send`, no HTTP 4xx / 429 / rate_limit | **PASS** — `grep -ciE` = 0; also `RenderedRequest` rows with `created_at >= 08:01:56Z` = **0** (`b3-live-health-restored-rows-since-start.json`) |
| Token scan (API-key-prefix / Bearer-prefix patterns over `b3-live-health-restored-*`) | **clean** — no files listed, exit 1 |
| Server stopped, `:9191`/`:8787` closed | **PASS** |

**Verdict: PASS (11b) on the substance** — the prober promotes an `unknown` document to `healthy` on the first passing cycle with a fresh `last_probe`, without spawning `claude`, without any Claude HTTP, and without hand-editing. The literal "stderr shows the promotion line" sub-check could not be met at the default log level; re-running with `RUST_LOG='gents::backend_health=info'` (or adding that target to `DEFAULT_LOG_FILTER`) would make it observable. User's call whether that counts against the bar.

## Anomalies

1. **Promotion log line filtered out** (above). `DEFAULT_LOG_FILTER` lists `gents::agent::{runtime,daemon,reconcile}`, `gents::hook`, `gents::session::sessions`, `gents::streaming`, `gents::trigger_engine` at info; `gents::backend_health` falls to `warn`. Worth adding to the filter so operators see promotions.
2. **Reconcile churn after the backend came back.** From 08:03:10 until the server was stopped at 08:04:42, `runtime reconcile applied` fired every ~2 s (generations 3 → 32, 31 applies in ~92 s), each `added_behaviors=0 removed_behaviors=0 updated_behaviors=1 default_changed=false unavailable_changed=false`, each rebuilding the `…:default` behavior runtime (`building behavior runtime` / `behavior started` / `executor online` x31). Only 2 `control_watcher` updates exist in the run, and `control_watcher` dedupes proposals on `configuration_fingerprint()`, so some other proposal path is re-proposing a snapshot whose fingerprint changes every time. It was still churning at stop. It caused **no document writes** (`last_probe` unchanged after 08:03:03.96, `RenderedRequest` since start = 0), no `claude` children, no HTTP — so it does not affect the 11b bar — but 11a (backend never came back) showed exactly 1 reconcile and #10b's server log 0, so this looks like a defect specific to the unavailable → available transition (or to a fresh `last_probe` entering the fingerprint). Needs filing; not investigated further in this run (no source edits).

Raw: `b3-live-health-restored-backend-{before,set-unknown,after}.json`, `b3-live-health-restored-mutation.json`, `b3-live-health-restored-rows-since-start.json`
Logs: `b3-live-health-restored-server.{stdout,stderr}.log`, `b3-live-health-restored-server.pid`, `b3-live-health-restored-{start,mutation,end}.ts`
