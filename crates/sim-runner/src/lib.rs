//! Plays seats with the agent CLI the user assigned to each, a fresh headless run per turn.
//!
//! This crate never calls a model and holds no keys: it launches the user's own CLI, which
//! plays through the world's MCP tools like any external agent. One provider can play any
//! number of seats, in one empire or in rival ones, because every run is isolated and knows
//! only its own seat's token.

pub mod author;
pub mod claude;
pub mod config;
mod fs;
pub mod runner;

pub use author::{AuthorError, AuthorJob, Authored, author};
pub use config::{AgentConfig, ClaudeConfig, ConfigError, RunnerConfig};
pub use runner::{Plan, PlanError, run};
