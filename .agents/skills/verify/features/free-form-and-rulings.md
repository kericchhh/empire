# Free-form actions and game-master rulings

## How to reach it

Start a scenario that defines a game-master seat (Rome has `gm`). A seat can submit a `freeform` order through `act`; the game master reads pending items in its perception and answers with `rule`. See the end-to-end example in `crates/sim-server/tests/e2e.rs`.

## Driving it

Submit `scripts/empire tool princeps act '{"tick":0,"orders":[{"kind":"freeform","character":"augustus","text":"Call a council"}]}'`, close the tick, then call `scripts/empire tool gm look '{}'`. Use the pending ruling id and the current tick in `scripts/empire tool gm rule '{"tick":1,"rulings":[{"pending_id":"<id>","outcome":"The council meets","effects":[]}]}'`.

## Proof

Capture the accepted free-form order, pending item in the game-master view, accepted ruling, and the outcome in a later view or chronicle.

## Gotchas

Only the game-master token can rule. Unruled items expire when the tick closes. Keep the text in-world; rulings are bounded by ruleset effects.
