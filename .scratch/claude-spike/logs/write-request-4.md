CLAUDE WRITE REQUEST #4 — APPROVED (Task 20 packaging reproduction smoke)

approved_by: human reply `yes`
approved_at: 2026-08-30 (session continuation)

purpose:
Prove Path A packaging end-to-end with Rust CLI only:
`gents claude-auth-probe` → `gents claude-proxy` (live) → isolated `gents chat` text turn.
Confirm assistant `pong`, tools=[], OAuthCredential=0, no oat in DefraDB.
Prod `~/.gents` and `:9191` stay untouched.

why_now:
Tasks 15–19 complete. This is the gated packaging smoke before marking Phase 6 Path A done.

scope_of_approval:
One packaging smoke turn (and any automatic sibling completions under the same owned-loop turn, e.g. title-gen), via:
1. Stop legacy Python proxy on `127.0.0.1:8787` if still listening
2. Start Rust live proxy:
   ```bash
   PROXY_USE_CLAUDE=1 CLAUDE_WRITE_APPROVED=1 \
     ./target/debug/gents claude-proxy \
       --config-dir .scratch/claude-spike/claude-config \
       --host 127.0.0.1 --port 8787 \
       --log-dir .scratch/claude-spike/logs \
       --workdir .scratch/claude-spike/workdir
   ```
3. Ensure spike server still on `127.0.0.1:9192` with home `.scratch/claude-spike/gents-home`
4. One chat:
   ```bash
   ./target/debug/gents chat \
     --home .scratch/claude-spike/gents-home \
     --graphql http://127.0.0.1:9192/api/v0/graphql \
     --timeout-secs 180 \
     "Reply with exactly: pong"
   ```

not_in_scope:
- Fresh `claude-login` / Anthropic OAuth (seat already Max / logged in)
- Prod home / prod ports
- Tool bridging / A2 native provider
- Billing UI scrape

preflight_ungated:
- Herdr probe: logged_in=true, auth_method=claude.ai, subscription_type=max, oauth_credential_written=false
- Existing spike home DID `did:key:z6MkgCE1AUd8uxQ6oEm3Phh54tftWiG7DfupDAUZpGvrwgu6`
- Legacy Python proxy currently on :8787 (pid historically 9495) — will be replaced by Rust proxy after approval
- Spike gents server historically pid 12718 on :9192; prod :9191 remains separate

abort_if:
- tool_use appears
- api-key / Console auth path
- any OAuthCredential upsert for Claude/Anthropic
- traffic leaves loopback
- prod ~/.gents or :9191 touched

approve:
Reply `yes` / `edit` / `no` to this write request #4.
