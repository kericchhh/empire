//! The runner against a real world host, with a fake `claude` that records how it was run.

#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use std::collections::{BTreeMap, BTreeSet};

use sim_core::{SeatId, Tick};
use sim_runner::{Plan, RunnerConfig};
use sim_server::game::{Config, Game};
use sim_server::http::{HttpConfig, router};
use sim_server::store::Store;

const TINY: &str = include_str!("../../sim-core/tests/fixtures/tiny.yaml");

/// Seats of both empires in the tiny world, all played by one provider.
const SEATS: [&str; 4] = ["crown", "rival", "folk", "pirates"];

struct Setup {
    game: Game,
    tokens: BTreeMap<SeatId, String>,
    dir: tempfile::TempDir,
}

impl Setup {
    fn calls(&self) -> PathBuf {
        self.dir.path().join("calls")
    }
    fn runs(&self) -> PathBuf {
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
}

/// A fake agent CLI: `body` runs after it saves its arguments and stdin under `calls/`.
fn fake_claude(dir: &Path, body: &str) -> PathBuf {
    let calls = dir.join("calls");
    std::fs::create_dir_all(&calls).unwrap();
    let path = dir.join("claude");
    let script = format!(
        "#!/bin/sh\n\
         id=$$\n\
         save() {{ cat > '{calls}'/$id.$1.tmp && mv '{calls}'/$id.$1.tmp '{calls}'/$id.$1; }}\n\
         printf '%s\\n' \"$@\" | save args\n\
         save stdin\n\
         pwd | save cwd\n\
         env | save env\n\
         {body}\n\
         echo '{{\"type\":\"result\",\"is_error\":false,\"num_turns\":1,\"duration_ms\":1,\"total_cost_usd\":0}}'\n",
        calls = calls.display(),
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
    let store = Store::in_memory().await.unwrap();
    let config = Config {
        tick_deadline: deadline,
        autoplay: false,
    };
    let created = Game::open(store, Some(TINY), 7, config).await.unwrap();
    tokio::spawn(created.game.clone().run_scheduler());
    let app = router(
        created.game.clone(),
        HttpConfig {
            admin_token: created.admin_token.clone(),
            allowed_hosts: vec![],
            max_wait: Duration::from_millis(200),
        },
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await });

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
    };
    tokio::spawn(sim_runner::run(plan));
    Setup {
        game: created.game,
        tokens: created.seat_tokens,
        dir,
    }
}

fn read(dir: &Path, ext: &str) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == ext))
        .map(|p| std::fs::read_to_string(p).unwrap())
        .collect();
    out.sort();
    out
}

#[tokio::test]
async fn one_provider_plays_seats_of_rival_empires_and_time_moves_on() {
    let s = start("", "max_concurrent: 4\n").await;
    s.wait_for_tick(2).await;

    let prompts = read(&s.calls(), "stdin");
    assert!(prompts.len() >= 2 * SEATS.len(), "{} runs", prompts.len());
    // Each run is told exactly one seat's token, and every agent seat got runs.
    let mut seen = BTreeSet::new();
    for p in &prompts {
        let held: Vec<&SeatId> = s
            .tokens
            .iter()
            .filter(|(_, t)| p.contains(t.as_str()))
            .map(|(id, _)| id)
            .collect();
        assert_eq!(held.len(), 1, "{p}");
        seen.insert(held[0].as_str().to_owned());
    }
    assert_eq!(seen, SEATS.iter().map(|s| s.to_string()).collect());
    for args in read(&s.calls(), "args") {
        let a: Vec<&str> = args.lines().collect();
        let after = |f: &str| a[a.iter().position(|x| *x == f).unwrap() + 1];
        assert_eq!(after("--tools"), "");
        assert!(!after("--allowedTools").contains("wait_for_turn"));
        assert_eq!(after("--model"), "haiku");
        assert!(a.contains(&"--strict-mcp-config"));
    }
    // Tokens never appear on a command line, where other processes could read them.
    for args in read(&s.calls(), "args") {
        assert!(!s.tokens.values().any(|t| args.contains(t.as_str())));
    }
    // Every run starts in its own empty directory.
    let cwds = read(&s.calls(), "cwd");
    let unique: BTreeSet<_> = cwds.iter().collect();
    assert_eq!(unique.len(), cwds.len());

    for seat in SEATS {
        let tick0 = s.runs().join(seat).join("tick-0.jsonl");
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
        let log = std::fs::read_to_string(s.runs().join(seat).join("tick-0.jsonl")).unwrap();
        assert!(!log.contains("\"type\":\"result\""), "{seat}: {log}");
    }
}

#[tokio::test]
async fn runs_never_exceed_the_concurrency_cap() {
    let s = start(
        "mkdir \"$0.lock\" 2>/dev/null || touch \"$0.overlap\"\nsleep 0.2\nrmdir \"$0.lock\"",
        "max_concurrent: 1\n",
    )
    .await;
    s.wait_for_tick(1).await;
    assert!(!s.dir.path().join("claude.overlap").exists());
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
    let mut played = BTreeSet::new();
    for p in read(&s.calls(), "stdin") {
        let token = s.tokens.values().find(|t| p.contains(t.as_str())).unwrap();
        let tick = p
            .split("It is tick ")
            .nth(1)
            .and_then(|r| r.split('.').next())
            .unwrap()
            .to_owned();
        assert!(
            played.insert((token.clone(), tick.clone())),
            "played twice: {tick}"
        );
    }
    // Every seat played, and each run was stopped well before its 60s timeout.
    assert!(played.len() >= SEATS.len(), "{played:?}");
    for seat in SEATS {
        assert!(s.runs().join(seat).join("tick-0.jsonl").exists(), "{seat}");
    }
}

#[tokio::test]
async fn runs_get_a_clean_environment_and_private_logs() {
    let s = start("", "max_concurrent: 4\n").await;
    s.wait_for_tick(1).await;
    for env in read(&s.calls(), "env") {
        assert!(!env.contains("CARGO_"), "{env}");
        assert!(env.lines().any(|l| l.starts_with("PATH=")));
    }
    let mode = std::fs::metadata(s.runs()).unwrap().permissions().mode();
    assert_eq!(mode & 0o077, 0, "{mode:o}");
    let log = s.runs().join("crown").join("tick-0.jsonl");
    let mode = std::fs::metadata(log).unwrap().permissions().mode();
    assert_eq!(mode & 0o077, 0, "{mode:o}");
}
