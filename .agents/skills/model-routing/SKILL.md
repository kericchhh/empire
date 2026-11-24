---
name: model-routing
description: Route each sub-agent, reviewer or delegated task to a model tier by its weight. Use before launching any sub-agent, reviewer or delegate_task.
---

# Model routing

Rate the task, take its tier from the table, and launch it with that tier's model and effort. Name the tier and model in the report.

| Tier | Use for | Claude | Codex | Antigravity (Google) |
|---|---|---|---|---|
| Heavy | architecture, design decisions, cross-module coding | `claude-opus-5-5`, effort `high` | — | — |
| Coding | routine coding, bounded fixes, tests, refactors | `claude-opus-5-5`, effort `medium` (`low` for trivial edits) | `gpt-6-luna`, reasoningEffort `medium` | — |
| Light | research, lookups, summaries, drafts | `claude-haiku-5-5`, effort `low` | — | `gemini-3.8-flash`, thought_level `low` |

Antigravity also lists `gemini-4-hydrogen`, two Claude models and `gpt-oss-120b-medium`. None is in the table yet: they are not benchmarked here, and the Claude ones are older versions than the direct Claude entries.

## Review

- Major change: Opus 5.5 plus a second reviewer from the other family (`gpt-6-luna`). A change is major when it adds a crate, module, MCP tool or event variant, changes the core model, or touches more than three files.
- Minor change: one Opus 5.5 reviewer, effort `medium`.

## Launch

- Claude models: the native `Agent` tool, `model` set to `opus` or `haiku`.
- Other providers: `delegate_task`, with the provider and model from `orchestrator_capabilities`. Antigravity is `antigravity`, with the `thought_level` option.

## Rules

- Heavy and Coding tiers write and review code. Light researches and looks things up.
- A slice builder (issue to PR, several stages) is Claude. `gpt-6-luna` reviews, re-checks and makes bounded fixes: as a builder of the "Trustworthy agents" slices #41 and #43 it stopped after reading or a partial start, three times, with nothing committed.
- Code quality comes first: keep a coding or review task in its tier, whatever the quota.
- If a tier's model is unavailable, use another model in that tier if one is listed. Otherwise report the gap. Never fall back to a weaker model silently.
- Light research prefers `gemini-3.8-flash`, and falls back to `claude-haiku-5-5` if Antigravity is unavailable. That fallback stays in Light.
- Keep this table current with the models T3 lists.
