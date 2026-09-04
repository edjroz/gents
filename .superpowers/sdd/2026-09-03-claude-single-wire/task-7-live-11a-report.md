# Task 7 Step 8 — live run #11a (expired seat health) — report

Status: complete. **PASS (11a)**; 11b pending the user's seat refresh.
Branch `spike/claude-b3-live-tools` @ `9a92c586`; binary inode `45228558`; server PID 55359, 07:50:38Z → 07:54:01Z, no `--claude-write-approved`, no chat turn, no Claude HTTP, nothing billed.

- Probe warning (stderr line 13, 07:52:48Z): `backend probe: measured health crossed the routing threshold backend_id=…:claude-backend endpoint=claude-cli://subscription previous_state=degraded next_state=unhealthy failure_count=3 error="Claude seat OAuth access token expired at 2026-09-04T00:55:22.769+00:00; run gents claude-login --config-dir <repo>/.scratch/claude-spike/claude-config"`
- Follow-on: control_watcher scheduled a reconcile; generation 2 removed the behavior (`unavailable_changed=true`, reason: backend measured unhealthy by the local prober; document `probe_status=healthy` is operator intent).
- `claude` processes spawned by the server: 0 (`pgrep -P 55359`); machine-wide `grep -c '[c]laude '` = 9, all unrelated Claude Code processes.
- `RenderedRequest` rows before/after: 1000/1000 (page cap); rows with `created_at >= start` = 0.
- 4xx/429/rate_limit/`live Claude Messages HTTP send` in stderr: 0. Token scan: clean. Server stopped; :9191/:8787 closed.
- `InferenceBackend` doc unchanged: `probe_status=healthy`, `last_probe=null` (prober never demotes).
- Observability note: only the threshold crossing is logged at WARN; cycle-1 Degraded is silent at the default level.

Files: `.scratch/claude-spike/logs/write-request-11.md`, `.scratch/claude-spike/logs/b3-live-health-expired-evidence.md` (+ `b3-live-health-expired-*` raw/logs). No source edits, no commits.
