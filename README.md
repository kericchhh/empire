# Empire

A sandbox for simulating an empire's whole life: politics, war, economy, culture and daily life. You describe the world in a scenario file. Agents from **any harness** (Codex, Claude Code, any MCP client) each take a seat and play its characters, and the world rises or falls from what they do.

The server never calls a model. Agents connect to it, the same way a player joins a multiplayer game.

## Run a world

```sh
cargo run -p sim-server -- run --db rome.db --scenario scenarios/rome.yaml --tick-seconds 300
```

On first start the server creates the world and prints one **admin token** and one **token per seat**. Restart it with the same `--db` to resume the world exactly where it stopped. After that, `--scenario` isn't needed.

| Flag | Meaning |
|---|---|
| `--tick-seconds N` | Close a tick after N seconds, even if some seats are still deciding (default 300). |
| `--autoplay` | Advance on the deadline even when no seat has joined. The rules run the world alone. |
| `--seed N` | Seed for random events. The same seed and the same orders give the same history. |
| `--bind ADDR` | Listen address (default `127.0.0.1:7878`). |
| `--allowed-host NAME` | Accept MCP requests addressed to this hostname. Needed when agents connect from another machine. |

Other commands: `sim-server check <scenario>` validates a file, and `sim-server schema` prints the JSON Schema that `schema/scenario.schema.json` holds.

## Connect an agent to a seat

Register the server once in each harness:

```sh
codex mcp add empire --url http://127.0.0.1:7878/mcp
claude mcp add --transport http empire http://127.0.0.1:7878/mcp
```

Then give the agent its seat token and a prompt like:

> You are playing the seat with token `<token>` in the Empire simulation (MCP server `empire`). Call `join`, then loop: `wait_for_turn`, read your perception, use `message` and `note` if useful, `act`, `end_turn`. Keep going until the status is `world_ended`.

### Seat tools

| Tool | Purpose |
|---|---|
| `join` | Claim the seat. Returns your characters, goals, secrets, actions and what you can see. |
| `wait_for_turn` | Long-poll (up to 25 s). Returns `your_turn` with fresh perception, `waiting`, or `world_ended`. |
| `look` | Re-read your perception mid-tick, e.g. after a message arrives. |
| `act` | Queue orders: a listed action, or a free-form attempt for the game master to rule on. Each order is accepted or rejected with a reason. |
| `rule` | Game master only: rule on pending free-form actions, with effects bounded by the scenario. |
| `message` | A private message from one of your characters to another character. |
| `note` | A private memory, shown back to you every turn. |
| `end_turn` | Finish the tick. It closes when every joined seat has ended its turn, or at the deadline. |

`act`, `rule` and `end_turn` take the `tick` from the perception the agent decided on. If the world has moved on, they're refused with a request to call `wait_for_turn`, so stale decisions never land in a later tick. Every result is self-contained, so an agent restarted with a fresh context can carry on from a single `wait_for_turn`.

Orders queued during a tick live in memory until the tick closes. If the server restarts mid-tick, they're lost and the seats re-submit. Everything already resolved is in the event log.

## Watch and intervene

The admin API needs `Authorization: Bearer <admin token>`:

```sh
curl -H "Authorization: Bearer $ADMIN" localhost:7878/api/overview          # full state + chronicle
curl -H "Authorization: Bearer $ADMIN" "localhost:7878/api/events?after=0"   # the event log
curl -X POST -H "Authorization: Bearer $ADMIN" localhost:7878/api/tick       # close the tick now
curl -X POST -H "Authorization: Bearer $ADMIN" -H 'content-type: application/json' \
  localhost:7878/api/intervene \
  -d '{"title":"A plague ship docks at Ostia","effects":[{"target":{"stat":{"region":"italia","stat":"unrest"}},"delta":0.2}]}'
```

## Write a scenario

A scenario defines the calendar, culture, theme, metrics, regions, roles, characters, seats, actions, rules and endings. Start from `scenarios/rome.yaml`, and point your editor at `schema/scenario.schema.json` for autocomplete. Effects, rules and endings are plain data, so the same engine runs any setting.

## Develop

```sh
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test --workspace
```

See `AGENTS.md` for the design and `CODE_RULES.md` for the code rules.
