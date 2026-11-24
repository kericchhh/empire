---
name: review
description: Review an Empire branch against AGENTS.md and CODE_RULES.md with a fresh context, reporting only findings you can prove. Use when a slice is ready for review, or when asked to review a branch, diff or PR in this repo.
---

# Review

You are the reviewer, not the builder: you report and the builder fixes. Work read-only: no edits, no commits.

## Steps

1. **Load the rules:** `AGENTS.md`, `CODE_RULES.md`, the issue the branch closes, and every record in `docs/decisions/` for an area the diff touches.
2. **Read the change:** `git diff origin/main...HEAD` (or the range you were given). For each changed symbol, trace its callers and callees with the code graph (`search_graph`, `trace_path`), so you judge the change by what it reaches, not just by its lines.
3. **Hunt every class below** across the whole diff.
4. **Prove each finding:** a failing test or command you ran, or an exact line plus a concrete input that goes wrong. Drop any finding you cannot prove.
5. **Report.**

Done when every class has been checked against every changed file, and every reported finding carries its proof.

## Classes

- **Correctness:** wrong results, missed edge cases, unhandled `Result`s, broken invariants.
- **Leak:** hidden state crossing seats or empires (anything not through `Sight` and `describe`), and host data reaching an agent run (environment, paths, files, account details).
- **Nondeterminism:** iteration over unordered maps, randomness outside `roll`, wall-clock time or the environment in `sim-core`, floats that can turn non-finite, replay that would differ from the live state.
- **Schema:** an event variant changed in meaning instead of added; a field added without `#[serde(default)]`; scenario types changed without regenerating `schema/`.
- **Core model:** a change that breaks an `AGENTS.md` core-model rule without the user having agreed to it.
- **Tests:** behaviour shipped without a test that fails when it breaks; the leak test not extended for new foreign data (`CODE_RULES.md`, Seats).
- **Tight:** code that could be smaller or simpler with the same behaviour: duplication, dead code, an abstraction with one user, a comment that restates the code.
- **Drift:** `AGENTS.md` (State, Workspace), `README.md`, a decision record or the schema no longer matching the code.

## Report

One list, most severe first. Per finding: class, `file:line`, the claim, its proof, and the smallest fix. The builder copies each finding into the PR's `## Review` as one list item ending `Fixed`, `Answered: <reason>` or `Deferred #<issue>`, so keep each finding to one item. Severity is `bug` (wrong behaviour or a leak), `rule` (breaks a written rule), `tight` or `nit`. With no findings, say so and list what you checked.
