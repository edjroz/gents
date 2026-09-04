# Task 7 Step 8 — live run #11b (promotion after seat restore) — report

Status: complete. **PASS (11b) on the document behavior**; the literal "stderr shows the promotion line" sub-check is NOT VISIBLE at the default log level (see anomaly 1).
Branch `spike/claude-b3-live-tools` @ `9a92c586`; binary inode `45229261` (not rebuilt); server PID 57284, 08:01:56Z → 08:04:42Z, no `--claude-write-approved`, no chat turn, no Claude HTTP, nothing billed.

- Mutation path: anonymous GraphQL `update_InferenceBackend(filter: {backend_id: {_eq: "…:claude-backend"}}, input: {probe_status: "unknown"})` via curl to `:9191` — accepted (`_docID bae-a714b38e-9793-5a38-bdf6-68a95a079718`). `backend set` fallback not needed; note there is no `gents backend` subcommand in this build, so before/after were captured with `gents query`.
- Before: `probe_status=healthy`, `last_probe=null`. After mutation: `unknown`, `null`. After one probe cycle: `probe_status=healthy`, `last_probe=2026-09-04T08:03:03.963955Z` (~51 s after the write; the prober's write is the only other control update on the doc).
- Promotion log line: **absent** — `gents::backend_health` logs it at INFO but `DEFAULT_LOG_FILTER` (`crates/gents-cli/src/lib.rs:57`) leaves that target at `warn`. Promotion is proven by the document + `control_watcher` update at 08:03:04.033Z.
- `claude` children of the server: 0. `live Claude Messages HTTP send` / 4xx / 429 / rate_limit: 0. `RenderedRequest` rows since start: 0. Token scan: clean. Server stopped; `:9191`/`:8787` closed.
- Anomaly 2 (probable defect, not in the bar): after the backend came back, `runtime reconcile applied … updated_behaviors=1` fired every ~2 s (generations 3→32 in ~92 s, behavior runtime rebuilt each time) until stop; only 2 control-watcher updates in the run, so some proposal path re-proposes a snapshot with a changing fingerprint. No document writes, no children, no HTTP resulted. 11a showed 1 reconcile total. Needs filing.

Files: `.scratch/claude-spike/logs/b3-live-health-restored-evidence.md` (+ `b3-live-health-restored-*` raw/logs), `write-request-11.md` (appended). No source edits, no commits.
