//! The runner against a real world host, with a fake `claude` that reports how it was run.
//! Runs are sandboxed away from the test's files, so the fake reports on its stdout, which
//! the runner keeps as the run's log.

use std::collections::BTreeSet;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use sim_core::{SeatId, Tick};
use sim_runner::{Plan, RunnerConfig};
use sim_server::game::Game;

mod common;

/// Seats of both empires in the tiny world, all played by one provider.
const SEATS: [&str; 4] = ["crown", "rival", "folk", "pirates"];
const LOGIN: &str = "sk-ant-oat01-test-login";

struct Setup {
    game: Game,
    login_refused: Arc<AtomicBool>,
    tokens: std::collections::BTreeMap<SeatId, String>,
    dir: tempfile::TempDir,
}

/// One run as the fake reported it.
#[derive(Debug, Default)]
struct Run {
    seat: String,
    args: Vec<String>,
    stdin: String,
    cwd: String,
    env: Vec<String>,
    net: Vec<String>,
    start: u128,
    end: Option<u128>,
}

impl Setup {
    fn runs_dir(&self) -> PathBuf {
        self.dir.path().join("runs")
    }

    async fn tick(&self) -> Tick {
        self.game.overview().await.state.tick
    }

    async fn wait_for_tick(&self, t: u64) {
        let until = tokio::time::Instant::now() + Duration::from_secs(20);
        while self.tick().await < Tick(t) {
            assert!(tokio::time::Instant::now() < until, "tick {t} never came");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    /// Every run logged so far, with the log file it is in.
    fn runs(&self) -> Vec<(PathBuf, Run)> {
        let mut out = Vec::new();
        for seat in SEATS {
            let Ok(logs) = std::fs::read_dir(self.runs_dir().join(seat)) else {
                continue;
            };
            for log in logs.map(|e| e.unwrap().path()) {
                if log.extension().is_none_or(|x| x != "jsonl") {
                    continue;
                }
                for run in parse(seat, &std::fs::read_to_string(&log).unwrap()) {
                    out.push((log.clone(), run));
                }
            }
        }
        out
    }
}

fn parse(seat: &str, log: &str) -> Vec<Run> {
    let mut runs: Vec<Run> = Vec::new();
    for line in log.lines() {
        if line == "begin" {
            runs.push(Run {
                seat: seat.into(),
                ..Run::default()
            });
            continue;
        }
        let (Some(r), Some((k, v))) = (runs.last_mut(), line.split_once('=')) else {
            continue;
        };
        match k {
            "arg" => r.args.push(v.into()),
            "stdin" => r.stdin.push_str(&format!("{v}\n")),
            "cwd" => r.cwd = v.into(),
            "env" => r.env.push(v.into()),
            "net" => r.net.push(v.into()),
            "start" => r.start = v.parse().unwrap(),
            "end" => r.end = Some(v.parse().unwrap()),
            _ => {}
        }
    }
    runs
}

/// A fake agent CLI that reports its arguments, prompt, directory and environment, runs
/// `body`, and then writes a closing summary.
fn fake_claude(dir: &Path, body: &str) -> PathBuf {
    let path = dir.join("claude");
    let script = format!(
        "#!/bin/sh\n\
         echo begin\n\
         echo \"start=$(date +%s%N)\"\n\
         for a in \"$@\"; do printf 'arg=%s\\n' \"$a\"; done\n\
         sed 's/^/stdin=/'; echo\n\
         echo \"cwd=$(pwd)\"\n\
         env | sed 's/^/env=/'\n\
         {body}\n\
         echo \"end=$(date +%s%N)\"\n\
         echo '{{\"type\":\"result\",\"is_error\":false,\"num_turns\":1,\"duration_ms\":1,\"total_cost_usd\":0}}'\n",
    );
    std::fs::write(&path, script).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

async fn start(body: &str, extra: &str) -> Setup {
    start_with_deadline(body, extra, Duration::from_secs(3600)).await
}

async fn start_with_deadline(body: &str, extra: &str, deadline: Duration) -> Setup {
    let dir = tempfile::tempdir().unwrap();
    let claude = fake_claude(dir.path(), body);
    let (created, addr) = common::serve(deadline).await;
    tokio::spawn(created.game.clone().run_scheduler());

    let seats: String = SEATS
        .iter()
        .map(|s| {
            format!(
                "  {s}: {{ claude: {{ model: haiku, command: '{}' }} }}\n",
                claude.display()
            )
        })
        .collect();
    let config = RunnerConfig::from_yaml(&format!("{extra}seats:\n{seats}")).unwrap();
    let plan = Plan {
        tokens: created.seat_tokens.clone(),
        config,
        mcp_url: format!("http://{addr}/mcp"),
        log_dir: dir.path().join("runs"),
        claude_token: Some(LOGIN.into()),
        login_refused: Arc::default(),
        bridge: env!("CARGO_BIN_EXE_sim-server").into(),
    };
    let login_refused = plan.login_refused.clone();
    tokio::spawn(sim_runner::run(plan));
    Setup {
        game: created.game,
        login_refused,
        tokens: created.seat_tokens,
        dir,
    }
}

#[tokio::test]
async fn one_provider_plays_seats_of_rival_empires_and_time_moves_on() {
    let s = start("", "max_concurrent: 4\n").await;
    s.wait_for_tick(2).await;

    // Runs of the next tick may still be starting.
    let runs: Vec<_> = s
        .runs()
        .into_iter()
        .filter(|(_, r)| r.end.is_some())
        .collect();
    assert!(runs.len() >= 2 * SEATS.len(), "{} runs", runs.len());
    // Each run is told only its own seat token.
    let mut seen = BTreeSet::new();
    for (_, r) in &runs {
        let held: Vec<&SeatId> = s
            .tokens
            .iter()
            .filter(|(_, t)| r.stdin.contains(t.as_str()))
            .map(|(id, _)| id)
            .collect();
        assert_eq!(held, [&SeatId::from(r.seat.as_str())], "{r:?}");
        seen.insert(r.seat.clone());

        let after = |f: &str| r.args[r.args.iter().position(|x| x == f).unwrap() + 1].as_str();
        assert_eq!(after("--tools"), "");
        assert!(!after("--allowedTools").contains("wait_for_turn"));
        assert_eq!(after("--model"), "haiku");
        assert!(r.args.iter().any(|a| a == "--strict-mcp-config"));
        // Tokens never appear on a command line, where other processes could read them.
        assert!(
            !s.tokens
                .values()
                .any(|t| r.args.iter().any(|a| a.contains(t)))
        );
    }
    assert_eq!(seen, SEATS.iter().map(|s| s.to_string()).collect());

    for seat in SEATS {
        let tick0 = s.runs_dir().join(seat).join("tick-0.jsonl");
        let log = std::fs::read_to_string(&tick0).unwrap();
        assert!(log.contains("\"type\":\"result\""), "{seat}: {log}");
    }
}

#[tokio::test]
async fn a_run_that_overstays_is_stopped_and_its_turn_ended() {
    let s = start("sleep 30", "turn_timeout_seconds: 1\nmax_concurrent: 4\n").await;
    let started = tokio::time::Instant::now();
    s.wait_for_tick(1).await;
    assert!(started.elapsed() < Duration::from_secs(10));
    for seat in SEATS {
        // Killed before it could write its closing summary.
        let log = std::fs::read_to_string(s.runs_dir().join(seat).join("tick-0.jsonl")).unwrap();
        assert!(!log.contains("\"type\":\"result\""), "{seat}: {log}");
    }
}

#[tokio::test]
async fn runs_never_exceed_the_concurrency_cap() {
    let s = start("sleep 0.2", "max_concurrent: 1\n").await;
    s.wait_for_tick(1).await;
    let mut spans: Vec<(u128, u128)> = s
        .runs()
        .into_iter()
        .filter_map(|(_, r)| Some((r.start, r.end?)))
        .collect();
    spans.sort();
    assert!(spans.len() >= SEATS.len(), "{spans:?}");
    assert!(spans.windows(2).all(|w| w[0].1 <= w[1].0), "{spans:?}");
}

#[tokio::test]
async fn a_tick_that_closes_on_its_deadline_stops_its_runs_and_none_is_played_twice() {
    let s = start_with_deadline(
        "sleep 30",
        "turn_timeout_seconds: 60\nmax_concurrent: 4\n",
        Duration::from_secs(2),
    )
    .await;
    s.wait_for_tick(4).await;
    // A tick's runs share its log, so a seat played twice in one tick shows up there.
    let mut played = BTreeSet::new();
    for (log, _) in s.runs() {
        assert!(
            played.insert(log.clone()),
            "played twice: {}",
            log.display()
        );
    }
    // Every seat played, and each run was stopped well before its 60s timeout.
    for seat in SEATS {
        assert!(
            played.contains(&s.runs_dir().join(seat).join("tick-0.jsonl")),
            "{seat}"
        );
    }
}

#[tokio::test]
async fn runs_are_sandboxed_with_only_the_login_and_private_logs() {
    let s = start("", "max_concurrent: 4\n").await;
    s.wait_for_tick(1).await;
    for (_, r) in s.runs().into_iter().filter(|(_, r)| r.end.is_some()) {
        assert_eq!(r.cwd, "/work", "{r:?}");
        assert!(!r.env.iter().any(|e| e.contains("CARGO")), "{:?}", r.env);
        for want in [
            "HOME=/home/agent",
            &format!("CLAUDE_CODE_OAUTH_TOKEN={LOGIN}"),
            "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1",
        ] {
            assert!(r.env.iter().any(|e| e == want), "{want}: {:?}", r.env);
        }
    }
    let mode = std::fs::metadata(s.runs_dir())
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o077, 0, "{mode:o}");
    let log = s.runs_dir().join("crown").join("tick-0.jsonl");
    let mode = std::fs::metadata(log).unwrap().permissions().mode();
    assert_eq!(mode & 0o077, 0, "{mode:o}");
}

/// Reports what a run can connect to: the world, the internet directly, and through the gate's
/// proxy a host its provider does not need.
const PROBE: &str = r#"python3 -c '
import json, os, socket, sys, urllib.parse
a = sys.argv[1:]
world = urllib.parse.urlparse(json.loads(a[a.index("--mcp-config") + 1])["mcpServers"]["empire"]["url"])
proxy = urllib.parse.urlparse(os.environ["HTTPS_PROXY"])
def probe(name, host, port, send=None):
    try:
        s = socket.create_connection((host, port), timeout=3)
        r = s.sendall(send) or s.recv(64).decode().split("\r\n")[0] if send else "open"
    except OSError:
        r = "unreachable"
    print("net=%s %s" % (name, r))
probe("world", world.hostname, world.port)
probe("internet", "1.1.1.1", 443)
probe("other-host", proxy.hostname, proxy.port, b"CONNECT example.com:443 HTTP/1.1\r\n\r\n")
' "$@""#;

#[tokio::test]
async fn a_run_reaches_the_world_and_its_provider_only() {
    let s = start(PROBE, "max_concurrent: 4\n").await;
    s.wait_for_tick(1).await;
    let (_, r) = s.runs().into_iter().find(|(_, r)| r.end.is_some()).unwrap();
    assert_eq!(
        r.net,
        [
            "world open",
            "internet unreachable",
            "other-host HTTP/1.1 403 Forbidden"
        ],
        "{r:?}"
    );
}

#[tokio::test]
async fn a_refused_login_raises_the_alert_until_a_run_succeeds() {
    let refused = "echo '{\"type\":\"result\",\"is_error\":true,\"api_error_status\":401}'; exit 0";
    let s = start(refused, "max_concurrent: 4\n").await;
    s.wait_for_tick(1).await;
    assert!(s.login_refused.load(Ordering::Relaxed));
}
