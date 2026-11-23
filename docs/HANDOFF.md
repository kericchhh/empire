# Handoff

The state of the project at the end of the first working session (2026-11-22), for whoever continues it. Read `AGENTS.md` (design and workflow) and `CODE_RULES.md` (code rules) first. This file covers what those don't: what exists, why some things are the way they are, and what to do next.

## Where things stand

The simulation core and its MCP host are built and reviewed. The second session (2026-11-22) made scenarios setting-only and added several empires per world, the first slice of Phase 1. The third change added `sim-runner`: one Claude Code login can play every seat, each turn in a fresh isolated run. Later changes sealed every run in an OS sandbox with a network limited to its provider and the world, and added Codex seats, on a ChatGPT login or a local model server. The hex map and actions between empires are still to come. `AGENTS.md` describes the target design, and its workspace table marks what is still planned.

Finish line for every change: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test --workspace`. It passes with 119 tests.

### Built

- **`crates/sim-core`:** pure and deterministic.
  - `scenario.rs`: the scenario format. Setting only: calendar, theme, empires (culture, regions, characters with their office, seats) and an optional game master. Strict validation (`deny_unknown_fields`, every reference checked, ids unique across empires).
  - `ruleset.rs` and `rules/standard.yaml`: the mechanics every world runs on. Numbers (with a `start_range` for seeded starts), offices, actions, rules and limits. Embedded in the binary and fixed: scenarios cannot change it.
  - `world.rs`: `World`, a scenario checked against a ruleset. The engine reads everything that is not state from it.
  - `state.rs`: `State`, changed only by `State::apply(&Event)`. Metrics are per empire; regions carry their owner; characters carry their empire, office and home region.
  - `engine.rs`:
    - starting values (`WorldGenerated`), order and ruling validation;
    - `close_tick`, which resolves a whole tick into events;
    - rules per empire or per region, with conditions, streaks and chance;
    - messages, notes and interventions.
  - `perceive.rs`: `Sight`, the one place that decides what an observer may see, plus `knows` (which characters an empire knows of), `perceive`, which builds a seat's view, and `describe`, which writes chronicle lines through a `Sight`.
- **`crates/sim-server`:**
  - `game.rs`: the `Game` service: seats and tokens, turn protocol, deadline scheduler, long-poll, write-ahead commit.
  - `mcp.rs`: 8 MCP tools (rmcp 3.11, streamable HTTP).
  - `http.rs`: admin REST under `/api`, behind a bearer token.
  - `store.rs`: SQLite with one file per world.
  - `main.rs`: the CLI (`run`, `check`, `schema`).
- **`scenarios/rome.yaml`:** the reference scenario, Rome and Parthia in 27 BC. Rome sets its starting numbers; Parthia sets none, so the seed draws them. Left to the rules alone, both decline (Parthia into civil war within a few years at seed 2026), so players have something to save.
- **Tests:**
  - unit tests next to the code;
  - property tests in `crates/sim-core/tests/replay.rs`: a replayed serialized log rebuilds the live state, a state survives a save and reload, and the same seed gives the same history;
  - golden snapshots in `crates/sim-core/tests/golden.rs`;
  - engine unit tests run on `tests/fixtures/tiny.yaml` with `tiny_rules.yaml`, so they don't break when the standard ruleset is rebalanced. Server tests run the same scenario on the standard ruleset;
  - a schema sync test;
  - end-to-end tests in `crates/sim-server/tests/e2e.rs`, using real MCP and HTTP clients on an ephemeral port.
- **Files added outside this session:** `.github/` (CI), `deny.toml` (it also bans provider SDK crates) and `rust-toolchain.toml`. Keep them passing.
- **CI checks only what is built:** fmt, clippy, tests (`cargo nextest`), docs, schema freshness, scenario validation against the schema, and `cargo deny`. Nightly runs the proptests with 10,000 cases and an advisory check. Phase 3 adds the jobs for the Tauri app in `app/` and its bundle. `cargo nextest` isn't installed locally; `cargo test --workspace` covers the same tests.

### Decided, not built

1. **A world with several empires on a hex map:** per-empire fog of war, and diplomacy, trade, war and conquest between empires.
2. **`sim-runner`** and **`app/`** are built; see Phases 2 and 3.

## Decisions that aren't obvious from the code

- **The scenario is a starting point, never a script.** The user decided that a scenario only describes the setting. Mechanics live in the standard ruleset, and there are no endings: the world runs until it is stopped. A rule like `civil_war` hurts an empire but never stops the world.
- **Offices, not roles, decide permissions.** A character's `title` ("Princeps", "Poet") is flavour; its `office` (`head_of_state`, `official`, `commander`, `governor`, `private`, `cohort`) decides its actions and sight. Offices are in `State`, so a later appointment or coup event can move them. No event does that yet.
- **Starting numbers are optional.** Anything a scenario leaves out is drawn from the ruleset's `start_range` with the seed (streams `start-metric:<metric>#<empire>`, `start-stat:<stat>@<region>` and `start-resource:<resource>@<character>`), rounded to 0.01. Every starting value, given or drawn, is written into the `WorldGenerated` event, so replay never runs generation again.
- **Fog between empires:** a seat sees its own empire's numbers through its offices and nothing of another empire's numbers. Foreign empires are known by name, description and culture. Foreign characters are known only if their office is `public` (heads of state), so diplomacy works through them. Actions, rule events and public rulings are visible to the acting empire only (`Visibility::Empire`). Interventions are public unless the admin names an `empire`.
- **The event schema was reset once, before any world was kept.** `Target::Metric` gained an `empire`, `RuleFired` an `empire`, and `WorldEnded` was removed. Worlds created before this change won't load. From here on, follow the rule in `CODE_RULES.md`: add, never repurpose.

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
- Done: several empires per world, setting-only scenarios, the standard ruleset, offices, seeded starting numbers, fog of war between empires, Rome and Parthia.
- A hex map of tiles in axial coordinates, with terrain, resources and an owner, generated from the seed (stored in an event, like the starting values) or placed by hand. Today's regions become provinces made of tiles, and starting numbers derive from terrain.
- Actions between empires: treaties (propose and accept), trade, declaring war, moving armies, settling and conquering tiles. Conquest moves `RegionState::owner`; an empire left with no territory collapses as an event. Untargeted actions and defaults act on the character's home region without checking its owner, so conquest must also move or check `CharacterState::region`.
- Office changes: appointments by the head of state, succession when a ruler dies, coups. These move `CharacterState::office`.
- Contacts: extend `Sight` and `knows` with which empires have met and which tiles are in sight, instead of every empire knowing every other by name.

**Phase 2: `sim-runner`.**
- Done: Claude Code seats (`--agents FILE`). Runs are isolated: `--tools ""`, only the turn's MCP tools (`join` and `wait_for_turn` are denied, so a run cannot drift into the next tick), `--strict-mcp-config`, `--restricted`, no session persistence, an empty working directory under the private log directory, a cleared environment (no `ANTHROPIC_API_KEY`), the token on stdin. A shared concurrency cap, per-turn timeouts, logs under `<db>.runs/` (mode 0700). The runner skips a turn whose tick closed while it queued, kills a run once its tick closes, and ends each seat's turn after its run, so a crashed or slow agent never holds up the tick.
- The game master seat is refused for now: its run would start with the others, before there is anything to rule on. Start it once only the game master is left to end its turn (expose `waiting_for` to it, or give it a later phase of the tick).
- Done: every run is sealed in an OS sandbox (`sandbox.rs`: bubblewrap on Linux, `sandbox-exec` on macOS) with an empty home and `/work`, so no user file, setting, `CLAUDE.md` or git status reaches it. On Linux both are size-capped tmpfs mounts that exist only inside the sandbox. `check_sandbox` runs before any agent starts, and the app shows why it fails. Tests against a live run showed that before this, a run inside a repo saw its git status and commit subjects, and every run saw the account's email. Claude seats now log in with a `claude setup-token` token saved by `sim-server login` or the app (`local.rs`, mode 0600 in the data directory), passed as `CLAUDE_CODE_OAUTH_TOKEN`. Sharing the user's own login file would break its token refresh. `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1` stops Claude Code fetching the account profile, which is what put the email in the model's context. A result with `api_error_status` 401 or 403 sets `Plan::login_refused`, which the app shows. The macOS profile is written but not yet tried on a Mac.
- Done: a run's network is limited (`net.rs`). On Linux the run has no network of its own: Empire's executable runs inside the sandbox as a bridge (`bridge_if_asked`, called first in every `main` that hosts a runner) and relays the agent's proxy address and the world's MCP address over Unix sockets to a gate in the runner. The gate's proxy admits `CONNECT` only to the agent kind's `HOSTS`, and logs each refusal; each tunnel reaches one loopback address. The sockets directory is mounted read-only, and a run whose world is not on loopback is refused (`PlanError::RemoteWorld`). On macOS the profile admits only the gate's and the tunnels' loopback ports. Agent executables and the bridge are copied, once per version, to `/var/tmp/empire-<uid>` (macOS: the per-user temp dir), so no mount names the user's home; unused copies go after ten minutes.
- Live runs confirmed the hosts: Claude Code needs only `api.anthropic.com` (not yet re-checked since this change: no `setup-token` login was saved on the dev machine), Codex only `chatgpt.com` (`auth.openai.com` is listed for login refreshes).
- Only standalone agent executables work in the sandbox: the native Claude Code install, not an npm one, whose `cli.js` needs a `node` from outside `/usr`.
- Worlds live in one per-user data directory (`dev.empire.app/worlds`), shared by the app and `sim-server --world NAME`.
- Done: prompt-generated worlds. `run --prompt` / `create --prompt` launch a sealed `claude -p` (no tools, no MCP) with `--json-schema` set to the scenario schema (minus its `$schema` line, which the CLI rejects). The draft is checked by `Scenario::from_yaml` and `World::standard`, and rejected drafts go back with the error, up to three attempts. `--play-all` and the agents file's `all` assign every non-GM seat, so no scenario needs its own agents file.
- Done: Codex seats (`codex.rs`, `codex exec --json --ephemeral --ignore-user-config --ignore-rules`, the world's MCP server with the turn tools only, built-in features off by `-c`). Codex has no `setup-token`, and an API key would bill extra, so it logs in once into Empire's own `CODEX_HOME` (`sim-server login codex`, or the app). The bridge fetches a copy of that `auth.json` from the gate into the run's tmpfs home before Codex starts and sends it back after (`Route::Carry`, capped at `fs::CARRY_LIMIT`), so the run has no writable host mount. The runner saves it only if it changed, parses as a login and no other run saved a newer one meanwhile (`codex::refreshed`). Two runs that refresh at the same moment can still race, since OpenAI rotates refresh tokens. The probe in this change showed `features.shell_tool=false` removes `exec_command`, but Codex documents that a user cannot switch every command feature off, so Codex runs also see no system programs (`Spec::programs`). Tool calls go through code mode, so `codex-code-mode-host` is staged beside `codex` (`Spec::helpers`); its JavaScript reaches only the given tools. A run without programs mounts only the CA certificates, `/etc/ld.so.cache` and the libraries `ldd` lists for the agent, its helpers and the bridge, so no shell or other program under `/usr/lib` is reachable either. Codex reports a refused login only as text ("sign in again"), which `codex::summary` matches.
- Done: local models, as a Codex seat with `local: <Responses API URL>` on loopback (Ollama, LM Studio). It needs no login, reaches no provider host, and gets a tunnel to that address. Tested against a stub server; not yet against a real model. A generic `command` kind for other local-model scripts is not built: such a script needs its interpreter and libraries from outside the sandbox's system paths.
- Only Claude Code writes scenarios for now; Codex has `--output-schema`, which an author run could use.
- A killed run takes its whole process tree with it on Linux (`--unshare-pid`, `--die-with-parent`). On macOS only the main process dies, so Codex's code-mode host may outlive it; kill the process group there.
- CI has a `macos-sandbox` job that runs the sim-runner and sim-server tests on `macos-latest`. It is the first time the macOS profile runs at all.

**Phase 3: the Tauri app in `app/`.**
- Done: a Tauri 2 app (React and TypeScript with plain CSS, no UI library). `app/src-tauri` hosts one world at a time through `sim_server::host`, the same path as `sim-server run`, on a random loopback port so outside agents can still join over MCP. Commands: `list_worlds`, `open_world` (saved, scenario file, or prompt), `close_world`, `overview`, `seat_view` (the seat's real perception), `chronicle`, `intervene`, `advance_tick`. Every logged event is emitted as `world-event` and the window refreshes on it. Worlds live in `dev.empire.app/worlds/<name>.db` in the user's data directory. The scenario palette and fonts become CSS variables.
- Views today: home (new world, saved worlds), the whole world, each empire, each seat as its agent sees it, the chronicle, interventions, and seats and agents (tokens and the MCP endpoint).
- Next, as the features behind them land: the hex map coloured by owner with overlay layers, the diplomacy graph, a timeline scrubber built on the event log, and per-seat agent settings and run logs in the app.

Every phase ends with a reviewer subagent pass, as `AGENTS.md` requires.

## Running it

```sh
cargo run -p sim-server -- run --world rome --scenario scenarios/rome.yaml --tick-seconds 300
codex mcp add empire --url http://127.0.0.1:7878/mcp
claude mcp add --transport http empire http://127.0.0.1:7878/mcp
```

The server prints an admin token and one token per seat. `README.md` covers the seat tools and the admin API.
