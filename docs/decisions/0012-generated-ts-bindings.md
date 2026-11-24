# 0012 The app's TS types are generated from Rust

Accepted 2026-11-24.

## Decision

Every Rust type the app reads or sends through a Tauri command derives `ts_rs::TS`, and the `bindings` test in `app/src-tauri` exports them, with all their dependencies, into `app/src/bindings/`. `api.ts` imports the generated types and holds only the `invoke` wrappers and helpers. CI's `test` job fails when the committed bindings differ from what the test writes.

## Why

The hand-written types in `api.ts` drifted from the Rust: `Perception` declared a top-level `actions` the server never sent and lacked fields it did send. Generating them makes the Rust types the single definition.

## Consequences

- `ts-rs` is a plain dependency of `sim-core`, `sim-server` and the app, not a cargo feature: a feature would wrap every derive in `cfg_attr`, and the derive adds no I/O to `sim-core`'s own code. The crate's purity lints also ban the `ts-rs` calls that write files or read the environment (`TS::export`, `TS::export_all`, `Config::from_env`).
- One test owns the directory: it deletes it and exports the root types (`Overview`, `TurnView`, `Effect` and the command payloads), so a type that leaves Rust leaves `bindings/` too. A new root type is added to that test; a type it reaches is exported on its own.
- `u64` and `i64` export as `number`, as `serde_json` sends them. A value past 2^53 would lose precision in the app.
- `no-serde-warnings` is on: the only serde attribute `ts-rs` does not parse is `transparent` on the newtype ids, which it already exports as their inner type. A new serde attribute that changes the JSON shape needs its bindings checked by hand.
