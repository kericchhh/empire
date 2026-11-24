Closes #1

## Why

A seat could not see its own grain.

## What changed

- `perceive` shows the seat's own metrics.

## Verified

`cargo test -p sim-core`: `test result: ok. 64 passed`

## Review

- rule: a comment restated the code. Fixed
- tight: `Sight::new` could take a reference. Answered: it is moved into the result, so a reference would need a clone
- nit: the chronicle line is long. Deferred #12

## Reflect

The missing metric would have been caught by the leak test; it now covers own metrics.
