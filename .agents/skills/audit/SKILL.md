---
name: audit
description: Audit the whole Empire repo on `origin/main` for code agents would copy, and file each finding as a `debt` issue. Use at a milestone boundary, before planning the next milestone, or when asked to audit the repo.
---

# Audit

A review sees one diff, so it misses a job done twice in two places, a function that grew one slice at a time, a rule the code stopped following. The audit reads the whole of `origin/main`. Leave the working tree untouched: probe and mutate only in a scratch worktree (`d=$(mktemp -d) && git worktree add -q --detach "$d/wt" origin/main`), and remove it when done (`git worktree remove --force "$d/wt" && rm -rf "$d"`). The only other writes are issues, in step 7.

## Steps

1. **Map.** Re-index with the code graph (`index_repository`), then `get_architecture`. Write down each module, what it depends on (`cargo tree --workspace -e normal --depth 1`), and which entry point reaches it: the `sim-server` binary (`crates/sim-server/src/main.rs`), the app's Tauri backend (`app/src-tauri/src`) or the UI (`app/src`).
2. **Size.** The largest files (`git ls-files '*.rs' '*.ts' '*.tsx' | xargs wc -l | sort -rn | head -15`) and every function over 100 lines (`cargo clippy --workspace --all-targets -- -W clippy::too_many_lines`). For each, say which jobs it mixes.
3. **Duplication.** Hunt it three ways. One job done in two places is a finding even when the lines differ.
   - Across binaries: list every job either one exposes, one per `sim-server` subcommand, `#[tauri::command]` and `main` start-up. For each, find every place that does it: `search_graph` by name, `search_code` for its distinctive calls (the constructor of a job struct, a call into a shared crate, an error message).
   - Per kind: for each enum with one variant per provider, mode or source, list every `match` on it outside its definition and parser (`git grep -n '<Enum>::'`), and every set of parallel functions, one per variant, in sibling modules. An enum matched in many places, or parallel functions repeating the same steps, is a finding.
   - Across languages: lists synced by hand between Rust and TS, and TS that recomputes what Rust already sends.
4. **Unused code.** `search_graph` for functions nothing calls that are not entry points, tests, MCP tools or Tauri commands; `pub` items used only inside their crate; TS exports nothing imports. Confirm each with `git grep` before you count it.
5. **Drift.** For each row of the rule tables, try the failure it names (a probe file, a mutated line, a past commit) and see whether its enforcer fires, and check every crate the rule covers, not only the one with the lint. Where a rule names one owner for a decision, grep for that decision made elsewhere, such as a value type built outside the module the rule names. Check `AGENTS.md` (State, Workspace), `README.md` and `docs/decisions/` against the code.
6. **Test gaps.** For each MCP tool, admin endpoint and Tauri command, find the test that fails when it breaks. For a few of them, in the scratch worktree, break one line of the handler (delete a check, invert a condition), run that crate's tests (`cargo test -p <crate>`), and confirm a test fails; then `git checkout .` there. One with no test that fails is a finding.
7. **File.** Prove each finding as the `review` skill's ladder asks. Only now read the open `debt` issues (`gh issue list --label debt --state open`). For each finding no issue covers, `gh issue create --label debt --title … --body …` with what is wrong, `file:line`, the proof and the smallest fix; for one an issue covers, comment there if you found more of it.
8. **Score the reviewers.** For each finding, find the commit that introduced it (`git log -S'<line>'`, `git blame`), its PR, and that PR's `## Review`: which reviewer models ran, and whether any flagged it.

## Report

The module map, then the findings by class (size, duplication, unused, drift, test gaps), each with its rung and its issue number, new or existing. Then a table per reviewer model: PRs reviewed, audit findings introduced in those PRs, and how many it flagged.

Done when every step has covered the whole repo, every finding has proof and an issue, and the reviewer table accounts for every finding.
