---
name: verify
description: Prove an Empire change works in the running game, not only that tests pass. Use after changing engine, ruleset, perception, MCP tools, the runner or the app, and to reproduce a reported bug before fixing it.
---

# Verify

Evidence comes from the real artifact: a running world, a seat's actual view, a chronicle entry or a live run's log. "It compiles" and "the tests pass" are the floor, not the proof.

## Launch

`scripts/empire` needs `cargo`, `curl` and `jq`. It starts a headless server with a throwaway database and stores its PID and tokens in a private temporary state directory.

```sh
scripts/empire start --scenario scenarios/rome.yaml --seed 2026
```

The JSON response prints the state directory. Set `EMPIRE_STATE_DIR` on every command to choose a different path; within this checkout the default is stable, so it is usually unnecessary.

## Doctor

Run this single read-only check before driving a world. It verifies the server process, an MCP initialize request and an authenticated admin overview request.

```sh
scripts/empire doctor
```

Healthy output includes `"status":"healthy"`, `"mcp":"answered"` and `"admin":"authorized"`.

## Drive

Use `scripts/empire tool <seat> <name> <json>` for MCP calls. Force-close ticks through the admin API, then inspect the chronicle or every seat's actual `look` view:

```sh
scripts/empire tool princeps look '{}'
scripts/empire tick 1
scripts/empire chronicle
scripts/empire views
```

The default world uses `scenarios/rome.yaml`; use another scenario when a feature needs different setup. The [feature map](features/README.md) gives a concrete route, proof and gotchas for each feature.

## Evidence

Create a named directory outside the repository and world state, then save command output there. The user's `/tmp` is RAM-backed; evidence survives `stop` because cleanup only removes the state directory.

```sh
evidence="${TMPDIR:-/tmp}/empire-evidence/appointments"
mkdir -p "$evidence"
scripts/empire tool princeps act '{"tick":0,"orders":[{"kind":"action","character":"augustus","action":"appoint","target":"sestius","choice":"commander"}]}' | tee "$evidence/appointment.json"
scripts/empire tick 1 | tee "$evidence/tick.json"
scripts/empire tool senate look '{}' | tee "$evidence/appointee.json"
```

In a PR, include the commands and output lines that prove each claim under `## Verified`.

## Cleanup

```sh
scripts/empire stop
test -d "$evidence" && echo 'evidence preserved'
rg -n 'accepted|Commander' "$evidence" >> /tmp/empire-verified-lines.txt
rm -rf "$evidence"
```

`stop` terminates only the recorded CLI server and removes its state directory. If the process already stopped, it still removes stale state. Delete the evidence directory after its key lines are in the PR's `## Verified` section.

## Steps

1. Name each claim from the issue's _Done when_ lines, or the reported misbehaviour, as an observable.
2. For a bug, show the observable going red on `origin/main` (or before the fix), then green.
3. Record each command and the few output lines that prove each claim under `## Verified`.

## By area

**Engine, ruleset, perception (`sim-core`)**
- `scripts/empire start`, `tick`, `chronicle` and `views` run a disposable world, close ticks through the admin API, and read every seat through MCP. Read the views of seats your change affects.
- Run the leak proptest hard, after extending it for new foreign data (`CODE_RULES.md`, Seats): `PROPTEST_CASES=2000 cargo test -p sim-core --test replay`.
- A rebalance shows in the golden snapshots (`cargo insta review`); explain every diff you accept.

**MCP tools and admin API (`sim-server`)**
- The e2e tests in `crates/sim-server/tests/e2e.rs` cover the contract. For behaviour, use `scripts/empire tool <seat> <name> <json>`; the CLI performs the MCP session handshake.

**Runner and sandbox (`sim-runner`)**
- `cargo test -p sim-runner && cargo test -p sim-server --test runner`.
- The [agent runs and sandbox guide](features/agent-runs-and-sandbox.md) describes live proof. A live run spends the user's subscription and needs the user's OK; do not start it otherwise.

**App (`app/`)**
- `pnpm --dir app exec tsc --noEmit`.
- Launch with `pnpm --dir app tauri dev` and show the screen the change affects: a screenshot, or ask the user to look when no display is available. The [app screens guide](features/app-screens.md) maps screens to code.
