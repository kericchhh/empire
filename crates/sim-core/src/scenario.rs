//! The creator-authored description of a world. Everything setting-specific lives here.

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ids::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub calendar: Calendar,
    /// Free-form facts every seat knows: religion, ideology, lifestyle, values.
    #[serde(default)]
    pub culture: BTreeMap<String, String>,
    #[serde(default)]
    pub theme: Theme,
    /// Empire-wide numbers.
    #[serde(default)]
    pub metrics: Vec<StatDef<MetricId>>,
    /// Numbers every region carries.
    #[serde(default)]
    pub region_stats: Vec<StatDef<StatId>>,
    /// Numbers every character carries.
    #[serde(default)]
    pub resources: Vec<StatDef<ResourceId>>,
    pub regions: Vec<RegionDef>,
    pub roles: Vec<RoleDef>,
    pub characters: Vec<CharacterDef>,
    pub seats: Vec<SeatDef>,
    #[serde(default)]
    pub actions: Vec<ActionDef>,
    #[serde(default)]
    pub rules: Vec<RuleDef>,
    /// Conditions that end the simulation: triumph, collapse, anything in between.
    #[serde(default)]
    pub endings: Vec<EndingDef>,
    #[serde(default)]
    pub limits: Limits,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
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
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Theme {
    #[serde(default)]
    pub palette: Vec<String>,
    #[serde(default)]
    pub font_display: Option<String>,
    #[serde(default)]
    pub font_body: Option<String>,
    /// Setting vocabulary, e.g. currency -> denarii, ruler -> Imperator.
    #[serde(default)]
    pub terms: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StatDef<Id> {
    pub id: Id,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub initial: f64,
    #[serde(default)]
    pub min: Option<f64>,
    #[serde(default)]
    pub max: Option<f64>,
    /// Roles that can see this number. Empty means everyone.
    #[serde(default)]
    pub visible_to: Vec<RoleId>,
}

impl<Id> StatDef<Id> {
    pub fn clamp(&self, v: f64) -> f64 {
        let v = self.min.map_or(v, |m| v.max(m));
        self.max.map_or(v, |m| v.min(m))
    }

    pub fn visible_to_role(&self, role: &RoleId) -> bool {
        self.visible_to.is_empty() || self.visible_to.contains(role)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RegionDef {
    pub id: RegionId,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Overrides of `region_stats` initial values.
    #[serde(default)]
    pub stats: BTreeMap<StatId, f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RoleDef {
    pub id: RoleId,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Actions characters in this role may take.
    #[serde(default)]
    pub actions: Vec<ActionId>,
    /// Taken for a character whose seat submitted no action this tick.
    #[serde(default)]
    pub default_action: Option<ActionId>,
    #[serde(default)]
    pub region_sight: RegionSight,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RegionSight {
    #[default]
    All,
    /// Only the character's home region.
    Home,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterDef {
    pub id: CharacterId,
    pub name: String,
    pub role: RoleId,
    #[serde(default)]
    pub region: Option<RegionId>,
    #[serde(default)]
    pub persona: String,
    #[serde(default)]
    pub goals: Vec<String>,
    /// Known only to this character's seat and the game master.
    #[serde(default)]
    pub secrets: Vec<String>,
    /// Overrides of `resources` initial values.
    #[serde(default)]
    pub resources: BTreeMap<ResourceId, f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SeatDef {
    pub id: SeatId,
    pub name: String,
    #[serde(default)]
    pub controls: Vec<CharacterId>,
    /// Sees everything and rules on free-form actions.
    #[serde(default)]
    pub game_master: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ActionDef {
    pub id: ActionId,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub target: TargetKind,
    /// Resources the actor spends. The action is rejected if any would drop below zero.
    #[serde(default)]
    pub cost: BTreeMap<ResourceId, f64>,
    #[serde(default)]
    pub effects: Vec<EffectDef>,
    /// Visible only to the actor's seat and the game master.
    #[serde(default)]
    pub secret: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TargetKind {
    #[default]
    None,
    Region,
    Character,
}

/// A change written in a scenario, resolved to concrete targets at run time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EffectDef {
    pub target: EffectTarget,
    pub delta: f64,
    /// Multiply `delta` by this value read in the same scope.
    #[serde(default)]
    pub scale_by: Option<ValueRef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EffectTarget {
    Metric(MetricId),
    /// The region in scope: the action's target region, else the actor's home region,
    /// or the region a per-region rule is evaluating.
    Stat(StatId),
    /// Every region.
    AllRegions(StatId),
    /// The acting character.
    Resource(ResourceId),
    /// The character an action targets.
    TargetResource(ResourceId),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ValueRef {
    Metric(MetricId),
    /// A stat of the region in scope.
    Stat(StatId),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RuleDef {
    pub id: RuleId,
    /// Chronicle line when the rule fires. `{region}` is replaced by the region name.
    /// Rules without a label change numbers silently.
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub scope: RuleScope,
    #[serde(default)]
    pub when: Option<Condition>,
    /// Probability of firing per tick (per region for regional rules) once `when` holds.
    #[serde(default = "one")]
    pub chance: f64,
    pub effects: Vec<EffectDef>,
}

fn one() -> f64 {
    1.0
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RuleScope {
    #[default]
    Empire,
    EachRegion,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Condition {
    pub value: ValueRef,
    #[serde(default)]
    pub below: Option<f64>,
    #[serde(default)]
    pub above: Option<f64>,
    /// Consecutive ticks the condition must hold before it counts.
    #[serde(default = "one_tick")]
    pub for_ticks: u32,
}

fn one_tick() -> u32 {
    1
}

impl Condition {
    pub fn holds(&self, v: f64) -> bool {
        self.below.is_none_or(|b| v < b) && self.above.is_none_or(|a| v > a)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EndingDef {
    pub id: EndingId,
    pub label: String,
    /// Evaluated at empire scope.
    pub when: Condition,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, default)]
pub struct Limits {
    pub orders_per_character: usize,
    pub text_chars: usize,
    pub notes_kept: usize,
    /// Largest absolute delta a game-master ruling may apply per effect.
    pub ruling_max_delta: f64,
    pub ruling_max_effects: usize,
    /// Messages plus notes one seat may write per tick.
    pub chat_per_seat: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            orders_per_character: 2,
            text_chars: 1200,
            notes_kept: 12,
            ruling_max_delta: 0.2,
            ruling_max_effects: 4,
            chat_per_seat: 10,
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ScenarioError {
    #[error("invalid YAML: {0}")]
    Yaml(String),
    #[error("duplicate {kind} id `{id}`")]
    Duplicate { kind: &'static str, id: String },
    #[error("{context} refers to unknown {kind} `{id}`")]
    Unknown {
        context: String,
        kind: &'static str,
        id: String,
    },
    #[error("{0}")]
    Invalid(String),
}

impl Scenario {
    pub fn from_yaml(src: &str) -> Result<Self, ScenarioError> {
        let s: Scenario = serde_yaml_ng::with::singleton_map_recursive::deserialize(
            serde_yaml_ng::Deserializer::from_str(src),
        )
        .map_err(|e| ScenarioError::Yaml(e.to_string()))?;
        s.validate()?;
        Ok(s)
    }

    pub fn role(&self, id: &RoleId) -> Option<&RoleDef> {
        self.roles.iter().find(|r| &r.id == id)
    }
    pub fn character(&self, id: &CharacterId) -> Option<&CharacterDef> {
        self.characters.iter().find(|c| &c.id == id)
    }
    pub fn seat(&self, id: &SeatId) -> Option<&SeatDef> {
        self.seats.iter().find(|s| &s.id == id)
    }
    pub fn action(&self, id: &ActionId) -> Option<&ActionDef> {
        self.actions.iter().find(|a| &a.id == id)
    }
    pub fn region(&self, id: &RegionId) -> Option<&RegionDef> {
        self.regions.iter().find(|r| &r.id == id)
    }
    pub fn metric(&self, id: &MetricId) -> Option<&StatDef<MetricId>> {
        self.metrics.iter().find(|m| &m.id == id)
    }
    pub fn region_stat(&self, id: &StatId) -> Option<&StatDef<StatId>> {
        self.region_stats.iter().find(|m| &m.id == id)
    }
    pub fn resource(&self, id: &ResourceId) -> Option<&StatDef<ResourceId>> {
        self.resources.iter().find(|m| &m.id == id)
    }
    pub fn seat_of(&self, c: &CharacterId) -> Option<&SeatDef> {
        self.seats.iter().find(|s| s.controls.contains(c))
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

    pub fn validate(&self) -> Result<(), ScenarioError> {
        unique("metric", self.metrics.iter().map(|m| m.id.as_str()))?;
        unique(
            "region stat",
            self.region_stats.iter().map(|m| m.id.as_str()),
        )?;
        unique("resource", self.resources.iter().map(|m| m.id.as_str()))?;
        unique("region", self.regions.iter().map(|m| m.id.as_str()))?;
        unique("role", self.roles.iter().map(|m| m.id.as_str()))?;
        unique("character", self.characters.iter().map(|m| m.id.as_str()))?;
        unique("seat", self.seats.iter().map(|m| m.id.as_str()))?;
        unique("action", self.actions.iter().map(|m| m.id.as_str()))?;
        unique("rule", self.rules.iter().map(|m| m.id.as_str()))?;
        unique("ending", self.endings.iter().map(|m| m.id.as_str()))?;

        if self.calendar.periods.is_empty() {
            return Err(ScenarioError::Invalid(
                "calendar.periods must name at least one period".into(),
            ));
        }

        let check_role = |ctx: &str, r: &RoleId| match self.role(r) {
            Some(_) => Ok(()),
            None => Err(unknown(ctx, "role", r.as_str())),
        };
        let check_action = |ctx: &str, a: &ActionId| match self.action(a) {
            Some(_) => Ok(()),
            None => Err(unknown(ctx, "action", a.as_str())),
        };

        for m in &self.metrics {
            for r in &m.visible_to {
                check_role(&format!("metric `{}`", m.id), r)?;
            }
        }
        for r in &self.regions {
            for s in r.stats.keys() {
                if self.region_stat(s).is_none() {
                    return Err(unknown(
                        &format!("region `{}`", r.id),
                        "region stat",
                        s.as_str(),
                    ));
                }
            }
        }
        for role in &self.roles {
            let ctx = format!("role `{}`", role.id);
            for a in &role.actions {
                check_action(&ctx, a)?;
            }
            if let Some(a) = &role.default_action {
                check_action(&ctx, a)?;
                if !role.actions.contains(a) {
                    return Err(ScenarioError::Invalid(format!(
                        "{ctx}: default_action `{a}` must also be listed in its actions"
                    )));
                }
                if self.action(a).is_some_and(|d| d.target != TargetKind::None) {
                    return Err(ScenarioError::Invalid(format!(
                        "{ctx}: default_action `{a}` must not need a target"
                    )));
                }
            }
        }
        for c in &self.characters {
            let ctx = format!("character `{}`", c.id);
            check_role(&ctx, &c.role)?;
            let default = self
                .role(&c.role)
                .and_then(|r| r.default_action.as_ref())
                .and_then(|a| self.action(a));
            if c.region.is_none()
                && let Some(a) = default
                && a.effects.iter().any(|e| {
                    matches!(e.target, EffectTarget::Stat(_))
                        || matches!(e.scale_by, Some(ValueRef::Stat(_)))
                })
            {
                return Err(ScenarioError::Invalid(format!(
                    "{ctx} has no region, but its default action `{}` affects one",
                    a.id
                )));
            }
            if let Some(r) = &c.region
                && self.region(r).is_none()
            {
                return Err(unknown(&ctx, "region", r.as_str()));
            }
            for r in c.resources.keys() {
                if self.resource(r).is_none() {
                    return Err(unknown(&ctx, "resource", r.as_str()));
                }
            }
        }
        let mut controlled = BTreeSet::new();
        for s in &self.seats {
            for c in &s.controls {
                if self.character(c).is_none() {
                    return Err(unknown(
                        &format!("seat `{}`", s.id),
                        "character",
                        c.as_str(),
                    ));
                }
                if !controlled.insert(c) {
                    return Err(ScenarioError::Invalid(format!(
                        "character `{c}` is controlled by more than one seat"
                    )));
                }
            }
        }
        for a in &self.actions {
            let ctx = format!("action `{}`", a.id);
            for r in a.cost.keys() {
                if self.resource(r).is_none() {
                    return Err(unknown(&ctx, "resource", r.as_str()));
                }
            }
            for e in &a.effects {
                self.check_effect(&ctx, e, a.target == TargetKind::Character)?;
                if let Some(v) = &e.scale_by {
                    self.check_value(&ctx, v, RuleScope::EachRegion)?;
                }
            }
        }
        for r in &self.rules {
            let ctx = format!("rule `{}`", r.id);
            if !(0.0..=1.0).contains(&r.chance) {
                return Err(ScenarioError::Invalid(format!(
                    "{ctx}: chance must be in 0..=1"
                )));
            }
            if let Some(c) = &r.when {
                self.check_value(&ctx, &c.value, r.scope)?;
                check_streak(&ctx, c)?;
            }
            for e in &r.effects {
                if matches!(
                    e.target,
                    EffectTarget::Resource(_) | EffectTarget::TargetResource(_)
                ) {
                    return Err(ScenarioError::Invalid(format!(
                        "{ctx}: rules cannot change character resources"
                    )));
                }
                if r.scope == RuleScope::Empire && matches!(e.target, EffectTarget::Stat(_)) {
                    return Err(ScenarioError::Invalid(format!(
                        "{ctx}: an empire rule has no region in scope; use all_regions"
                    )));
                }
                self.check_effect(&ctx, e, false)?;
                if let Some(v) = &e.scale_by {
                    self.check_value(&ctx, v, r.scope)?;
                }
            }
        }
        for e in &self.endings {
            let ctx = format!("ending `{}`", e.id);
            self.check_value(&ctx, &e.when.value, RuleScope::Empire)?;
            check_streak(&ctx, &e.when)?;
        }
        Ok(())
    }

    fn check_effect(
        &self,
        ctx: &str,
        e: &EffectDef,
        has_target_character: bool,
    ) -> Result<(), ScenarioError> {
        match &e.target {
            EffectTarget::Metric(m) if self.metric(m).is_none() => {
                Err(unknown(ctx, "metric", m.as_str()))
            }
            EffectTarget::Stat(s) | EffectTarget::AllRegions(s)
                if self.region_stat(s).is_none() =>
            {
                Err(unknown(ctx, "region stat", s.as_str()))
            }
            EffectTarget::Resource(r) | EffectTarget::TargetResource(r)
                if self.resource(r).is_none() =>
            {
                Err(unknown(ctx, "resource", r.as_str()))
            }
            EffectTarget::TargetResource(_) if !has_target_character => {
                Err(ScenarioError::Invalid(format!(
                    "{ctx}: target_resource needs an action with target: character"
                )))
            }
            _ => Ok(()),
        }
    }

    fn check_value(&self, ctx: &str, v: &ValueRef, scope: RuleScope) -> Result<(), ScenarioError> {
        match v {
            ValueRef::Metric(m) if self.metric(m).is_none() => {
                Err(unknown(ctx, "metric", m.as_str()))
            }
            ValueRef::Stat(s) if self.region_stat(s).is_none() => {
                Err(unknown(ctx, "region stat", s.as_str()))
            }
            ValueRef::Stat(_) if scope == RuleScope::Empire => Err(ScenarioError::Invalid(
                format!("{ctx}: a stat value needs a region in scope"),
            )),
            _ => Ok(()),
        }
    }
}

fn check_streak(ctx: &str, c: &Condition) -> Result<(), ScenarioError> {
    if c.for_ticks == 0 {
        return Err(ScenarioError::Invalid(format!(
            "{ctx}: for_ticks must be at least 1"
        )));
    }
    Ok(())
}

fn unique<'a>(kind: &'static str, ids: impl Iterator<Item = &'a str>) -> Result<(), ScenarioError> {
    let mut seen = BTreeSet::new();
    for id in ids {
        // `@` and `:` separate parts of the engine's streak and random-stream keys.
        if id.is_empty() || id.contains(['@', ':']) {
            return Err(ScenarioError::Invalid(format!(
                "{kind} id `{id}` must be non-empty and may not contain `@` or `:`"
            )));
        }
        if !seen.insert(id) {
            return Err(ScenarioError::Duplicate {
                kind,
                id: id.to_owned(),
            });
        }
    }
    Ok(())
}

fn unknown(ctx: &str, kind: &'static str, id: &str) -> ScenarioError {
    ScenarioError::Unknown {
        context: ctx.to_owned(),
        kind,
        id: id.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::{ROME, TINY};

    fn with(edit: impl Fn(&mut Scenario)) -> Result<(), ScenarioError> {
        let mut s = Scenario::from_yaml(TINY).unwrap();
        edit(&mut s);
        s.validate()
    }

    #[test]
    fn reference_scenarios_are_valid() {
        Scenario::from_yaml(ROME).unwrap();
        Scenario::from_yaml(TINY).unwrap();
    }

    #[test]
    fn unknown_fields_are_rejected_so_typos_surface() {
        let src = TINY.replace("name: Tiny Kingdom", "name: Tiny Kingdom\nnmae: oops");
        assert!(matches!(
            Scenario::from_yaml(&src),
            Err(ScenarioError::Yaml(_))
        ));
    }

    #[test]
    fn dangling_references_are_rejected() {
        assert_eq!(
            with(|s| s.characters[0].role = "pirate".into()),
            Err(ScenarioError::Unknown {
                context: "character `queen`".into(),
                kind: "role",
                id: "pirate".into()
            })
        );
        assert!(matches!(
            with(|s| s.seats[0].controls.push("ghost".into())),
            Err(ScenarioError::Unknown {
                kind: "character",
                ..
            })
        ));
        assert!(matches!(
            with(|s| s.roles[0].actions.push("fly".into())),
            Err(ScenarioError::Unknown { kind: "action", .. })
        ));
        assert!(matches!(
            with(|s| s.actions[0].effects[0].target = EffectTarget::Metric("mana".into())),
            Err(ScenarioError::Unknown { kind: "metric", .. })
        ));
    }

    #[test]
    fn duplicate_ids_are_rejected() {
        assert_eq!(
            with(|s| s.regions.push(s.regions[0].clone())),
            Err(ScenarioError::Duplicate {
                kind: "region",
                id: "north".into()
            })
        );
    }

    #[test]
    fn a_character_belongs_to_at_most_one_seat() {
        assert!(matches!(
            with(|s| s.seats[1].controls.push("queen".into())),
            Err(ScenarioError::Invalid(_))
        ));
    }

    #[test]
    fn default_actions_must_be_allowed_and_untargeted() {
        assert!(matches!(
            with(|s| s.roles[0].default_action = Some("work".into())),
            Err(ScenarioError::Invalid(_))
        ));
        assert!(matches!(
            with(|s| s.roles[0].default_action = Some("feast".into())),
            Err(ScenarioError::Invalid(_))
        ));
    }

    #[test]
    fn effects_need_a_scope_that_provides_their_target() {
        assert!(matches!(
            with(|s| s.actions[0].effects[0].target = EffectTarget::TargetResource("favor".into())),
            Err(ScenarioError::Invalid(_))
        ));
        assert!(matches!(
            with(|s| s.rules[0].scope = RuleScope::Empire),
            Err(ScenarioError::Invalid(_))
        ));
        assert!(matches!(
            with(|s| s.rules[0].effects[0].target = EffectTarget::Resource("favor".into())),
            Err(ScenarioError::Invalid(_))
        ));
        assert!(matches!(
            with(|s| s.rules[0].chance = 1.5),
            Err(ScenarioError::Invalid(_))
        ));
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

    #[test]
    fn a_zero_tick_streak_is_rejected() {
        assert!(matches!(
            with(|s| s.rules[0].when.as_mut().unwrap().for_ticks = 0),
            Err(ScenarioError::Invalid(_))
        ));
        assert!(matches!(
            with(|s| s.endings[0].when.for_ticks = 0),
            Err(ScenarioError::Invalid(_))
        ));
    }

    #[test]
    fn action_scaling_must_name_a_real_value() {
        assert!(matches!(
            with(|s| s.actions[0].effects[0].scale_by = Some(ValueRef::Stat("wealthh".into()))),
            Err(ScenarioError::Unknown {
                kind: "region stat",
                ..
            })
        ));
    }

    #[test]
    fn ids_cannot_contain_key_separators() {
        for bad in ["a@b", "a:b", ""] {
            assert!(
                matches!(
                    with(|s| s.rules[0].id = bad.into()),
                    Err(ScenarioError::Invalid(_))
                ),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn a_regionless_character_cannot_default_to_a_regional_action() {
        assert!(matches!(
            with(|s| {
                s.characters[2].region = None;
                s.actions[3].effects[0].target = EffectTarget::Stat("unrest".into());
            }),
            Err(ScenarioError::Invalid(_))
        ));
    }
}
