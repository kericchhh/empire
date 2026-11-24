# 0011 Actions take one general choice

Accepted 2026-11-24.

## Decision

An action may declare a `choice` kind in the ruleset (`office` for now). An order for it carries a `choice` string, which must be one of the values that kind lists, besides its `target`. Perception shows each such action's kind and its valid options by id and name.

## Why

`appoint` must name an office, and treaties will need to name a treaty kind. One parameter with a declared kind serves both, instead of a new order field per mechanic.

## Consequences

- A new choice kind adds a `ChoiceKind` variant, its options in `Ruleset::choices`, and what it resolves to in `resolve_action`.
- A choice is validated like a target: missing, unexpected or unknown choices are rejected with the valid options in the reason.
- An office choice needs `target: own_character`, so offices move only inside the actor's empire. `OfficeChanged` is visible to that empire alone.
- Only offices marked `appointable` can be chosen or appointed away. The standard ruleset leaves out `head_of_state`, so an empire keeps one ruler until succession or a coup moves the office, and `cohort`, since a populace is not a person to promote.
- An office moves at the end of its tick, after every order and default action, so each of those resolves under the office it was accepted for.
