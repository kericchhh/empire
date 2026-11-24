# Feature map

Each guide starts from a running disposable world (`scripts/empire start`) and gives a way to drive the feature and inspect its observable result. The CLI maps seats to their tokens and exposes all eight MCP tools. Admin endpoints are authenticated with the stored admin token.

| Feature | Guide |
|---|---|
| Turn loop | [turn-loop.md](turn-loop.md) |
| Actions and rejections | [actions-and-rejections.md](actions-and-rejections.md) |
| Appointments | [appointments.md](appointments.md) |
| Messages and notes | [messages-and-notes.md](messages-and-notes.md) |
| Free-form actions and game-master rulings | [free-form-and-rulings.md](free-form-and-rulings.md) |
| Interventions | [interventions.md](interventions.md) |
| Fog of war | [fog-of-war.md](fog-of-war.md) |
| Tick deadline and force-close | [tick-deadline.md](tick-deadline.md) |
| Agent runs and sandbox | [agent-runs-and-sandbox.md](agent-runs-and-sandbox.md) |
| App screens | [app-screens.md](app-screens.md) |

Tool coverage: `join`, `wait_for_turn`, `look`, `act`, `rule`, `message`, `note`, `end_turn`. Admin API coverage: `GET /api/overview`, `GET /api/events`, `POST /api/intervene`, `POST /api/tick`.
