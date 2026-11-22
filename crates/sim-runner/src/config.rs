//! Which seats are played by an agent, and how. Seats left out fall back to the rules engine.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use serde::Deserialize;
use sim_core::SeatId;

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerConfig {
    /// Agent runs allowed at the same time, across all seats. Every run uses the same
    /// subscription, so this is the main lever on usage limits.
    #[serde(default = "default_concurrency")]
    pub max_concurrent: usize,
    /// A run still going after this long is stopped and the seat's turn is ended for it.
    #[serde(default = "default_timeout")]
    pub turn_timeout_seconds: u64,
    /// The agent for every seat not listed in `seats`, except the game master's. Lets one
    /// file, or the `--play-all` flag, fit any scenario.
    #[serde(default)]
    pub all: Option<AgentConfig>,
    #[serde(default)]
    pub seats: BTreeMap<SeatId, AgentConfig>,
}

impl Default for RunnerConfig {
    fn default() -> Self {
        Self {
            max_concurrent: default_concurrency(),
            turn_timeout_seconds: default_timeout(),
            all: None,
            seats: BTreeMap::new(),
        }
    }
}

fn default_concurrency() -> usize {
    2
}

fn default_timeout() -> u64 {
    240
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentConfig {
    /// Claude Code, run headless with `claude -p` on the user's own login.
    Claude(ClaudeConfig),
}

impl std::str::FromStr for AgentConfig {
    type Err = ConfigError;

    /// `claude` or `claude:<model>`, as the command line takes it.
    fn from_str(s: &str) -> Result<Self, ConfigError> {
        let (kind, model) = match s.split_once(':') {
            Some((k, m)) => (k, Some(m.to_owned())),
            None => (s, None),
        };
        match kind {
            "claude" => Ok(Self::Claude(ClaudeConfig {
                model: model.filter(|m| !m.is_empty()),
                effort: None,
                command: default_claude(),
            })),
            other => Err(ConfigError::Invalid(format!(
                "unknown agent `{other}`: expected `claude` or `claude:<model>`"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaudeConfig {
    /// A model alias such as `sonnet` or `haiku`, or a full model name. Default: the CLI's.
    #[serde(default)]
    pub model: Option<String>,
    /// `low`, `medium`, `high`, `xhigh` or `max`. Default: the CLI's.
    #[serde(default)]
    pub effort: Option<String>,
    /// The executable to launch.
    #[serde(default = "default_claude")]
    pub command: PathBuf,
}

fn default_claude() -> PathBuf {
    PathBuf::from("claude")
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("invalid agent config: {0}")]
    Yaml(#[source] serde_yaml_ng::Error),
    #[error("{0}")]
    Invalid(String),
}

impl RunnerConfig {
    pub fn from_yaml(src: &str) -> Result<Self, ConfigError> {
        let c: RunnerConfig = serde_yaml_ng::with::singleton_map_recursive::deserialize(
            serde_yaml_ng::Deserializer::from_str(src),
        )
        .map_err(ConfigError::Yaml)?;
        if c.max_concurrent == 0 {
            return Err(ConfigError::Invalid(
                "max_concurrent must be at least 1".into(),
            ));
        }
        if c.turn_timeout_seconds == 0 {
            return Err(ConfigError::Invalid(
                "turn_timeout_seconds must be at least 1".into(),
            ));
        }
        if c.seats.is_empty() && c.all.is_none() {
            return Err(ConfigError::Invalid(
                "no seats are assigned an agent: list `seats` or set `all`".into(),
            ));
        }
        Ok(c)
    }

    /// Give `all`'s agent to every seat in `seats` that has none yet. Run once the world's
    /// seats are known; the game master is never in `seats`.
    pub fn resolve<'a>(&mut self, seats: impl IntoIterator<Item = &'a SeatId>) {
        if let Some(all) = self.all.take() {
            for s in seats {
                self.seats.entry(s.clone()).or_insert_with(|| all.clone());
            }
        }
    }

    pub fn turn_timeout(&self) -> Duration {
        Duration::from_secs(self.turn_timeout_seconds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_provider_can_play_seats_of_several_empires() {
        let c = RunnerConfig::from_yaml(
            "seats:\n  princeps: { claude: { model: sonnet } }\n  parthia: { claude: {} }\n",
        )
        .unwrap();
        assert_eq!(c.max_concurrent, 2);
        assert_eq!(c.turn_timeout(), Duration::from_secs(240));
        let AgentConfig::Claude(rome) = &c.seats[&SeatId::from("princeps")];
        assert_eq!(rome.model.as_deref(), Some("sonnet"));
        let AgentConfig::Claude(parthia) = &c.seats[&SeatId::from("parthia")];
        assert_eq!(parthia.command, PathBuf::from("claude"));
    }

    #[test]
    fn one_agent_can_play_every_seat_of_any_scenario() {
        let mut c = RunnerConfig::from_yaml(
            "all: { claude: { model: haiku } }\nseats:\n  a: { claude: { model: opus } }\n",
        )
        .unwrap();
        let ids: Vec<SeatId> = ["a", "b", "c"].map(SeatId::from).into();
        c.resolve(&ids);
        assert_eq!(c.seats.len(), 3);
        let model = |s: &str| {
            let AgentConfig::Claude(a) = &c.seats[&SeatId::from(s)];
            a.model.clone()
        };
        assert_eq!(model("a").as_deref(), Some("opus"));
        assert_eq!(model("c").as_deref(), Some("haiku"));
    }

    #[test]
    fn an_agents_file_may_set_only_all() {
        let c = RunnerConfig::from_yaml("all: { claude: {} }\n").unwrap();
        assert!(c.seats.is_empty() && c.all.is_some());
    }

    #[test]
    fn the_command_line_names_an_agent_and_a_model() {
        let AgentConfig::Claude(c) = "claude:sonnet".parse().unwrap();
        assert_eq!(c.model.as_deref(), Some("sonnet"));
        let AgentConfig::Claude(c) = "claude".parse().unwrap();
        assert_eq!(c.model, None);
        assert!("gpt:4".parse::<AgentConfig>().is_err());
    }

    #[test]
    fn bad_configs_are_rejected() {
        for src in [
            "seats: {}",
            "max_concurrent: 0\nseats:\n  a: { claude: {} }",
            "turn_timeout_seconds: 0\nseats:\n  a: { claude: {} }",
            "seats:\n  a: { gpt: {} }",
            "seats:\n  a: { claude: { modle: x } }",
            "seatz:\n  a: { claude: {} }",
        ] {
            assert!(RunnerConfig::from_yaml(src).is_err(), "{src}");
        }
    }
}
