# B3 gated live smoke (C2 Messages HTTP)

**When:** 2026-09-02T21:38Z–21:59Z
**Branch:** `spike/claude-b3-live-tools` @ `3643bf7f` plus uncommitted Keychain fallback + omit `temperature`
**Binary:** `./target/debug/gents` inode `44630734` (mtime Sep 2 17:51)
**Server:** PID `51717` `--home ~/.gents --tool-root <repo> --tool-ceiling readwrite --claude-config-dir .scratch/claude-spike/claude-config --claude-write-approved`
**Behavior:** `did:key:z6MkpEq4KsC2oibBmHe6faw1246CU5gmtVJJEYf3wvo1yKiQ:default` tools `list_files, read_file, glob, grep, write_file, edit_file, bash_unrestricted, context_budget`
**Prompt:** use gents `list_files` on `.`, then reply `listed` (not Claude `Bash`)

## Bar

| Check | Result |
|---|---|
| Messages HTTP reached Anthropic (not process CLI, not `:8787`) | **yes** (400 then 429 from `api.anthropic.com`) |
| Seat auth from Keychain, no `.credentials.json` | **yes** |
| Claude `OAuthCredential` | **0** (only existing row is `xai-oauth`) |
| `:8787` | **closed** |
| CLI Bash in spike workdir | **none** (workdir empty) |
| Token printed / logged | **none** (`sk-ant` absent from `b3-live*` logs) |
| `AgentToolCall ≥ 1` on these requests | **no** — Anthropic 429 before any `tool_use` |

**Stop:** live tool parity is **not** demonstrated this session. Do not keep hammering 429.

## Attempts

1. **Hang (discarded).** First binary used `SecKeychainFindGenericPassword` first. Unsigned `gents` is not on the item ACL; the worker blocked in `processing` with no Messages log. Request `88229d80-dfd8-4167-be1f-42d9b695d997`. Fix: `security(1)` first for service `Claude Code-credentials-6b2dc9d7` / account `$USER`. Log: `b3-live-server-hang.stderr.log`.
2. **400 `temperature` is deprecated for this model.** Auth worked. Anthropic `req_011CefM1aoN9FQVm8pWHsUM1`. Request `e515f635-383a-49ab-a96f-49a283911d45` / session `95ff7d82-1a09-4ede-b155-812c1acec523`. Fix: omit `temperature` on the Messages body. Raw: `b3-live-tool-temp400.json`.
3. **429 rate_limit_error** after the omit, retry budget 2, `retry after 60s`. Request `9d27687a-98d9-4415-90ae-4c04624a21fa`. Raw: `b3-live-tool-429.json`.
4. **429 again** after 70s wait. Request `28ef9ed4-8e66-45ce-bb16-0cb5a3a7bee0`.
5. **429 again** after 180s wait. Request `da940909-0ae2-4d30-aa34-3b35530a381a` / session `1a31b535-85d8-42cb-8ca5-0128b44810da`. Last raw: `b3-live-tool.json`.

Keychain metadata (no secrets): `b3-keychain-meta.json` — JSON `claudeAiOauth` with `accessToken` present, prefix `sk-ant`, `expiresAt` in the future at read time.

## Pins from this live

- macOS seat token is Keychain `Claude Code-credentials-{sha256(config_dir)[:8]}`, not the file. File still wins when present.
- Prefer `security find-generic-password -w` over Security.framework in the unsigned debug binary (ACL hang).
- Do **not** send `temperature` to `claude-sonnet-5` Messages.
- Claude Code identity system prefix was **not** added (do not invent). First live call was 400 validation, not 401; later 429s may be quota from those retries. Revisit only with a numbered live retry after cooldown.

## Uncommitted (this smoke)

- `crates/gents/src/claude_seat_auth.rs` — file then Keychain
- `crates/gents/src/claude_messages.rs` — omit `temperature`
