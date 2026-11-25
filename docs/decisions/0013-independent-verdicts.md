# 0013 A PR merges on an independent verdict tied to its patch

Accepted 2026-11-24.

## Decision

A PR is ready to merge when `scripts/merge-ready <pr>` prints `MERGE-READY`: `ci-success` is green on the head, the head contains `origin/main`, `scripts/pr-check` passes, and the latest verdict comment for the head's patch-id is `PASS` or `PASS+NOTES` from a run that wrote none of the PR's commits. A PR that `dependabot[bot]` opened needs only the first two, as CI's `pr` job already exempts it from `pr-check`. The patch-id is `git patch-id --stable` of `origin/main...<head>`.

A run is named by its run id: `$T3CODE_THREAD_ID`, else `$CLAUDE_CODE_SESSION_ID`, else `$CODEX_THREAD_ID`. Every commit names the run that wrote it in a `Builder: <run id>` line, or `Builder: user` for the user's own, and a verdict names its run in a `Verifier: <run id>` line. A verdict and its verifier must be single words. `merge-ready` reads `Builder:` lines from the whole commit message, so a squash keeps every builder. It refuses a commit with none, and a verdict whose verifier any commit names.

## Why

The builder's own word was the last check before a merge, and the builder is the agent most likely to miss what it got wrong. The comment author and the commit author cannot tell runs apart: every agent posts and commits as the same user. The run id comes from the environment, so an agent does not choose it, and a run that both built and verified names itself twice.

The threat is an agent's honest mistake, such as a builder verifying its own work or a verdict outliving a fix. It does not guard against a user or agent that sets out to fake an id.

## Consequences

- A patch-id ignores line numbers and commit structure, so rebasing onto a newer `main` keeps a verdict, and any change to the diff voids it. A rebase whose conflicts change the diff needs a fresh verdict.
- A native subagent sees its parent's `T3CODE_THREAD_ID` and `CLAUDE_CODE_SESSION_ID`, so a verifier that a builder spawns natively is refused. Verdict agents are launched through `delegate_task`.
- A branch behind `main` is not ready, even when its patch-id still matches. The `main` ruleset does not require branches to be up to date, so otherwise a PR could merge onto a `main` it was never built or verified against. Rebasing reruns CI, and the verdict holds if the diff is unchanged.
- `Head:` in a verdict records what was checked; `merge-ready` compares only the patch-id.
