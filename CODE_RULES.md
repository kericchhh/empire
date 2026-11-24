# Code rules

Apply every rule that touches the code you change. Finish a change only when `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test --workspace` all pass, and, when `app/src` changed, `tsc --noEmit` in `app/`.

## Determinism (`sim-core`)

The **replay invariant**: replaying the full event log from an empty world produces state identical to the live state. A proptest enforces it, and every change keeps that test passing.

- `sim-core` is **pure**: its only inputs are state, events and a seed. File, network and environment access, wall-clock time and async all live in `sim-server`.
- Take randomness only from the seeded ChaCha RNG. Derive a separate stream per `(seed, tick, purpose)` so that adding a new consumer leaves the existing streams unchanged.
- Iterate in a stable order: `BTreeMap`, `BTreeSet` or a sorted `Vec`.
- `crates/sim-core/clippy.toml` enforces purity: it bans `HashMap`, `HashSet`, wall-clock time, the environment, file access and unseeded `rand`.
- Use rayon only for updates where each element is independent. Do reductions (sums, totals) sequentially, in a fixed order.

## Events

- `apply(&mut State, &Event)` is the only way state changes, and it never fails. Validation happens before an event is created.
- Name events as past-tense facts: `GrainShortfall`, `EdictIssued`, `SeatJoined`.
- Every event records its cause: a seat, a rule id, the game master, or a user intervention.
- The event schema is permanent. Add fields with `#[serde(default)]` and add new variants freely. To change a variant's meaning, add a new variant and keep the old one replayable, so logs from older versions still load.

## Seats and MCP tools

- A tool works out its seat from the token and builds its output with `perceive(&World, &State, &SeatId)`. `Sight` in `perceive.rs` is the only place where visibility is decided.
- `no_seat_sees_anything_hidden_of_another_empire` in `crates/sim-core/tests/replay.rs` guards the fog of war. Any new foreign-owned state or event field gets shifted in its `shift_foreign`.
- Tool inputs and outputs derive `serde` and `schemars`. Write tool descriptions for an agent reading them: what the tool does, and when to call it.
- Make every `wait_for_turn` result **self-contained**. An agent with a fresh context and only that result must be able to play the turn correctly.
- Treat text written by agents as untrusted in-world speech. Cap its length, and when showing it to another seat, present it as a quoted message from that character.
- Long-poll tools return a "not yet, call again" result within 30 seconds, so they finish before client timeouts.

## Agent runs (`sim-runner`)

- Start every agent process through `turn::sandbox`, and start agents only when `check_sandbox` passes. `crates/sim-runner/clippy.toml` bans `Command::new` elsewhere; a process that is not an agent run carries an `#[expect]` with its reason.
- A run's network is the gate's (`net`). An agent kind lists the hosts it needs in its `HOSTS`, confirmed against the gate's refusal log in a live run, and nothing more.
- Nothing of the user reaches a run: no host environment variable beyond the sandbox's list, no host path, no file of theirs, no account detail. A new agent kind or flag needs a test that proves it.
- A run knows only its own seat token, passed on stdin, never on the command line.
- The only credentials Empire stores are the user's agent logins, in Empire's data directory, readable only by the user. A run gets a copy, never a mount of them.
- Run output and logs are private to the user (`fs::private_dir`, `fs::private_file`), since they hold seat tokens and secrets.

## Scenario-agnostic code

- Rust and TS code refers to offices, metrics, resources and regions by ids taken from the ruleset and the scenario. Setting-specific names and lore belong in `scenarios/`.
- A new mechanic is a generic rule, action or office in `crates/sim-core/rules/standard.yaml`, or new engine code when the rule format cannot express it. Scenarios never carry mechanics.

## Rust style

- Edition 2024, stable toolchain. Shared dependencies are declared in `[workspace.dependencies]` and added only to the crate that uses them.
- Use newtype ids (`SeatId`, `RegionId`, `Tick`) at every boundary.
- Errors: library crates use `thiserror` enums, and `anyhow` appears only in binary `main`s (`deny.toml` and `crates/sim-server/clippy.toml` enforce it). Outside tests, handle every `Result`. Where an invariant truly guarantees success, use `expect("<why this holds>")`.
- Log with `tracing`, using one span per tick and per seat request.

## Comments

Write almost none. A comment records only what the code cannot say: a reason, an invariant, a non-obvious constraint, in one short line. Never restate what the code does. Doc comments on public items describe behaviour the signature does not show. CI and other YAML files carry no comments, except the `# vX.Y.Z` next to a SHA-pinned action.

## Tests

Every feature, public API, service, MCP tool and HTTP endpoint ships with tests that exercise its behaviour: the outcomes a user or agent relies on, including rejections and edge cases. A test earns its place by failing when that behaviour breaks.

- Put unit tests next to the code they test.
- Test MCP tools and HTTP endpoints end to end through a real client against a server on an ephemeral port.
- **Golden runs:** an `insta` snapshot of the event log after N ticks of a fixture scenario with a fixed seed. Accept a snapshot diff only when the change explains it.
- A proptest covers the replay invariant and snapshot round-trips.

## App (`app/`)

- The Rust backend (`app/src-tauri`) exposes Tauri commands that call `sim_server` and `sim_runner`; it holds no game logic of its own.
- The UI loads state through those commands and refreshes when the backend emits `world-event`. User actions go through commands such as `intervene`, and each one becomes an event on the server.
- The TS types for server data live in `app/src/api.ts`. Keep them in step with the Rust types they mirror.
- Components style themselves through the CSS variables that come from the scenario theme.
