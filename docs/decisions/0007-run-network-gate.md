# 0007 A run's network is the gate's

Accepted 2026-11-23.

## Decision

On Linux a run has no network of its own. Empire's executable runs inside the sandbox as a bridge (`bridge_if_asked`, called first in every `main` that hosts a runner) and relays the agent's proxy address and the world's MCP address over Unix sockets to a gate in the runner (`net.rs`). The gate's proxy admits `CONNECT` only to the agent kind's `HOSTS` and logs each refusal; each tunnel reaches one loopback address. On macOS the profile admits only the gate's and the tunnels' loopback ports.

## Why

A run must reach its provider and the world, and nothing else: no local network, no other host.

## Consequences

- The sockets directory is mounted read-only. A world not on loopback is refused (`PlanError::RemoteWorld`).
- `HOSTS` lists only what a live run's refusal log confirmed: Claude Code needs `api.anthropic.com`, Codex `chatgpt.com`, plus `auth.openai.com` for login refreshes.
