---
name: review
description: Review an Empire branch against AGENTS.md and CODE_RULES.md with a fresh context, proving each finding as far as it goes. Use when a slice is ready for review, or when asked to review a branch, diff or PR in this repo.
---

# Review

You are the reviewer, not the builder: you report and the builder fixes. Work read-only: no edits, no commits.

## Steps

1. **Load the rules:** `AGENTS.md`, `CODE_RULES.md`, the issue the branch closes, and every record in `docs/decisions/` for an area the diff touches.
2. **Read the change:** `git diff origin/main...HEAD` (or the range you were given). For each changed symbol, trace its callers and callees with the code graph (`search_graph`, `trace_path`), so you judge the change by what it reaches, not just by its lines.
3. **Hunt every class below** across the whole diff.
4. **Prove each finding** as far down this ladder as it goes, and name the rung it reached:
   1. said: you claim it;
   2. pointed at: the exact `file:line`;
   3. walked through: a concrete input traced step by step to the wrong result;
   4. ran: a test or command you ran fails on it;
   5. reproduced: it shows in a running world or the app.

   A finding about behaviour (`bug`) needs rung 4; one about the code's text (a rule, duplication, size) needs rung 2. Label any finding short of its rung `unproven`.
5. **Report.**

Done when every class has been checked against every changed file, and every reported finding carries its proof.

## Classes

- **Correctness:** wrong results, missed edge cases, unhandled `Result`s, broken invariants.
- **Leak:** hidden state crossing seats or empires (anything not through `Sight` and `describe`), and host data reaching an agent run (environment, paths, files, account details).
- **Nondeterminism:** iteration over unordered maps, randomness outside `roll`, wall-clock time or the environment in `sim-core`, floats that can turn non-finite, replay that would differ from the live state.
- **Schema:** an event variant changed in meaning instead of added; a field added without `#[serde(default)]`; scenario types changed without regenerating `schema/`.
- **Core model:** a change that breaks an `AGENTS.md` core-model rule without the user having agreed to it.
- **Tests:** behaviour shipped without a test that fails when it breaks; the leak test not extended for new foreign data (`CODE_RULES.md`, Seats).
- **Duplication:** for each new function, `search_graph` by name and `search_code` for its distinctive calls to find one that already does the job. For new code in one binary, `crates/sim-server/src/main.rs` or `app/src-tauri/src`, find the same job in the other: one job done two ways in two binaries is a finding, even when the lines differ.
- **Size:** a function added or grown past 100 lines: `cargo clippy -p <crate> -- -W clippy::too_many_lines` on each changed crate.
- **Tight:** code that could be smaller or simpler with the same behaviour: dead code, an abstraction with one user, a comment that restates the code.
- **Drift:** `AGENTS.md` (State, Workspace), `README.md`, a decision record or the schema no longer matching the code, or a rule-table row whose enforcer cannot catch the failure it claims (try the failure and see if the enforcer fires).

## Report

One list, most severe first. Per finding: class, `file:line`, the claim, its proof and rung, and the smallest fix. The builder copies each finding into the PR's `## Review` as one list item ending `Fixed`, `Answered: <reason>` or `Deferred #<issue>`, so keep each finding to one item. Severity is `bug` (wrong behaviour or a leak), `rule` (breaks a written rule), `tight` or `nit`. With no findings, say so and list what you checked.
