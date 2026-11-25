---
name: slice
description: Take one Empire GitHub issue from claim to a verified, reviewed pull request. Use to work on an issue or the next slice of a milestone.
---

# Slice

A slice is one issue, one branch, one PR. The issue's _Done when_ is the contract; the user merges once `scripts/merge-ready <pr>` prints `MERGE-READY`.

Every commit on the branch ends with the trailer `Builder: <run id>`, naming the run that wrote it, where the run id is `${T3CODE_THREAD_ID:-${CLAUDE_CODE_SESSION_ID:-$CODEX_THREAD_ID}}`; the user's own commits carry `Builder: user`. A fix made by another run carries that run's id, and a squash keeps every `Builder:` line. `merge-ready` refuses a commit with no `Builder:` line, and a verdict from any run one of them names.

## Steps

1. **Claim.** Take the issue you were given, or the next one: the lowest-numbered open issue in the oldest open milestone whose _Depends on_ issues are all closed. Then `git fetch` and branch `<area>/<slug>` from `origin/main`. Link the issue to this thread when the host offers it.
2. **Find the seam.** Locate the modules the issue touches with the code graph. If the issue changes a module's interface, design it with the `codebase-design` vocabulary before you code. If the work turns out to change the core model in a way the issue does not state, stop and ask the user.
3. **Go red.** Write the test that encodes _Done when_ before the code, and watch it fail for the right reason. Its failing output goes under `## Verified`.
4. **Go green.** Implement until it passes, keeping the code tight. Then run the finish line from `CODE_RULES.md`, regenerate `schema/` if scenario types changed, and update `AGENTS.md` (State, Workspace) or add a `docs/decisions/` record when the slice changes what they say.
5. **Verify** with the `verify` skill.
6. **Review** with two reviewers in fresh contexts, in parallel, each told to follow `.agents/skills/review/SKILL.md` for the range `origin/main...HEAD` and the issue: one from your own model family through your native subagent tool, one from the other family (Claude or Codex) through T3's `delegate_task`. Launch both at the models `.agents/skills/model-routing` gives a major change. If `delegate_task` is unavailable, run the one and say so in the PR.
7. **Resolve.** Fix every finding, answer it with a reason ("kept by design" is an answer), or open an issue for it. After a fix that changes behaviour, re-run verify, and re-review that part.
8. **Reflect.** For each finding and each mistake you made along the way, ask what would have caught it first, and encode the answer at the strongest level that works: a lint (`clippy.toml`, `deny.toml`), a test, a line in `CODE_RULES.md` or `AGENTS.md`, or an edit to one of these skills. Put it in this PR, or open an issue when it is out of scope.
9. **Ship.** Commit, then push only once the commit succeeded (`git commit … && git push`), and open the PR against `main` with `Closes #<issue>`, and sections for what changed, `## Verified`, `## Review`, `## Reflect`, and `## Rule changes` when the PR changes `CODE_RULES.md`, `AGENTS.md`, a skill, a `clippy.toml`, `deny.toml`, a CI workflow, `scripts/pr-check` or `scripts/merge-ready`. Under `## Review`, each finding is one list item that ends with its outcome: `Fixed`, `Answered: <reason>` or `Deferred #<issue>`. Run `scripts/pr-check <pr>`, which CI's `pr` job also runs; after editing the body, re-run that job. Link the PR, then watch it until `ci-success` is green, fixing failures as they come.
10. **Verdict.** Launch a fresh agent of either family that built none of the PR, through T3's `delegate_task`, so it runs under its own run id. Any change to the diff after a verdict voids it, so each fix round gets its own fresh verdict. The verdict agent:
    - checks out the PR head in a fresh worktree and builds with `export CARGO_TARGET_DIR="$(dirname "$(git rev-parse --path-format=absolute --git-common-dir)")/target"`, the main checkout's `target/`;
    - runs `scripts/merge-ready <pr>` and takes the head and patch-id from its first line, `head <sha> patch-id <id>`, after checking that `git rev-parse HEAD` is that head;
    - drives, with the `verify` skill, each entry of `.agents/skills/verify/features/` that the slice touches, and each _Done when_ line, as a user would;
    - confirms with `ps -eo pid,args | grep '[d]ebug/sim-server'` that no world it or the builder started is still running: it stops its own, and notes any other;
    - posts one new PR comment in exactly this format, never editing an earlier one, since `merge-ready` refuses an edited or hidden verdict, where `PASS+NOTES` means every claim holds with notes that need no change, and `FAIL` means a claim does not hold:

      ```text
      Verdict: PASS | PASS+NOTES | FAIL
      Head: <sha>
      Patch-id: <id>
      Verifier: <its run id>

      <each entry and Done when line driven: the commands and the output lines that prove it>
      ```

    Then run `scripts/merge-ready <pr>`. On a `FAIL`, fix the branch and go back to step 7.

Done when the PR is open, every review finding is fixed, answered or deferred to an issue, the PR body carries the verify evidence, and `scripts/merge-ready <pr>` prints `MERGE-READY`. Hand back with the PR link and that output.
