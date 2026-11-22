use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::event::{Effect, Event, EventKind, Target};
use crate::ids::*;
use crate::scenario::Scenario;

/// The world at one moment. Built only by `State::new` followed by `apply` for each event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct State {
    pub tick: Tick,
    pub seed: u64,
    pub ended: Option<Ending>,
    pub metrics: BTreeMap<MetricId, f64>,
    pub regions: BTreeMap<RegionId, BTreeMap<StatId, f64>>,
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
pub struct Ending {
    pub ending: EndingId,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CharacterState {
    pub resources: BTreeMap<ResourceId, f64>,
    pub notes: Vec<String>,
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

fn clamp((min, max): Range, v: f64) -> f64 {
    let v = min.map_or(v, |m| v.max(m));
    max.map_or(v, |m| v.min(m))
}

impl State {
    pub fn new(sc: &Scenario) -> Self {
        let metrics = sc
            .metrics
            .iter()
            .map(|m| (m.id.clone(), m.clamp(m.initial)))
            .collect();
        let regions = sc
            .regions
            .iter()
            .map(|r| {
                let stats = sc
                    .region_stats
                    .iter()
                    .map(|s| {
                        let v = r.stats.get(&s.id).copied().unwrap_or(s.initial);
                        (s.id.clone(), s.clamp(v))
                    })
                    .collect();
                (r.id.clone(), stats)
            })
            .collect();
        let characters = sc
            .characters
            .iter()
            .map(|c| {
                let resources = sc
                    .resources
                    .iter()
                    .map(|s| {
                        let v = c.resources.get(&s.id).copied().unwrap_or(s.initial);
                        (s.id.clone(), s.clamp(v))
                    })
                    .collect();
                (
                    c.id.clone(),
                    CharacterState {
                        resources,
                        notes: Vec::new(),
                    },
                )
            })
            .collect();
        let bounds = Bounds {
            metrics: sc
                .metrics
                .iter()
                .map(|m| (m.id.clone(), (m.min, m.max)))
                .collect(),
            stats: sc
                .region_stats
                .iter()
                .map(|m| (m.id.clone(), (m.min, m.max)))
                .collect(),
            resources: sc
                .resources
                .iter()
                .map(|m| (m.id.clone(), (m.min, m.max)))
                .collect(),
            notes_kept: sc.limits.notes_kept,
        };
        Self {
            tick: Tick(0),
            seed: 0,
            ended: None,
            metrics,
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
    pub fn replay<'a>(sc: &Scenario, events: impl IntoIterator<Item = &'a Event>) -> Self {
        let mut s = Self::new(sc);
        for e in events {
            s.apply(e);
        }
        s
    }

    pub fn value(&self, t: &Target) -> Option<f64> {
        match t {
            Target::Metric(m) => self.metrics.get(m).copied(),
            Target::Stat { region, stat } => self.regions.get(region)?.get(stat).copied(),
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

    /// The only way state changes. Infallible: events are validated before they exist.
    pub fn apply(&mut self, e: &Event) {
        match &e.kind {
            EventKind::WorldStarted { seed } => self.seed = *seed,
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
            EventKind::WorldEnded { ending, label } => {
                self.ended = Some(Ending {
                    ending: ending.clone(),
                    label: label.clone(),
                });
            }
        }
        self.recent.push(e.clone());
    }

    fn apply_effects(&mut self, effects: &[Effect]) {
        for Effect { target, delta, .. } in effects {
            let (slot, range) = match target {
                Target::Metric(m) => (self.metrics.get_mut(m), self.bounds.metrics.get(m)),
                Target::Stat { region, stat } => (
                    self.regions.get_mut(region).and_then(|r| r.get_mut(stat)),
                    self.bounds.stats.get(stat),
                ),
                Target::Resource {
                    character,
                    resource,
                } => (
                    self.characters
                        .get_mut(character)
                        .and_then(|c| c.resources.get_mut(resource)),
                    self.bounds.resources.get(resource),
                ),
            };
            if let Some(v) = slot {
                let next = clamp(range.copied().unwrap_or((None, None)), *v + delta);
                if next.is_finite() {
                    *v = next;
                }
            }
        }
    }
}
