//! The creator-authored starting point of a world: the setting, the empires and their people.
//! It holds no mechanics and no endings. What happens next comes from the ruleset and the
//! agents' decisions.

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::def::{self, DefinitionError, invalid, unique, unknown};
use crate::ids::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    pub name: String,
    /// The situation at the start, known to every seat.
    #[serde(default)]
    pub description: String,
    pub calendar: Calendar,
    #[serde(default)]
    pub theme: Theme,
    pub empires: Vec<EmpireDef>,
    /// A seat that sees everything and rules on free-form actions.
    #[serde(default)]
    pub game_master: Option<GameMasterDef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct Calendar {
    pub start_year: i64,
    /// One name per tick in a year, e.g. the four seasons.
    pub periods: Vec<String>,
    /// Suffix for years below zero, e.g. "BC". Without it, negative years print as-is.
    #[serde(default)]
    pub negative_suffix: Option<String>,
    #[serde(default)]
    pub positive_suffix: Option<String>,
}

/// Presentation hints the UI applies as CSS variables and vocabulary.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct Theme {
    #[serde(default)]
    pub palette: Vec<String>,
    #[serde(default)]
    pub font_display: Option<String>,
    #[serde(default)]
    pub font_body: Option<String>,
    /// Setting vocabulary, e.g. currency -> denarii, head_of_state -> Princeps.
    #[serde(default)]
    pub terms: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct EmpireDef {
    pub id: EmpireId,
    pub name: String,
    /// Known to every seat.
    #[serde(default)]
    pub description: String,
    /// Free-form facts this empire's people live by: religion, ideology, lifestyle, values.
    #[serde(default)]
    pub culture: BTreeMap<String, String>,
    /// Optional starting values for the ruleset's empire metrics. Unset ones are generated.
    #[serde(default)]
    pub start: BTreeMap<MetricId, f64>,
    /// The territory the empire starts with.
    pub regions: Vec<RegionDef>,
    pub characters: Vec<CharacterDef>,
    pub seats: Vec<SeatDef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct RegionDef {
    pub id: RegionId,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Optional starting values for the ruleset's region stats. Unset ones are generated.
    #[serde(default)]
    pub start: BTreeMap<StatId, f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct CharacterDef {
    pub id: CharacterId,
    pub name: String,
    /// How the setting names this character's position, e.g. "Princeps" or "Poet".
    #[serde(default)]
    pub title: String,
    /// The ruleset office the character starts in. It decides what they can do and see.
    pub office: OfficeId,
    /// Home region. Must belong to the character's empire.
    pub region: RegionId,
    #[serde(default)]
    pub persona: String,
    #[serde(default)]
    pub goals: Vec<String>,
    /// Known only to this character's seat and the game master.
    #[serde(default)]
    pub secrets: Vec<String>,
    /// Optional starting values for the ruleset's resources. Unset ones are generated.
    #[serde(default)]
    pub start: BTreeMap<ResourceId, f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct SeatDef {
    pub id: SeatId,
    pub name: String,
    /// Characters of this seat's empire that it plays.
    pub controls: Vec<CharacterId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct GameMasterDef {
    pub id: SeatId,
    pub name: String,
}

impl Scenario {
    /// Parse and check the scenario on its own. `World::new` then checks it against a ruleset.
    pub fn from_yaml(src: &str) -> Result<Self, DefinitionError> {
        let s: Scenario = def::from_yaml(src)?;
        s.validate()?;
        Ok(s)
    }

    pub fn regions(&self) -> impl Iterator<Item = (&EmpireDef, &RegionDef)> {
        self.empires
            .iter()
            .flat_map(|e| e.regions.iter().map(move |r| (e, r)))
    }
    pub fn characters(&self) -> impl Iterator<Item = (&EmpireDef, &CharacterDef)> {
        self.empires
            .iter()
            .flat_map(|e| e.characters.iter().map(move |c| (e, c)))
    }
    pub fn seats(&self) -> impl Iterator<Item = (&EmpireDef, &SeatDef)> {
        self.empires
            .iter()
            .flat_map(|e| e.seats.iter().map(move |s| (e, s)))
    }

    /// Every seat id, including the game master's.
    pub fn seat_ids(&self) -> impl Iterator<Item = &SeatId> {
        self.seats()
            .map(|(_, s)| &s.id)
            .chain(self.game_master.iter().map(|g| &g.id))
    }

    /// Human-readable date for a tick, e.g. "Spring, 27 BC".
    pub fn date(&self, tick: Tick) -> String {
        let per_year = self.calendar.periods.len().max(1) as u64;
        let year = self.calendar.start_year + (tick.0 / per_year) as i64;
        let period = self
            .calendar
            .periods
            .get((tick.0 % per_year) as usize)
            .map(String::as_str)
            .unwrap_or("Year");
        let year = match (
            year < 0,
            &self.calendar.negative_suffix,
            &self.calendar.positive_suffix,
        ) {
            (true, Some(s), _) => format!("{} {s}", -year),
            (false, _, Some(s)) => format!("{year} {s}"),
            _ => year.to_string(),
        };
        format!("{period}, {year}")
    }

    /// Checks that need no ruleset: unique ids and references within the scenario.
    pub fn validate(&self) -> Result<(), DefinitionError> {
        if self.calendar.periods.is_empty() {
            return Err(invalid("calendar.periods must name at least one period"));
        }
        if self.empires.is_empty() {
            return Err(invalid("a scenario needs at least one empire"));
        }
        unique("empire", self.empires.iter().map(|e| e.id.as_str()))?;
        unique("region", self.regions().map(|(_, r)| r.id.as_str()))?;
        unique("character", self.characters().map(|(_, c)| c.id.as_str()))?;
        unique("seat", self.seat_ids().map(SeatId::as_str))?;

        for e in &self.empires {
            if e.regions.is_empty() {
                return Err(invalid(format!(
                    "empire `{}` needs at least one region",
                    e.id
                )));
            }
            for c in &e.characters {
                if !e.regions.iter().any(|r| r.id == c.region) {
                    return Err(unknown(
                        &format!("character `{}`", c.id),
                        "region of its empire",
                        c.region.as_str(),
                    ));
                }
            }
            let mut controlled = BTreeSet::new();
            for s in &e.seats {
                for c in &s.controls {
                    if !e.characters.iter().any(|x| &x.id == c) {
                        return Err(unknown(
                            &format!("seat `{}`", s.id),
                            "character of its empire",
                            c.as_str(),
                        ));
                    }
                    if !controlled.insert(c) {
                        return Err(invalid(format!(
                            "character `{c}` is controlled by more than one seat"
                        )));
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::{ROME, TINY};

    fn with(edit: impl Fn(&mut Scenario)) -> Result<(), DefinitionError> {
        let mut s = Scenario::from_yaml(TINY).unwrap();
        edit(&mut s);
        s.validate()
    }

    #[test]
    fn reference_scenarios_parse() {
        Scenario::from_yaml(ROME).unwrap();
        Scenario::from_yaml(TINY).unwrap();
    }

    #[test]
    fn mechanics_and_endings_are_not_part_of_a_scenario() {
        for field in ["rules: []", "endings: []", "actions: []", "metrics: []"] {
            let src = format!("{TINY}\n{field}\n");
            assert!(
                matches!(Scenario::from_yaml(&src), Err(DefinitionError::Yaml(_))),
                "{field}"
            );
        }
    }

    #[test]
    fn unknown_fields_are_rejected_so_typos_surface() {
        let src = TINY.replace("name: Tiny World", "name: Tiny World\nnmae: oops");
        assert!(matches!(
            Scenario::from_yaml(&src),
            Err(DefinitionError::Yaml(_))
        ));
    }

    #[test]
    fn references_must_stay_inside_their_empire() {
        let cases: [fn(&mut Scenario); 4] = [
            |s| s.empires[0].seats[0].controls.push("ghost".into()),
            |s| s.empires[0].characters[0].region = "atlantis".into(),
            // `isle` and `pirate_king` belong to another empire.
            |s| s.empires[0].characters[0].region = "isle".into(),
            |s| s.empires[0].seats[0].controls.push("pirate_king".into()),
        ];
        for (i, edit) in cases.into_iter().enumerate() {
            assert!(
                matches!(with(edit), Err(DefinitionError::Unknown { .. })),
                "case {i}"
            );
        }
    }

    #[test]
    fn ids_are_unique_across_empires() {
        assert_eq!(
            with(|s| {
                let r = s.empires[0].regions[0].clone();
                s.empires[1].regions.push(r);
            }),
            Err(DefinitionError::Duplicate {
                kind: "region",
                id: "north".into()
            })
        );
        assert!(matches!(
            with(|s| s.game_master.as_mut().unwrap().id = "crown".into()),
            Err(DefinitionError::Duplicate { kind: "seat", .. })
        ));
    }

    #[test]
    fn a_character_has_one_seat_and_an_empire_has_territory() {
        let cases: [fn(&mut Scenario); 3] = [
            |s| s.empires[0].seats[1].controls.push("queen".into()),
            |s| s.empires[1].regions.clear(),
            |s| s.empires.clear(),
        ];
        for (i, edit) in cases.into_iter().enumerate() {
            assert!(
                matches!(with(edit), Err(DefinitionError::Invalid(_))),
                "case {i}"
            );
        }
    }

    #[test]
    fn dates_follow_the_calendar() {
        let rome = Scenario::from_yaml(ROME).unwrap();
        assert_eq!(rome.date(Tick(0)), "Spring, 27 BC");
        assert_eq!(rome.date(Tick(5)), "Summer, 26 BC");
        assert_eq!(rome.date(Tick(4 * 27)), "Spring, 0 AD");
        let tiny = Scenario::from_yaml(TINY).unwrap();
        assert_eq!(tiny.date(Tick(3)), "Autumn, 2");
    }
}
