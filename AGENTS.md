# Empire

A desktop sandbox where several empires share one world and AI agents play every role in them, like a game of Civilization with no human players. A creator describes the world in a scenario: the map, the empires, and each empire's setting, religion, ideology, characters and roles (ruler, general, poet, peasants, or anything else). Agents then govern, trade, research, scheme and go to war while the world advances tick by tick. Which empires flourish and which collapse comes out of what happens, not from a script. Users watch the whole world, any single empire or any single role, and can inject events by hand.

**Code rules:** read `CODE_RULES.md` before writing, editing or reviewing any code.

## Workflow

- **Navigate with the code graph.** The repo is indexed by `codebase-memory-mcp` (MCP server `codebase-memory-mcp`, project `home-karim-Code-empire`). To find symbols, callers and impact, use its `search_graph`, `trace_path`, `get_code_snippet` and `get_architecture` before you grep or read whole files. Re-run `index_repository` after large changes. Without the MCP server, the same tools run from the shell: `codebase-memory-mcp cli <tool> --help`.
- **Check the docs first.** Before using a crate, tool or API, read its documentation for the exact version in `Cargo.lock`: docs.rs, or the crate's source under `~/.cargo/registry/src`. Take best practices from official docs or credible references, such as the Rust API Guidelines or the MCP specification. When the docs disagree with what you remember, follow the docs.
- **Review every major change.** A major change is one that adds a crate, module, MCP tool or event variant, changes the core model, or touches more than three files. Once it compiles and its tests pass, send a reviewer subagent with a fresh context to check the change against this file and `CODE_RULES.md`. It looks for correctness bugs, rule violations and leaks of nondeterminism or hidden state. The change is done once every finding is either fixed or answered with a reason.
- **Ship every major change as a pull request.** Work on a branch off `main`, never on `main` itself. Commit once `cargo fmt --check`, clippy and the tests pass, push, and open a PR against `main`. Run the review before opening it, or on the open PR. CI's `ci-success` check must be green before merging.

## Core model

These rules hold everywhere. A feature that breaks one is a design change: raise it with the user before building it.

- **Bring your own agent.** This project never calls a model and holds no API keys or provider SDKs. The **runner** launches the agent CLI the user assigned to a seat (Codex, Claude Code, a local-model script) as a fresh headless run each turn, using the user's own subscription. The agent plays through the MCP tools, exactly as an externally connected agent would. When a feature seems to need "the AI to do X", make it a seat or a tool that a seat calls.
- **World and empires:** one world holds several **empires** on a shared map. Each empire has its own territory, numbers, culture, characters and roles. Empires interact only through actions the rules adjudicate: diplomacy, trade, treaties, war, settling and conquering territory.
- **Hex map:** the world is a grid of hex tiles with terrain and resources, generated from the scenario's seed. Ownership of tiles defines borders, and borders move only through events.
- **Seat:** a slot for a role in one empire, claimed with a token. A seat can control one character or a group of them. Seats with no agent assigned fall back to the rules engine.
- **Single source of truth:** world state is rebuilt by replaying an append-only **event** log. Every change is an event: agent actions, rule outcomes, game-master rulings and user interventions alike. Replay, time travel and forking depend on this.
- **Agents propose, the engine adjudicates.** A seat submits actions. The engine checks them against the role's permissions and resources, the rules engine computes every number, and the optional game-master seat rules on free-form actions.
- **Perception and fog of war:** each seat sees only what its character could know. Its own empire is visible according to its role; other empires only through what they reveal publicly, through contact and through tiles in sight. If hidden state leaks across seats or empires, the experiment is invalid.
- **Tick:** one step of simulation time (a season by default). A tick closes when every playing seat has called `end_turn` or when the deadline passes. A seat that misses the deadline acts according to its persona's default behaviour, applied by the rules engine.
- **Cohort:** a population group (the peasants of Gaul) that the rules engine simulates as aggregate numbers. An agent can optionally take a cohort's seat to voice it.
- **Scenario-agnostic engine:** everything specific to a setting lives in the scenario YAML, including the UI theme (palette, fonts, terms). The same code runs Rome against Carthage, rival megacorps or a galaxy of space federations unchanged.

## Workspace

| Path | Owns |
|---|---|
| `crates/sim-core` | Scenario format, world state, events, rules engine, perception. Pure and deterministic. |
| `crates/sim-server` | The world host: MCP tools (rmcp, streamable HTTP at `/mcp`), seats, tick scheduler, admin API at `/api`, SQLite event log. Runs inside the desktop app and as a headless binary. |
| `crates/sim-runner` (planned) | Launches the assigned agent CLI for each seat, a fresh headless run each turn. |
| `app/` (planned) | Tauri desktop app: React UI for the map, empires, roles and chronicle, and for configuring agents and interventions. Hosts `sim-server` and `sim-runner` in-process. |
| `scenarios/` | Scenario files. `rome.yaml` is the reference scenario (single empire; becomes multi-empire with the world model). |
| `schema/` | `scenario.schema.json`, generated by `sim-server schema`. Regenerate it whenever scenario types change. |

If a change alters the core model or a crate's ownership above, update this file in the same change.
