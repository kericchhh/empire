---
name: verify
description: Prove an Empire change works in the running game, not only that tests pass. Use after changing engine, ruleset, perception, MCP tools, the runner or the app, and to reproduce a reported bug before fixing it.
---

# Verify

Evidence comes from the real artifact: a running world, a seat's actual view, a live run's log. "It compiles" and "the tests pass" are the floor, not the proof.

## Steps

1. **Name the claim.** Take the issue's _Done when_ lines, or for a bug, the misbehaviour. Each one becomes an observable: a chronicle line, a number in a seat's view, a refused order, a log entry.
2. **Reproduce first, for a bug.** Show the observable going _red_ on `origin/main` (or before your fix) before you show it green.
3. **Run the branch below for every area the change touches.**
4. **Record the evidence** under `## Verified` in the PR body: each command, and the few output lines that prove each claim.

Done when every claim has a command and output that shows it, and a bug fix also shows its red run.

## By area

**Engine, ruleset, perception (`sim-core`)**
- `scripts/verify-world.sh [scenario] [ticks] [seed]` runs a headless world on a throwaway database, closes the ticks, and prints the recent chronicle and every seat's `look` view over real MCP. Read the views of the seats your change affects.
- Fog of war: compare a seat of each empire. Nothing foreign beyond name, description, culture and public figures appears.
- Run the leak proptest hard, after extending it for new foreign data (`CODE_RULES.md`, Seats): `PROPTEST_CASES=2000 cargo test -p sim-core --test replay`.
- A rebalance shows in the golden snapshots (`cargo insta review`); explain every diff you accept.

**MCP tools and admin API (`sim-server`)**
- The e2e tests in `crates/sim-server/tests/e2e.rs` cover the contract. For behaviour, call the tool against `scripts/verify-world.sh`'s world: its `mcp` function shows the session handshake.
- A `wait_for_turn` result must be self-contained: read one as if you had no other context, and check that it suffices to play the turn.

**Runner and sandbox (`sim-runner`)**
- `cargo test -p sim-runner && cargo test -p sim-server --test runner`.
- A change to `HOSTS`, mounts or the environment needs a live run: `sim-server run --db <tmp> --scenario scenarios/rome.yaml --play-all claude:haiku --tick-seconds 120`, then read the gate's refusal log and the run logs under `<db>.runs/`. A live run spends the user's subscription, so ask before starting one.

**App (`app/`)**
- `pnpm --dir app exec tsc --noEmit`.
- Launch with `pnpm --dir app tauri dev` and show the screen the change affects: a screenshot, or ask the user to look when no display is available.
