//! One headless Codex turn for one seat.
//!
//! Sealed like a Claude run (see [`crate::claude`]), with what differs for Codex:
//! - its command tool, other built-in tools, plugins, apps, web search and analytics are
//!   switched off. Codex does not let a user switch every command feature off, so its sandbox
//!   also holds no system programs: a command would have no shell or program to run. Its tool
//!   calls go through code mode, whose host ships beside it and reaches only these tools;
//! - no user config or rules (`--ignore-user-config`, `--ignore-rules`), nothing saved
//!   (`--ephemeral`, no history), and of the MCP servers only the game's, with the turn tools;
//! - the login is a copy of Empire's own Codex home's `auth.json` (see [`crate::local`]), carried in,
//!   and saved back when Codex refreshed it. A local model server needs no login at all.

use std::ffi::OsString;
use std::future::Future;
use std::path::Path;
use std::time::Duration;

use serde::Deserialize;

use crate::config::CodexConfig;
use crate::net::{Exits, loopback};
use crate::sandbox::Spec;
use crate::turn::{
    RunError, RunReport, SERVER, Summary, TURN_TOOLS, Turn, launch, prompt, sandbox,
};

/// What to tell the user when the provider refuses the saved login.
pub const LOGIN_REFUSED: &str = "Codex refused Empire's login (expired or revoked): run \
     `sim-server login codex` again, or reconnect Codex in the app";

/// Codex reaches only ChatGPT's Codex backend, and its login service to refresh the login.
const HOSTS: [&str; 2] = ["chatgpt.com:443", "auth.openai.com:443"];

/// Built-in features a turn has no use for.
const OFF: [&str; 18] = [
    "shell_tool",
    "unified_exec",
    "shell_snapshot",
    "multi_agent",
    "apps",
    "plugins",
    "remote_plugin",
    "hooks",
    "goals",
    "memories",
    "image_generation",
    "view_image",
    "browser_use",
    "computer_use",
    "in_app_browser",
    "tool_suggest",
    "skill_search",
    "skill_mcp_dependency_install",
];

/// The provider id a local model server goes by.
const LOCAL: &str = "empire_local";

/// A TOML value for `-c`: JSON strings and arrays of strings are valid TOML.
fn toml(v: impl serde::Serialize) -> String {
    serde_json::to_string(&v).expect("strings serialize")
}

pub fn args(cfg: &CodexConfig, mcp_url: &str) -> Vec<OsString> {
    let mut set: Vec<String> = OFF.iter().map(|f| format!("features.{f}=false")).collect();
    set.extend([
        "agents.enabled=false".into(),
        "analytics.enabled=false".into(),
        "feedback.enabled=false".into(),
        "check_for_update_on_startup=false".into(),
        "history.persistence=\"none\"".into(),
        "web_search=\"disabled\"".into(),
        format!("mcp_servers.{SERVER}.url={}", toml(mcp_url)),
        format!("mcp_servers.{SERVER}.enabled_tools={}", toml(TURN_TOOLS)),
        format!("mcp_servers.{SERVER}.required=true"),
    ]);
    if let Some(effort) = &cfg.effort {
        set.push(format!("model_reasoning_effort={}", toml(effort)));
    }
    if let Some(url) = &cfg.local {
        set.extend([
            format!("model_provider={}", toml(LOCAL)),
            format!("model_providers.{LOCAL}.name=\"local\""),
            format!("model_providers.{LOCAL}.base_url={}", toml(url)),
            format!("model_providers.{LOCAL}.wire_api=\"responses\""),
        ]);
    }
    let mut a: Vec<OsString> = [
        "exec",
        "-",
        "--json",
        "--ephemeral",
        "--skip-git-repo-check",
        "--ignore-user-config",
        "--ignore-rules",
        // Codex's own sandbox needs programs the run does not have; Empire's seals it.
        "--dangerously-bypass-approvals-and-sandbox",
    ]
    .map(OsString::from)
    .into();
    if let Some(m) = &cfg.model {
        a.extend(["-m".into(), m.into()]);
    }
    for s in set {
        a.extend(["-c".into(), s.into()]);
    }
    a
}

/// Run one turn in a fresh sandbox, logged in from Empire's Codex home `login` unless the
/// seat uses a local model server. Stdout (the event stream) goes to `log`, stderr next to
/// it. The run is killed at `timeout`, or as soon as `stop` completes.
pub async fn play(
    cfg: &CodexConfig,
    login: Option<&Path>,
    bridge: &Path,
    turn: &Turn<'_>,
    log: &Path,
    timeout: Duration,
    stop: impl Future<Output = ()>,
) -> Result<RunReport, RunError> {
    let mut tunnels = vec![loopback(turn.mcp_url)?];
    let hosts = match &cfg.local {
        Some(url) => {
            tunnels.push(loopback(url)?);
            Vec::new()
        }
        None => HOSTS.map(String::from).into(),
    };
    let saved = login.map(|home| home.join("auth.json"));
    let original = saved.as_deref().map(std::fs::read).transpose()?;
    let mut run = sandbox(&Spec {
        slot: "codex",
        program: cfg.command.clone(),
        helpers: &["codex-code-mode-host"],
        programs: false,
        env: Vec::new(),
        exits: Exits { hosts, tunnels },
        carry: original.clone().map(|o| (".codex/auth.json", o)),
        bridge: bridge.to_owned(),
    })?;
    let args = args(cfg, turn.mcp_url);
    let outcome = launch(&mut run, args, &prompt(turn), log, timeout, stop).await?;
    if let (Some(saved), Some(original)) = (&saved, &original)
        && let Some(new) = refreshed(original, run.carried(), &std::fs::read(saved)?)
    {
        let dir = saved.parent().expect("auth.json is in the Codex home");
        let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
        std::io::Write::write_all(&mut tmp, &new)?;
        tmp.persist(saved).map_err(|e| e.error)?;
    }
    let summary = summary(&tokio::fs::read_to_string(log).await.unwrap_or_default());
    Ok(RunReport { outcome, summary })
}

/// The login to save after a run that started from `original`: what the run left, if Codex
/// refreshed it into a well-formed login, and only while no other run has saved a newer one.
fn refreshed(original: &[u8], left: Option<Vec<u8>>, now: &[u8]) -> Option<Vec<u8>> {
    let left = left.filter(|l| l != original && now == original)?;
    let login: serde_json::Value = serde_json::from_slice(&left).ok()?;
    login["tokens"]["refresh_token"].is_string().then_some(left)
}

/// The run's summary from Codex's `--json` events: its turns, and whether it failed.
pub(crate) fn summary(stream: &str) -> Option<Summary> {
    #[derive(Deserialize)]
    struct Event {
        #[serde(rename = "type")]
        kind: String,
        #[serde(default)]
        message: Option<String>,
        #[serde(default)]
        error: Option<Failure>,
    }
    #[derive(Deserialize)]
    struct Failure {
        message: String,
    }
    let mut s: Option<Summary> = None;
    for e in stream
        .lines()
        .filter_map(|l| serde_json::from_str::<Event>(l).ok())
    {
        let s = s.get_or_insert(Summary {
            is_error: false,
            num_turns: 0,
            duration_ms: 0,
            total_cost_usd: 0.0,
            api_error_status: None,
        });
        let failure = match e.kind.as_str() {
            "turn.completed" => {
                s.num_turns += 1;
                None
            }
            "turn.failed" => e.error.map(|f| f.message),
            "error" => e.message,
            _ => None,
        };
        if let Some(m) = failure {
            s.is_error = true;
            // Codex reports a refused login only in words.
            if m.contains("sign in again") {
                s.api_error_status = Some(401);
            }
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> CodexConfig {
        CodexConfig {
            model: Some("gpt-5.5".into()),
            effort: Some("low".into()),
            command: "codex".into(),
            local: None,
        }
    }

    fn strings(a: Vec<OsString>) -> Vec<String> {
        a.into_iter().map(|s| s.into_string().unwrap()).collect()
    }

    #[test]
    fn a_run_gets_only_the_turn_tools_and_nothing_of_the_users() {
        let a = strings(args(&cfg(), "http://127.0.0.1:7878/mcp"));
        for flag in [
            "--ephemeral",
            "--ignore-user-config",
            "--ignore-rules",
            "--json",
        ] {
            assert!(a.contains(&flag.to_owned()), "{flag}");
        }
        let set: Vec<&str> = a
            .windows(2)
            .filter(|w| w[0] == "-c")
            .map(|w| w[1].as_str())
            .collect();
        for want in [
            "features.shell_tool=false",
            "web_search=\"disabled\"",
            "analytics.enabled=false",
            "mcp_servers.empire.url=\"http://127.0.0.1:7878/mcp\"",
            "model_reasoning_effort=\"low\"",
        ] {
            assert!(set.contains(&want), "{want}: {set:?}");
        }
        let tools = set
            .iter()
            .find_map(|s| s.strip_prefix("mcp_servers.empire.enabled_tools="))
            .unwrap();
        assert!(tools.contains("\"end_turn\"") && !tools.contains("wait_for_turn"));
        assert!(!set.iter().any(|s| s.starts_with("model_provider")));
    }

    #[test]
    fn a_local_server_replaces_the_login() {
        let local = CodexConfig {
            local: Some("http://127.0.0.1:11434/v1".into()),
            ..cfg()
        };
        let a = strings(args(&local, "http://127.0.0.1:7878/mcp"));
        assert!(a.contains(&format!("model_provider=\"{LOCAL}\"")));
        assert!(a.contains(&format!(
            "model_providers.{LOCAL}.base_url=\"http://127.0.0.1:11434/v1\""
        )));
    }

    #[test]
    fn only_a_fresh_well_formed_login_is_saved_back() {
        let v1 = br#"{"tokens":{"refresh_token":"one"}}"#;
        let v2 = br#"{"tokens":{"refresh_token":"two"}}"#.to_vec();
        assert_eq!(refreshed(v1, Some(v2.clone()), v1), Some(v2.clone()));
        assert_eq!(refreshed(v1, Some(v1.to_vec()), v1), None, "unchanged");
        assert_eq!(
            refreshed(v1, Some(v2.clone()), &v2),
            None,
            "another run saved one"
        );
        assert_eq!(
            refreshed(v1, Some(b"{\"tokens\":".to_vec()), v1),
            None,
            "cut short"
        );
        assert_eq!(refreshed(v1, None, v1), None);
    }

    #[test]
    fn the_summary_counts_turns_and_spots_a_refused_login() {
        let ok =
            "{\"type\":\"thread.started\"}\nnoise\n{\"type\":\"turn.completed\",\"usage\":{}}\n";
        let s = summary(ok).unwrap();
        assert_eq!((s.is_error, s.num_turns), (false, 1));
        let refused = "{\"type\":\"turn.failed\",\"error\":{\"message\":\"Your access token could \
             not be refreshed. Please log out and sign in again.\"}}";
        assert!(summary(refused).unwrap().login_refused());
        let other = "{\"type\":\"error\",\"message\":\"stream disconnected\"}";
        let s = summary(other).unwrap();
        assert!(s.is_error && !s.login_refused());
        assert_eq!(summary("nothing"), None);
    }
}
