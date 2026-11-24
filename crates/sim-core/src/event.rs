//! Events are the single source of truth. This schema is persistent: add fields with
//! `#[serde(default)]` and add variants; never repurpose an existing one.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ids::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Event {
    pub tick: Tick,
    pub cause: Cause,
    pub visibility: Visibility,
    pub kind: EventKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Cause {
    System,
    Seat(SeatId),
    Rule(RuleId),
    GameMaster(SeatId),
    User,
}

/// Game-master seats see every event regardless of visibility.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    /// Every seat in the world.
    Public,
    /// Every seat of one empire.
    Empire(EmpireId),
    Seats(Vec<SeatId>),
    /// Every seat of these empires, sorted and deduplicated.
    Empires(Vec<EmpireId>),
}

/// A concrete number to change. Ruleset effects resolve to these before they are logged.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Target {
    Metric {
        empire: EmpireId,
        metric: MetricId,
    },
    Stat {
        region: RegionId,
        stat: StatId,
    },
    Resource {
        character: CharacterId,
        resource: ResourceId,
    },
}

impl Target {
    pub(crate) fn metric(empire: &EmpireId, metric: &MetricId) -> Self {
        let (empire, metric) = (empire.clone(), metric.clone());
        Self::Metric { empire, metric }
    }

    pub(crate) fn stat(region: &RegionId, stat: &StatId) -> Self {
        let (region, stat) = (region.clone(), stat.clone());
        Self::Stat { region, stat }
    }

    pub(crate) fn resource(character: &CharacterId, resource: &ResourceId) -> Self {
        let (character, resource) = (character.clone(), resource.clone());
        Self::Resource {
            character,
            resource,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Effect {
    pub target: Target,
    pub delta: f64,
    /// The number this delta was scaled by. An observer who cannot see it does not see the
    /// effect either, since the delta would reveal it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scaled_by: Option<Target>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum EventKind {
    WorldStarted {
        seed: u64,
    },
    /// Every starting number, as the scenario set it or as drawn from the seed. Stored so a
    /// replay never runs generation again.
    WorldGenerated {
        values: Vec<StartingValue>,
    },
    SeatJoined {
        seat: SeatId,
    },
    TickStarted,
    ActionResolved {
        character: CharacterId,
        action: ActionId,
        #[serde(default)]
        target: Option<String>,
        effects: Vec<Effect>,
    },
    OfficeChanged {
        character: CharacterId,
        from: OfficeId,
        to: OfficeId,
    },
    ActionRejected {
        character: CharacterId,
        action: ActionId,
        reason: String,
    },
    FreeformDeclared {
        id: String,
        character: CharacterId,
        text: String,
    },
    FreeformRuled {
        id: String,
        outcome: String,
        effects: Vec<Effect>,
    },
    FreeformExpired {
        id: String,
    },
    MessageSent {
        from: CharacterId,
        to: CharacterId,
        text: String,
    },
    NoteWritten {
        character: CharacterId,
        text: String,
    },
    RuleFired {
        rule: RuleId,
        /// The empire whose numbers the rule read.
        empire: EmpireId,
        #[serde(default)]
        region: Option<RegionId>,
        #[serde(default)]
        label: Option<String>,
        effects: Vec<Effect>,
    },
    Intervention {
        title: String,
        #[serde(default)]
        text: String,
        effects: Vec<Effect>,
    },
    /// Two empires learn of each other.
    EmpiresMet {
        a: EmpireId,
        b: EmpireId,
    },
    TickEnded {
        /// Consecutive-tick counters for rule and ending conditions after this tick.
        streaks: BTreeMap<String, u32>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StartingValue {
    pub target: Target,
    pub value: f64,
}
