# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

For now, the developer building Empire: they create worlds, watch agents play them and steer the engine while they test it. The UI should be dense and practical, not a showcase.

## Product Purpose

Empire is a desktop sandbox where AI agents play every role in several empires that share one world. The user describes a starting point; agents govern, trade, scheme and go to war tick by tick, and the history emerges. Success means the user can start a world, see exactly what every empire and seat sees, and intervene, all without a terminal.

## Positioning

The user brings their own agent: the app never calls a model or holds API keys. It launches the user's own logged-in agent CLI as a sealed run per seat. Every change is an event in an append-only log, and each seat sees only what its character could know.

## Operating Context

- A Tauri desktop app on Linux and macOS that hosts one world at a time.
- The agent CLI (Claude Code today) runs on the user's own subscription.
- Outside agents connect over MCP with seat tokens.
- Worlds are saved as SQLite files in the app's data directory.

## Capabilities and Constraints

- Create a world from a prompt (an agent writes the scenario) or from a scenario file, or reopen a saved world.
- View the whole world, any empire, or any seat's exact perception.
- Read the chronicle, inject events with effects, advance a tick, and manage seat tokens.
- Providers: Claude Code is supported, with model aliases. Codex is not supported yet, because its sandbox cannot seal a seat.
- Scenarios carry their own theme (palette, fonts, terms). The ruleset names no setting.

## Brand Commitments

- Name: Empire.
- The user explicitly pinned the following:
  - a brutalist, dark-themed app;
  - a palette of dark green and gold;
  - an app feel, not a web page;
  - no text selection outside fields;
  - a rendered city scene on the home screen only.

## Evidence on Hand

The reference scenario is `scenarios/rome.yaml`. There are no customers, testimonials or metrics, and none may be invented.

## Product Principles

- Show exactly what the engine knows. Never decorate over hidden state.
- The setting belongs to the scenario. The app's chrome stays setting-neutral.
- Every control maps to a real engine action.
