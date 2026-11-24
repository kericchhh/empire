# Tick deadline and force-close

## How to reach it

The configured deadline and scheduler live in `crates/sim-server/src/game.rs` and `host.rs`. The admin force-close endpoint is `POST /api/tick`; MCP `end_turn` also contributes to closing a tick.

## Driving it

For deterministic manual proof, call `scripts/empire tick 1` and inspect the returned tick. To exercise a deadline, start `scripts/empire start --scenario scenarios/rome.yaml --tick-seconds 2`, join a seat with `scripts/empire tool princeps join '{}'`, wait three seconds, then query `scripts/empire chronicle`. Clean up with `scripts/empire stop`.

## Proof

Capture the tick number returned by force-close and the chronicle after close. Deadline proof should show a tick advancing after the configured timeout without all seats ending.

## Gotchas

Force-close bypasses waiting and is an admin operation. The deadline does not normally close an empty tick unless autoplay is enabled or a seat has joined or played.
