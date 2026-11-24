# 0003 Events store resolved effects

Accepted 2026-11-22.

## Decision

An event stores the numbers it changed (`treasury −40`), never the order that caused them. Replay only applies stored numbers: rules and the RNG never run again.

## Why

Old logs must still replay after the ruleset is rebalanced, and replay, time travel and forking all rest on the log alone.

## Consequences

- Random draws come from `roll(seed, tick, stream)`, with the stream keyed by rule id and region, never by list position. Adding a rule never shifts another rule's draws.
- `serde_json` has `float_roundtrip` on workspace-wide. Without it an `f64` can come back one ULP off and silently break replay (the replay proptest caught this).
- Every stored number stays finite, since JSON has no infinity. Hence `MAX_INTERVENTION_DELTA`, and `resolve_effects` and `State::apply_effects` skip non-finite values.
- The event schema was reset once, before any world was kept: `Target::Metric` gained an `empire`, `RuleFired` an `empire`, `WorldEnded` was removed. From then on the schema is permanent (`CODE_RULES.md`, Events).
