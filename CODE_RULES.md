# Code rules

Apply every rule that touches the code you change. Each rule names what enforces it: a lint, a test, a CI job, or `judgment`, which the `review` skill checks and whose example shows the failure to look for. A change is finished when CI's `ci-success` would pass: `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, and `tsc --noEmit` in `app/` when `app/src` changed.

## Determinism (`sim-core`)

| Rule | Enforced by |
|---|---|
| **Replay invariant:** replaying the event log from an empty world gives the live state, and a state snapshot round-trips. | proptests `replaying_a_serialized_log_rebuilds_the_live_state`, `a_state_snapshot_round_trips_through_json` (`sim-core/tests/replay.rs`) |
| `sim-core` is pure: its inputs are state, events and a seed. Files, network, environment, wall-clock time and async live in `sim-server`. | `crates/sim-core/clippy.toml`; async: judgment, e.g. an `async fn` that awaits a timer in the engine |
| Randomness comes only from `engine::roll`, one stream per `(seed, tick, purpose)`, so a new consumer leaves the others unchanged. | `crates/sim-core/clippy.toml` (bans unseeded `rand`); test `adding_a_rule_does_not_shift_other_rules_random_draws` |
| Iterate in a stable order: `BTreeMap`, `BTreeSet` or a sorted `Vec`. | `crates/sim-core/clippy.toml` (bans `HashMap`, `HashSet`); test `close_tick_is_deterministic_and_order_independent_across_seats` |
| Use rayon only where each element is independent; reduce sequentially in a fixed order. | judgment, e.g. a parallel `f64` sum whose total changes with thread scheduling |

## Events

| Rule | Enforced by |
|---|---|
| `State::apply(&mut self, &Event)` is the only way state changes, and it never fails; validate before creating the event. | its `()` return; the replay proptest |
| Name events as past-tense facts: `GrainShortfall`, `EdictIssued`, `SeatJoined`. | judgment, e.g. `RaiseTaxes`, a command, where the fact is `TaxesRaised` |
| Every event records its cause: a seat, a rule id, the game master or a user intervention. | compiler: `Event::cause` is a required `Cause` |
| The event schema is permanent: add fields with `#[serde(default)]` and new variants freely; to change a variant's meaning, add a variant and keep the old one replayable. | judgment, e.g. turning a field from a delta into a total, so old logs replay to other numbers |

## Seats and MCP tools

| Rule | Enforced by |
|---|---|
| A tool finds its seat from the token and builds output with `perceive(&World, &State, &SeatId)`. `Sight` in `perceive.rs` alone decides visibility, and new foreign-owned state or event fields are shifted in `shift_foreign`. | proptest `no_seat_sees_anything_hidden_of_another_empire` (`sim-core/tests/replay.rs`) |
| Tool inputs and outputs derive `serde` and `schemars`. | compiler: rmcp's `Parameters` and `Json` require `JsonSchema` |
| Tool descriptions are written for an agent: what the tool does and when to call it. | judgment, e.g. "Ends the turn." with no word on what happens to queued orders |
| Every `wait_for_turn` result is self-contained: an agent with a fresh context and only that result can play the turn. | judgment (`verify` reads one), e.g. a result that says "act now" but omits the actions the office allows |
| Agent text is untrusted in-world speech: capped, and shown to another seat as a quoted message from its character. | tests `message_text_is_bounded`, `agent_text_cannot_forge_chronicle_lines`, `chat_is_capped_per_seat_per_tick` |
| Long-poll tools return "not yet, call again" within 30 seconds. | test `wait_for_turn_times_out_while_others_decide_and_wakes_when_the_tick_closes` |

## Agent runs (`sim-runner`)

| Rule | Enforced by |
|---|---|
| Start every agent process through `turn::sandbox`, and only once `check_sandbox` passes. A process that is not an agent run carries an `#[expect]` with its reason. | `crates/sim-runner/clippy.toml` (bans `Command::new`); the `check_sandbox` gate: judgment, e.g. a new run path that skips the check `host.rs` and `author.rs` make |
| A run's network is the gate's (`net`). An agent kind's `HOSTS` lists only the hosts a live run's refusal log confirmed. | tests `a_run_reaches_the_world_and_its_provider_only`, `the_proxy_joins_admitted_hosts_only`; the host list: judgment, e.g. a whole `*.cdn` domain added on a guess |
| Nothing of the user reaches a run: no host environment variable beyond the sandbox's list, no host path, no file, no account detail. A new agent kind or flag needs a test that proves it. | tests in `sandbox.rs`, `a_run_gets_only_the_turn_tools_and_nothing_of_the_users`, `runs_are_sandboxed_with_only_the_login_and_private_logs` |
| A run knows only its own seat token, on stdin, never on the command line. | test `the_token_travels_in_the_prompt_never_on_the_command_line` |
| The only credentials Empire stores are the user's agent logins, in its data directory, readable only by the user; a run gets a copy, never a mount. | tests `a_saved_token_is_private_and_reads_back`, `runs_are_sandboxed_with_only_the_login_and_private_logs` |
| Run output and logs are private (`fs::private_dir`, `fs::private_file`): they hold seat tokens. | test `runs_are_sandboxed_with_only_the_login_and_private_logs` |

## Scenario-agnostic code

| Rule | Enforced by |
|---|---|
| Rust and TS code names offices, metrics, resources and regions by ids from the ruleset and scenario; setting names and lore live in `scenarios/`. | judgment, e.g. `if office == "consul"` |
| A new mechanic is a generic rule, action or office in `rules/standard.yaml`, or engine code; scenarios carry no mechanics. | `scenarios` CI job: the scenario schema has no rule fields and rejects unknown ones |

## Rust style

| Rule | Enforced by |
|---|---|
| Edition 2024, stable toolchain; shared dependencies sit in `[workspace.dependencies]`, added only to the crate that uses them. | judgment, e.g. a crate pinning its own `serde = "1"` |
| Newtype ids (`SeatId`, `RegionId`, `Tick`) at every boundary. | judgment, e.g. `fn seat(id: &str)` on a public API |
| Library crates return `thiserror` enums; `anyhow` only in binary `main`s. | `deny.toml`, `crates/sim-server/clippy.toml` |
| Outside tests, handle every `Result`; where an invariant guarantees success, `expect("<why this holds>")`. | rustc `unused_must_use` under `-D warnings`; the rest judgment, e.g. `let _ = store.append(&e)` dropping a failed write |
| Log with `tracing`, one span per tick and per seat request. | judgment, e.g. `println!` in the tick loop |

## Comments

| Rule | Enforced by |
|---|---|
| Write almost none: only what the code cannot say (a reason, an invariant, a non-obvious constraint), in one short line. Doc comments describe behaviour the signature does not show. | judgment, e.g. `// increment the tick` above `tick += 1` |
| CI and other YAML files carry no comments, except the `# vX.Y.Z` beside a SHA-pinned action. | judgment, e.g. `# install deps` above a step |

## Tests

| Rule | Enforced by |
|---|---|
| Every feature, public API, service, MCP tool and HTTP endpoint ships with tests of the outcomes a user or agent relies on, rejections and edge cases included; a test earns its place by failing when that breaks. | judgment (the `slice` red step), e.g. a test that only checks the call returns `Ok` |
| Unit tests sit next to the code they test. | judgment, e.g. a private helper tested from `tests/` through a `pub` added for it |
| MCP tools and HTTP endpoints are tested end to end through a real client against a server on an ephemeral port. | judgment, e.g. calling the handler function directly; `sim-server/tests/e2e.rs` is the pattern |
| Golden runs: an `insta` snapshot of the event log after N ticks of a fixture scenario; accept a diff only when the change explains it. | test `rome_and_parthia_left_to_the_rules_alone`; `test` CI job fails on pending snapshots |

## App (`app/`)

| Rule | Enforced by |
|---|---|
| The Rust backend (`app/src-tauri`) exposes Tauri commands that call `sim_server` and `sim_runner`, and holds no game logic. | judgment, e.g. a command that computes a metric itself |
| The UI loads state through those commands and refreshes on `world-event`; user actions go through commands such as `intervene`, each becoming an event on the server. | judgment, e.g. the UI editing its copy of the state without an event |
| The TS types for server data live in `app/src/api.ts`, kept in step with the Rust types they mirror. | judgment (until #39), e.g. a Rust field renamed while `api.ts` keeps the old name |
| Components style themselves through the CSS variables from the scenario theme. | judgment, e.g. a hard-coded `#c9a227` gold |
