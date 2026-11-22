use sim_core::engine::{self, close_tick};
use sim_core::perceive::describe;
use sim_core::{Scenario, State};

const ROME: &str = include_str!("../../../scenarios/rome.yaml");

/// Rome left to the rules alone. A diff here means the simulation's behaviour changed.
#[test]
fn rome_left_to_the_rules_alone() {
    let sc = Scenario::from_yaml(ROME).unwrap();
    let mut log = engine::start(&sc, 2026);
    let mut state = State::replay(&sc, &log);
    for _ in 0..40 {
        let events = close_tick(&sc, &state, &[]);
        if events.is_empty() {
            break;
        }
        for e in events {
            state.apply(&e);
            log.push(e);
        }
    }
    let chronicle: Vec<String> = log
        .iter()
        .filter(|e| !matches!(e.kind, sim_core::EventKind::ActionResolved { .. }))
        .filter_map(|e| {
            describe(&sc, e, &sim_core::perceive::Sight::all())
                .map(|d| format!("[{}] {d}", sc.date(e.tick)))
        })
        .collect();
    insta::assert_yaml_snapshot!("rome_chronicle", chronicle);
    insta::assert_yaml_snapshot!("rome_final_metrics", state.metrics);
}
