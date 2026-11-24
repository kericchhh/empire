# 0012 Death and succession

Accepted 2026-11-24.

## Decision

A character can die through `CharacterDied`, for now only by an intervention's `deaths`. The dead keep their office in the state, but take no action and no default action, send no message or note, cannot be targeted, and give their seat no sight of numbers. An order from or on a dead character is rejected with "`<id>` is dead".

The ruleset's `succession` names the office that passes on and the resource that ranks the heirs. At the end of a tick, if every holder of that office in an empire is dead, it passes by `OfficeChanged` to that empire's living character who holds an appointable office and has the most of that resource, ties to the lowest id. With no such character the office stays vacant, and the check runs again each tick.

## Why

Succession is a mechanic, so the ruleset names the office, never the engine. Only holders of appointable offices inherit, because `appointable` already marks the offices people hold (0011): a cohort is a populace, not an heir. Keeping the dead's office lets the empire see who died in which office, and lets a vacancy be found from the state alone.

## Consequences

- `CharacterDied` and the succession move are visible to the dead character's empire only, plus the game master. Contacts (#13) will decide what reaches other empires.
- Abroad, only living holders of public offices are known. So a foreign ruler's death shows abroad as a change of who rules, without the chronicle line, and while the office is vacant the ruler simply disappears from foreign views. `shift_foreign` therefore flips whether foreign holders of private offices live, but not of public ones.
- Succession moves join the end-of-tick moves after appointments, and the moves pass reads each `from` again.
- A seat whose characters are all dead still gets turns. Retiring it is left for later.
