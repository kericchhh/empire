---
name: slice
description: Take one Empire GitHub issue from claim to a verified, reviewed pull request. Use to work on an issue or the next slice of a milestone.
---

# Slice

A slice is one issue, one branch, one PR. The issue's _Done when_ is the contract; the user merges.

## Steps

1. **Claim.** Take the issue you were given, or the next one: the lowest-numbered open issue in the oldest open milestone whose _Depends on_ issues are all closed. Then `git fetch` and branch `<area>/<slug>` from `origin/main`. Link the issue to this thread when the host offers it.
2. **Find the seam.** Locate the modules the issue touches with the code graph. If the issue changes a module's interface, design it with the `codebase-design` vocabulary before you code. If the work turns out to change the core model in a way the issue does not state, stop and ask the user.
3. **Go red.** Write the test that encodes _Done when_ before the code, and watch it fail for the right reason. Its failing output goes under `## Verified`.
4. **Go green.** Implement until it passes, keeping the code tight. Then run the finish line from `CODE_RULES.md`, regenerate `schema/` if scenario types changed, and update `AGENTS.md` (State, Workspace) or add a `docs/decisions/` record when the slice changes what they say.
5. **Verify** with the `verify` skill.
6. **Review** with two reviewers in fresh contexts, in parallel, each told to follow `.agents/skills/review/SKILL.md` for the range `origin/main...HEAD` and the issue: one from your own model family through your native subagent tool, one from the other family (Claude or Codex) through T3's `delegate_task`. Launch both at the models `.agents/skills/model-routing` gives a major change. If `delegate_task` is unavailable, run the one and say so in the PR.
7. **Resolve.** Fix every finding, answer it with a reason ("kept by design" is an answer), or open an issue for it. After a fix that changes behaviour, re-run verify, and re-review that part.
8. **Reflect.** Run the `correct` skill on this slice's review findings and your own mistakes along the way: each class that has now happened twice becomes a check proven against the commit that made it. Put it in this PR, or open an issue when it is out of scope.
9. **Ship.** Commit, then push only once the commit succeeded (`git commit … && git push`), and open the PR against `main` with `Closes #<issue>`, and sections for what changed, `## Verified`, `## Review`, `## Reflect`, and `## Rule changes` when the PR changes `CODE_RULES.md`, `AGENTS.md`, a skill, a `clippy.toml`, `deny.toml`, a CI workflow, `scripts/pr-check` or a `scripts/check-*`. Under `## Review`, each finding is one list item that ends with its outcome: `Fixed`, `Answered: <reason>` or `Deferred #<issue>`. Run `scripts/pr-check <pr>`, which CI's `pr` job also runs; after editing the body, re-run that job. Link the PR, then watch it until `ci-success` is green, fixing failures as they come.

Done when the PR is open with `ci-success` green, every review finding is fixed, answered or deferred to an issue, and the PR body carries the verify evidence. Hand back with the PR link.
