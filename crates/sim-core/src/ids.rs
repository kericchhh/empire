use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

macro_rules! string_id {
    ($($(#[$doc:meta])* $name:ident),* $(,)?) => {$(
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema, TS)]
        #[serde(transparent)]
        pub struct $name(pub String);

        impl $name {
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.pad(&self.0)
            }
        }

        impl From<&str> for $name {
            fn from(s: &str) -> Self {
                Self(s.to_owned())
            }
        }
    )*};
}

string_id!(
    /// A slot that one external agent or human claims with a token.
    SeatId,
    /// A named individual, or a population group voiced as one, played by a seat.
    CharacterId,
    /// A power on the map with its own territory, numbers and people.
    EmpireId,
    /// A position of power defined by the ruleset, such as head of state. It decides which
    /// actions a character may take and which numbers they see.
    OfficeId,
    RegionId,
    /// A per-empire number such as treasury or stability.
    MetricId,
    /// A per-region number such as population or unrest.
    StatId,
    /// A per-character number such as influence or wealth.
    ResourceId,
    ActionId,
    RuleId,
);

/// One step of simulation time.
#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    TS,
)]
#[serde(transparent)]
pub struct Tick(pub u64);

impl Tick {
    pub fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

impl fmt::Display for Tick {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
