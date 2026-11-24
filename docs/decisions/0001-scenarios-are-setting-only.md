# 0001 Scenarios are setting-only

Accepted 2026-11-22.

## Decision

A scenario describes only the starting point: calendar, theme, empires (culture, regions, characters with their office, seats) and an optional game master. The mechanics (numbers, offices, actions, rules, limits) live in `crates/sim-core/rules/standard.yaml`, embedded in the binary and fixed. There are no endings: a rule like `civil_war` hurts an empire but never stops the world.

Starting numbers are optional. Anything a scenario leaves out is drawn from the ruleset's `start_range` with the seed, on streams `start-metric:<metric>#<empire>`, `start-stat:<stat>@<region>` and `start-resource:<resource>@<character>`, rounded to 0.01. Every starting value, given or drawn, is written into the `WorldGenerated` event, so replay never runs generation again.

## Why

The same engine must run Rome against Parthia or a galaxy of federations unchanged, and which empires flourish must come from play, not from a script.

## Consequences

- Validation is strict: `deny_unknown_fields`, every reference checked, ids unique across empires.
- Scenario YAML enums are `{ variant: value }` maps, read through `singleton_map_recursive` in `Scenario::from_yaml`. Plain `serde_yaml_ng::from_str` would demand `!tag` syntax.
- `schema/scenario.schema.json` is generated: `cargo run -p sim-server -- schema > schema/scenario.schema.json`. A test fails when it is stale.
- In `rome.yaml`, Rome sets its starting numbers and Parthia sets none, so both paths stay exercised. Left to the rules alone both decline, so players have something to save.
