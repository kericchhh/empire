## What this adds

This PR adds `app/`, the Tauri desktop app. It has a React UI in `app/src` and a Rust backend in `app/src-tauri` that hosts worlds in-process through a new `sim_server::host` module. The headless binary uses the same module.

With the app you can:
- create a world from a description (Claude Code writes the scenario) or from a scenario file, or reopen a saved one;
- watch the whole world, any empire, or exactly what any one seat sees;
- read the history, add events by hand, skip to the next turn, and copy agents' access keys.

## The UI

- **Home:** a dark papyrus scroll between wooden rollers, over a static render of a sunset city. The scroll rolls to each tab's height. Only transforms animate, so the compositor does the work; it measured 42–67fps in headless Chrome with no GPU, where the first version ran at 9fps. Keys 1–3 switch tabs and Escape leaves a field.
- **Providers tab:** shows the local Claude Code version and login through the new `claude_status` command. It runs no model and gives up after 10 seconds if the CLI stalls.
- **World view:** the same papyrus and gold theme.
  - Scrolling never redraws the background: the world view measured 145fps, and a 4,000-entry chronicle 121fps.
  - The chronicle shows 200 entries at a time. The backend reads every page, so long worlds don't fall behind.
- **Type:** Departure Mono, a pixel font, for labels, titles and numbers, always placed on whole pixels so it stays crisp. JetBrains Mono at 13px for running text.
- **Labels:** written in plain language.

## Supporting changes

- **CI:** a `frontend` job runs the TypeScript check and the Vite build. The Rust jobs install the system libraries Tauri needs, and Dependabot now covers `app/`.
- **Assets:** both fonts are self-hosted (Departure Mono under the OFL). The city image comes from `app/scripts/city.py`, a Blender script run once offline. The papyrus texture comes from `app/scripts/paper.svg`. Neither script runs in the app.
- **Docs:** `AGENTS.md` describes `app/` as built rather than planned.

## Review

A reviewer subagent with a fresh context checked the whole frontend against `AGENTS.md` and `CODE_RULES.md`. It reported 14 findings:
- **Fixed (12):** duplicate chronicle entries from overlapping refreshes, a refresh firing after close, reduced motion being ignored, a hanging status check, double-submitted events, stale seat-view responses, unreachable keyboard targets, and dead CSS.
- **Left as is (1):** a one-frame timer in the roll animation isn't cancelled on unmount. The reviewer rated it harmless.
- **Deferred to backend work, by decision (2):**
  - `api.ts` types are still written by hand. `CODE_RULES.md` says they should be generated with `ts-rs`.
  - macOS GUI apps get a minimal `PATH`, so the app can miss `claude`.

## Checks

`cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings`, `cargo test --workspace` (119 passed) and `pnpm build` all pass locally.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
