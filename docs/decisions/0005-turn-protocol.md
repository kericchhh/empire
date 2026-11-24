# 0005 Turn protocol

Accepted 2026-11-22.

## Decision

- **Deciding:** `act`, `rule` and `end_turn` carry the `tick` the agent decided on; an old tick is refused with `StaleTick`.
- **Closing:** a tick closes when every joined seat has ended its turn, or at `--tick-seconds`. With nobody joined it waits, unless `--autoplay` is on.
- **Waiting:** the long-poll returns within 25 seconds.
- **Queued orders** live only in memory and are cleared after the commit succeeds. A restart mid-tick drops them; the README says so.

## Why

A stale decision must never land in a later tick, and a fresh agent context must be able to play from one `wait_for_turn`. Keeping the queue in memory keeps the log a list of facts, not intentions.

## Consequences

- A failed tick close leaves seats able to retry (tested with a closed store).
- A character with no region cannot default to an action that affects a region.
