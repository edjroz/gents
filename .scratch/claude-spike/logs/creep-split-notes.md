# Creep split — 2026-09-03
- fix/session-fork-retry ← 0b89e688 (fork retry at last human user turn). Needs a Lean model of the cut point before review.
- fix/strip-idless-reasoning ← 63ff2ff3. Review findings C3 (unconditional 4th sanitize stage, Provider.lean still says three stages; conformance driver mints rs-lean-* ids so the stage is never exercised by spec) and C4 (provider_view over-drains legacy sessions with null compacted_through_sequence) must be addressed there, Lean-first.
