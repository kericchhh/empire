# Handoff

The state of the project at the end of the first working session (2026-11-22), for whoever continues it. Read `AGENTS.md` (design and workflow) and `CODE_RULES.md` (code rules) first. This file covers what those don't: what exists, why some things are the way they are, and what to do next.

## Where things stand

The single-empire simulation core and its MCP host are built, reviewed once, and fixed. The **multi-empire desktop redesign is decided but not started**. `AGENTS.md` already describes the target design, and its workspace table marks what is still planned.

Finish line for every change: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test --workspace`. It passes with 83 tests.

### Built

- **`crates/sim-core`:** pure and deterministic.
  - `scenario.rs`: the scenario format, with strict validation (`deny_unknown_fields`, every reference checked).
  - `state.rs`: `State`, changed only by `State::apply(&Event)`.
  - `engine.rs`:
    - order and ruling validation;
    - `close_tick`, which resolves a whole tick into events;
    - rules with conditions, streaks and chance;
    - endings;
    - messages, notes and interventions.
  - `perceive.rs`: `Sight`, the one place that decides what an observer may see, plus `perceive`, which builds a seat's view, and `describe`, which writes chronicle lines through a `Sight`.
- **`crates/sim-server`:**
  - `game.rs`: the `Game` service: seats and tokens, turn protocol, deadline scheduler, long-poll, write-ahead commit.
  - `mcp.rs`: 8 MCP tools (rmcp 3.11, streamable HTTP).
  - `http.rs`: admin REST under `/api`, behind a bearer token.
  - `store.rs`: SQLite with one file per world.
  - `main.rs`: the CLI (`run`, `check`, `schema`).
- **`scenarios/rome.yaml`:** the reference scenario. Left to the rules alone it collapses into civil war around 19 BC, so players have something to save.
- **Tests:**
  - unit tests next to the code;
  - property tests in `crates/sim-core/tests/replay.rs`: a replayed serialized log rebuilds the live state, a state survives a save and reload, and the same seed gives the same history;
  - golden snapshots in `crates/sim-core/tests/golden.rs`;
  - a schema sync test;
  - end-to-end tests in `crates/sim-server/tests/e2e.rs`, using real MCP and HTTP clients on an ephemeral port.
- **Files added outside this session:** `.github/` (CI), `deny.toml` (it also bans provider SDK crates) and `rust-toolchain.toml`. Keep them passing.
- **CI checks only what is built:** fmt, clippy, tests (`cargo nextest`), docs, schema freshness, scenario validation against the schema, and `cargo deny`. Nightly runs the proptests with 10,000 cases and an advisory check. Phase 3 adds the jobs for the Tauri app in `app/` and its bundle. `cargo nextest` isn't installed locally; `cargo test --workspace` covers the same tests.

### Decided, not built

1. **A world with several empires on a hex map:** per-empire fog of war, and diplomacy, trade, war and conquest between empires.
2. **`sim-runner`:** the app launches the agent CLI the user assigned to each seat (Codex, Claude Code, a local-model script), a fresh headless run each turn.
3. **`app/`:** a Tauri desktop app with a React UI, hosting the server and runner in-process. No browser UI.

## Decisions that aren't obvious from the code

- **Events store resolved effects** (`treasury −40`), never the order that caused them. Replay only applies stored numbers. Rules and RNG never run again, so old logs still replay after rules change.
- **`serde_json` has `float_roundtrip` enabled** workspace-wide. Without it, an `f64` can come back from JSON one ULP off and silently break replay. The replay property test caught this. Keep it on.
- **Every stored number stays finite.** JSON has no infinity, so one would corrupt the log. That's why `MAX_INTERVENTION_DELTA` exists, and why `resolve_effects` and `State::apply_effects` skip non-finite values.
- **Random draws come from `roll(seed, tick, stream)`**, where the stream is keyed by rule id and region. Adding a rule never shifts another rule's draws.
- **Agent text is quoted in-world speech.** `check_text` rejects line breaks, control characters, `«` and `»`, so agents can't forge chronicle lines or break out of the quotes.
- **Perception has two layers.** `Sight` decides which numbers an observer sees. `describe` applies it to every chronicle line: effects (and what scaled them), rule events in unseen regions, and rule labels whose condition reads a hidden value. New event kinds must go through both.
- **Free-form plans are private.** The declaration and its expiry are visible to the author and the game master. A ruling is private to the author unless `public: true`.
- **The turn protocol:**
  - **Deciding:** `act`, `rule` and `end_turn` carry the `tick` the agent decided on, and an old tick is refused with `StaleTick`.
  - **Closing:** a tick closes when every joined seat has ended its turn, or at `--tick-seconds`. With nobody joined, it waits unless `--autoplay` is on.
  - **Waiting:** the long-poll returns within 25 seconds.
- **Orders queued mid-tick live only in memory.** The queue is cleared after the commit succeeds. A restart mid-tick drops it, and the README says so.
- **Scenario YAML enums are written as `{ variant: value }` maps.** That works through `singleton_map_recursive` in `Scenario::from_yaml`. Use that entry point; plain `serde_yaml_ng::from_str` would demand `!tag` syntax.
- **`schema/scenario.schema.json` is generated.** Regenerate it with `cargo run -p sim-server -- schema > schema/scenario.schema.json`. A test fails when it goes stale.

## Review history

**Review 1 (8 findings, all fixed):**
1. The chronicle leaked hidden numbers.
2. Free-form plans were public, and agent text could forge chronicle lines.
3. `for_ticks: 0` fired every tick.
4. Orders could land in a later tick.
5. Queued orders were dropped on a storage error.
6. Random streams were keyed by list position.
7. There were test gaps.
8. Inputs were unbounded.

**Not done, deliberately:** a stdio transport, since HTTP is the only transport now, and a WebSocket endpoint, which waits for the app (the broadcast it will use, `Game::subscribe`, is already built and tested).

**Review 2 (verifying those fixes):** all 8 confirmed fixed, with no regressions. It found 5 more issues, all now fixed with tests:
1. A rule's label leaked its hidden condition. `describe` now drops a rule line when the observer can't see the value its condition reads.
2. An effect scaled by a hidden value revealed that value. `Effect` gained `scaled_by`, and `Sight::effect` hides such effects.
3. Action targets were named by guessing between region and character. They're now named by the action's target kind.
4. Ids containing `@` or `:` could collide in streak keys. Those characters are now rejected in ids.
5. A failed tick close left the seat stuck in "ended". The seat can now retry, with a test using a closed store.

Also closed: a character with no region can't default to an action that affects a region.

**Open:** nothing from either review.

## Next steps, in order

**Phase 1: world model in `sim-core`.**
- A scenario defines `empires`. Each one gets its own metrics, roles, characters and seats.
- A hex map of tiles in axial coordinates, with terrain, resources and an owner, generated from the seed or placed by hand. Today's regions become cities or provinces on tiles.
- Actions between empires: treaties (propose and accept), trade, declaring war, moving armies, settling and conquering tiles.
- Fog of war, built by extending `Sight` with each empire's visible tiles and contacts.
- Replace `rome.yaml` with a two-empire Rome vs Carthage, and keep the golden and replay tests across empires.

**Phase 2: `sim-runner`.**
- Read the current Codex and Claude Code docs for headless runs and per-run MCP setup first.
- Configure one CLI per seat. Each turn, launch it with the seat token and a short play-loop prompt (the server's `instructions` text already states the loop).
- Add timeouts, capped concurrency and logs per seat.

**Phase 3: the Tauri app in `app/`.**
- Read the Tauri v2 docs first.
- Host `Game` in-process and reach it through Tauri commands. The admin API is the model for those commands.
- Use the event broadcast for live updates.
- Build the views:
  - hex map coloured by owner, with overlay layers;
  - dashboards per empire;
  - role and character views;
  - chronicle;
  - diplomacy graph;
  - agent setup;
  - interventions;
  - a timeline scrubber built on the event log.
- Apply the scenario theme as CSS variables.

Every phase ends with a reviewer subagent pass, as `AGENTS.md` requires.

## Running it

```sh
cargo run -p sim-server -- run --db rome.db --scenario scenarios/rome.yaml --tick-seconds 300
codex mcp add empire --url http://127.0.0.1:7878/mcp
claude mcp add --transport http empire http://127.0.0.1:7878/mcp
```

The server prints an admin token and one token per seat. `README.md` covers the seat tools and the admin API.
