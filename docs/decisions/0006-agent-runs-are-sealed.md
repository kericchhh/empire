# 0006 Agent runs are sealed

Accepted 2026-11-23.

## Decision

Every agent run is a fresh headless process sealed in an OS sandbox (`sim-runner/src/sandbox.rs`: bubblewrap on Linux, `sandbox-exec` on macOS) with an empty home and `/work`. On Linux both are size-capped tmpfs mounts that exist only inside the sandbox. `check_sandbox` runs before any agent starts, and the app shows why it fails.

A run gets only the turn's MCP tools (`join` and `wait_for_turn` are denied so it cannot drift into the next tick), a cleared environment, and its seat token on stdin. The runner skips a turn whose tick closed while it queued, kills a run once its tick closes, and ends the seat's turn after the run, so a crashed or slow agent never holds up the tick.

## Why

Live tests before this showed that a run inside a repo saw its git status and commit subjects, and every run saw the account's email.

## Consequences

- Claude seats log in with a `claude setup-token` token saved by `sim-server login` or the app (`local.rs`, mode 0600), passed as `CLAUDE_CODE_OAUTH_TOKEN`. Sharing the user's own login file would break its token refresh. `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1` stops Claude Code fetching the account profile, which is what put the email in context. A 401 or 403 result sets `Plan::login_refused`.
- Only standalone agent executables work: the native Claude Code install, not npm's, whose `cli.js` needs a `node` from outside `/usr`.
- Agent executables and the bridge are copied, once per version, to `/var/tmp/empire-<uid>` (macOS: the per-user temp dir), so no mount names the user's home. Unused copies go after ten minutes.
- A killed run takes its process tree with it on Linux (`--unshare-pid`, `--die-with-parent`). On macOS only the main process dies.
- `clippy.toml` in `sim-runner` bans `Command::new` outside the sandbox module.
- The runner refuses the game-master seat for now: its run would start with the others, before there is anything to rule on. It can start once only the game master is left to end its turn, e.g. with `waiting_for` on `TurnView`.
