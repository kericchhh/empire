# 0013 A PR merges on an independent verdict tied to its patch

Accepted 2026-11-24.

## Decision

A PR is ready to merge when `scripts/merge-ready <pr>` prints `MERGE-READY`: `ci-success` is green on the head, `scripts/pr-check` passes, and the latest verdict comment for the head's patch-id is `PASS` or `PASS+NOTES` from a run that wrote none of the PR's commits. The patch-id is `git patch-id --stable` of `origin/main...<head>`.

A run is named by its run id: `$T3CODE_THREAD_ID`, else `$CLAUDE_CODE_SESSION_ID`, else `$CODEX_THREAD_ID`. Every commit names the run that wrote it in a `Builder: <run id>` trailer, and a verdict names its run in a `Verifier: <run id>` line. `merge-ready` refuses a commit with no trailer and a verdict whose verifier is any commit's builder.

## Why

The builder's own word was the last check before a merge, and the builder is the agent most likely to miss what it got wrong. The comment author and the commit author cannot tell runs apart: every agent posts and commits as the same user. The run id comes from the environment, so an agent does not choose it, and a run that both built and verified names itself twice.

The threat is an agent's honest mistake, such as a builder verifying its own work or a verdict outliving a fix. It does not guard against a user or agent that sets out to fake an id.

## Consequences

- A patch-id ignores line numbers and commit structure, so rebasing onto a newer `main` keeps a verdict, and any change to the diff voids it. A rebase whose conflicts change the diff needs a fresh verdict.
- A native subagent can inherit its parent's environment and so its run id, and a verifier that does is refused. Verdict agents are launched through `delegate_task`.
- `Head:` in a verdict records what was checked; `merge-ready` compares only the patch-id.
