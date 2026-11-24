use crate::engine::{self, Order, Ruling, Submission, TurnInput};
use crate::event::{Event, Target};
use crate::ids::*;
use crate::ruleset::Ruleset;
use crate::scenario::Scenario;
use crate::state::State;
use crate::world::World;

pub const TINY: &str = include_str!("../tests/fixtures/tiny.yaml");
pub const TINY_RULES: &str = include_str!("../tests/fixtures/tiny_rules.yaml");
pub const THREE: &str = include_str!("../tests/fixtures/three.yaml");
pub const ROME: &str = include_str!("../../../scenarios/rome.yaml");

/// A world and its live state, advanced the way the server does it.
pub struct Sim {
    pub w: World,
    pub state: State,
}

impl Sim {
    /// The tiny scenario on the tiny ruleset.
    pub fn tiny() -> Self {
        let sc = Scenario::from_yaml(TINY).expect("fixture scenario is valid");
        let rules = Ruleset::from_yaml(TINY_RULES).expect("fixture ruleset is valid");
        Self::start(World::new(sc, rules).expect("fixtures fit together"), 7)
    }

    /// Rome on the standard ruleset.
    pub fn rome(seed: u64) -> Self {
        let sc = Scenario::from_yaml(ROME).expect("rome is valid");
        Self::start(
            World::standard(sc).expect("rome fits the standard ruleset"),
            seed,
        )
    }

    /// Three empires on the standard ruleset: the Vale and the Ridge know each other, the
    /// Marsh knows nobody.
    pub fn three() -> Self {
        let sc = Scenario::from_yaml(THREE).expect("fixture scenario is valid");
        Self::start(
            World::standard(sc).expect("fixture fits the standard ruleset"),
            7,
        )
    }

    pub fn start(w: World, seed: u64) -> Self {
        let state = State::replay(&w, &engine::start(&w, seed));
        Self { w, state }
    }

    pub fn push(&mut self, e: Event) {
        self.state.apply(&e);
    }

    pub fn close(&mut self, inputs: &[TurnInput]) -> Vec<Event> {
        let events = engine::close_tick(&self.w, &self.state, inputs);
        for e in &events {
            self.state.apply(e);
        }
        events
    }

    pub fn metric(&self, empire: &str, id: &str) -> f64 {
        self.state.empires[&EmpireId::from(empire)].metrics[&MetricId::from(id)]
    }

    pub fn set_metric(&mut self, empire: &str, id: &str, v: f64) {
        self.state
            .empires
            .get_mut(&EmpireId::from(empire))
            .unwrap()
            .metrics
            .insert(id.into(), v);
    }

    pub fn stat(&self, region: &str, stat: &str) -> f64 {
        self.state.regions[&RegionId::from(region)].stats[&StatId::from(stat)]
    }

    pub fn resource(&self, character: &str, res: &str) -> f64 {
        self.state.characters[&CharacterId::from(character)].resources[&ResourceId::from(res)]
    }

    pub fn set_resource(&mut self, character: &str, res: &str, v: f64) {
        self.state
            .characters
            .get_mut(&CharacterId::from(character))
            .unwrap()
            .resources
            .insert(res.into(), v);
    }
}

pub fn metric(empire: &str, metric: &str) -> Target {
    Target::Metric {
        empire: empire.into(),
        metric: metric.into(),
    }
}

pub fn act(seat: &str, character: &str, action: &str, target: Option<&str>) -> TurnInput {
    TurnInput {
        seat: seat.into(),
        submission: Submission::Order(Order::Action {
            character: character.into(),
            action: action.into(),
            target: target.map(str::to_owned),
            choice: None,
        }),
    }
}

pub fn appoint(seat: &str, character: &str, target: &str, office: &str) -> TurnInput {
    TurnInput {
        seat: seat.into(),
        submission: Submission::Order(Order::Action {
            character: character.into(),
            action: "appoint".into(),
            target: Some(target.into()),
            choice: Some(office.into()),
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
