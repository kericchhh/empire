//! The mechanics every world runs on: which numbers exist, which offices characters can hold,
//! what each office can do, and the rules that move the numbers each tick. A ruleset names no
//! setting; scenarios only describe the starting point.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::def::{self, DefinitionError, invalid, unique, unknown};
use crate::ids::*;

const STANDARD: &str = include_str!("../rules/standard.yaml");

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Ruleset {
    /// Numbers every empire carries.
    pub metrics: Vec<StatDef<MetricId>>,
    /// Numbers every region carries.
    pub region_stats: Vec<StatDef<StatId>>,
    /// Numbers every character carries.
    pub resources: Vec<StatDef<ResourceId>>,
    pub offices: Vec<OfficeDef>,
    pub actions: Vec<ActionDef>,
    #[serde(default)]
    pub rules: Vec<RuleDef>,
    #[serde(default)]
    pub limits: Limits,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StatDef<Id> {
    pub id: Id,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// The value before a world starts, and the start when no range is given.
    #[serde(default)]
    pub initial: f64,
    /// A world's start is drawn uniformly from `[low, high)` unless the scenario sets it.
    #[serde(default)]
    pub start_range: Option<[f64; 2]>,
    #[serde(default)]
    pub min: Option<f64>,
    #[serde(default)]
    pub max: Option<f64>,
    /// Offices whose holders see this number. Empty means everyone who sees its owner.
    #[serde(default)]
    pub visible_to: Vec<OfficeId>,
}

impl<Id> StatDef<Id> {
    pub fn clamp(&self, v: f64) -> f64 {
        let v = self.min.map_or(v, |m| v.max(m));
        self.max.map_or(v, |m| v.min(m))
    }

    pub fn visible_to_office(&self, office: &OfficeId) -> bool {
        self.visible_to.is_empty() || self.visible_to.contains(office)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OfficeDef {
    pub id: OfficeId,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Holders are known by name in every empire.
    #[serde(default)]
    pub public: bool,
    /// Actions holders may take.
    #[serde(default)]
    pub actions: Vec<ActionId>,
    /// Taken for a holder whose seat submitted no action this tick.
    #[serde(default)]
    pub default_action: Option<ActionId>,
    #[serde(default)]
    pub region_sight: RegionSight,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RegionSight {
    /// Every region the holder's empire owns.
    #[default]
    Empire,
    /// Only the holder's home region.
    Home,
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
    /// A region the actor's empire owns.
    Region,
    /// Any character the actor knows of.
    Character,
}

/// A change written in a ruleset, resolved to concrete targets at run time.
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
    /// A metric of the empire in scope: the actor's, or the one a rule is evaluating.
    Metric(MetricId),
    /// The region in scope: the action's target region, else the actor's home region,
    /// or the region a per-region rule is evaluating.
    Stat(StatId),
    /// Every region the empire in scope owns.
    AllRegions(StatId),
    /// The acting character.
    Resource(ResourceId),
    /// The character an action targets.
    TargetResource(ResourceId),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ValueRef {
    /// A metric of the empire in scope.
    Metric(MetricId),
    /// A stat of the region in scope.
    Stat(StatId),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RuleDef {
    pub id: RuleId,
    /// Chronicle line when the rule fires. `{region}` and `{empire}` are replaced by names.
    /// Rules without a label change numbers silently.
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub scope: RuleScope,
    #[serde(default)]
    pub when: Option<Condition>,
    /// Probability of firing per tick (per empire or region) once `when` holds.
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
    /// Evaluated once for every empire.
    #[default]
    EachEmpire,
    /// Evaluated once for every region, in the scope of the empire that owns it.
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

impl Ruleset {
    /// The ruleset every world runs on.
    pub fn standard() -> Self {
        Self::from_yaml(STANDARD).expect("the embedded standard ruleset is valid; a test checks it")
    }

    pub fn from_yaml(src: &str) -> Result<Self, DefinitionError> {
        let r: Ruleset = def::from_yaml(src)?;
        r.validate()?;
        Ok(r)
    }

    pub fn office(&self, id: &OfficeId) -> Option<&OfficeDef> {
        self.offices.iter().find(|o| &o.id == id)
    }
    pub fn action(&self, id: &ActionId) -> Option<&ActionDef> {
        self.actions.iter().find(|a| &a.id == id)
    }
    pub fn rule(&self, id: &RuleId) -> Option<&RuleDef> {
        self.rules.iter().find(|r| &r.id == id)
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

    pub fn validate(&self) -> Result<(), DefinitionError> {
        unique("metric", self.metrics.iter().map(|m| m.id.as_str()))?;
        unique(
            "region stat",
            self.region_stats.iter().map(|m| m.id.as_str()),
        )?;
        unique("resource", self.resources.iter().map(|m| m.id.as_str()))?;
        unique("office", self.offices.iter().map(|m| m.id.as_str()))?;
        unique("action", self.actions.iter().map(|m| m.id.as_str()))?;
        unique("rule", self.rules.iter().map(|m| m.id.as_str()))?;

        let offices = |ctx: String, list: &[OfficeId]| {
            list.iter().try_for_each(|o| match self.office(o) {
                Some(_) => Ok(()),
                None => Err(unknown(&ctx, "office", o.as_str())),
            })
        };
        for m in &self.metrics {
            let ctx = format!("metric `{}`", m.id);
            offices(ctx.clone(), &m.visible_to)?;
            check_range(&ctx, m)?;
        }
        for m in &self.region_stats {
            let ctx = format!("region stat `{}`", m.id);
            offices(ctx.clone(), &m.visible_to)?;
            check_range(&ctx, m)?;
        }
        for m in &self.resources {
            let ctx = format!("resource `{}`", m.id);
            offices(ctx.clone(), &m.visible_to)?;
            check_range(&ctx, m)?;
        }

        for office in &self.offices {
            let ctx = format!("office `{}`", office.id);
            for a in &office.actions {
                if self.action(a).is_none() {
                    return Err(unknown(&ctx, "action", a.as_str()));
                }
            }
            if let Some(a) = &office.default_action {
                if !office.actions.contains(a) {
                    return Err(invalid(format!(
                        "{ctx}: default_action `{a}` must also be listed in its actions"
                    )));
                }
                // A default runs with no target, from the holder's home region, and every
                // character has one, so it may read and change that region.
                if self.action(a).is_some_and(|d| d.target != TargetKind::None) {
                    return Err(invalid(format!(
                        "{ctx}: default_action `{a}` must not need a target"
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
                return Err(invalid(format!("{ctx}: chance must be in 0..=1")));
            }
            if let Some(c) = &r.when {
                self.check_value(&ctx, &c.value, r.scope)?;
                if c.for_ticks == 0 {
                    return Err(invalid(format!("{ctx}: for_ticks must be at least 1")));
                }
            }
            for e in &r.effects {
                if matches!(
                    e.target,
                    EffectTarget::Resource(_) | EffectTarget::TargetResource(_)
                ) {
                    return Err(invalid(format!(
                        "{ctx}: rules cannot change character resources"
                    )));
                }
                if r.scope == RuleScope::EachEmpire && matches!(e.target, EffectTarget::Stat(_)) {
                    return Err(invalid(format!(
                        "{ctx}: an empire rule has no region in scope; use all_regions"
                    )));
                }
                self.check_effect(&ctx, e, false)?;
                if let Some(v) = &e.scale_by {
                    self.check_value(&ctx, v, r.scope)?;
                }
            }
        }
        Ok(())
    }

    fn check_effect(
        &self,
        ctx: &str,
        e: &EffectDef,
        has_target_character: bool,
    ) -> Result<(), DefinitionError> {
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
            EffectTarget::TargetResource(_) if !has_target_character => Err(invalid(format!(
                "{ctx}: target_resource needs an action with target: character"
            ))),
            _ => Ok(()),
        }
    }

    fn check_value(
        &self,
        ctx: &str,
        v: &ValueRef,
        scope: RuleScope,
    ) -> Result<(), DefinitionError> {
        match v {
            ValueRef::Metric(m) if self.metric(m).is_none() => {
                Err(unknown(ctx, "metric", m.as_str()))
            }
            ValueRef::Stat(s) if self.region_stat(s).is_none() => {
                Err(unknown(ctx, "region stat", s.as_str()))
            }
            ValueRef::Stat(_) if scope == RuleScope::EachEmpire => Err(invalid(format!(
                "{ctx}: a stat value needs a region in scope"
            ))),
            _ => Ok(()),
        }
    }
}

fn check_range<Id>(ctx: &str, d: &StatDef<Id>) -> Result<(), DefinitionError> {
    if let Some([lo, hi]) = d.start_range
        && !(lo.is_finite() && hi.is_finite() && lo <= hi)
    {
        return Err(invalid(format!(
            "{ctx}: start_range must be two finite numbers, low first"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::TINY_RULES;

    fn with(edit: impl Fn(&mut Ruleset)) -> Result<(), DefinitionError> {
        let mut r = Ruleset::from_yaml(TINY_RULES).unwrap();
        edit(&mut r);
        r.validate()
    }

    #[test]
    fn the_standard_ruleset_is_valid_and_every_office_can_act() {
        let r = Ruleset::standard();
        assert!(!r.offices.is_empty() && !r.actions.is_empty() && !r.rules.is_empty());
        for office in &r.offices {
            assert!(
                !office.actions.is_empty(),
                "office `{}` can do nothing",
                office.id
            );
        }
    }

    #[test]
    fn unknown_fields_are_rejected_so_typos_surface() {
        let src = TINY_RULES.replace("metrics:", "metrcs: []\nmetrics:");
        assert!(matches!(
            Ruleset::from_yaml(&src),
            Err(DefinitionError::Yaml(_))
        ));
    }

    #[test]
    fn dangling_references_are_rejected() {
        assert!(matches!(
            with(|r| r.offices[0].actions.push("fly".into())),
            Err(DefinitionError::Unknown { kind: "action", .. })
        ));
        assert!(matches!(
            with(|r| r.actions[0].effects[0].target = EffectTarget::Metric("mana".into())),
            Err(DefinitionError::Unknown { kind: "metric", .. })
        ));
        assert!(matches!(
            with(|r| r.metrics[0].visible_to.push("pope".into())),
            Err(DefinitionError::Unknown { kind: "office", .. })
        ));
        assert!(matches!(
            with(|r| r.actions[0].effects[0].scale_by = Some(ValueRef::Stat("wealthh".into()))),
            Err(DefinitionError::Unknown {
                kind: "region stat",
                ..
            })
        ));
    }

    #[test]
    fn default_actions_must_be_allowed_and_untargeted() {
        let feast = ActionId::from("feast");
        assert!(matches!(
            with(|r| r.offices[0].default_action = Some("work".into())),
            Err(DefinitionError::Invalid(_))
        ));
        assert!(matches!(
            with(|r| r.offices[0].default_action = Some(feast.clone())),
            Err(DefinitionError::Invalid(_))
        ));
    }

    #[test]
    fn effects_need_a_scope_that_provides_their_target() {
        assert!(matches!(
            with(|r| r.actions[0].effects[0].target =
                EffectTarget::TargetResource("influence".into())),
            Err(DefinitionError::Invalid(_))
        ));
        assert!(matches!(
            with(|r| r.rules[0].scope = RuleScope::EachEmpire),
            Err(DefinitionError::Invalid(_))
        ));
        assert!(matches!(
            with(|r| r.rules[0].effects[0].target = EffectTarget::Resource("influence".into())),
            Err(DefinitionError::Invalid(_))
        ));
        assert!(matches!(
            with(|r| r.rules[0].chance = 1.5),
            Err(DefinitionError::Invalid(_))
        ));
        assert!(matches!(
            with(|r| r.rules[0].when.as_mut().unwrap().for_ticks = 0),
            Err(DefinitionError::Invalid(_))
        ));
    }

    #[test]
    fn start_ranges_must_be_ordered_and_finite() {
        for bad in [[1.0, 0.0], [0.0, f64::INFINITY], [f64::NAN, 1.0]] {
            assert!(matches!(
                with(|r| r.metrics[0].start_range = Some(bad)),
                Err(DefinitionError::Invalid(_))
            ));
        }
    }

    #[test]
    fn ids_cannot_contain_key_separators() {
        for bad in ["a@b", "a:b", "a#b", ""] {
            assert!(
                matches!(
                    with(|r| r.rules[0].id = bad.into()),
                    Err(DefinitionError::Invalid(_))
                ),
                "{bad:?}"
            );
        }
    }
}
