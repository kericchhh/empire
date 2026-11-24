# 0008 Codex and local-model seats

Accepted 2026-11-23.

## Decision

Codex seats run `codex exec --json --ephemeral --ignore-user-config --ignore-rules` with the world's MCP server limited to the turn tools and built-in features off by `-c` (`codex.rs`). Codex has no `setup-token`, and an API key would bill extra, so it logs in once into Empire's own `CODEX_HOME` (`sim-server login codex`, or the app).

The bridge fetches a copy of that `auth.json` from the gate into the run's tmpfs home before Codex starts and sends it back after (`Route::Carry`, capped at `fs::CARRY_LIMIT`), so the run has no writable host mount. The runner saves it only if it changed, parses as a login, and no other run saved a newer one meanwhile (`codex::refreshed`).

Local models are a Codex seat with `local: <Responses API URL>` on loopback (Ollama, LM Studio): no login, no provider host, a tunnel to that address.

## Why

Codex documents that a user cannot switch every command feature off, so tool switches alone cannot seal it.

## Consequences

- Codex runs see no system programs (`Spec::programs`): only the CA certificates, `/etc/ld.so.cache` and the libraries `ldd` lists for the agent, its helpers and the bridge.
- Tool calls go through code mode, so `codex-code-mode-host` is staged beside `codex` (`Spec::helpers`); its JavaScript reaches only the given tools.
- Codex reports a refused login only as text ("sign in again"), which `codex::summary` matches.
- Two runs refreshing at once can still race, since OpenAI rotates refresh tokens.
