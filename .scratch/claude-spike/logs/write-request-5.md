CLAUDE WRITE REQUEST #5 — APPROVED (Codex shim → Claude Path A live smoke)

approved_by: human reply `proceed with request 5`
approved_at: 2026-08-31 (session continuation)

purpose:
Prove the personal goal path one step further:
`gents codex --remote ws://127.0.0.1:9293/` → spike Codex shim → owned loop →
`OpenAiCompatible` → Rust `gents claude-proxy` → Claude Max seat.
Confirm assistant `pong`, tools stripped / AgentToolCall=0, OAuthCredential=0.
Prod `~/.gents` and `:9191` stay untouched.

why_now:
Path A packaging (Tasks 14–20 / write #4) is done. Wiring for Codex-on-Claude is ready;
this is the first gated live Codex turn against the subscription completer.

scope_of_approval:
One Codex text-only smoke turn (and any automatic sibling completions under the same
owned-loop turn, e.g. title-gen), via:
1. Preflight seat probe in interactive Herdr pane (keychain-visible):
   `./target/debug/gents claude-auth-probe --config-dir .scratch/claude-spike/claude-config`
2. Confirm existing live stack still up:
   - spike server pid historically `12718` on `127.0.0.1:9192` / shim `:9293`
   - Rust proxy pid historically `51050` on `127.0.0.1:8787` with
     `PROXY_USE_CLAUDE=1` + `CLAUDE_WRITE_APPROVED=1`
3. One Codex launch:
   ```bash
   ./target/debug/gents codex \
     --remote ws://127.0.0.1:9293/ \
     --no-alt-screen \
     "Reply with exactly: pong"
   ```
4. Harvest DefraDB + proxy evidence; no extra Claude turns for the evidence pack.

not_in_scope:
- Fresh `claude-login` / Anthropic OAuth (seat already Max / logged in)
- Prod home / prod ports
- Tool bridging / A2 native provider
- Billing UI scrape
- Multi-turn Codex session beyond the single seed prompt

preflight_ungated:
- Ports listening: proxy `:8787` (gents 51050), GraphQL `:9192` + shim `:9293` (gents 12718)
- Spike home DID `did:key:z6MkgCE1AUd8uxQ6oEm3Phh54tftWiG7DfupDAUZpGvrwgu6`
- Sandbox probe `logged_in=false` (keychain invisible) — live probe must run in Herdr
- Codex CLI on PATH: `codex-cli 0.151.0`
- Text-only ToolSelection already configured from Task 10/20

abort_if:
- tool_use appears / AgentToolCall > 0
- api-key / Console auth path
- any OAuthCredential upsert for Claude/Anthropic
- traffic leaves loopback for the completer path
- prod ~/.gents or :9191 touched
- proxy mode is canned/fake instead of live `claude`

approve:
Human already replied `proceed with request 5`.
