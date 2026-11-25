//! Plays seats with the agent CLI the user assigned to each, a fresh headless run per turn.
//!
//! This crate never calls a model and holds no keys: it launches the user's own CLI, which
//! plays through the world's MCP tools like any external agent. One provider can play any
//! number of seats, in one empire or in rival ones, because every run is isolated and knows
//! only its own seat's token, and nothing of the user's machine.

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
compile_error!("Empire runs on Linux and macOS only.");

pub mod author;
pub mod claude;
pub mod codex;
pub mod config;
mod fs;
mod local;
mod net;
pub mod runner;
mod sandbox;
mod stage;
pub mod turn;

pub use author::{AuthorError, AuthorJob, Authored, author};
pub use config::{AgentConfig, ClaudeConfig, CodexConfig, ConfigError, RunnerConfig};
pub use local::{
    LocalError, claude_token, codex_login, codex_login_command, logins, save_claude_token,
    world_db, worlds_dir,
};
pub use net::bridge_if_asked;
pub use runner::{Plan, PlanError, run};
pub use sandbox::{SandboxError, check as check_sandbox};
