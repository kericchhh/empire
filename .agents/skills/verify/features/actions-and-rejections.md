# Actions and rejections

## How to reach it

Use an ordinary seat in a started world. Available actions and limits are included in the character's perception; validation and resolution are handled by `act` in `crates/sim-server/src/game.rs`.

## Driving it

Call `scripts/empire tool princeps look '{}'`, then submit an action and an invalid action with `scripts/empire tool princeps act '<JSON object>'`. Use the `tick` returned by perception and the exact action, target and choice values it lists. Inspect the append-only log with `GET /api/events?after=0&limit=500`; its admin token is stored privately at `$EMPIRE_STATE_DIR/admin` (default `/tmp/empire-control-$UID/admin`).

## Proof

Keep the `act` result showing `accepted` and the rejected result's reason. Close the tick and save `scripts/empire chronicle`; the resolution should appear in the chronicle or event log.

## Gotchas

Accepted orders are queued until the tick closes. A stale tick, missing choice, unavailable action or exhausted order limit is rejected. Rejections are expected tool results, not MCP protocol failures.
