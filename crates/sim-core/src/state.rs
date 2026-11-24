use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::event::{Effect, Event, EventKind, Target};
use crate::ids::*;
use crate::ruleset::StatDef;
use crate::world::World;

/// The world at one moment. Built only by `State::new` followed by `apply` for each event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct State {
    pub tick: Tick,
    pub seed: u64,
    pub empires: BTreeMap<EmpireId, EmpireState>,
    pub regions: BTreeMap<RegionId, RegionState>,
    pub characters: BTreeMap<CharacterId, CharacterState>,
    pub joined: BTreeSet<SeatId>,
    /// Free-form actions awaiting a game-master ruling.
    pub pending: BTreeMap<String, Pending>,
    pub streaks: BTreeMap<String, u32>,
    /// Events of the previous and the current tick, for perception.
    pub recent: Vec<Event>,
    bounds: Bounds,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EmpireState {
    pub metrics: BTreeMap<MetricId, f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RegionState {
    pub owner: EmpireId,
    pub stats: BTreeMap<StatId, f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CharacterState {
    pub empire: EmpireId,
    pub office: OfficeId,
    pub region: RegionId,
    pub resources: BTreeMap<ResourceId, f64>,
    pub notes: Vec<String>,
    #[serde(default = "alive")]
    pub alive: bool,
}

fn alive() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Pending {
    pub character: CharacterId,
    pub text: String,
    pub declared: Tick,
}

type Range = (Option<f64>, Option<f64>);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
struct Bounds {
    metrics: BTreeMap<MetricId, Range>,
    stats: BTreeMap<StatId, Range>,
    resources: BTreeMap<ResourceId, Range>,
    notes_kept: usize,
}

impl Bounds {
    fn of(&self, t: &Target) -> Range {
        match t {
            Target::Metric { metric, .. } => self.metrics.get(metric),
            Target::Stat { stat, .. } => self.stats.get(stat),
            Target::Resource { resource, .. } => self.resources.get(resource),
        }
        .copied()
        .unwrap_or_default()
    }
}

fn clamp((min, max): Range, v: f64) -> f64 {
    let v = min.map_or(v, |m| v.max(m));
    max.map_or(v, |m| v.min(m))
}

fn initial<Id: Clone + Ord>(defs: &[StatDef<Id>]) -> BTreeMap<Id, f64> {
    defs.iter()
        .map(|d| (d.id.clone(), clamp((d.min, d.max), d.initial)))
        .collect()
}

fn ranges<Id: Clone + Ord>(defs: &[StatDef<Id>]) -> BTreeMap<Id, Range> {
    defs.iter()
        .map(|d| (d.id.clone(), (d.min, d.max)))
        .collect()
}

impl State {
    /// The world before its first event: every number at the ruleset's `initial`, until
    /// `WorldGenerated` sets the starting values.
    pub fn new(w: &World) -> Self {
        let rules = &w.rules;
        let empires = w
            .scenario
            .empires
            .iter()
            .map(|e| {
                let metrics = initial(&rules.metrics);
                (e.id.clone(), EmpireState { metrics })
            })
            .collect();
        let regions = w
            .scenario
            .regions()
            .map(|(e, r)| {
                let owner = e.id.clone();
                let stats = initial(&rules.region_stats);
                (r.id.clone(), RegionState { owner, stats })
            })
            .collect();
        let characters = w
            .scenario
            .characters()
            .map(|(e, c)| {
                let cs = CharacterState {
                    empire: e.id.clone(),
                    office: c.office.clone(),
                    region: c.region.clone(),
                    resources: initial(&rules.resources),
                    notes: Vec::new(),
                    alive: true,
                };
                (c.id.clone(), cs)
            })
            .collect();
        let bounds = Bounds {
            metrics: ranges(&rules.metrics),
            stats: ranges(&rules.region_stats),
            resources: ranges(&rules.resources),
            notes_kept: rules.limits.notes_kept,
        };
        Self {
            tick: Tick(0),
            seed: 0,
            empires,
            regions,
            characters,
            joined: BTreeSet::new(),
            pending: BTreeMap::new(),
            streaks: BTreeMap::new(),
            recent: Vec::new(),
            bounds,
        }
    }

    /// Rebuild state from a full event log.
    pub fn replay<'a>(w: &World, events: impl IntoIterator<Item = &'a Event>) -> Self {
        let mut s = Self::new(w);
        for e in events {
            s.apply(e);
        }
        s
    }

    pub fn value(&self, t: &Target) -> Option<f64> {
        match t {
            Target::Metric { empire, metric } => self.metric(empire, metric),
            Target::Stat { region, stat } => self.stat(region, stat),
            Target::Resource {
                character,
                resource,
            } => self
                .characters
                .get(character)?
                .resources
                .get(resource)
                .copied(),
        }
    }

    pub fn metric(&self, empire: &EmpireId, metric: &MetricId) -> Option<f64> {
        self.empires.get(empire)?.metrics.get(metric).copied()
    }

    pub fn stat(&self, region: &RegionId, stat: &StatId) -> Option<f64> {
        self.regions.get(region)?.stats.get(stat).copied()
    }

    /// Regions `empire` owns, in id order.
    pub fn regions_of<'a>(&'a self, empire: &'a EmpireId) -> impl Iterator<Item = &'a RegionId> {
        self.regions
            .iter()
            .filter(move |(_, r)| &r.owner == empire)
            .map(|(id, _)| id)
    }

    /// Set the number `t` names to `next` of its current value, clamped to its bounds. A
    /// missing target or a non-finite result leaves it unchanged.
    fn adjust(&mut self, t: &Target, next: impl FnOnce(f64) -> f64) {
        let range = self.bounds.of(t);
        let slot = match t {
            Target::Metric { empire, metric } => self
                .empires
                .get_mut(empire)
                .and_then(|e| e.metrics.get_mut(metric)),
            Target::Stat { region, stat } => self
                .regions
                .get_mut(region)
                .and_then(|r| r.stats.get_mut(stat)),
            Target::Resource {
                character,
                resource,
            } => self
                .characters
                .get_mut(character)
                .and_then(|c| c.resources.get_mut(resource)),
        };
        if let Some(v) = slot {
            let n = clamp(range, next(*v));
            if n.is_finite() {
                *v = n;
            }
        }
    }

    /// The only way state changes. Infallible: events are validated before they exist.
    pub fn apply(&mut self, e: &Event) {
        match &e.kind {
            EventKind::WorldStarted { seed } => self.seed = *seed,
            EventKind::WorldGenerated { values } => {
                for v in values {
                    self.adjust(&v.target, |_| v.value);
                }
            }
            EventKind::SeatJoined { seat } => {
                self.joined.insert(seat.clone());
            }
            EventKind::TickStarted => {
                self.tick = e.tick;
                let keep_from = e.tick.0.saturating_sub(1);
                self.recent.retain(|r| r.tick.0 >= keep_from);
            }
            EventKind::ActionResolved { effects, .. }
            | EventKind::RuleFired { effects, .. }
            | EventKind::Intervention { effects, .. } => self.apply_effects(effects),
            EventKind::OfficeChanged { character, to, .. } => {
                if let Some(c) = self.characters.get_mut(character) {
                    c.office = to.clone();
                }
            }
            EventKind::CharacterDied { character } => {
                if let Some(c) = self.characters.get_mut(character) {
                    c.alive = false;
                }
            }
            EventKind::ActionRejected { .. } | EventKind::MessageSent { .. } => {}
            EventKind::FreeformDeclared {
                id,
                character,
                text,
            } => {
                self.pending.insert(
                    id.clone(),
                    Pending {
                        character: character.clone(),
                        text: text.clone(),
                        declared: e.tick,
                    },
                );
            }
            EventKind::FreeformRuled { id, effects, .. } => {
                self.pending.remove(id);
                self.apply_effects(effects);
            }
            EventKind::FreeformExpired { id } => {
                self.pending.remove(id);
            }
            EventKind::NoteWritten { character, text } => {
                if let Some(c) = self.characters.get_mut(character) {
                    c.notes.push(text.clone());
                    let excess = c.notes.len().saturating_sub(self.bounds.notes_kept);
                    c.notes.drain(..excess);
                }
            }
            EventKind::TickEnded { streaks } => self.streaks = streaks.clone(),
        }
        self.recent.push(e.clone());
    }

    fn apply_effects(&mut self, effects: &[Effect]) {
        for Effect { target, delta, .. } in effects {
            self.adjust(target, |v| v + delta);
        }
    }
}
