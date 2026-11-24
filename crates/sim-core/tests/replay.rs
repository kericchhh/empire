use std::collections::BTreeMap;

use proptest::prelude::*;
use sim_core::engine::{self, close_tick};
use sim_core::perceive::{Sight, can_see, describe};
use sim_core::ruleset::TargetKind;
use sim_core::{
    CharacterId, Effect, EmpireId, Event, EventKind, OfficeId, Order, RegionId, Ruling, Scenario,
    SeatId, State, Submission, Target, TurnInput, Visibility, World, perceive,
};

const ROME: &str = include_str!("../../../scenarios/rome.yaml");
const THREE: &str = include_str!("fixtures/three.yaml");

/// One thing that can happen during a tick. Choices are indexes, resolved against the live
/// state, so every generated history is playable.
#[derive(Debug, Clone)]
enum Step {
    Order(usize),
    Appoint(usize),
    Freeform(usize),
    Message(usize, usize),
    Note(usize),
    Intervene(usize, i32),
    Introduce(usize, usize),
    RuleFirstPending(bool, i32),
}

fn step() -> impl Strategy<Value = Step> {
    prop_oneof![
        4 => any::<usize>().prop_map(Step::Order),
        1 => any::<usize>().prop_map(Step::Appoint),
        1 => any::<usize>().prop_map(Step::Freeform),
        1 => (any::<usize>(), any::<usize>()).prop_map(|(a, b)| Step::Message(a, b)),
        1 => any::<usize>().prop_map(Step::Note),
        1 => (any::<usize>(), -100..100i32).prop_map(|(t, d)| Step::Intervene(t, d)),
        1 => (any::<usize>(), any::<usize>()).prop_map(|(a, b)| Step::Introduce(a, b)),
        1 => (any::<bool>(), -20..20i32).prop_map(|(p, d)| Step::RuleFirstPending(p, d)),
    ]
}

fn history() -> impl Strategy<Value = Vec<Vec<Step>>> {
    prop::collection::vec(prop::collection::vec(step(), 0..8), 1..30)
}

/// Every order the live offices allow, split into those without a choice and those with
/// one. Foreign targets and invalid choices too, so rejections are part of the history.
fn all_orders(w: &World, state: &State) -> (Vec<TurnInput>, Vec<TurnInput>) {
    let sc = &w.scenario;
    let (mut plain, mut chosen) = (Vec::new(), Vec::new());
    for (_, seat) in sc.seats() {
        for c in &seat.controls {
            let office = w.rules.office(&state.characters[c].office).unwrap();
            for a in &office.actions {
                let adef = w.rules.action(a).unwrap();
                let targets: Vec<Option<String>> = match adef.target {
                    TargetKind::None => vec![None],
                    TargetKind::Region => sc.regions().map(|(_, r)| Some(r.id.0.clone())).collect(),
                    TargetKind::Character | TargetKind::OwnCharacter => {
                        sc.characters().map(|(_, x)| Some(x.id.0.clone())).collect()
                    }
                };
                let choices: Vec<Option<String>> = match adef.choice {
                    None => vec![None],
                    Some(kind) => w
                        .rules
                        .choices(kind)
                        .map(|(id, _)| Some(id.to_owned()))
                        .chain([Some("emperor".into()), None])
                        .collect(),
                };
                let out = if adef.choice.is_some() {
                    &mut chosen
                } else {
                    &mut plain
                };
                for target in &targets {
                    for choice in &choices {
                        out.push(TurnInput {
                            seat: seat.id.clone(),
                            submission: Submission::Order(Order::Action {
                                character: c.clone(),
                                action: a.clone(),
                                target: target.clone(),
                                choice: choice.clone(),
                            }),
                        });
                    }
                }
            }
        }
    }
    (plain, chosen)
}

fn run(w: &World, seed: u64, ticks: &[Vec<Step>]) -> (State, Vec<Event>) {
    let sc = &w.scenario;
    let players: Vec<(SeatId, sim_core::CharacterId)> = sc
        .seats()
        .flat_map(|(_, s)| s.controls.iter().map(|c| (s.id.clone(), c.clone())))
        .collect();
    let targets: Vec<Target> = sc
        .empires
        .iter()
        .flat_map(|e| {
            w.rules.metrics.iter().map(|m| Target::Metric {
                empire: e.id.clone(),
                metric: m.id.clone(),
            })
        })
        .chain(sc.regions().map(|(_, r)| Target::Stat {
            region: r.id.clone(),
            stat: "unrest".into(),
        }))
        .collect();
    let gm = sc.game_master.as_ref().unwrap().id.clone();

    let mut log = engine::start(w, seed);
    let mut state = State::replay(w, &log);
    let push = |state: &mut State, log: &mut Vec<Event>, e: Event| {
        state.apply(&e);
        log.push(e);
    };
    for steps in ticks {
        let mut inputs = Vec::new();
        let (orders, appointments) = all_orders(w, &state);
        for s in steps {
            match *s {
                Step::Order(i) => inputs.push(orders[i % orders.len()].clone()),
                Step::Appoint(i) if !appointments.is_empty() => {
                    inputs.push(appointments[i % appointments.len()].clone());
                }
                Step::Appoint(_) => {}
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
                    if let Ok(e) = engine::message(w, &state, seat, from, to, "We should talk") {
                        push(&mut state, &mut log, e);
                    }
                }
                Step::Note(i) => {
                    let (seat, c) = &players[i % players.len()];
                    if let Ok(e) = engine::note(w, &state, seat, c, "Remember this") {
                        push(&mut state, &mut log, e);
                    }
                }
                Step::Intervene(t, d) => {
                    let effect = Effect {
                        target: targets[t % targets.len()].clone(),
                        delta: f64::from(d) / 37.0,
                        scaled_by: None,
                    };
                    if let Ok(e) = engine::intervene(w, &state, "An omen", "", vec![effect], None) {
                        push(&mut state, &mut log, e);
                    }
                }
                Step::Introduce(a, b) => {
                    let e = |i: usize| &sc.empires[i % sc.empires.len()].id;
                    if let Ok(e) = engine::introduce(&state, e(a), e(b)) {
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
                                    target: Target::Metric {
                                        empire: sc.empires[0].id.clone(),
                                        metric: "stability".into(),
                                    },
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
        for e in close_tick(w, &state, &inputs) {
            push(&mut state, &mut log, e);
        }
    }
    (state, log)
}

/// Rome, whose empires know each other from the start, or three empires of which one knows
/// nobody until an intervention introduces it.
fn world(three: bool) -> World {
    World::standard(Scenario::from_yaml(if three { THREE } else { ROME }).unwrap()).unwrap()
}

fn owner<'a>(state: &'a State, t: &'a Target) -> Option<&'a EmpireId> {
    match t {
        Target::Metric { empire, .. } => Some(empire),
        Target::Stat { region, .. } => state.regions.get(region).map(|r| &r.owner),
        Target::Resource { character, .. } => state.characters.get(character).map(|c| &c.empire),
    }
}

/// Changes everything of other empires that `ours` must not see: their numbers, notes and
/// plans, where their characters stand, which private offices they hold and which offices
/// they move between, every logged delta whose target or scale is theirs, and the words of
/// every event they caused that reaches none of ours. Of an empire `ours` has not met, also
/// its names, culture and every office; and which other empires have met each other.
fn shift_foreign(
    w: &World,
    state: &State,
    log: &[Event],
    ours: &EmpireId,
) -> (World, State, Vec<Event>) {
    let unmet = |e: &EmpireId| !state.met(ours, e);
    let foreign = |t: &Target| owner(state, t).is_some_and(|e| e != ours);
    let theirs = |c: &CharacterId| state.characters.get(c).is_some_and(|c| &c.empire != ours);
    let reaches_us = |t: &Option<String>| {
        t.as_deref().is_some_and(|t| {
            let (c, r) = (CharacterId::from(t), RegionId::from(t));
            state.characters.get(&c).is_some_and(|c| &c.empire == ours)
                || state.regions.get(&r).is_some_and(|r| &r.owner == ours)
        })
    };
    let bump = |effects: &mut Vec<Effect>| {
        effects
            .iter_mut()
            .filter(|f| foreign(&f.target) || f.scaled_by.as_ref().is_some_and(foreign))
            .for_each(|f| f.delta += 1.0)
    };
    let secret = " in secret";
    let plans: BTreeMap<String, CharacterId> = log
        .iter()
        .filter_map(|e| match &e.kind {
            EventKind::FreeformDeclared { id, character, .. } => {
                Some((id.clone(), character.clone()))
            }
            _ => None,
        })
        .collect();
    let their_plan = |id: &String| plans.get(id).is_some_and(theirs);
    let shift = |mut e: Event| -> Option<Event> {
        let to_them = matches!(&e.visibility, Visibility::Empire(v) if v != ours);
        match &mut e.kind {
            EventKind::ActionResolved {
                character,
                action,
                target,
                effects,
            } => {
                if theirs(character) && !reaches_us(target) {
                    *action = "hidden".into();
                }
                bump(effects);
            }
            EventKind::RuleFired {
                empire,
                label,
                effects,
                ..
            } => {
                if let Some(l) = label.as_mut().filter(|_| empire != ours) {
                    l.push_str(secret);
                }
                bump(effects);
            }
            EventKind::Intervention { title, effects, .. } => {
                if to_them {
                    title.push_str(secret);
                }
                bump(effects);
            }
            EventKind::FreeformRuled {
                id,
                outcome,
                effects,
            } => {
                if their_plan(id) {
                    outcome.push_str(secret);
                }
                bump(effects);
            }
            EventKind::FreeformExpired { id } if their_plan(id) => id.push_str(secret),
            EventKind::OfficeChanged {
                character,
                from,
                to,
            } if theirs(character) => {
                *from = "hidden".into();
                *to = "hidden".into();
            }
            EventKind::ActionRejected {
                character, reason, ..
            } if theirs(character) => reason.push_str(secret),
            EventKind::FreeformDeclared {
                character, text, ..
            } if theirs(character) => text.push_str(secret),
            EventKind::MessageSent { from, to, text } if theirs(from) && theirs(to) => {
                text.push_str(secret)
            }
            EventKind::EmpiresMet { a, b } if a != ours && b != ours => return None,
            _ => {}
        }
        Some(e)
    };
    let next = |all: &[&RegionId], r: &RegionId| {
        let i = all.iter().position(|x| *x == r)?;
        Some(all[(i + 1) % all.len()].clone())
    };
    let mut s = state.clone();
    for (id, es) in &mut s.empires {
        if id != ours {
            es.metrics.values_mut().for_each(|v| *v += 1.0);
        }
    }
    for rs in s.regions.values_mut().filter(|r| &r.owner != ours) {
        rs.stats.values_mut().for_each(|v| *v += 1.0);
    }
    let offices = |public: bool| -> Vec<&OfficeId> {
        let all = w.rules.offices.iter();
        all.filter(|o| public || !o.public).map(|o| &o.id).collect()
    };
    for cs in s.characters.values_mut().filter(|c| &c.empire != ours) {
        let offices = offices(unmet(&cs.empire));
        cs.resources.values_mut().for_each(|v| *v += 1.0);
        cs.notes.push("A hidden thought".into());
        let homes: Vec<&RegionId> = state.regions_of(&cs.empire).collect();
        if let Some(r) = next(&homes, &cs.region) {
            cs.region = r;
        }
        if let Some(i) = offices.iter().position(|o| *o == &cs.office) {
            cs.office = offices[(i + 1) % offices.len()].clone();
        }
    }
    for p in s.pending.values_mut().filter(|p| theirs(&p.character)) {
        p.text.push_str(secret);
    }
    let empires: Vec<&EmpireId> = state.empires.keys().collect();
    for (i, a) in empires.iter().enumerate().filter(|(_, a)| **a != ours) {
        for b in empires[i + 1..].iter().filter(|b| **b != ours) {
            let (a, b) = ((*a).clone(), (*b).clone());
            if !s.contacts.remove(&(a.clone(), b.clone())) {
                s.contacts.insert((a, b));
            }
        }
    }
    s.recent = s.recent.into_iter().filter_map(shift).collect();
    let mut w = w.clone();
    for e in w.scenario.empires.iter_mut().filter(|e| unmet(&e.id)) {
        e.name.push_str(secret);
        e.description.push_str(secret);
        e.culture.insert("hidden".into(), secret.into());
        for r in &mut e.regions {
            r.name.push_str(secret);
        }
        for c in &mut e.characters {
            c.name.push_str(secret);
            c.title.push_str(secret);
        }
    }
    (w, s, log.iter().cloned().filter_map(shift).collect())
}

fn chronicle(w: &World, state: &State, log: &[Event], seat: &SeatId) -> Vec<String> {
    let sight = Sight::of_seat(w, state, seat).unwrap();
    log.iter()
        .filter(|e| can_see(w, seat, e))
        .filter_map(|e| describe(w, e, &sight))
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn replaying_a_serialized_log_rebuilds_the_live_state(three in any::<bool>(), seed in any::<u64>(), ticks in history()) {
        let w = world(three);
        let (live, log) = run(&w, seed, &ticks);
        let stored: Vec<String> = log.iter().map(|e| serde_json::to_string(e).unwrap()).collect();
        let loaded: Vec<Event> = stored.iter().map(|s| serde_json::from_str(s).unwrap()).collect();
        prop_assert_eq!(&loaded, &log);
        prop_assert_eq!(State::replay(&w, &loaded), live);
    }

    #[test]
    fn a_state_snapshot_round_trips_through_json(three in any::<bool>(), seed in any::<u64>(), ticks in history()) {
        let w = world(three);
        let (live, _) = run(&w, seed, &ticks);
        let restored: State = serde_json::from_str(&serde_json::to_string(&live).unwrap()).unwrap();
        prop_assert_eq!(restored, live);
    }

    #[test]
    fn the_same_seed_and_steps_produce_the_same_history(three in any::<bool>(), seed in any::<u64>(), ticks in history()) {
        let w = world(three);
        prop_assert_eq!(run(&w, seed, &ticks).1, run(&w, seed, &ticks).1);
    }

    #[test]
    fn no_seat_sees_anything_hidden_of_another_empire(three in any::<bool>(), seed in any::<u64>(), ticks in history()) {
        let w = world(three);
        let (state, log) = run(&w, seed, &ticks);
        for (empire, seat) in w.scenario.seats() {
            let (ow, other, other_log) = shift_foreign(&w, &state, &log, &empire.id);
            prop_assert_eq!(perceive(&ow, &other, &seat.id), perceive(&w, &state, &seat.id));
            prop_assert_eq!(
                chronicle(&ow, &other, &other_log, &seat.id),
                chronicle(&w, &state, &log, &seat.id)
            );
        }
    }
}
