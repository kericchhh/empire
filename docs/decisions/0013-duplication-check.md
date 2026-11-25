# 0013 Duplication is checked by jscpd against a fingerprint baseline

Accepted 2026-11-25.

## Decision

`scripts/dupes` runs jscpd 5.4.3 (pinned, through `npx`) over the Rust and TS sources with `.jscpd.json`: exact token clones of at least 40 tokens and 5 lines, the generated `app/src/bindings/` left out. Each clone has a fingerprint, and `.jscpd-baseline.json` holds main's. CI fails when a clone's fingerprint is missing from the baseline. A clone that must stay goes in with `scripts/dupes --update-baseline` and a reason in the PR. A PR that removes one shrinks the baseline the same way.

## Why

Two tools were tried on this repo, on main and on a branch that copies the authoring job (`sim_runner::author`) into a third Tauri command:

| | main | third copy | runtime |
|---|---|---|---|
| jscpd 5.4.3, 40 tokens | 5 clones, 2 of them noise (an import list, a test loop) | 2 new clones, both the copied block | 0.9 s warm, 1.7 s with the `npx` fetch |
| jscpd 5.4.3, 50 tokens (default) | 1 clone, noise | the copied block | same |
| jscpd 5.4.3, 30 tokens, 3 lines | 52 clones, mostly short test and serde shapes | | same |
| jscpd 5.4.3, renamed identifiers and literals | 123 clones | | same |
| code graph `SIMILAR_TO` (`codebase-memory-mcp` 0.12.1) | 2 whole-function pairs | nothing new: it compares whole functions, and the copied block sits in a larger one | 1 s with its daemon running, plus a binary CI does not have |

At 40 tokens jscpd finds the debt the milestone already plans to remove (both mains' logging init, #44; `play` in the Claude and Codex kinds, #46) and little else. The graph only finds functions that are near-identical as a whole. A fingerprint baseline beats a committed count: a PR that removes one clone and adds another still fails.

## Consequences

- jscpd does not see the two existing authoring calls as clones, because their field values differ. #44 removes them by design, not through this check.
- jscpd needs Node; CI already has it, and so does every machine that builds the app.
- The baseline counts each fingerprint, so a third copy of an existing clone is new too.
