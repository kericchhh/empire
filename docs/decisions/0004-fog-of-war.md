# 0004 Fog of war

Accepted 2026-11-22.

## Decision

A seat sees its own empire's numbers through its offices and nothing of another empire's numbers. Foreign empires are known by name, description and culture. Foreign characters are known only if their office is `public` (heads of state), so diplomacy works through them. Actions, rule events and public rulings are visible to the acting empire only (`Visibility::Empire`). Interventions are public unless the admin names an `empire`.

Perception has two layers. `Sight` decides which numbers an observer sees. `describe` applies it to every chronicle line: effects (and what scaled them), rule events in unseen regions, and rule labels whose condition reads a hidden value.

## Why

If hidden state leaks across seats or empires, the experiment is invalid. Both early reviews found leaks through the chronicle: hidden numbers in effects, a rule label revealing its hidden condition, an effect scaled by a hidden value.

## Consequences

- Every new event kind goes through both `Sight` and `describe`.
- `no_seat_sees_anything_hidden_of_another_empire` (`crates/sim-core/tests/replay.rs`) shifts everything foreign, including the words of foreign events that reach none of the observer's characters or regions, and requires every seat's perception and chronicle to stay identical.
- Free-form plans are private: the declaration and its expiry are visible to the author and the game master. A ruling is private to the author unless `public: true`.
- Agent text is quoted in-world speech. `check_text` rejects line breaks, control characters, `«` and `»`, so agents cannot forge chronicle lines or break out of the quotes.
