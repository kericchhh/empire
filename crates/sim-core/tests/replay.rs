use proptest::prelude::*;
use sim_core::engine::{self, close_tick};
use sim_core::scenario::TargetKind;
use sim_core::{
    Effect, Event, Order, Ruling, Scenario, SeatId, State, Submission, Target, TurnInput,
};

const ROME: &str = include_str!("../../../scenarios/rome.yaml");

/// One thing that can happen during a tick. Choices are indexes, resolved against the live
/// state, so every generated history is playable.
#[derive(Debug, Clone)]
enum Step {
    Order(usize),
    Freeform(usize),
    Message(usize, usize),
    Note(usize),
    Intervene(usize, i32),
    RuleFirstPending(bool, i32),
}

fn step() -> impl Strategy<Value = Step> {
    prop_oneof![
        4 => any::<usize>().prop_map(Step::Order),
        1 => any::<usize>().prop_map(Step::Freeform),
        1 => (any::<usize>(), any::<usize>()).prop_map(|(a, b)| Step::Message(a, b)),
        1 => any::<usize>().prop_map(Step::Note),
        1 => (any::<usize>(), -100..100i32).prop_map(|(t, d)| Step::Intervene(t, d)),
        1 => (any::<bool>(), -20..20i32).prop_map(|(p, d)| Step::RuleFirstPending(p, d)),
    ]
}

fn history() -> impl Strategy<Value = Vec<Vec<Step>>> {
    prop::collection::vec(prop::collection::vec(step(), 0..8), 1..30)
}

fn all_orders(sc: &Scenario) -> Vec<TurnInput> {
    let mut out = Vec::new();
    for seat in &sc.seats {
        for c in &seat.controls {
            let role = sc.role(&sc.character(c).unwrap().role).unwrap();
            for a in &role.actions {
                let targets: Vec<Option<String>> = match sc.action(a).unwrap().target {
                    TargetKind::None => vec![None],
                    TargetKind::Region => sc.regions.iter().map(|r| Some(r.id.0.clone())).collect(),
                    TargetKind::Character => {
                        sc.characters.iter().map(|x| Some(x.id.0.clone())).collect()
                    }
                };
                for target in targets {
                    out.push(TurnInput {
                        seat: seat.id.clone(),
                        submission: Submission::Order(Order::Action {
                            character: c.clone(),
                            action: a.clone(),
                            target,
                        }),
                    });
                }
            }
        }
    }
    out
}

fn run(sc: &Scenario, seed: u64, ticks: &[Vec<Step>]) -> (State, Vec<Event>) {
    let orders = all_orders(sc);
    let players: Vec<(SeatId, sim_core::CharacterId)> = sc
        .seats
        .iter()
        .flat_map(|s| s.controls.iter().map(|c| (s.id.clone(), c.clone())))
        .collect();
    let targets: Vec<Target> = sc
        .metrics
        .iter()
        .map(|m| Target::Metric(m.id.clone()))
        .chain(sc.regions.iter().map(|r| Target::Stat {
            region: r.id.clone(),
            stat: "unrest".into(),
        }))
        .collect();
    let gm = sc.seats.iter().find(|s| s.game_master).unwrap().id.clone();

    let mut log = engine::start(sc, seed);
    let mut state = State::replay(sc, &log);
    let push = |state: &mut State, log: &mut Vec<Event>, e: Event| {
        state.apply(&e);
        log.push(e);
    };
    for steps in ticks {
        let mut inputs = Vec::new();
        for s in steps {
            match *s {
                Step::Order(i) => inputs.push(orders[i % orders.len()].clone()),
                Step::Freeform(i) => {
                    let (seat, c) = &players[i % players.len()];
                    inputs.push(TurnInput {
                        seat: seat.clone(),
                        submission: Submission::Order(Order::Freeform {
                            character: c.clone(),
                            text: format!("A bold plan by {c}"),
                        }),
                    });
                }
                Step::Message(a, b) => {
                    let (seat, from) = &players[a % players.len()];
                    let to = &players[b % players.len()].1;
                    if let Ok(e) = engine::message(sc, &state, seat, from, to, "We should talk") {
                        push(&mut state, &mut log, e);
                    }
                }
                Step::Note(i) => {
                    let (seat, c) = &players[i % players.len()];
                    if let Ok(e) = engine::note(sc, &state, seat, c, "Remember this") {
                        push(&mut state, &mut log, e);
                    }
                }
                Step::Intervene(t, d) => {
                    let effect = Effect {
                        target: targets[t % targets.len()].clone(),
                        delta: f64::from(d) / 37.0,
                        scaled_by: None,
                    };
                    if let Ok(e) = engine::intervene(sc, &state, "An omen", "", vec![effect]) {
                        push(&mut state, &mut log, e);
                    }
                }
                Step::RuleFirstPending(public, d) => {
                    if let Some(id) = state.pending.keys().next() {
                        inputs.push(TurnInput {
                            seat: gm.clone(),
                            submission: Submission::Ruling(Ruling {
                                pending_id: id.clone(),
                                outcome: "So it shall be".into(),
                                effects: vec![Effect {
                                    target: Target::Metric("stability".into()),
                                    delta: f64::from(d) / 100.0,
                                    scaled_by: None,
                                }],
                                public,
                            }),
                        });
                    }
                }
            }
        }
        for e in close_tick(sc, &state, &inputs) {
            push(&mut state, &mut log, e);
        }
    }
    (state, log)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn replaying_a_serialized_log_rebuilds_the_live_state(seed in any::<u64>(), ticks in history()) {
        let sc = Scenario::from_yaml(ROME).unwrap();
        let (live, log) = run(&sc, seed, &ticks);
        let stored: Vec<String> = log.iter().map(|e| serde_json::to_string(e).unwrap()).collect();
        let loaded: Vec<Event> = stored.iter().map(|s| serde_json::from_str(s).unwrap()).collect();
        prop_assert_eq!(&loaded, &log);
        prop_assert_eq!(State::replay(&sc, &loaded), live);
    }

    #[test]
    fn a_state_snapshot_round_trips_through_json(seed in any::<u64>(), ticks in history()) {
        let sc = Scenario::from_yaml(ROME).unwrap();
        let (live, _) = run(&sc, seed, &ticks);
        let restored: State = serde_json::from_str(&serde_json::to_string(&live).unwrap()).unwrap();
        prop_assert_eq!(restored, live);
    }

    #[test]
    fn the_same_seed_and_steps_produce_the_same_history(seed in any::<u64>(), ticks in history()) {
        let sc = Scenario::from_yaml(ROME).unwrap();
        prop_assert_eq!(run(&sc, seed, &ticks).1, run(&sc, seed, &ticks).1);
    }
}
