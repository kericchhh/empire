use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::event::{Effect, Event, EventKind, Target, Visibility};
use crate::ids::*;
use crate::scenario::{RegionSight, Scenario, TargetKind, ValueRef};
use crate::state::State;

/// Everything one seat may know this tick. Self-contained so a fresh agent can play from it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Perception {
    pub world: WorldView,
    pub seat: SeatId,
    pub game_master: bool,
    pub you_play: Vec<CharacterView>,
    pub metrics: Vec<Number>,
    pub regions: Vec<RegionView>,
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
    pub culture: BTreeMap<String, String>,
    pub ended: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CharacterView {
    pub id: CharacterId,
    pub name: String,
    pub role: String,
    pub role_description: String,
    pub region: Option<RegionId>,
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
    pub stats: Vec<Number>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Person {
    pub id: CharacterId,
    pub name: String,
    pub role: String,
    pub region: Option<RegionId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PendingView {
    pub id: String,
    pub character: CharacterId,
    pub text: String,
}

pub fn can_see(sc: &Scenario, seat: &SeatId, e: &Event) -> bool {
    if sc.seat(seat).is_some_and(|s| s.game_master) {
        return true;
    }
    match &e.visibility {
        Visibility::Public => true,
        Visibility::Seats(seats) => seats.contains(seat),
    }
}

/// `None` for an unknown seat.
pub fn perceive(sc: &Scenario, state: &State, seat: &SeatId) -> Option<Perception> {
    let sdef = sc.seat(seat)?;
    let gm = sdef.game_master;
    let chars: Vec<_> = sdef
        .controls
        .iter()
        .filter_map(|c| sc.character(c))
        .collect();
    let sight = Sight::of_seat(sc, seat)?;

    let you_play = chars
        .iter()
        .map(|c| {
            let role = sc.role(&c.role);
            let cs = state.characters.get(&c.id);
            CharacterView {
                id: c.id.clone(),
                name: c.name.clone(),
                role: role.map_or_else(String::new, |r| r.name.clone()),
                role_description: role.map_or_else(String::new, |r| r.description.clone()),
                region: c.region.clone(),
                persona: c.persona.clone(),
                goals: c.goals.clone(),
                secrets: c.secrets.clone(),
                resources: sc
                    .resources
                    .iter()
                    .map(|r| Number {
                        id: r.id.0.clone(),
                        name: r.name.clone(),
                        value: cs
                            .and_then(|s| s.resources.get(&r.id))
                            .copied()
                            .unwrap_or(0.0),
                    })
                    .collect(),
                notes: cs.map(|s| s.notes.clone()).unwrap_or_default(),
                actions: role
                    .into_iter()
                    .flat_map(|r| &r.actions)
                    .filter_map(|a| sc.action(a))
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

    let metrics = sc
        .metrics
        .iter()
        .filter(|m| sight.metric(&m.id))
        .map(|m| Number {
            id: m.id.0.clone(),
            name: m.name.clone(),
            value: state.metrics.get(&m.id).copied().unwrap_or(0.0),
        })
        .collect();

    let regions = sc
        .regions
        .iter()
        .filter(|r| sight.region(&r.id))
        .map(|r| RegionView {
            id: r.id.clone(),
            name: r.name.clone(),
            description: r.description.clone(),
            stats: sc
                .region_stats
                .iter()
                .filter(|s| sight.stat(&s.id))
                .map(|s| Number {
                    id: s.id.0.clone(),
                    name: s.name.clone(),
                    value: state
                        .regions
                        .get(&r.id)
                        .and_then(|st| st.get(&s.id))
                        .copied()
                        .unwrap_or(0.0),
                })
                .collect(),
        })
        .collect();

    let people = sc
        .characters
        .iter()
        .map(|c| Person {
            id: c.id.clone(),
            name: c.name.clone(),
            role: sc
                .role(&c.role)
                .map_or_else(String::new, |r| r.name.clone()),
            region: c.region.clone(),
        })
        .collect();

    let recent = state
        .recent
        .iter()
        .filter(|e| can_see(sc, seat, e))
        .filter_map(|e| describe(sc, e, &sight).map(|d| format!("[{}] {d}", sc.date(e.tick))))
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
            name: sc.name.clone(),
            description: sc.description.clone(),
            date: sc.date(state.tick),
            tick: state.tick,
            culture: sc.culture.clone(),
            ended: state.ended.as_ref().map(|e| e.label.clone()),
        },
        seat: seat.clone(),
        game_master: gm,
        you_play,
        metrics,
        regions,
        people,
        recent,
        pending_rulings,
        orders_per_character: sc.limits.orders_per_character,
    })
}

fn name_of(sc: &Scenario, c: &CharacterId) -> String {
    sc.character(c)
        .map_or_else(|| c.to_string(), |d| d.name.clone())
}

fn target_name(sc: &Scenario, t: &Target) -> String {
    match t {
        Target::Metric(m) => sc
            .metric(m)
            .map_or_else(|| m.to_string(), |d| d.name.clone()),
        Target::Stat { region, stat } => format!(
            "{} in {}",
            sc.region_stat(stat)
                .map_or_else(|| stat.to_string(), |d| d.name.clone()),
            sc.region(region)
                .map_or_else(|| region.to_string(), |d| d.name.clone()),
        ),
        Target::Resource {
            character,
            resource,
        } => format!(
            "{} of {}",
            sc.resource(resource)
                .map_or_else(|| resource.to_string(), |d| d.name.clone()),
            name_of(sc, character),
        ),
    }
}

fn effects_text(sc: &Scenario, sight: &Sight, effects: &[Effect]) -> String {
    let parts: Vec<String> = effects
        .iter()
        .filter(|e| sight.effect(e))
        .map(|e| format!("{} {:+}", target_name(sc, &e.target), round(e.delta)))
        .collect();
    if parts.is_empty() {
        return String::new();
    }
    format!(" ({})", parts.join(", "))
}

/// What numbers one observer may see. The only place visibility of numbers is decided.
#[derive(Debug, Clone)]
pub struct Sight {
    all: bool,
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
            metrics: BTreeSet::new(),
            stats: BTreeSet::new(),
            regions: BTreeSet::new(),
            characters: BTreeSet::new(),
        }
    }

    pub fn of_seat(sc: &Scenario, seat: &SeatId) -> Option<Self> {
        let sdef = sc.seat(seat)?;
        if sdef.game_master {
            return Some(Self::all());
        }
        let chars: Vec<_> = sdef
            .controls
            .iter()
            .filter_map(|c| sc.character(c))
            .collect();
        let roles: BTreeSet<&RoleId> = chars.iter().map(|c| &c.role).collect();
        let sees_all_regions = chars.iter().any(|c| {
            sc.role(&c.role)
                .is_some_and(|r| r.region_sight == RegionSight::All)
        });
        Some(Self {
            all: false,
            metrics: sc
                .metrics
                .iter()
                .filter(|m| roles.iter().any(|r| m.visible_to_role(r)))
                .map(|m| m.id.clone())
                .collect(),
            stats: sc
                .region_stats
                .iter()
                .filter(|s| roles.iter().any(|r| s.visible_to_role(r)))
                .map(|s| s.id.clone())
                .collect(),
            regions: if sees_all_regions {
                sc.regions.iter().map(|r| r.id.clone()).collect()
            } else {
                chars.iter().filter_map(|c| c.region.clone()).collect()
            },
            characters: sdef.controls.iter().cloned().collect(),
        })
    }

    pub fn metric(&self, m: &MetricId) -> bool {
        self.all || self.metrics.contains(m)
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
            Target::Metric(m) => self.metric(m),
            Target::Stat { region, stat } => self.region(region) && self.stat(stat),
            Target::Resource { character, .. } => self.all || self.characters.contains(character),
        }
    }
}

fn round(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

/// One chronicle line, or `None` for bookkeeping events. Agent-written text is quoted so it
/// reads as in-world speech, never as instructions.
/// A rule's label announces that its condition held, so it is only shown to observers who
/// could see the value the condition reads.
fn condition_visible(
    sc: &Scenario,
    sight: &Sight,
    rule: &RuleId,
    region: &Option<RegionId>,
) -> bool {
    let Some(cond) = sc
        .rules
        .iter()
        .find(|r| &r.id == rule)
        .and_then(|r| r.when.as_ref())
    else {
        return true;
    };
    match &cond.value {
        ValueRef::Metric(m) => sight.metric(m),
        ValueRef::Stat(s) => sight.stat(s) && region.as_ref().is_none_or(|r| sight.region(r)),
    }
}

pub fn describe(sc: &Scenario, e: &Event, sight: &Sight) -> Option<String> {
    Some(match &e.kind {
        EventKind::WorldStarted { .. }
        | EventKind::TickStarted
        | EventKind::TickEnded { .. }
        | EventKind::NoteWritten { .. }
        | EventKind::RuleFired { label: None, .. } => return None,
        EventKind::RuleFired {
            region: Some(r), ..
        } if !sight.region(r) => return None,
        EventKind::RuleFired { rule, region, .. }
            if !condition_visible(sc, sight, rule, region) =>
        {
            return None;
        }
        EventKind::SeatJoined { seat } => format!(
            "{} has taken their seat.",
            sc.seat(seat)
                .map_or_else(|| seat.to_string(), |s| s.name.clone())
        ),
        EventKind::ActionResolved {
            character,
            action,
            target,
            effects,
        } => {
            let aname = sc
                .action(action)
                .map_or_else(|| action.to_string(), |a| a.name.clone());
            let kind = sc.action(action).map(|a| a.target);
            let on = target
                .as_ref()
                .and_then(|t| {
                    let name = match kind? {
                        TargetKind::Region => {
                            let region = RegionId::from(t.as_str());
                            if !sight.region(&region) {
                                return None;
                            }
                            sc.region(&region)?.name.clone()
                        }
                        TargetKind::Character => name_of(sc, &CharacterId::from(t.as_str())),
                        TargetKind::None => return None,
                    };
                    Some(format!(" on {name}"))
                })
                .unwrap_or_default();
            format!(
                "{}: {aname}{on}{}",
                name_of(sc, character),
                effects_text(sc, sight, effects)
            )
        }
        EventKind::ActionRejected {
            character,
            action,
            reason,
        } => format!("{} could not {action}: {reason}", name_of(sc, character)),
        EventKind::FreeformDeclared {
            id,
            character,
            text,
        } => format!("{} attempts [{id}]: «{text}»", name_of(sc, character)),
        EventKind::FreeformRuled {
            id,
            outcome,
            effects,
        } => format!(
            "Ruling on [{id}]: «{outcome}»{}",
            effects_text(sc, sight, effects)
        ),
        EventKind::FreeformExpired { id } => format!("[{id}] came to nothing."),
        EventKind::MessageSent { from, to, text } => format!(
            "Message from {} to {}: «{text}»",
            name_of(sc, from),
            name_of(sc, to)
        ),
        EventKind::RuleFired {
            label: Some(l),
            effects,
            ..
        } => format!("{l}{}", effects_text(sc, sight, effects)),
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
            format!("{title}{body}{}", effects_text(sc, sight, effects))
        }
        EventKind::WorldEnded { label, .. } => format!("THE END: {label}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::note;
    use crate::testkit::*;

    fn view(w: &World, seat: &str) -> Perception {
        perceive(&w.sc, &w.state, &seat.into()).unwrap()
    }

    #[test]
    fn unknown_seat_perceives_nothing() {
        let w = World::tiny();
        assert!(perceive(&w.sc, &w.state, &"nobody".into()).is_none());
    }

    #[test]
    fn a_seat_sees_its_own_characters_with_secrets_and_actions() {
        let w = World::tiny();
        let v = view(&w, "crown");
        assert_eq!(v.you_play.len(), 1);
        let queen = &v.you_play[0];
        assert_eq!(queen.name, "Queen Ada");
        let actions: Vec<_> = queen.actions.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(actions, ["decree", "feast", "spy"]);
        assert_eq!(v.people.len(), 3);
        assert_eq!(v.world.date, "Spring, 1");
        assert_eq!(v.world.culture["religion"], "The Old Gods");
    }

    #[test]
    fn restricted_metrics_are_hidden_from_other_roles() {
        let w = World::tiny();
        let ids =
            |seat| -> Vec<String> { view(&w, seat).metrics.into_iter().map(|m| m.id).collect() };
        assert_eq!(ids("crown"), ["gold", "order"]);
        assert_eq!(ids("folk"), ["order"]);
        assert_eq!(ids("gm"), ["gold", "order"]);
    }

    #[test]
    fn home_sight_limits_regions_to_the_home_region() {
        let w = World::tiny();
        let regions =
            |seat| -> Vec<RegionId> { view(&w, seat).regions.into_iter().map(|r| r.id).collect() };
        assert_eq!(regions("folk"), [RegionId::from("north")]);
        assert_eq!(regions("crown").len(), 2);
    }

    #[test]
    fn notes_are_private_to_their_seat() {
        let mut w = World::tiny();
        let e = note(
            &w.sc,
            &w.state,
            &"crown".into(),
            &"queen".into(),
            "Trust no one",
        )
        .unwrap();
        w.push(e);
        assert_eq!(view(&w, "crown").you_play[0].notes, ["Trust no one"]);
        assert!(
            !view(&w, "rival")
                .recent
                .iter()
                .any(|l| l.contains("Trust no one"))
        );
    }

    #[test]
    fn only_the_game_master_sees_pending_rulings() {
        let mut w = World::tiny();
        w.close(&[freeform("crown", "queen", "I build a tower")]);
        assert_eq!(view(&w, "gm").pending_rulings.len(), 1);
        assert!(view(&w, "crown").pending_rulings.is_empty());
        assert!(
            view(&w, "crown")
                .recent
                .iter()
                .any(|l| l.contains("«I build a tower»"))
        );
    }

    #[test]
    fn recent_covers_the_previous_and_current_tick_only() {
        let mut w = World::tiny();
        w.close(&[act("crown", "queen", "decree", None)]);
        assert!(
            view(&w, "crown")
                .recent
                .iter()
                .any(|l| l.contains("Decree"))
        );
        w.close(&[]);
        assert!(
            !view(&w, "crown")
                .recent
                .iter()
                .any(|l| l.contains("Decree"))
        );
    }

    #[test]
    fn chronicle_hides_effects_on_numbers_the_seat_cannot_see() {
        let mut w = World::tiny();
        w.close(&[act("crown", "queen", "feast", Some("south"))]);
        let folk = view(&w, "folk").recent.join("\n");
        assert!(folk.contains("Queen Ada: Feast"));
        assert!(
            !folk.contains("Gold"),
            "gold is restricted to rulers: {folk}"
        );
        assert!(
            !folk.contains("South"),
            "the south is outside the folk's sight: {folk}"
        );
        let crown = view(&w, "crown").recent.join("\n");
        assert!(crown.contains("Feast on The South (Gold -10, Unrest in The South -0.5)"));
    }

    #[test]
    fn rule_events_in_unseen_regions_are_not_announced() {
        let mut w = World::tiny();
        w.close(&[]);
        w.close(&[]);
        assert!(
            view(&w, "crown")
                .recent
                .iter()
                .any(|l| l.contains("Trouble in The South"))
        );
        assert!(
            !view(&w, "folk")
                .recent
                .iter()
                .any(|l| l.contains("Trouble"))
        );
    }

    #[test]
    fn scaled_effects_do_not_reveal_hidden_numbers() {
        let mut w = World::new(ROME, 3);
        w.close(&[act("princeps", "augustus", "raise_taxes", Some("gallia"))]);
        let people = view(&w, "people").recent.join("\n");
        assert!(people.contains("Raise taxes"));
        assert!(!people.contains("Treasury"), "{people}");
        assert!(!people.contains("tribute"));
    }

    #[test]
    fn a_rule_label_is_hidden_when_its_condition_reads_a_hidden_value() {
        let mut w = World::new(ROME, 3);
        w.state.metrics.insert("treasury".into(), -50.0);
        w.close(&[]);
        let line = |seat| {
            view(&w, seat)
                .recent
                .iter()
                .any(|l| l.contains("treasury is empty"))
        };
        assert!(line("princeps"));
        assert!(!line("people"), "the people cannot see the treasury");
    }

    #[test]
    fn effects_scaled_by_a_hidden_value_are_hidden() {
        let w = World::tiny();
        let folk = Sight::of_seat(&w.sc, &"folk".into()).unwrap();
        let plain = Effect {
            target: Target::Metric("order".into()),
            delta: 0.1,
            scaled_by: None,
        };
        let scaled = Effect {
            scaled_by: Some(Target::Metric("gold".into())),
            ..plain.clone()
        };
        assert!(folk.effect(&plain));
        assert!(!folk.effect(&scaled));
        assert!(Sight::all().effect(&scaled));
    }

    #[test]
    fn resolved_effects_record_what_scaled_them() {
        let mut w = World::new(ROME, 3);
        let events = w.close(&[act("princeps", "augustus", "raise_taxes", Some("gallia"))]);
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
        let mut sc = Scenario::from_yaml(TINY).unwrap();
        let mut duchy = sc.regions[0].clone();
        duchy.id = "duke".into();
        duchy.name = "The Duchy".into();
        sc.regions.push(duchy);
        let state = State::replay(&sc, &crate::engine::start(&sc, 1));
        let mut w = World { sc, state };
        w.close(&[act("crown", "queen", "spy", Some("duke"))]);
        let crown = view(&w, "crown").recent.join("\n");
        assert!(crown.contains("Spy on Duke Bors"), "{crown}");
    }
}
