# 0013 Empires know only the empires they have met

Accepted 2026-11-24.

## Decision

`State.contacts` holds the pairs of empires that have met, each with the smaller id first. An empire's seats list, know and reach only their own empire and the empires it has met: `perceive` leaves every other empire out of `empires`, and `knows` admits a foreign public figure only once the two empires have met, so messages and actions on a foreign character need contact too. The game master knows every empire.

Contacts come only from `EmpiresMet` events, visible to the two empires alone (`Visibility::Empires`). A scenario's `contacts` are logged as `EmpiresMet` in `engine::start`, so replay never re-reads the scenario. A user introduces two more through `engine::introduce` (`/api/introduce`, the app's intervention screen). Introducing an empire to itself or to one it has already met is rejected (`AlreadyMet`), so every `EmpiresMet` in a log records a first meeting.

## Why

Diplomacy, trade and war between empires begin with knowing the other exists. If an empire learned of a power it never met, or of a meeting between two others, the fog of war would leak.

## Consequences

- No event removes a contact: once met, two empires stay aware of each other.
- The leak proptest runs on Rome and on a three-empire fixture (`crates/sim-core/tests/fixtures/three.yaml`) where one empire knows nobody, and generates introductions. Its `shift_foreign` changes the personas, goals, secrets and seat names of every other empire, and the names, descriptions, culture and every office of each empire the observer has not met, flips every contact between two other empires, and drops their meetings from the log.
- The scenario author prompt asks for `contacts`, since a generated world without them would start with every empire alone.
- A game-master ruling cannot introduce empires yet; it would need a field on `Ruling` (#32).
- A world saved before contacts existed has neither `contacts` in its scenario nor `EmpiresMet` in its log, so it reopens with every empire alone. Accepted while the project is pre-release.
- `end_turn` names only the caller's own empire's seats in `waiting_for`, so a seat never learns of a foreign seat or empire that way.
