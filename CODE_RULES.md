# Code rules

Apply every rule that touches the code you change. Finish a change only when `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test --workspace` all pass.

## Determinism (`sim-core`)

The **replay invariant**: replaying the full event log from an empty world produces state identical to the live state. A proptest enforces it, and every change keeps that test passing.

- `sim-core` is **pure**: its only inputs are state, events and a seed. File, network and environment access, wall-clock time and async all live in `sim-server`.
- Take randomness only from the seeded ChaCha RNG. Derive a separate stream per `(seed, tick, purpose)` so that adding a new consumer leaves the existing streams unchanged.
- Iterate in a stable order: `BTreeMap`, `IndexMap` or sorted `Vec`. A `HashMap` is only for lookups, never for iteration inside the simulation.
- Use rayon only for updates where each element is independent. Do reductions (sums, totals) sequentially, in a fixed order.

## Events

- `apply(&mut State, &Event)` is the only way state changes, and it never fails. Validation happens before an event is created.
- Name events as past-tense facts: `GrainShortfall`, `EdictIssued`, `SeatJoined`.
- Every event records its cause: a seat, a rule id, the game master, or a user intervention.
- The event schema is permanent. Add fields with `#[serde(default)]` and add new variants freely. To change a variant's meaning, add a new variant and keep the old one replayable, so logs from older versions still load.

## Seats and MCP tools

- A tool works out its seat from the token and builds its output with `perceive(&State, SeatId)`. That function is the only place where visibility is decided.
- Tool inputs and outputs derive `serde` and `schemars`. Write tool descriptions for an agent reading them: what the tool does, and when to call it.
- Make every `wait_for_turn` result **self-contained**. An agent with a fresh context and only that result must be able to play the turn correctly.
- Treat text written by agents as untrusted in-world speech. Cap its length, and when showing it to another seat, present it as a quoted message from that character.
- Long-poll tools return a "not yet, call again" result within 30 seconds, so they finish before client timeouts.

## Scenario-agnostic code

- Rust and TS code refers to roles, metrics, resources and regions by ids taken from the scenario. Setting-specific names and lore belong in `scenarios/`.
- A new mechanic is either a generic Rust rule configured by the scenario or a Rhai script the scenario ships with. Rhai runs sandboxed, with operation and memory limits.

## Rust style

- Edition 2024, stable toolchain. Shared dependencies are declared in `[workspace.dependencies]` and added only to the crate that uses them.
- Use newtype ids (`SeatId`, `RegionId`, `Tick`) at every boundary.
- Errors: library crates use `thiserror` enums, and `anyhow` appears only in binary `main`s. Outside tests, handle every `Result`. Where an invariant truly guarantees success, use `expect("<why this holds>")`.
- Log with `tracing`, using one span per tick and per seat request.

## Comments

Write a comment only to record what the code cannot say: a reason, an invariant, a non-obvious constraint. Doc comments on public items describe behaviour the signature does not show.

## Tests

Every feature, public API, service, MCP tool and HTTP endpoint ships with tests that exercise its behaviour: the outcomes a user or agent relies on, including rejections and edge cases. A test earns its place by failing when that behaviour breaks.

- Put unit tests next to the code they test.
- Test MCP tools and HTTP endpoints end to end through a real client against a server on an ephemeral port.
- **Golden runs:** an `insta` snapshot of the event log after N ticks of a fixture scenario with a fixed seed. Accept a snapshot diff only when the change explains it.
- A proptest covers the replay invariant and snapshot round-trips.

## Web

- The UI loads one snapshot over REST, then applies the WebSocket event stream to its local copy. User actions go through intervention endpoints, and each one becomes an event on the server.
- Import types from the files `ts-rs` generates. Those files are the only TS definition of server types.
- Components style themselves through the CSS variables that come from the scenario theme.
