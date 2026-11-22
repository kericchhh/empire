//! The author loop against a fake `claude` that returns prepared drafts in turn.

#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::Duration;

use serde_json::{Value, json};
use sim_runner::{AuthorError, AuthorJob, ClaudeConfig, author};

const TINY: &str = include_str!("../../sim-core/tests/fixtures/tiny.yaml");

/// A fake CLI that answers its n-th call with `drafts/<n>.json` as the structured output,
/// and saves its arguments and prompt.
fn fake(dir: &Path, drafts: &[Value]) -> ClaudeConfig {
    let d = dir.join("drafts");
    std::fs::create_dir_all(&d).unwrap();
    for (i, v) in drafts.iter().enumerate() {
        let out = json!({ "type": "result", "is_error": false, "num_turns": 1,
                          "structured_output": v });
        std::fs::write(d.join(format!("{}.json", i + 1)), out.to_string()).unwrap();
    }
    let path = dir.join("claude");
    std::fs::write(
        &path,
        format!(
            "#!/bin/sh\n\
             d='{d}'\n\
             n=$(($(ls \"$d\" | grep -c '^call.*args$') + 1))\n\
             printf '%s\\n' \"$@\" > \"$d/call$n.args\"\n\
             cat > \"$d/call$n.stdin\"\n\
             cat \"$d/$n.json\"\n",
            d = d.display()
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

fn tiny() -> Value {
    serde_yaml_ng::from_str(TINY).unwrap()
}

fn job<'a>(agent: &'a ClaudeConfig, dir: &'a Path) -> AuthorJob<'a> {
    AuthorJob {
        agent,
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
    let agent = fake(dir.path(), &[bad, tiny()]);
    let a = author(&job(&agent, dir.path())).await.unwrap();
    assert_eq!(a.attempts, 2);
    assert_eq!(a.world.scenario.empires.len(), 2);
    assert!(a.yaml.contains("pirate_king"));

    let d = dir.path().join("drafts");
    let first = std::fs::read_to_string(d.join("call1.stdin")).unwrap();
    assert!(first.contains("a kingdom and some pirate isles"));
    assert!(!first.contains("<problem>"));
    let second = std::fs::read_to_string(d.join("call2.stdin")).unwrap();
    assert!(
        second.contains("<problem>") && second.contains("pope"),
        "{second}"
    );

    // The author gets no tools and no MCP server: only the schema to answer with.
    let args = std::fs::read_to_string(d.join("call1.args")).unwrap();
    let a: Vec<&str> = args.lines().collect();
    let after = |f: &str| a[a.iter().position(|x| *x == f).unwrap() + 1];
    assert_eq!(after("--tools"), "");
    assert_eq!(after("--output-format"), "json");
    assert!(a.contains(&"--strict-mcp-config") && !a.contains(&"--mcp-config"));
    let schema: Value = serde_json::from_str(after("--json-schema")).unwrap();
    assert!(schema.get("$schema").is_none());
}

#[tokio::test]
async fn it_gives_up_after_the_last_attempt() {
    let dir = tempfile::tempdir().unwrap();
    let mut bad = tiny();
    bad["empires"] = json!([]);
    let agent = fake(dir.path(), &[bad.clone(), bad.clone(), bad]);
    match author(&job(&agent, dir.path())).await {
        Err(AuthorError::Rejected { attempts: 3, last }) => assert!(!last.is_empty()),
        other => panic!("{other:?}"),
    }
}
