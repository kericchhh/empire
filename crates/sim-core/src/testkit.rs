use crate::engine::{self, Order, Ruling, Submission, TurnInput};
use crate::event::Event;
use crate::ids::*;
use crate::scenario::Scenario;
use crate::state::State;

pub const TINY: &str = include_str!("../tests/fixtures/tiny.yaml");
pub const ROME: &str = include_str!("../../../scenarios/rome.yaml");

pub struct World {
    pub sc: Scenario,
    pub state: State,
}

impl World {
    pub fn tiny() -> Self {
        Self::new(TINY, 7)
    }

    pub fn new(yaml: &str, seed: u64) -> Self {
        let sc = Scenario::from_yaml(yaml).expect("fixture scenario is valid");
        let state = State::replay(&sc, &engine::start(&sc, seed));
        Self { sc, state }
    }

    pub fn push(&mut self, e: Event) {
        self.state.apply(&e);
    }

    pub fn close(&mut self, inputs: &[TurnInput]) -> Vec<Event> {
        let events = engine::close_tick(&self.sc, &self.state, inputs);
        for e in &events {
            self.state.apply(e);
        }
        events
    }

    pub fn metric(&self, id: &str) -> f64 {
        self.state.metrics[&MetricId::from(id)]
    }

    pub fn stat(&self, region: &str, stat: &str) -> f64 {
        self.state.regions[&RegionId::from(region)][&StatId::from(stat)]
    }

    pub fn resource(&self, character: &str, res: &str) -> f64 {
        self.state.characters[&CharacterId::from(character)].resources[&ResourceId::from(res)]
    }
}

pub fn act(seat: &str, character: &str, action: &str, target: Option<&str>) -> TurnInput {
    TurnInput {
        seat: seat.into(),
        submission: Submission::Order(Order::Action {
            character: character.into(),
            action: action.into(),
            target: target.map(str::to_owned),
        }),
    }
}

pub fn freeform(seat: &str, character: &str, text: &str) -> TurnInput {
    TurnInput {
        seat: seat.into(),
        submission: Submission::Order(Order::Freeform {
            character: character.into(),
            text: text.into(),
        }),
    }
}

pub fn ruling(seat: &str, ruling: Ruling) -> TurnInput {
    TurnInput {
        seat: seat.into(),
        submission: Submission::Ruling(ruling),
    }
}

pub fn approx(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}
