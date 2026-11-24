# Empire

A sandbox for simulating empires sharing one world: politics, war, economy, culture and daily life. You describe the starting point, in a sentence or in a scenario file: the empires, their cultures and their people. Agents from **any harness** (Codex, Claude Code, any MCP client) each take a seat and play its characters, and the world rises or falls from what they do.

The server never calls a model. Agents connect to it, the same way a player joins a multiplayer game.

## The desktop app

```sh
cd app && pnpm install && pnpm tauri dev
```

Create a world by describing it or from a scenario file, and let agents play every seat. Then watch the whole world, any one empire, or exactly what any one seat's agent sees. Read the chronicle, advance a tick by hand, inject events, and copy a seat's token to connect an outside agent. Worlds are saved in Empire's data directory on your machine (`~/.local/share/dev.empire.app/worlds` on Linux, `~/Library/Application Support/dev.empire.app/worlds` on macOS) and reopen where they stopped.

On Linux the app needs WebKitGTK 4.1 (`libwebkit2gtk-4.1-dev` on Debian and Ubuntu, `webkit2gtk-4.1` on Arch).

## Run a world headless

Empire runs on Linux and macOS. On Linux, agent runs are sandboxed with bubblewrap: install `bubblewrap` (on Ubuntu 24.04 and later, also allow unprivileged user namespaces for it). macOS uses its built-in `sandbox-exec`.

Connect Claude Code once. Run `claude setup-token`, which prints a long-lived token for your Claude subscription, then save it for Empire:

```sh
claude setup-token
cargo run -p sim-server -- login
```

The token lasts a year. When it expires or is revoked, the server log and the app say so; run both commands again.

To play with Codex instead, log Empire's own Codex home in once, in the browser window it opens. Empire needs the standalone Codex install (`codex` with `codex-code-mode-host` beside it), not an npm one. This login is kept apart from your own Codex and uses your ChatGPT subscription:

```sh
cargo run -p sim-server -- login codex
```

Describe the world you want, and let Claude Code write it and play every seat:

```sh
cargo run -p sim-server -- run --world delta \
  --prompt "Three rival river city-states in a bronze-age delta: priest-kings, merchants, warriors" \
  --play-all claude:haiku --tick-seconds 900
```

Your agent writes the scenario (the setting only: no rules, no endings), the engine checks it, and a rejected draft goes back to the agent with the reasons, for up to three attempts in all. The result is saved beside the world as `delta.scenario.yaml`, so you can read it, edit it, or start it again. `--author claude:<model>` picks the model that writes it. To write one without running it: `sim-server create --prompt "..." --out world.yaml` (its run logs go to `world.runs/`).

Or run a scenario file you wrote or saved:

```sh
cargo run -p sim-server -- run --world rome --scenario scenarios/rome.yaml --tick-seconds 300
```

`--world NAME` keeps the world in Empire's data directory, where the desktop app finds it too; `--db FILE` puts it in a file of your choosing instead. On first start the server creates the world and prints one **admin token** and one **token per seat**. Restart it with the same `--world` to resume the world exactly where it stopped. After that, `--scenario` isn't needed.

| Flag | Meaning |
|---|---|
| `--world NAME` | The world, kept in Empire's data directory. Or `--db FILE` for a file of your choosing. |
| `--runs DIR` | Where run logs go (default: `<world>.runs` beside the world). |
| `--tick-seconds N` | Close a tick after N seconds, even if some seats are still deciding (default 300). |
| `--autoplay` | Advance on the deadline even when no seat has joined. The rules run the world alone. |
| `--seed N` | Seed for random events. The same seed and the same orders give the same history. |
| `--bind ADDR` | Listen address (default `127.0.0.1:7878`). |
| `--prompt TEXT` | Create the world from a description instead of `--scenario`. New databases only. |
| `--play-all AGENT` | One agent plays every seat but the game master's: `claude`, `codex`, or either with `:<model>`. |
| `--agents FILE` | Seats that a local agent CLI plays, and how. See below. |
| `--allowed-host NAME` | Accept MCP requests addressed to this hostname. Needed when agents connect from another machine. |

Other commands: `sim-server login` saves the Claude login, `sim-server login codex` logs Codex in, `sim-server check <scenario>` validates a file, and `sim-server schema` prints the JSON Schema that `schema/scenario.schema.json` holds.

## Let Claude Code or Codex play the seats

One subscription can play any number of seats, in one empire or in rival ones. `--play-all claude:haiku` gives every seat the same agent. For a mix, use an agents file; `all` covers every seat it doesn't list, so the same file fits any scenario:

```sh
cargo run -p sim-server -- run --world rome --scenario scenarios/rome.yaml --agents agents/rome-claude.yaml
```

```yaml
max_concurrent: 2          # agent runs at once, across all seats
turn_timeout_seconds: 240
all: { claude: { model: haiku } }
seats:                     # optional overrides by seat id
  princeps: { claude: { model: sonnet } }
  parthia: { codex: { effort: low } }
  armenia: { codex: { local: "http://127.0.0.1:11434/v1", model: "gpt-oss:20b" } }
```

A Codex seat with `local` plays on a model server on this machine instead of your ChatGPT login: anything that speaks the OpenAI Responses API, such as Ollama (`http://127.0.0.1:11434/v1`) or LM Studio (`http://127.0.0.1:1234/v1`). It needs no login.

Each turn, every listed seat gets a fresh headless run (`claude -p` or `codex exec`) that knows only its own seat token. Seats of rival empires on the same login stay apart, and nothing of yours reaches them:

- the run is sandboxed by the operating system with an empty home and an empty working directory. Your files, the world database, the run logs, your own agent settings, history and `CLAUDE.md` or `AGENTS.md`, and any git repository around the world are out of its reach. Empire needs the native Claude Code install (`claude install`), not an npm one;
- its network reaches only its provider (`api.anthropic.com` for Claude Code, `chatgpt.com` and `auth.openai.com` for Codex), the world's MCP server and its local model server, if it has one. Your local network and every other site are unreachable;
- it logs in with Empire's own login, never your own. Claude Code's non-essential traffic is off, so it never fetches your account profile: your name and email don't reach the model;
- it has no built-in tools, so no files and no shell, only the world's MCP tools for one turn (`look`, `act`, `rule`, `message`, `note`, `end_turn`). A Codex run also sees no system programs, so even a command it tried would have nothing to run. It loads no settings files and keeps no session;
- what a character remembers between turns is its `note`s.

When a run finishes, times out, or its tick closes on the deadline, the runner stops it and ends the seat's turn. Each run's output is added to `<world>.runs/<seat>/tick-<n>.jsonl` beside the world, readable only by you since it holds the seat's token (change the location with `--runs DIR`). Runs queue for `max_concurrent` slots, so keep `ceil(seats / max_concurrent) × turn_timeout_seconds` below `--tick-seconds`; the server warns when it isn't. The game master seat can't be given to the runner yet, because it would rule before the other seats act.

Seats you leave out work as before: connect an agent yourself, or leave them to the rules engine.

## Connect an agent to a seat yourself

Register the server once in each harness:

```sh
codex mcp add empire --url http://127.0.0.1:7878/mcp
claude mcp add --transport http empire http://127.0.0.1:7878/mcp
```

Then give the agent its seat token and a prompt like:

> You are playing the seat with token `<token>` in the Empire simulation (MCP server `empire`). Call `join`, then loop: `wait_for_turn`, read your perception, use `message` and `note` if useful, `act`, `end_turn`. The world has no ending: keep playing until the server stops.

### Seat tools

| Tool | Purpose |
|---|---|
| `join` | Claim the seat. Returns your characters, goals, secrets, actions and what you can see. |
| `wait_for_turn` | Long-poll (up to 25 s). Returns `your_turn` with fresh perception, or `waiting`. |
| `look` | Re-read your perception mid-tick, e.g. after a message arrives. |
| `act` | Queue orders: a listed action, or a free-form attempt for the game master to rule on. Each order is accepted or rejected with a reason. |
| `rule` | Game master only: rule on pending free-form actions, with effects bounded by the ruleset. |
| `message` | A private message from one of your characters to anyone in your empire, or to a foreign head of state. |
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
  -d '{"title":"A plague ship docks at Ostia","empire":"rome","effects":[{"target":{"stat":{"region":"italia","stat":"unrest"}},"delta":0.2}]}'
```

`empire` is optional: with it, only that empire's seats hear of the event.

Empires know only the empires they have met: those the scenario lists under `contacts`, and those an intervention introduces. Only the two empires hear of it:

```sh
curl -X POST -H "Authorization: Bearer $ADMIN" -H 'content-type: application/json' \
  localhost:7878/api/introduce -d '{"a":"rome","b":"parthia"}'
```

## Write a scenario

`--prompt` writes one for you. To write or edit one by hand: a scenario is the starting point and nothing more: the calendar, the theme, the empires with their culture and regions, the characters with their persona, goals, secrets and the office they hold, the seats that play them, and which empires already know each other (`contacts`). Start from `scenarios/rome.yaml`, and point your editor at `schema/scenario.schema.json` for autocomplete.

Starting numbers are optional. Set them under `start` on an empire, region or character; anything you leave out is drawn from the seed. The mechanics (numbers, offices, actions, rules) come from the engine's standard ruleset in `crates/sim-core/rules/standard.yaml`, the same for every setting. There are no endings: the world runs until you stop it.

## Develop

```sh
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test --workspace
```

See `AGENTS.md` for the design and `CODE_RULES.md` for the code rules.
