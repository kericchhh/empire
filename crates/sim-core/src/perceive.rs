use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::event::{Effect, Event, EventKind, Target, Visibility};
use crate::ids::*;
use crate::ruleset::{RegionSight, TargetKind, ValueRef};
use crate::state::State;
use crate::world::World;

/// Everything one seat may know this tick. Self-contained so a fresh agent can play from it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Perception {
    pub world: WorldView,
    pub seat: SeatId,
    pub game_master: bool,
    /// The empire this seat plays for. `None` for the game master.
    pub your_empire: Option<EmpireId>,
    /// Every empire in the world. Numbers appear only where this seat can see them.
    pub empires: Vec<EmpireView>,
    pub you_play: Vec<CharacterView>,
    pub regions: Vec<RegionView>,
    /// Everyone this seat knows of: its own empire's people, and foreign holders of
    /// public offices.
    pub people: Vec<Person>,
    /// What this seat saw happen during the previous and the current tick, oldest first.
    pub recent: Vec<String>,
    /// Game master only: free-form actions to rule on before the tick closes.
    pub pending_rulings: Vec<PendingView>,
    pub orders_per_character: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WorldView {
    pub name: String,
    pub description: String,
    pub date: String,
    pub tick: Tick,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EmpireView {
    pub id: EmpireId,
    pub name: String,
    pub description: String,
    pub culture: BTreeMap<String, String>,
    pub metrics: Vec<Number>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CharacterView {
    pub id: CharacterId,
    pub name: String,
    pub title: String,
    pub office: String,
    pub office_description: String,
    pub region: RegionId,
    pub persona: String,
    pub goals: Vec<String>,
    pub secrets: Vec<String>,
    pub resources: Vec<Number>,
    /// Your own notes from earlier turns, oldest first.
    pub notes: Vec<String>,
    pub actions: Vec<ActionView>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ActionView {
    pub id: ActionId,
    pub name: String,
    pub description: String,
    pub target: TargetKind,
    pub cost: BTreeMap<ResourceId, f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Number {
    pub id: String,
    pub name: String,
    pub value: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RegionView {
    pub id: RegionId,
    pub name: String,
    pub description: String,
    pub owner: EmpireId,
    pub stats: Vec<Number>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Person {
    pub id: CharacterId,
    pub name: String,
    pub title: String,
    pub office: String,
    pub empire: EmpireId,
    /// Shown only when the region is in sight.
    pub region: Option<RegionId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PendingView {
    pub id: String,
    pub character: CharacterId,
    pub text: String,
}

/// Whether an observer in `empire` knows `character` exists: everyone in their own empire,
/// and foreign holders of public offices. `None` is the game master, who knows everyone.
pub fn knows(w: &World, state: &State, empire: Option<&EmpireId>, character: &CharacterId) -> bool {
    let Some(cs) = state.characters.get(character) else {
        return false;
    };
    match empire {
        None => true,
        Some(e) => &cs.empire == e || w.rules.office(&cs.office).is_some_and(|o| o.public),
    }
}

pub fn can_see(w: &World, seat: &SeatId, e: &Event) -> bool {
    let Some(s) = w.seat(seat) else {
        return false;
    };
    if s.game_master {
        return true;
    }
    match &e.visibility {
        Visibility::Public => true,
        Visibility::Empire(empire) => s.empire == Some(empire),
        Visibility::Seats(seats) => seats.contains(seat),
    }
}

/// `None` for an unknown seat.
pub fn perceive(w: &World, state: &State, seat: &SeatId) -> Option<Perception> {
    let sref = w.seat(seat)?;
    let gm = sref.game_master;
    let sight = Sight::of_seat(w, state, seat)?;
    let rules = &w.rules;

    let you_play = sref
        .controls
        .iter()
        .filter_map(|c| Some((w.character(c)?, state.characters.get(c)?)))
        .map(|(c, cs)| {
            let office = rules.office(&cs.office);
            CharacterView {
                id: c.id.clone(),
                name: c.name.clone(),
                title: c.title.clone(),
                office: office.map_or_else(|| cs.office.to_string(), |o| o.name.clone()),
                office_description: office.map_or_else(String::new, |o| o.description.clone()),
                region: cs.region.clone(),
                persona: c.persona.clone(),
                goals: c.goals.clone(),
                secrets: c.secrets.clone(),
                resources: rules
                    .resources
                    .iter()
                    .map(|r| Number {
                        id: r.id.0.clone(),
                        name: r.name.clone(),
                        value: cs.resources.get(&r.id).copied().unwrap_or(0.0),
                    })
                    .collect(),
                notes: cs.notes.clone(),
                actions: office
                    .into_iter()
                    .flat_map(|o| &o.actions)
                    .filter_map(|a| rules.action(a))
                    .map(|a| ActionView {
                        id: a.id.clone(),
                        name: a.name.clone(),
                        description: a.description.clone(),
                        target: a.target,
                        cost: a.cost.clone(),
                    })
                    .collect(),
            }
        })
        .collect();

    let empires = w
        .scenario
        .empires
        .iter()
        .map(|e| EmpireView {
            id: e.id.clone(),
            name: e.name.clone(),
            description: e.description.clone(),
            culture: e.culture.clone(),
            metrics: rules
                .metrics
                .iter()
                .filter(|m| sight.metric(&e.id, &m.id))
                .map(|m| Number {
                    id: m.id.0.clone(),
                    name: m.name.clone(),
                    value: state.metric(&e.id, &m.id).unwrap_or(0.0),
                })
                .collect(),
        })
        .collect();

    let regions = w
        .scenario
        .regions()
        .filter(|(_, r)| sight.region(&r.id))
        .filter_map(|(_, r)| Some((r, state.regions.get(&r.id)?)))
        .map(|(r, rs)| RegionView {
            id: r.id.clone(),
            name: r.name.clone(),
            description: r.description.clone(),
            owner: rs.owner.clone(),
            stats: rules
                .region_stats
                .iter()
                .filter(|s| sight.stat(&s.id))
                .map(|s| Number {
                    id: s.id.0.clone(),
                    name: s.name.clone(),
                    value: rs.stats.get(&s.id).copied().unwrap_or(0.0),
                })
                .collect(),
        })
        .collect();

    let people = w
        .scenario
        .characters()
        .filter(|(_, c)| knows(w, state, sref.empire, &c.id))
        .filter_map(|(_, c)| Some((c, state.characters.get(&c.id)?)))
        .map(|(c, cs)| Person {
            id: c.id.clone(),
            name: c.name.clone(),
            title: c.title.clone(),
            office: rules
                .office(&cs.office)
                .map_or_else(|| cs.office.to_string(), |o| o.name.clone()),
            empire: cs.empire.clone(),
            region: sight.region(&cs.region).then(|| cs.region.clone()),
        })
        .collect();

    let recent = state
        .recent
        .iter()
        .filter(|e| can_see(w, seat, e))
        .filter_map(|e| describe(w, e, &sight).map(|d| format!("[{}] {d}", w.date(e.tick))))
        .collect();

    let pending_rulings = if gm {
        state
            .pending
            .iter()
            .map(|(id, p)| PendingView {
                id: id.clone(),
                character: p.character.clone(),
                text: p.text.clone(),
            })
            .collect()
    } else {
        Vec::new()
    };

    Some(Perception {
        world: WorldView {
            name: w.scenario.name.clone(),
            description: w.scenario.description.clone(),
            date: w.date(state.tick),
            tick: state.tick,
        },
        seat: seat.clone(),
        game_master: gm,
        your_empire: sref.empire.cloned(),
        empires,
        you_play,
        regions,
        people,
        recent,
        pending_rulings,
        orders_per_character: rules.limits.orders_per_character,
    })
}

fn name_of(w: &World, c: &CharacterId) -> String {
    w.character(c)
        .map_or_else(|| c.to_string(), |d| d.name.clone())
}

fn target_name(w: &World, t: &Target) -> String {
    match t {
        Target::Metric { empire, metric } => format!(
            "{} of {}",
            w.rules
                .metric(metric)
                .map_or_else(|| metric.to_string(), |d| d.name.clone()),
            w.empire(empire)
                .map_or_else(|| empire.to_string(), |d| d.name.clone()),
        ),
        Target::Stat { region, stat } => format!(
            "{} in {}",
            w.rules
                .region_stat(stat)
                .map_or_else(|| stat.to_string(), |d| d.name.clone()),
            w.region(region)
                .map_or_else(|| region.to_string(), |d| d.name.clone()),
        ),
        Target::Resource {
            character,
            resource,
        } => format!(
            "{} of {}",
            w.rules
                .resource(resource)
                .map_or_else(|| resource.to_string(), |d| d.name.clone()),
            name_of(w, character),
        ),
    }
}

fn effects_text(w: &World, sight: &Sight, effects: &[Effect]) -> String {
    let parts: Vec<String> = effects
        .iter()
        .filter(|e| sight.effect(e))
        .map(|e| format!("{} {:+}", target_name(w, &e.target), round(e.delta)))
        .collect();
    if parts.is_empty() {
        return String::new();
    }
    format!(" ({})", parts.join(", "))
}

/// What numbers one observer may see. The only place visibility of numbers is decided.
/// A seat sees its own empire's metrics that its offices may see, the regions its offices
/// look over, and its own characters' resources. Nothing of other empires' numbers.
#[derive(Debug, Clone)]
pub struct Sight {
    all: bool,
    empire: Option<EmpireId>,
    metrics: BTreeSet<MetricId>,
    stats: BTreeSet<StatId>,
    regions: BTreeSet<RegionId>,
    characters: BTreeSet<CharacterId>,
}

impl Sight {
    /// For the game master and the admin.
    pub fn all() -> Self {
        Self {
            all: true,
            empire: None,
            metrics: BTreeSet::new(),
            stats: BTreeSet::new(),
            regions: BTreeSet::new(),
            characters: BTreeSet::new(),
        }
    }

    pub fn of_seat(w: &World, state: &State, seat: &SeatId) -> Option<Self> {
        let sref = w.seat(seat)?;
        let Some(empire) = sref.empire else {
            return Some(Self::all());
        };
        let chars: Vec<_> = sref
            .controls
            .iter()
            .filter_map(|c| state.characters.get(c))
            .collect();
        let offices: BTreeSet<&OfficeId> = chars.iter().map(|c| &c.office).collect();
        let sees_empire = offices.iter().any(|o| {
            w.rules
                .office(o)
                .is_some_and(|o| o.region_sight == RegionSight::Empire)
        });
        let rules = &w.rules;
        Some(Self {
            all: false,
            empire: Some(empire.clone()),
            metrics: rules
                .metrics
                .iter()
                .filter(|m| offices.iter().any(|o| m.visible_to_office(o)))
                .map(|m| m.id.clone())
                .collect(),
            stats: rules
                .region_stats
                .iter()
                .filter(|s| offices.iter().any(|o| s.visible_to_office(o)))
                .map(|s| s.id.clone())
                .collect(),
            regions: if sees_empire {
                state.regions_of(empire).cloned().collect()
            } else {
                chars
                    .iter()
                    .filter(|c| {
                        state
                            .regions
                            .get(&c.region)
                            .is_some_and(|r| &r.owner == empire)
                    })
                    .map(|c| c.region.clone())
                    .collect()
            },
            characters: sref.controls.iter().cloned().collect(),
        })
    }

    pub fn metric(&self, empire: &EmpireId, m: &MetricId) -> bool {
        self.all || (self.empire.as_ref() == Some(empire) && self.metrics.contains(m))
    }

    pub fn stat(&self, s: &StatId) -> bool {
        self.all || self.stats.contains(s)
    }

    pub fn region(&self, r: &RegionId) -> bool {
        self.all || self.regions.contains(r)
    }

    /// Whether this observer may see an effect: its target, and whatever scaled it.
    pub fn effect(&self, e: &Effect) -> bool {
        self.target(&e.target) && e.scaled_by.as_ref().is_none_or(|t| self.target(t))
    }

    pub fn target(&self, t: &Target) -> bool {
        match t {
            Target::Metric { empire, metric } => self.metric(empire, metric),
            Target::Stat { region, stat } => self.region(region) && self.stat(stat),
            Target::Resource { character, .. } => self.all || self.characters.contains(character),
        }
    }
}

fn round(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

/// A rule's label announces that its condition held, so it is only shown to observers who
/// could see the value the condition reads.
fn condition_visible(
    w: &World,
    sight: &Sight,
    rule: &RuleId,
    empire: &EmpireId,
    region: &Option<RegionId>,
) -> bool {
    let Some(cond) = w.rules.rule(rule).and_then(|r| r.when.as_ref()) else {
        return true;
    };
    match &cond.value {
        ValueRef::Metric(m) => sight.metric(empire, m),
        ValueRef::Stat(s) => sight.stat(s) && region.as_ref().is_none_or(|r| sight.region(r)),
    }
}

/// One chronicle line, or `None` for bookkeeping events. Agent-written text is quoted so it
/// reads as in-world speech, never as instructions.
pub fn describe(w: &World, e: &Event, sight: &Sight) -> Option<String> {
    Some(match &e.kind {
        EventKind::WorldStarted { .. }
        | EventKind::WorldGenerated { .. }
        | EventKind::TickStarted
        | EventKind::TickEnded { .. }
        | EventKind::NoteWritten { .. }
        | EventKind::RuleFired { label: None, .. } => return None,
        EventKind::RuleFired {
            region: Some(r), ..
        } if !sight.region(r) => return None,
        EventKind::RuleFired {
            rule,
            empire,
            region,
            ..
        } if !condition_visible(w, sight, rule, empire, region) => {
            return None;
        }
        EventKind::SeatJoined { seat } => format!(
            "{} has taken their seat.",
            w.seat(seat)
                .map_or_else(|| seat.to_string(), |s| s.name.to_owned())
        ),
        EventKind::ActionResolved {
            character,
            action,
            target,
            effects,
        } => {
            let adef = w.rules.action(action);
            let aname = adef.map_or_else(|| action.to_string(), |a| a.name.clone());
            let on = target
                .as_ref()
                .and_then(|t| {
                    let name = match adef?.target {
                        TargetKind::Region => {
                            let region = RegionId::from(t.as_str());
                            if !sight.region(&region) {
                                return None;
                            }
                            w.region(&region)?.name.clone()
                        }
                        TargetKind::Character => name_of(w, &CharacterId::from(t.as_str())),
                        TargetKind::None => return None,
                    };
                    Some(format!(" on {name}"))
                })
                .unwrap_or_default();
            format!(
                "{}: {aname}{on}{}",
                name_of(w, character),
                effects_text(w, sight, effects)
            )
        }
        EventKind::ActionRejected {
            character,
            action,
            reason,
        } => format!("{} could not {action}: {reason}", name_of(w, character)),
        EventKind::FreeformDeclared {
            id,
            character,
            text,
        } => format!("{} attempts [{id}]: «{text}»", name_of(w, character)),
        EventKind::FreeformRuled {
            id,
            outcome,
            effects,
        } => format!(
            "Ruling on [{id}]: «{outcome}»{}",
            effects_text(w, sight, effects)
        ),
        EventKind::FreeformExpired { id } => format!("[{id}] came to nothing."),
        EventKind::MessageSent { from, to, text } => format!(
            "Message from {} to {}: «{text}»",
            name_of(w, from),
            name_of(w, to)
        ),
        EventKind::RuleFired {
            label: Some(l),
            effects,
            ..
        } => format!("{l}{}", effects_text(w, sight, effects)),
        EventKind::Intervention {
            title,
            text,
            effects,
        } => {
            let body = if text.is_empty() {
                String::new()
            } else {
                format!(" «{text}»")
            };
            format!("{title}{body}{}", effects_text(w, sight, effects))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::note;
    use crate::testkit::*;

    fn view(s: &Sim, seat: &str) -> Perception {
        perceive(&s.w, &s.state, &seat.into()).unwrap()
    }

    fn recent(s: &Sim, seat: &str) -> String {
        view(s, seat).recent.join("\n")
    }

    fn metric_ids(v: &Perception, empire: &str) -> Vec<String> {
        v.empires
            .iter()
            .find(|e| e.id.as_str() == empire)
            .unwrap()
            .metrics
            .iter()
            .map(|m| m.id.clone())
            .collect()
    }

    #[test]
    fn unknown_seat_perceives_nothing() {
        let s = Sim::tiny();
        assert!(perceive(&s.w, &s.state, &"nobody".into()).is_none());
    }

    #[test]
    fn a_seat_sees_its_own_characters_with_secrets_and_office_actions() {
        let s = Sim::tiny();
        let v = view(&s, "crown");
        assert_eq!(v.your_empire, Some("kingdom".into()));
        assert_eq!(v.you_play.len(), 1);
        let queen = &v.you_play[0];
        assert_eq!(
            (queen.name.as_str(), queen.title.as_str()),
            ("Queen Ada", "Queen")
        );
        assert_eq!(queen.office, "Head of state");
        let actions: Vec<_> = queen.actions.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(actions, ["decree", "feast", "spy"]);
        assert_eq!(v.world.date, "Spring, 1");
        assert_eq!(v.empires[0].culture["religion"], "The Old Gods");
    }

    #[test]
    fn a_seat_knows_its_own_people_and_foreign_public_figures() {
        let s = Sim::tiny();
        let ids: Vec<String> = view(&s, "crown")
            .people
            .into_iter()
            .map(|p| p.id.0)
            .collect();
        assert_eq!(ids, ["queen", "duke", "northfolk", "pirate_king"]);
        let king = view(&s, "crown")
            .people
            .into_iter()
            .find(|p| p.id.as_str() == "pirate_king")
            .unwrap();
        assert_eq!(king.region, None, "his region is not in sight");
        assert_eq!(view(&s, "gm").people.len(), 5);
    }

    #[test]
    fn restricted_metrics_are_hidden_from_other_offices() {
        let s = Sim::tiny();
        assert_eq!(
            metric_ids(&view(&s, "crown"), "kingdom"),
            ["treasury", "stability"]
        );
        assert_eq!(metric_ids(&view(&s, "folk"), "kingdom"), ["stability"]);
        assert_eq!(
            metric_ids(&view(&s, "gm"), "isles"),
            ["treasury", "stability"]
        );
    }

    #[test]
    fn foreign_numbers_and_territory_are_hidden() {
        let s = Sim::tiny();
        let v = view(&s, "crown");
        assert!(metric_ids(&v, "isles").is_empty());
        assert_eq!(v.empires[1].name, "The Isles", "but the empire is known");
        let regions: Vec<_> = v.regions.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(regions, ["north", "south"]);
    }

    #[test]
    fn home_sight_limits_regions_to_the_home_region() {
        let s = Sim::tiny();
        let regions =
            |seat| -> Vec<RegionId> { view(&s, seat).regions.into_iter().map(|r| r.id).collect() };
        assert_eq!(regions("folk"), [RegionId::from("north")]);
        assert_eq!(regions("gm").len(), 3);
    }

    #[test]
    fn notes_are_private_to_their_seat() {
        let mut s = Sim::tiny();
        let e = note(
            &s.w,
            &s.state,
            &"crown".into(),
            &"queen".into(),
            "Trust no one",
        )
        .unwrap();
        s.push(e);
        assert_eq!(view(&s, "crown").you_play[0].notes, ["Trust no one"]);
        assert!(!recent(&s, "rival").contains("Trust no one"));
    }

    #[test]
    fn only_the_game_master_sees_pending_rulings() {
        let mut s = Sim::tiny();
        s.close(&[freeform("crown", "queen", "I build a tower")]);
        assert_eq!(view(&s, "gm").pending_rulings.len(), 1);
        assert!(view(&s, "crown").pending_rulings.is_empty());
        assert!(recent(&s, "crown").contains("«I build a tower»"));
    }

    #[test]
    fn recent_covers_the_previous_and_current_tick_only() {
        let mut s = Sim::tiny();
        s.close(&[act("crown", "queen", "decree", None)]);
        assert!(recent(&s, "crown").contains("Decree"));
        s.close(&[]);
        assert!(!recent(&s, "crown").contains("Decree"));
    }

    #[test]
    fn chronicle_hides_effects_on_numbers_the_seat_cannot_see() {
        let mut s = Sim::tiny();
        s.close(&[act("crown", "queen", "feast", Some("south"))]);
        let folk = recent(&s, "folk");
        assert!(folk.contains("Queen Ada: Feast"));
        assert!(
            !folk.contains("Treasury"),
            "treasury is for the head of state: {folk}"
        );
        assert!(
            !folk.contains("South"),
            "the south is outside the folk's sight: {folk}"
        );
        assert!(recent(&s, "crown").contains(
            "Feast on The South (Treasury of The Kingdom -10, Unrest in The South -0.5)"
        ));
    }

    #[test]
    fn rule_events_in_unseen_regions_are_not_announced() {
        let mut s = Sim::tiny();
        s.close(&[]);
        s.close(&[]);
        assert!(recent(&s, "crown").contains("Trouble in The South"));
        assert!(!recent(&s, "folk").contains("Trouble"));
    }

    #[test]
    fn effects_scaled_by_a_hidden_value_are_hidden() {
        let s = Sim::tiny();
        let folk = Sight::of_seat(&s.w, &s.state, &"folk".into()).unwrap();
        let plain = Effect {
            target: metric("kingdom", "stability"),
            delta: 0.1,
            scaled_by: None,
        };
        let scaled = Effect {
            scaled_by: Some(metric("kingdom", "treasury")),
            ..plain.clone()
        };
        let foreign = Effect {
            target: metric("isles", "stability"),
            ..plain.clone()
        };
        assert!(folk.effect(&plain));
        assert!(!folk.effect(&scaled));
        assert!(!folk.effect(&foreign));
        assert!(Sight::all().effect(&scaled));
    }

    #[test]
    fn scaled_effects_do_not_reveal_hidden_numbers() {
        let mut s = Sim::rome(3);
        s.close(&[act("princeps", "augustus", "raise_taxes", Some("gallia"))]);
        let people = recent(&s, "people");
        assert!(people.contains("Raise taxes"));
        assert!(!people.contains("Treasury"), "{people}");
        assert!(!recent(&s, "parthia").contains("Raise taxes"));
    }

    #[test]
    fn a_rule_label_is_hidden_when_its_condition_reads_a_hidden_value() {
        let mut s = Sim::rome(3);
        s.set_metric("rome", "treasury", -50.0);
        s.close(&[]);
        let line = |seat| recent(&s, seat).contains("is empty; soldiers and officials go unpaid");
        assert!(line("princeps"));
        assert!(!line("people"), "the people cannot see the treasury");
        assert!(!line("parthia"), "nor can a foreign power");
    }

    #[test]
    fn resolved_effects_record_what_scaled_them() {
        let mut s = Sim::rome(3);
        let events = s.close(&[act("princeps", "augustus", "raise_taxes", Some("gallia"))]);
        let taxed = events
            .iter()
            .find_map(|e| match &e.kind {
                EventKind::ActionResolved {
                    action, effects, ..
                } if action.as_str() == "raise_taxes" => Some(effects.clone()),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            taxed[0].scaled_by,
            Some(Target::Stat {
                region: "gallia".into(),
                stat: "wealth".into()
            })
        );
        assert_eq!(taxed[1].scaled_by, None);
    }

    #[test]
    fn action_targets_are_named_by_the_actions_target_kind() {
        let mut sc = s_tiny_scenario();
        let mut duchy = sc.empires[0].regions[0].clone();
        duchy.id = "duke".into();
        duchy.name = "The Duchy".into();
        sc.empires[0].regions.push(duchy);
        let rules = crate::ruleset::Ruleset::from_yaml(TINY_RULES).unwrap();
        let mut s = Sim::start(World::new(sc, rules).unwrap(), 1);
        s.close(&[act("crown", "queen", "spy", Some("duke"))]);
        let crown = recent(&s, "crown");
        assert!(crown.contains("Spy on Duke Bors"), "{crown}");
    }

    fn s_tiny_scenario() -> crate::scenario::Scenario {
        crate::scenario::Scenario::from_yaml(TINY).unwrap()
    }
}
