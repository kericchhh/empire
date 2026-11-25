---
name: milestone
description: Plan an Empire milestone as GitHub issues, one slice each.
disable-model-invocation: true
---

# Milestone

Turn a goal into a GitHub milestone whose issues are slices: each one PR, each with a checkable _Done when_.

## Steps

1. **Read the state:** open `debt` issues first (`gh issue list --label debt --state open`; run the `audit` skill if no `debt` issue was opened since the last closed milestone's close date, from `gh api 'repos/{owner}/{repo}/milestones?state=closed'`), then the State section of `AGENTS.md`, open milestones and issues (`gh issue list --state open`, `gh api repos/{owner}/{repo}/milestones`), recently merged PRs, and the `docs/decisions/` records for the areas the goal touches.
2. **Map the seams.** With the code graph, find the modules the goal reaches and what each one's interface would gain. Note which parts change the core model in `AGENTS.md`.
3. **Cut slices.** Each slice is a tracer bullet: thin, end to end, shippable alone, and small enough to review in one sitting. Order them so the riskiest seam (perception, the event schema, the sandbox) lands early and is tested hard. Each gets:
   - **Goal:** one sentence.
   - **Seam:** the modules and the interface change.
   - **Done when:** observable checks, each a test name or a `verify` observation.
   - **Touches:** `perception`, `events`, `sandbox`, `app`, or `none`, so the reviewers know where to dig.
   - **Depends on:** issue numbers, or none.
4. **Consult.** Show the user the slices, their order, and every open decision (core-model changes first, each with a recommendation). Wait for approval and apply their changes.
5. **Create.** Make the milestone with `gh api repos/{owner}/{repo}/milestones -f title=... -f description=...`, then one issue per slice with `gh issue create --milestone <title> --title ... --body-file ...`, using the fields above as the body. Fill in _Depends on_ numbers as issues get created.

Done when the milestone exists, every slice is an issue with all five fields, and the user approved the plan.
