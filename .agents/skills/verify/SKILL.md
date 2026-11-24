---
name: verify
description: Prove an Empire change works in the running game, not only that tests pass. Use after changing engine, ruleset, perception, MCP tools, the runner or the app, and to reproduce a reported bug before fixing it.
---

# Verify

Evidence comes from the real artifact: a running world, a seat's actual view, a chronicle entry or a live run's log. "It compiles" and "the tests pass" are the floor, not the proof.

## Launch

`scripts/empire` needs `cargo`, `curl` and `jq`; `scripts/empire --help` lists its commands.

```sh
scripts/empire start --scenario scenarios/rome.yaml --seed 2026
```

## Doctor

Check the world before driving it:

```sh
scripts/empire doctor
```

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

Save command output in a named directory outside the repository; it survives `stop`, which removes only the state directory.

```sh
evidence="${TMPDIR:-/tmp}/empire-evidence/appointments"
mkdir -p "$evidence"
scripts/empire tool princeps act '{"tick":0,"orders":[{"kind":"action","character":"augustus","action":"appoint","target":"sestius","choice":"commander"}]}' | tee "$evidence/appointment.json"
scripts/empire tick 1 | tee "$evidence/tick.json"
scripts/empire tool senate look '{}' | tee "$evidence/appointee.json"
```

## Cleanup

```sh
scripts/empire stop
rm -rf "$evidence"
```

Delete the evidence directory once its key lines are in the PR's `## Verified` section. Before reporting done, `ps -eo pid,args | grep '[d]ebug/sim-server'` shows no world you started.

## Steps

1. Name each claim from the issue's _Done when_ lines, or the reported misbehaviour, as an observable.
2. For a bug, show the observable going red on `origin/main` (or before the fix), then green.
3. Record each command and the few output lines that prove each claim under `## Verified`.

## By area

**Engine, ruleset, perception (`sim-core`)**
- Read the views of the seats your change affects.
- Run the leak proptest hard, after extending it for new foreign data (`CODE_RULES.md`, Seats): `PROPTEST_CASES=2000 cargo test -p sim-core --test replay`.
- A rebalance shows in the golden snapshots (`cargo insta review`); explain every diff you accept.

**MCP tools and admin API (`sim-server`)**
- The e2e tests in `crates/sim-server/tests/e2e.rs` cover the contract. For behaviour, use `scripts/empire tool`.

**Runner and sandbox (`sim-runner`)**
- `cargo test -p sim-runner && cargo test -p sim-server --test runner`.
- The [agent runs and sandbox guide](features/agent-runs-and-sandbox.md) describes live proof. A live run spends the user's subscription and needs the user's OK; do not start it otherwise.

**App (`app/`)**
- `pnpm --dir app exec tsc --noEmit`.
- Launch with `pnpm --dir app tauri dev` and show the screen the change affects: a screenshot, or ask the user to look when no display is available. The [app screens guide](features/app-screens.md) maps screens to code.
