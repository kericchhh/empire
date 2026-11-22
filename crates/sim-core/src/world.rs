//! A scenario joined to the ruleset it runs on: everything the engine reads that is not state.

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
        let finite = |ctx: &str, v: f64| {
            if v.is_finite() {
                Ok(())
            } else {
                Err(invalid(format!("{ctx}: starting values must be finite")))
            }
        };
        for e in &scenario.empires {
            let ctx = format!("empire `{}`", e.id);
            for (m, v) in &e.start {
                rules
                    .metric(m)
                    .ok_or_else(|| unknown(&ctx, "metric", m.as_str()))?;
                finite(&ctx, *v)?;
            }
            for r in &e.regions {
                let ctx = format!("region `{}`", r.id);
                for (s, v) in &r.start {
                    rules
                        .region_stat(s)
                        .ok_or_else(|| unknown(&ctx, "region stat", s.as_str()))?;
                    finite(&ctx, *v)?;
                }
            }
            for c in &e.characters {
                let ctx = format!("character `{}`", c.id);
                rules
                    .office(&c.office)
                    .ok_or_else(|| unknown(&ctx, "office", c.office.as_str()))?;
                for (r, v) in &c.start {
                    rules
                        .resource(r)
                        .ok_or_else(|| unknown(&ctx, "resource", r.as_str()))?;
                    finite(&ctx, *v)?;
                }
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
        assert!(matches!(
            tiny(|s| s.empires[0].characters[0].office = "pope".into()),
            Err(DefinitionError::Unknown { kind: "office", .. })
        ));
        assert!(matches!(
            tiny(|s| {
                s.empires[0].start.insert("mana".into(), 1.0);
            }),
            Err(DefinitionError::Unknown { kind: "metric", .. })
        ));
        assert!(matches!(
            tiny(|s| {
                s.empires[0].regions[0].start.insert("mana".into(), 1.0);
            }),
            Err(DefinitionError::Unknown {
                kind: "region stat",
                ..
            })
        ));
        assert!(matches!(
            tiny(|s| {
                s.empires[0].characters[0].start.insert("mana".into(), 1.0);
            }),
            Err(DefinitionError::Unknown {
                kind: "resource",
                ..
            })
        ));
    }

    #[test]
    fn starting_values_must_be_finite() {
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
