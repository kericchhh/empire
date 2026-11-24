# 0009 Prompt-generated worlds

Accepted 2026-11-22.

## Decision

`run --prompt` and `create --prompt` launch a sealed `claude -p` (no tools, no MCP) with `--json-schema` set to the scenario schema, minus its `$schema` line, which the CLI rejects. `Scenario::from_yaml` and `World::standard` check the draft, and a rejected draft goes back with the error, up to three attempts. `--play-all` and the agents file's `all` assign every non-GM seat, so no scenario needs its own agents file.

## Why

The engine, not the author agent, decides what a valid world is, so a prompt can never smuggle in rules.

## Consequences

- Only Claude Code writes scenarios for now. Codex's `--output-schema` could serve an author run.
