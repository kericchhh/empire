# Appointments

## How to reach it

Start `scenarios/rome.yaml`. Seat `princeps` plays Augustus (head of state), and seat `senate` plays Piso and Sestius. The standard rules define `appoint` for the head of state in `crates/sim-core/rules/standard.yaml`; an existing MCP e2e example is in `crates/sim-server/tests/e2e.rs`.

## Driving it

Ask the head of state for the available choices, appoint Sestius as commander, close the tick, then inspect the senate seat:

```sh
scripts/empire tool princeps look '{}'
scripts/empire tool princeps act '{"tick":0,"orders":[{"kind":"action","character":"augustus","action":"appoint","target":"sestius","choice":"commander"}]}'
scripts/empire tick 1
scripts/empire tool senate look '{}'
```

## Proof

Save the `act` result showing acceptance and the senate `look` result. Sestius should have `office: Commander`, and recent events should say he becomes Commander. `scripts/empire chronicle` captures the public chronicle.

## Gotchas

Use the action choices from perception, not display labels guessed from the setting. The default seed and scenario start at tick 0. If random state or a prior order changes the tick, use the current `tick` from `look`.
