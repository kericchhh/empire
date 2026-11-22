//! A scenario joined to the ruleset it runs on: everything the engine reads that is not state.

use std::collections::BTreeMap;
use std::fmt;

use serde::Serialize;

use crate::def::{DefinitionError, invalid, unknown};
use crate::ids::*;
use crate::ruleset::Ruleset;
use crate::scenario::{CharacterDef, EmpireDef, RegionDef, Scenario, SeatDef};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct World {
    pub scenario: Scenario,
    pub rules: Ruleset,
}

/// A seat as the engine sees it: an empire seat, or the game master.
#[derive(Debug, Clone, Copy)]
pub struct SeatRef<'a> {
    pub id: &'a SeatId,
    pub name: &'a str,
    /// `None` for the game master.
    pub empire: Option<&'a EmpireId>,
    pub controls: &'a [CharacterId],
    pub game_master: bool,
}

impl World {
    /// Check `scenario` against `rules`: every office and starting value it names must exist.
    pub fn new(scenario: Scenario, rules: Ruleset) -> Result<Self, DefinitionError> {
        scenario.validate()?;
        for e in &scenario.empires {
            let ctx = format!("empire `{}`", e.id);
            check_start(&ctx, "metric", &e.start, |m| rules.metric(m).is_some())?;
            for r in &e.regions {
                let ctx = format!("region `{}`", r.id);
                check_start(&ctx, "region stat", &r.start, |s| {
                    rules.region_stat(s).is_some()
                })?;
            }
            for c in &e.characters {
                let ctx = format!("character `{}`", c.id);
                rules
                    .office(&c.office)
                    .ok_or_else(|| unknown(&ctx, "office", c.office.as_str()))?;
                check_start(&ctx, "resource", &c.start, |r| rules.resource(r).is_some())?;
            }
        }
        Ok(Self { scenario, rules })
    }

    /// The scenario on the standard ruleset, as every real world runs.
    pub fn standard(scenario: Scenario) -> Result<Self, DefinitionError> {
        Self::new(scenario, Ruleset::standard())
    }

    pub fn empire(&self, id: &EmpireId) -> Option<&EmpireDef> {
        self.scenario.empires.iter().find(|e| &e.id == id)
    }
    pub fn region(&self, id: &RegionId) -> Option<&RegionDef> {
        self.scenario
            .regions()
            .map(|(_, r)| r)
            .find(|r| &r.id == id)
    }
    pub fn character(&self, id: &CharacterId) -> Option<&CharacterDef> {
        self.scenario
            .characters()
            .map(|(_, c)| c)
            .find(|c| &c.id == id)
    }

    pub fn seat(&self, id: &SeatId) -> Option<SeatRef<'_>> {
        if let Some(g) = self.scenario.game_master.as_ref().filter(|g| &g.id == id) {
            return Some(SeatRef {
                id: &g.id,
                name: &g.name,
                empire: None,
                controls: &[],
                game_master: true,
            });
        }
        self.scenario
            .seats()
            .find(|(_, s)| &s.id == id)
            .map(|(e, s)| SeatRef {
                id: &s.id,
                name: &s.name,
                empire: Some(&e.id),
                controls: &s.controls,
                game_master: false,
            })
    }

    pub fn seat_of(&self, c: &CharacterId) -> Option<&SeatDef> {
        self.scenario
            .seats()
            .map(|(_, s)| s)
            .find(|s| s.controls.contains(c))
    }

    pub fn date(&self, tick: Tick) -> String {
        self.scenario.date(tick)
    }
}

/// Every id in `start` must name a ruleset number of `kind`, with a finite value.
fn check_start<Id: fmt::Display>(
    ctx: &str,
    kind: &'static str,
    start: &BTreeMap<Id, f64>,
    exists: impl Fn(&Id) -> bool,
) -> Result<(), DefinitionError> {
    for (id, v) in start {
        if !exists(id) {
            return Err(unknown(ctx, kind, &id.to_string()));
        }
        if !v.is_finite() {
            return Err(invalid(format!("{ctx}: starting values must be finite")));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::{ROME, TINY, TINY_RULES};

    fn tiny(edit: impl Fn(&mut Scenario)) -> Result<World, DefinitionError> {
        let mut s = Scenario::from_yaml(TINY).unwrap();
        edit(&mut s);
        World::new(s, Ruleset::from_yaml(TINY_RULES).unwrap())
    }

    #[test]
    fn reference_scenarios_run_on_the_standard_ruleset() {
        World::standard(Scenario::from_yaml(ROME).unwrap()).unwrap();
        World::standard(Scenario::from_yaml(TINY).unwrap()).unwrap();
    }

    #[test]
    fn offices_and_starting_values_must_exist_in_the_ruleset() {
        type Edit = fn(&mut Scenario);
        let cases: [(&str, Edit); 4] = [
            ("office", |s| {
                s.empires[0].characters[0].office = "pope".into()
            }),
            ("metric", |s| {
                s.empires[0].start.insert("mana".into(), 1.0);
            }),
            ("region stat", |s| {
                s.empires[0].regions[0].start.insert("mana".into(), 1.0);
            }),
            ("resource", |s| {
                s.empires[0].characters[0].start.insert("mana".into(), 1.0);
            }),
        ];
        for (kind, edit) in cases {
            assert!(
                matches!(tiny(edit), Err(DefinitionError::Unknown { kind: k, .. }) if k == kind),
                "{kind}"
            );
        }
        assert!(matches!(
            tiny(|s| {
                s.empires[0].start.insert("treasury".into(), f64::INFINITY);
            }),
            Err(DefinitionError::Invalid(_))
        ));
    }

    #[test]
    fn seats_resolve_to_their_empire_or_the_game_master() {
        let w = tiny(|_| {}).unwrap();
        let crown = w.seat(&"crown".into()).unwrap();
        assert_eq!(crown.empire, Some(&EmpireId::from("kingdom")));
        assert!(!crown.game_master);
        let gm = w.seat(&"gm".into()).unwrap();
        assert!(gm.game_master && gm.empire.is_none());
        assert!(w.seat(&"nobody".into()).is_none());
    }
}
