//! The author loop against a fake `claude` that returns prepared drafts in turn.

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::Duration;

use serde_json::{Value, json};
use sim_runner::{AuthorError, AuthorJob, ClaudeConfig, author};

const TINY: &str = include_str!("../../sim-core/tests/fixtures/tiny.yaml");

/// A fake CLI that answers with `first`, or with `retry` once the prompt carries a rejection.
/// Runs are sandboxed, so it reports its arguments and prompt on stderr, which the author
/// loop logs.
fn fake(dir: &Path, first: &Value, retry: &Value) -> ClaudeConfig {
    let out = |v: &Value| json!({ "type": "result", "is_error": false, "num_turns": 1, "structured_output": v });
    let path = dir.join("claude");
    std::fs::write(
        &path,
        format!(
            "#!/bin/sh\n\
             input=$(cat)\n\
             printf '%s\\n' \"$@\" '--stdin--' \"$input\" >&2\n\
             case \"$input\" in *'<problem>'*) echo '{retry}' ;; *) echo '{first}' ;; esac\n",
            first = out(first),
            retry = out(retry),
        ),
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    ClaudeConfig {
        model: Some("haiku".into()),
        effort: None,
        command: path,
    }
}

/// What the fake recorded on its n-th call: its arguments, and its prompt.
fn call(dir: &Path, n: u32) -> (Vec<String>, String) {
    let log = std::fs::read_to_string(dir.join(format!("author-{n}.stderr.log"))).unwrap();
    let (args, stdin) = log.split_once("--stdin--\n").unwrap();
    (args.lines().map(str::to_owned).collect(), stdin.to_owned())
}

fn tiny() -> Value {
    serde_yaml_ng::from_str(TINY).unwrap()
}

fn job<'a>(agent: &'a ClaudeConfig, dir: &'a Path) -> AuthorJob<'a> {
    AuthorJob {
        agent,
        login: "sk-ant-oat01-test",
        request: "a kingdom and some pirate isles",
        log_dir: dir,
        attempts: 3,
        timeout: Duration::from_secs(20),
    }
}

#[tokio::test]
async fn a_rejected_draft_goes_back_with_the_reason_and_the_fix_is_accepted() {
    let dir = tempfile::tempdir().unwrap();
    let mut bad = tiny();
    bad["empires"][0]["characters"][0]["office"] = "pope".into();
    let agent = fake(dir.path(), &bad, &tiny());
    let a = author(&job(&agent, dir.path())).await.unwrap();
    assert_eq!(a.attempts, 2);
    assert_eq!(a.world.scenario.empires.len(), 2);
    assert!(a.yaml.contains("pirate_king"));

    let (a, first) = call(dir.path(), 1);
    assert!(first.contains("a kingdom and some pirate isles"));
    assert!(!first.contains("<problem>"));
    let (_, second) = call(dir.path(), 2);
    assert!(
        second.contains("<problem>") && second.contains("pope"),
        "{second}"
    );

    // The author gets no tools and no MCP server: only the schema to answer with.
    let after = |f: &str| a[a.iter().position(|x| x == f).unwrap() + 1].as_str();
    assert_eq!(after("--tools"), "");
    assert_eq!(after("--output-format"), "json");
    assert!(a.iter().any(|x| x == "--strict-mcp-config") && !a.iter().any(|x| x == "--mcp-config"));
    let schema: Value = serde_json::from_str(after("--json-schema")).unwrap();
    assert!(schema.get("$schema").is_none());
}

#[tokio::test]
async fn it_gives_up_after_the_last_attempt() {
    let dir = tempfile::tempdir().unwrap();
    let mut bad = tiny();
    bad["empires"] = json!([]);
    let agent = fake(dir.path(), &bad, &bad);
    match author(&job(&agent, dir.path())).await {
        Err(AuthorError::Rejected { attempts: 3, last }) => assert!(!last.is_empty()),
        other => panic!("{other:?}"),
    }
}
