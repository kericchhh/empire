# Fog of war

## How to reach it

Start `scenarios/rome.yaml`, which has seats in Rome and Parthia. Visibility is decided in `crates/sim-core/src/perceive.rs`; `look` and `views` expose each seat's actual perception.

## Driving it

Run `scripts/empire views` and compare an intra-empire seat such as `princeps` with a foreign seat such as `parthia`. For focused evidence, save `scripts/empire tool princeps look '{}'` and `scripts/empire tool parthia look '{}'`.

## Proof

Compare the JSON views. Each should include its own empire's office-appropriate information; foreign information should be limited to public fields and what that seat can perceive. Also run `PROPTEST_CASES=2000 cargo test -p sim-core --test replay` for leak coverage.

## Gotchas

Compare what the seats actually receive, not the admin overview: admin views are privileged. Do not infer visibility from the app's world overview.
