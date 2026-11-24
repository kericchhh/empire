# Turn loop

## How to reach it

Run `scripts/empire start`. Each scenario seat is listed in its JSON output. The MCP handlers are in `crates/sim-server/src/mcp.rs`; turn state and deadlines are in `crates/sim-server/src/game.rs`.

## Driving it

Call `join` once with `{}` for your seat, then `wait_for_turn` when `status` is waiting, `look` to refresh perception, `act` with the current tick, and `end_turn` with that tick. After a close, wait again. For example: `scripts/empire tool princeps join '{}'`.

## Proof

Capture `join` or `wait_for_turn` output and then `end_turn`; confirm the returned status/tick and a later turn's perception. `scripts/empire views` reads every seat via `look`.

## Gotchas

The call to `end_turn` closes the tick only after all joined seats end it. `wait_for_turn` may return `waiting`; call again.
