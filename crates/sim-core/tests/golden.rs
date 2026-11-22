use sim_core::engine::{self, close_tick};
use sim_core::perceive::{Sight, describe};
use sim_core::{EventKind, Scenario, State, World};

const ROME: &str = include_str!("../../../scenarios/rome.yaml");

/// Rome and Parthia left to the rules alone. A diff here means the simulation's behaviour
/// changed.
#[test]
fn rome_and_parthia_left_to_the_rules_alone() {
    let w = World::standard(Scenario::from_yaml(ROME).unwrap()).unwrap();
    let mut log = engine::start(&w, 2026);
    let mut state = State::replay(&w, &log);
    for _ in 0..40 {
        for e in close_tick(&w, &state, &[]) {
            state.apply(&e);
            log.push(e);
        }
    }
    let chronicle: Vec<String> = log
        .iter()
        .filter(|e| !matches!(e.kind, EventKind::ActionResolved { .. }))
        .filter_map(|e| describe(&w, e, &Sight::all()).map(|d| format!("[{}] {d}", w.date(e.tick))))
        .collect();
    insta::assert_yaml_snapshot!("rome_chronicle", chronicle);
    insta::assert_yaml_snapshot!("rome_final_metrics", state.empires);
}
