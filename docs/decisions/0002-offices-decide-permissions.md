# 0002 Offices, not titles, decide permissions

Accepted 2026-11-22.

## Decision

A character's `title` ("Princeps", "Poet") is flavour. Its `office` (`head_of_state`, `official`, `commander`, `governor`, `private`, `cohort`) decides its actions and what it sees. Offices are part of `State`, so an event can move them.

## Why

Permissions must come from the ruleset, which names no setting, while titles come from the scenario.

## Consequences

- `Sight::of_seat` reads offices from the live state, so a seat's sight follows its character's office with no extra code.
- No event moves an office yet. Appointments, succession and coups will add one.
