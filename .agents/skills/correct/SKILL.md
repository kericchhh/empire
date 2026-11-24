---
name: correct
description: Turn a mistake agents repeat in Empire into a check that fails on it, proven against the past commit that made it. Use in a slice's Reflect step, when the same correction comes up a second time, or to mine the repo's history for mistake classes.
---

# Correct

Every contributor is an agent that sees only the files it opened, copies the nearest example, and takes the shortest path that compiles. Change the repo so the next one cannot repeat the mistake.

## Steps

1. **Mine.** Collect mistakes from:
   - commits: `git log --format='%h %s%n%b' origin/main`, read for fixes, reverts, "address review", and code that was added and later removed (`git log --oneline -S'<text>'` shows where a line came and went);
   - merged PRs' `## Review` and `## Reflect`: `gh pr list --state merged --limit 100 --json number`, then `gh pr view <n> --json body`;
   - `debt` issues: `gh issue list --label debt --state all`;
   - in a slice: its review findings and your own mistakes.
2. **Group into classes.** A class is one mistake an agent could make again somewhere else, e.g. "a `cfg(unix)` guard in code that runs only on Linux and macOS". It counts once it happened twice. List the commits or PR findings behind each one.
3. **Pick the level.** For each class, take the highest level that works:
   1. Architecture: give each job one owner and one supported way, and delete the old way an agent would copy.
   2. Types: make the bad state fail to compile.
   3. A check whose error names the fix (the file, type or function to use instead): a `clippy.toml` `disallowed-*` entry, `deny.toml`, or a `scripts/` check that CI runs. If the pattern is already common, fail only on new instances.
   4. A test of the behaviour.
   5. A `judgment` row with an example of the failure, when nothing above can decide it.

   Read the rule tables (`AGENTS.md`, `CODE_RULES.md`) first. A class whose rule exists with `judgment` as its enforcer is a repeat: move it up a level.
4. **Prove it.** Run the new check against the commit that made the mistake, and keep the failing output. A `scripts/` check takes the revision (`git grep … <rev>`); a lint or test runs in a scratch `git worktree add` of that commit, with the check added. Then show it passes on `HEAD`. Until a check has failed on a real past mistake, label it unproven.
5. **Wire it.** CI runs the same command you ran, and keeps re-proving it against the past commit when that is cheap, as the `check-platforms` steps in CI's `pr` job do. Point the rule's row in the table at the new enforcer, and drop a rule whose mistake can no longer happen. Make one commit per class.

## Report

Per class: its name, the evidence, the level picked and why no higher one works, and the check's failing output on the past commit. Then the one-offs, and the classes left at `judgment` with the issue that tracks each, or why it stays there.

Done when every class has evidence and a level, every new check has failed on a past commit, and the rule table names it.
