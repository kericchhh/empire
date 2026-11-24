//! Turns a request in plain words ("two rival megacorps on Mars") into a scenario, by asking
//! the user's own agent CLI to write one.
//!
//! The agent answers with JSON constrained to the scenario schema. The engine then checks it
//! the same way it checks a hand-written file, and a rejected draft goes back to a fresh run
//! with the reasons. The agent only writes the starting point: the ruleset, and so every
//! mechanic, stays the engine's, and a scenario still cannot add rules or endings.

use std::future;
use std::path::Path;
use std::time::Duration;

use serde_json::Value;
use sim_core::{Ruleset, Scenario, World};

use crate::claude;
use crate::config::ClaudeConfig;
use crate::sandbox::SandboxError;
use crate::turn::{Outcome, RunError, Summary, launch, sandbox};

/// A scenario an agent wrote and the engine accepted.
#[derive(Debug, Clone)]
pub struct Authored {
    /// The scenario as YAML, ready to save, edit and run.
    pub yaml: String,
    pub world: World,
    /// Runs it took, counting the accepted one.
    pub attempts: u32,
    /// The closing summary of each run, in order.
    pub runs: Vec<Option<Summary>>,
}

#[derive(Debug, thiserror::Error)]
pub enum AuthorError {
    #[error(transparent)]
    Run(#[from] RunError),
    #[error(transparent)]
    Sandbox(#[from] SandboxError),
    #[error("the agent run {0}")]
    Failed(String),
    #[error("{}", crate::claude::LOGIN_REFUSED)]
    LoginRefused,
    #[error("no acceptable scenario after {attempts} attempts; last problem: {last}")]
    Rejected { attempts: u32, last: String },
}

/// How to write one scenario.
#[derive(Debug, Clone)]
pub struct AuthorJob<'a> {
    pub agent: &'a ClaudeConfig,
    /// The token from `claude setup-token`.
    pub login: &'a str,
    /// An Empire executable that answers [`crate::bridge_if_asked`].
    pub bridge: &'a Path,
    /// What the user asked for.
    pub request: &'a str,
    /// Run output and working directories go here.
    pub log_dir: &'a Path,
    pub attempts: u32,
    pub timeout: Duration,
}

/// The scenario schema as the CLI's `--json-schema` takes it: without the `$schema` line,
/// which it refuses.
pub fn schema() -> Value {
    let mut s = serde_json::to_value(schemars::schema_for!(Scenario))
        .expect("the scenario schema is plain JSON");
    if let Some(o) = s.as_object_mut() {
        o.remove("$schema");
    }
    s
}

/// The instruction for one authoring run. It describes the offices from the ruleset itself,
/// so it never goes stale, and names no setting of its own.
pub fn prompt(rules: &Ruleset, request: &str, rejected: Option<(&str, &str)>) -> String {
    let offices: String = rules
        .offices
        .iter()
        .map(|o| format!("- `{}` ({}): {}\n", o.id, o.name, o.description))
        .collect();
    let mut p = format!(
        "You design the starting point of a world for Empire, a simulation where several \
         empires share one world and AI agents play every character. Write only the setting: \
         the mechanics are fixed by the engine, so a scenario has no rules, no victory \
         conditions and no endings.\n\n\
         The request:\n<request>\n{request}\n</request>\n\n\
         Return one scenario that matches the JSON schema you were given:\n\
         - At least two empires unless the request says otherwise, each with its own culture, \
         regions, characters and seats.\n\
         - Every character holds one of these offices, which decide what it may do:\n{offices}\
         - Give each empire exactly one `head_of_state`, and a `cohort` for its common people.\n\
         - A character's `region` is one of its own empire's regions. A seat controls \
         characters of its own empire, and every character belongs to exactly one seat.\n\
         - Ids are short lowercase words joined by `_`, unique across the whole scenario.\n\
         - Give characters personas, goals and secrets that pull against each other, inside \
         their empire and across empires.\n\
         - List in `contacts` each pair of empires that know each other at the start, as \
         the setting has it. An empire in no pair knows nobody until it meets someone.\n\
         - Leave out every `start` value unless the request gives numbers: the engine draws \
         them from the seed.\n\
         - Leave out `game_master` unless the request asks for one.\n"
    );
    if let Some((draft, problem)) = rejected {
        p.push_str(&format!(
            "\nA previous draft was rejected by the engine:\n<problem>\n{problem}\n</problem>\n\
             <draft>\n{draft}\n</draft>\n\
             Return a corrected scenario that keeps what was good about it.\n"
        ));
    }
    p
}

fn args(cfg: &ClaudeConfig) -> Vec<std::ffi::OsString> {
    let mut a = claude::sealed_args(cfg, "json");
    a.push("--json-schema".into());
    a.push(schema().to_string().into());
    a
}

/// What a run's output holds: the structured scenario, or why there is none. Only the last
/// readable line counts, since a log left by an earlier attempt is appended to.
fn draft(output: &str) -> Result<Value, String> {
    let v: Value = output
        .lines()
        .rev()
        .find_map(|l| serde_json::from_str(l).ok())
        .ok_or("wrote no readable result")?;
    if v["is_error"] == true {
        return Err(format!(
            "reported an error: {}",
            v["result"].as_str().unwrap_or("no detail")
        ));
    }
    match v.get("structured_output") {
        Some(s) if s.is_object() => Ok(s.clone()),
        _ => Err("returned no structured scenario".into()),
    }
}

/// The engine's verdict on a draft, with the YAML it would save.
pub fn check(draft: &Value) -> Result<(String, World), String> {
    let yaml = serde_yaml_ng::to_string(draft).map_err(|e| e.to_string())?;
    let scenario = Scenario::from_yaml(&yaml).map_err(|e| e.to_string())?;
    let world = World::standard(scenario).map_err(|e| e.to_string())?;
    if let Some(bad) = world
        .scenario
        .seat_ids()
        .find(|s| !crate::runner::seat_id_ok(s))
    {
        return Err(format!(
            "seat id `{bad}` may not contain `/` or `\\`, or be `.` or `..`"
        ));
    }
    Ok((yaml, world))
}

/// Write a scenario for `job.request`. Each attempt is a fresh run whose output lands in
/// `<log_dir>/author-<n>.json`; a draft the engine rejects, or a run that returns none, sends
/// the reason to the next attempt.
pub async fn author(job: &AuthorJob<'_>) -> Result<Authored, AuthorError> {
    let rules = Ruleset::standard();
    crate::sandbox::check()?;
    let mut rejected: Option<(String, String)> = None;
    let mut runs = Vec::new();
    let attempts = job.attempts.max(1);
    for attempt in 1..=attempts {
        let input = prompt(
            &rules,
            job.request,
            rejected.as_ref().map(|(d, p)| (d.as_str(), p.as_str())),
        );
        let log = job.log_dir.join(format!("author-{attempt}.json"));
        let mut run = sandbox(&claude::spec(job.agent, job.login, job.bridge, Vec::new()))?;
        let outcome = launch(
            &mut run,
            args(job.agent),
            &input,
            &log,
            job.timeout,
            future::pending(),
        )
        .await?;
        let output = tokio::fs::read_to_string(&log).await.unwrap_or_default();
        let summary = claude::summary(&output);
        if summary.as_ref().is_some_and(Summary::login_refused) {
            return Err(AuthorError::LoginRefused);
        }
        runs.push(summary);
        match outcome {
            Outcome::Exited(Some(0)) => {}
            Outcome::TimedOut => return Err(AuthorError::Failed("timed out".into())),
            other => {
                return Err(AuthorError::Failed(format!(
                    "ended with {other:?}; see {}",
                    log.with_extension("stderr.log").display()
                )));
            }
        }
        rejected = Some(match draft(&output) {
            Err(problem) => {
                tracing::info!(attempt, %problem, "author run returned no draft");
                let last = rejected.map(|(d, _)| d).unwrap_or_default();
                (last, format!("the run {problem}"))
            }
            Ok(d) => match check(&d) {
                Ok((yaml, world)) => {
                    return Ok(Authored {
                        yaml,
                        world,
                        attempts: attempt,
                        runs,
                    });
                }
                Err(problem) => {
                    tracing::info!(attempt, %problem, "draft scenario rejected");
                    (d.to_string(), problem)
                }
            },
        });
    }
    Err(AuthorError::Rejected {
        attempts,
        last: rejected.map(|(_, p)| p).unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TINY: &str = include_str!("../../sim-core/tests/fixtures/tiny.yaml");

    #[test]
    fn the_schema_has_no_schema_line_for_the_cli_to_refuse() {
        let s = schema();
        assert!(s.get("$schema").is_none());
        assert_eq!(s["type"], "object");
    }

    #[test]
    fn the_prompt_lists_every_office_and_repeats_a_rejection() {
        let rules = Ruleset::standard();
        let p = prompt(&rules, "pirates of the outer rim", None);
        for o in &rules.offices {
            assert!(p.contains(&format!("`{}`", o.id)), "{}", o.id);
        }
        assert!(p.contains("pirates of the outer rim") && !p.contains("<draft>"));
        assert!(p.contains("`contacts`"));
        let p = prompt(
            &rules,
            "x",
            Some(("{\"name\":\"a\"}", "unknown office `pope`")),
        );
        assert!(p.contains("unknown office `pope`") && p.contains("{\"name\":\"a\"}"));
    }

    #[test]
    fn a_valid_draft_becomes_yaml_the_engine_accepts() {
        let v: Value = serde_yaml_ng::from_str(TINY).unwrap();
        let (yaml, world) = check(&v).unwrap();
        assert_eq!(world.scenario, Scenario::from_yaml(&yaml).unwrap());
        assert!(world.scenario.empires.len() >= 2);
    }

    #[test]
    fn engine_problems_are_reported_for_the_next_attempt() {
        let mut v: Value = serde_yaml_ng::from_str(TINY).unwrap();
        v["empires"][0]["characters"][0]["office"] = "pope".into();
        let problem = check(&v).unwrap_err();
        assert!(problem.contains("pope"), "{problem}");
    }

    #[test]
    fn seat_ids_that_cannot_name_a_log_directory_are_sent_back() {
        let mut v: Value = serde_yaml_ng::from_str(TINY).unwrap();
        v["empires"][0]["seats"][0]["id"] = "crown/queen".into();
        assert!(check(&v).unwrap_err().contains("crown/queen"));
    }

    #[test]
    fn a_result_without_structured_output_is_not_a_draft() {
        assert!(draft("{\"type\":\"result\",\"result\":\"hi\"}").is_err());
        assert!(draft("{\"type\":\"result\",\"is_error\":true,\"result\":\"limit\"}").is_err());
        assert!(draft("not json").is_err());
        let after_a_cut_off_run = "{\"type\":\"res\n{\"structured_output\":{\"name\":\"x\"}}";
        assert_eq!(draft(after_a_cut_off_run).unwrap()["name"], "x");
        let ok = draft("{\"type\":\"result\",\"structured_output\":{\"name\":\"x\"}}").unwrap();
        assert_eq!(ok["name"], "x");
    }
}
