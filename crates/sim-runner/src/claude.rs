//! One headless Claude Code turn for one seat.
//!
//! Several seats of rival empires may run on the same login, so every run is sealed off from
//! the machine and from the other seats:
//! - an OS sandbox ([`crate::sandbox`]) with an empty home and working directory, so the run
//!   sees none of the user's files, settings, history or git repositories;
//! - no built-in tools (`--tools ""`), and of the MCP servers only the game's
//!   (`--strict-mcp-config`), with only the tools of one turn: no `join` or `wait_for_turn`,
//!   so a run cannot drift into the next tick;
//! - no settings files, slash commands or saved session (`--restricted`,
//!   `--disable-slash-commands`, `--no-session-persistence`);
//! - the login from `claude setup-token`, and no non-essential traffic, which also keeps
//!   Claude Code from fetching the account's profile, so the user's name and email never
//!   reach the model;
//! - the seat token on stdin, never on the command line where other processes could read it.

use std::ffi::OsString;
use std::future::Future;
use std::net::SocketAddr;
use std::path::Path;
use std::time::Duration;

use serde::Deserialize;

use crate::config::ClaudeConfig;
use crate::net::{Exits, loopback};
use crate::sandbox::Spec;
use crate::turn::{
    RUNNER_TOOLS, RunError, RunReport, SERVER, Summary, TURN_TOOLS, Turn, launch, prompt, sandbox,
    tools,
};

/// What to tell the user when the provider refuses the saved login.
pub const LOGIN_REFUSED: &str = "Claude Code refused Empire's login (expired or revoked): run \
     `claude setup-token` again and save the new token";

pub fn args(cfg: &ClaudeConfig, mcp_url: &str) -> Vec<OsString> {
    let mcp = serde_json::json!({ "mcpServers": { SERVER: { "type": "http", "url": mcp_url } } });
    let mut a = sealed_args(cfg, "stream-json");
    a.push("--verbose".into());
    a.push("--allowedTools".into());
    a.push(tools(&TURN_TOOLS).into());
    a.push("--disallowedTools".into());
    a.push(tools(&RUNNER_TOOLS).into());
    a.push("--mcp-config".into());
    a.push(mcp.to_string().into());
    a
}

/// Claude Code reaches only its API: non-essential traffic is off.
const HOSTS: [&str; 1] = ["api.anthropic.com:443"];

/// A sandboxed Claude Code logged in with `login`, that may also reach `tunnels`.
pub(crate) fn spec(
    cfg: &ClaudeConfig,
    login: &str,
    bridge: &Path,
    tunnels: Vec<SocketAddr>,
) -> Spec {
    Spec {
        slot: "claude",
        program: cfg.command.clone(),
        helpers: &[],
        programs: true,
        env: vec![
            ("CLAUDE_CODE_OAUTH_TOKEN", login.into()),
            ("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "1".into()),
        ],
        exits: Exits {
            hosts: HOSTS.map(String::from).into(),
            tunnels,
        },
        carry: None,
        bridge: bridge.to_owned(),
    }
}

/// Run one turn in a fresh sandbox. Stdout (the event stream) goes to `log`,
/// stderr next to it. The run is killed at `timeout`, or as soon as `stop` completes.
pub async fn play(
    cfg: &ClaudeConfig,
    login: &str,
    bridge: &Path,
    turn: &Turn<'_>,
    log: &Path,
    timeout: Duration,
    stop: impl Future<Output = ()>,
) -> Result<RunReport, RunError> {
    let mut run = sandbox(&spec(cfg, login, bridge, vec![loopback(turn.mcp_url)?]))?;
    let args = args(cfg, turn.mcp_url);
    let outcome = launch(&mut run, args, &prompt(turn), log, timeout, stop).await?;
    let summary = summary(&tokio::fs::read_to_string(log).await.unwrap_or_default());
    Ok(RunReport { outcome, summary })
}

/// The arguments every run shares, whatever it is for: no built-in tools, no MCP servers
/// beyond those a caller adds, no settings files and no saved session.
pub(crate) fn sealed_args(cfg: &ClaudeConfig, output_format: &str) -> Vec<OsString> {
    let mut a: Vec<OsString> = [
        "-p",
        "--output-format",
        output_format,
        "--tools",
        "",
        "--strict-mcp-config",
        "--permission-mode",
        "dontAsk",
        "--restricted",
        "--disable-slash-commands",
        "--no-session-persistence",
    ]
    .into_iter()
    .map(OsString::from)
    .collect();
    for (flag, value) in [("--model", &cfg.model), ("--effort", &cfg.effort)] {
        if let Some(v) = value {
            a.extend([flag.into(), v.into()]);
        }
    }
    a
}

pub(crate) fn summary(stream: &str) -> Option<Summary> {
    #[derive(Deserialize)]
    struct Line {
        #[serde(rename = "type")]
        kind: String,
        #[serde(flatten)]
        rest: serde_json::Value,
    }
    stream
        .lines()
        .rev()
        .filter_map(|l| serde_json::from_str::<Line>(l).ok())
        .find(|l| l.kind == "result")
        .and_then(|l| serde_json::from_value(l.rest).ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim_core::{SeatId, Tick};

    fn cfg() -> ClaudeConfig {
        ClaudeConfig {
            model: Some("haiku".into()),
            effort: None,
            command: "claude".into(),
        }
    }

    fn turn(seat: &SeatId) -> Turn<'_> {
        Turn {
            seat,
            token: "secret-token",
            tick: Tick(7),
            game_master: false,
            mcp_url: "http://127.0.0.1:7878/mcp",
        }
    }

    #[test]
    fn a_run_gets_only_the_turn_tools_of_the_game_server() {
        let a: Vec<String> = args(&cfg(), "http://127.0.0.1:7878/mcp")
            .into_iter()
            .map(|s| s.into_string().unwrap())
            .collect();
        let after = |flag: &str| a[a.iter().position(|x| x == flag).unwrap() + 1].clone();
        assert_eq!(after("--tools"), "");
        let allowed = after("--allowedTools");
        assert!(allowed.contains("mcp__empire__act") && allowed.contains("mcp__empire__end_turn"));
        assert!(!allowed.contains("wait_for_turn") && !allowed.contains("join"));
        assert_eq!(
            after("--disallowedTools"),
            "mcp__empire__join,mcp__empire__wait_for_turn"
        );
        assert_eq!(after("--permission-mode"), "dontAsk");
        assert_eq!(after("--model"), "haiku");
        for flag in [
            "-p",
            "--strict-mcp-config",
            "--restricted",
            "--no-session-persistence",
        ] {
            assert!(a.iter().any(|x| x == flag), "{flag}");
        }
        let mcp: serde_json::Value = serde_json::from_str(&after("--mcp-config")).unwrap();
        assert_eq!(
            mcp["mcpServers"]["empire"]["url"],
            "http://127.0.0.1:7878/mcp"
        );
        assert_eq!(mcp["mcpServers"].as_object().unwrap().len(), 1);
    }

    #[test]
    fn the_token_travels_in_the_prompt_never_on_the_command_line() {
        let a = args(&cfg(), "http://x/mcp");
        assert!(!a.iter().any(|x| x.to_string_lossy().contains("secret")));
        let seat = SeatId::from("princeps");
        let p = prompt(&turn(&seat));
        assert!(p.contains("secret-token") && p.contains("tick 7"));
        assert!(!p.contains("pending_rulings"));
        let gm = prompt(&Turn {
            game_master: true,
            ..turn(&seat)
        });
        assert!(gm.contains("pending_rulings"));
    }

    #[test]
    fn the_closing_summary_is_read_from_the_stream() {
        let stream = "{\"type\":\"system\"}\nnot json\n{\"type\":\"result\",\"is_error\":false,\"num_turns\":4,\"duration_ms\":900,\"total_cost_usd\":0.01}\n";
        assert_eq!(
            summary(stream),
            Some(Summary {
                is_error: false,
                num_turns: 4,
                duration_ms: 900,
                total_cost_usd: 0.01,
                api_error_status: None,
            })
        );
        let refused = "{\"type\":\"result\",\"is_error\":true,\"api_error_status\":401}";
        assert!(summary(refused).unwrap().login_refused());
        assert_eq!(summary("{\"type\":\"assistant\"}"), None);
    }
}
