# Empire

A desktop sandbox where several empires share one world and AI agents play every role in them, like a game of Civilization with no human players. A creator describes only the starting point, in a scenario file or in a prompt that their own agent turns into one: the map, the empires, and each empire's setting, religion, ideology and characters (ruler, general, poet, peasants, or anything else) with the office each one holds. Agents then govern, trade, research, scheme and go to war while the world advances tick by tick. Which empires flourish and which collapse comes out of what happens, not from a script: a scenario defines no rules and no endings. Users watch the whole world, any single empire or any single role, and can inject events by hand.

**Code rules:** read `CODE_RULES.md` before writing, editing or reviewing any code.

## Workflow

- **Navigate with the code graph.** The repo is indexed by `codebase-memory-mcp` (MCP server `codebase-memory-mcp`, project `home-karim-Code-empire`). To find symbols, callers and impact, use its `search_graph`, `trace_path`, `get_code_snippet` and `get_architecture` before you grep or read whole files. Re-run `index_repository` after large changes. Without the MCP server, the same tools run from the shell: `codebase-memory-mcp cli <tool> --help`.
- **Check the docs first.** Before using a crate, tool or API, read its documentation for the exact version in `Cargo.lock`: docs.rs, or the crate's source under `~/.cargo/registry/src`. Take best practices from official docs or credible references, such as the Rust API Guidelines or the MCP specification. When the docs disagree with what you remember, follow the docs.
- **Keep the code tight.** Write the fewest lines that stay clear: no duplication, no dead code, no abstraction without a second user, and comments only for a non-obvious why.
- **Work in slices.** GitHub milestones and issues are the task list: one issue is one slice is one PR. `/milestone` plans a milestone with the user. The `slice` skill takes an issue to a pull request: a red test, then green, then `verify`, then two reviewers from different model families (`review`), each finding fixed or answered, then the lessons encoded, then the PR. Skills live in `.agents/skills` (`.claude/skills` links there). The user merges every PR.
- **Review every major change**, slice or not, with the `review` skill in a fresh context. A major change adds a crate, module, MCP tool or event variant, changes the core model, or touches more than three files.
- **Encode lessons in structure.** When a mistake could recur, make the strongest guard that works: a lint (`clippy.toml` in each crate, `deny.toml`), then a test, then a rule in this file or `CODE_RULES.md`.
- **Record decisions** the code cannot explain in `docs/decisions/`, one numbered file each, and read an area's records before changing it.
- **Ship from a branch.** Work on a branch off `main`, commit once `cargo fmt --check`, clippy and the tests pass, push, and open a PR against `main`.
- **Say when a rule changes.** A PR that changes `CODE_RULES.md`, `AGENTS.md`, `.agents/skills/`, a `clippy.toml`, `deny.toml`, a CI workflow, `scripts/pr-check` or a `scripts/check-*` says which rule changed, how and why under `## Rule changes`.

| Rule | Enforced by |
|---|---|
| Changes reach `main` only through a PR with `ci-success` green. | the `main` ruleset |
| A PR body has `## Verified`, `## Review` and `## Reflect`; each finding ends `Fixed`, `Answered: <reason>` or `Deferred #<issue>`; a rule change has `## Rule changes`. | `pr` CI job (`scripts/pr-check`, tested by `scripts/test-pr-check`) |
| No model calls and no provider SDKs. | `deny.toml` (bans provider SDK crates); a direct call to a provider API: judgment, e.g. a `reqwest` call to `api.anthropic.com` |
| No Windows-specific code: no `cfg(windows)`, no `cfg(unix)` guards, no Windows workarounds. | `scripts/check-platforms` (CI `pr` job), which also proves it still fails on fe486f6 |
| Every agent run is sealed, and nothing of the user reaches it. | the tests under Agent runs in `CODE_RULES.md` |
| State is rebuilt by replaying the event log; hidden state never crosses seats or empires. | proptests in `crates/sim-core/tests/replay.rs` |
| Every other rule in this file. | judgment (`review`), e.g. an action allowed by a character's title in the setting instead of its office |

## State

- **Built:** in `sim-core`, setting-only scenarios, the standard ruleset, several empires per world, offices and appointments to them, seeded starting numbers and fog of war between empires. In `sim-server`, eight MCP tools, the admin API and the SQLite log. In `sim-runner`, sealed Claude Code, Codex and local-model seats, and prompt-generated worlds. In `app/`, worlds, every empire's and seat's view, the chronicle, interventions and agent setup.
- **Not yet:** the hex map, actions between empires, succession and coups, contacts between empires, and a game-master seat for agents.
- Planned and open work is in GitHub milestones and issues.

## Core model

These rules hold everywhere. A feature that breaks one is a design change: raise it with the user before building it.

- **Bring your own agent.** This project never calls a model and holds no API keys or provider SDKs. The **runner** launches the agent CLI the user assigned to a seat (Codex, Claude Code, a local-model script) as a fresh headless run each turn, using the user's own subscription. The agent plays through the MCP tools, exactly as an externally connected agent would. When a feature seems to need "the AI to do X", make it a seat or a tool that a seat calls. A run gets no built-in tools, only the world's MCP server, and only its own seat token, so seats on the same login stay as separated as seats on different machines. Every run is sealed in an OS sandbox (bubblewrap on Linux, `sandbox-exec` on macOS) with an empty home and working directory, and its network reaches only its provider's hosts, the world and its local model server, so no user's files, settings, history, repositories, local network or account details (name, email) reach an agent. An agent kind whose own tools cannot all be switched off also sees no system programs. The only credentials Empire keeps are the user's own subscription logins for agent CLIs (a `claude setup-token` token, a Codex home of Empire's own), readable only by that user.
- **World and empires:** one world holds several **empires** on a shared map. Each empire has its own territory, numbers, culture, characters and roles. Empires interact only through actions the rules adjudicate: diplomacy, trade, treaties, war, settling and conquering territory.
- **Hex map:** the world is a grid of hex tiles with terrain and resources, generated from the scenario's seed. Ownership of tiles defines borders, and borders move only through events.
- **Seat:** a slot in one empire, claimed with a token. A seat can control one character or a group of them. Seats with no agent assigned fall back to the rules engine.
- **Single source of truth:** world state is rebuilt by replaying an append-only **event** log. Every change is an event: agent actions, rule outcomes, game-master rulings and user interventions alike. Replay, time travel and forking depend on this.
- **Office:** a position the ruleset defines (head of state, official, commander, governor, private person, populace). A character's office, not its title in the setting, decides which actions it may take and which numbers it sees. Offices are part of the world state, so events can move them from one character to another.
- **Agents propose, the engine adjudicates.** A seat submits actions. The engine checks them against the character's office and resources, the rules engine computes every number, and the optional game-master seat rules on free-form actions.
- **Perception and fog of war:** each seat sees only what its character could know. Its own empire is visible according to its offices; other empires only through what they reveal publicly, through contact and through tiles in sight. If hidden state leaks across seats or empires, the experiment is invalid.
- **Tick:** one step of simulation time (a season by default). A tick closes when every playing seat has called `end_turn` or when the deadline passes. A seat that misses the deadline acts according to its persona's default behaviour, applied by the rules engine.
- **Cohort:** a population group (the peasants of Gaul) that the rules engine simulates as aggregate numbers. An agent can optionally take a cohort's seat to voice it.
- **The scenario is a starting point; the ruleset is the mechanics.** Everything specific to a setting lives in the scenario YAML: names, lore, culture, personas, the UI theme (palette, fonts, terms) and optional starting numbers. The mechanics (which numbers exist, offices, actions, rules) live in the engine's standard ruleset, which names no setting. A scenario cannot add rules, actions or endings. Starting numbers it leaves out are drawn from the seed. The same code runs Rome against Parthia, rival megacorps or a galaxy of space federations unchanged.
- **No scripted ending.** A world runs until it is stopped. Empires rise, fall into civil war and recover through the rules and the agents' decisions.

## Workspace

The game runs on Linux and macOS only. Windows is a later feature: write no Windows-specific code until then.

| Path | Owns |
|---|---|
| `crates/sim-core` | Scenario format, the standard ruleset (`rules/standard.yaml`), world state, events, rules engine, perception. Pure and deterministic. |
| `crates/sim-server` | The world host: MCP tools (rmcp, streamable HTTP at `/mcp`), seats, tick scheduler, admin API at `/api`, SQLite event log. Runs inside the desktop app and as a headless binary. |
| `crates/sim-runner` | Launches the user's agent CLI (Claude Code, or Codex on a subscription or a local model server): a fresh, sandboxed headless run per seat each turn, and the author run that turns a prompt into a scenario the engine then validates. One provider may play any number of seats of any empires. |
| `app/` | Tauri desktop app: a React UI (`app/src`) and its Rust backend (`app/src-tauri`), which hosts worlds in-process through `sim_server::host`. Creating and opening worlds, every empire and every seat's view, the chronicle, interventions and agent setup. The hex map, diplomacy graph and timeline come with the features behind them. |
| `scenarios/` | Scenario files. `rome.yaml` is the reference scenario: Rome and Parthia in 27 BC. |
| `scripts/` | Developer scripts. `empire` controls a throwaway headless world for verification. `pr-check <pr>` checks a PR body against the slice contract. `check-platforms [rev]` fails on Windows-specific code. |
| `docs/decisions/` | Decision records: what was decided, why, and what follows from it. |
| `.agents/skills/` | The workflow's skills: `milestone`, `slice`, `verify`, `review`. |
| `schema/` | `scenario.schema.json`, generated by `sim-server schema`. Regenerate it whenever scenario types change. |

If a change alters the core model or a crate's ownership above, update this file in the same change.
