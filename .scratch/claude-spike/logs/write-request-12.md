# CLAUDE WRITE REQUEST #12 — non-interactive seat refresh attempt (2026-09-04)

Approval: user /goal directive ("proceed with them as we've done … If we hit 429s you can stop"); no browser login possible from the session.
Goal: refresh the expired spike seat (Keychain `Claude Code-credentials-6b2dc9d7`) using the Claude CLI's own refresh path so #10b/#11b can run. Gents never writes the seat; the `claude` binary does.
Steps: (1) `claude auth status </dev/null`; (2) if expiry unchanged, one `claude -p 'Reply with exactly: pong' --model claude-haiku-4-5-20251001 --max-turns 1 </dev/null` with a 90 s watchdog. Stop on any 429 or auth error; never print tokens; only the Keychain `expiresAt` number is read.
