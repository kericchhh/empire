# Agent runs and sandbox

## How to reach it

Agent launch and isolation are owned by `crates/sim-runner/src/turn.rs` and `sandbox.rs`; the host configures seat runs in `crates/sim-server/src/host.rs`. Sandbox checks, provider hosts and run logs are documented in `README.md` and `CODE_RULES.md`.

## Driving it

The live route is `cargo run -p sim-server -- run --db "$EMPIRE_STATE_DIR/agent.db" --scenario scenarios/rome.yaml --play-all claude:haiku --tick-seconds 120`. To test a single configured seat, use the corresponding `--agents` option described by `cargo run -p sim-server -- run --help`.

## Proof

Inspect the agent run log under `$EMPIRE_STATE_DIR/agent.runs/` and the sandbox refusal log; confirm the seat's own MCP actions and that denied paths or hosts remain denied. Automated baseline: `cargo test -p sim-runner && cargo test -p sim-server --test runner`.

## Gotchas

Live agent runs spend the user's subscription. Get the user's OK before running `--play-all` or `--agents`. Do not place credentials in commands, evidence or logs. Remove the agent database and its `.runs` directory during cleanup.
