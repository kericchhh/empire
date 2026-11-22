use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context;
use clap::{Parser, Subcommand};
use sim_core::{Scenario, World};
use sim_runner::{Plan, RunnerConfig};
use sim_server::game::{Config, Game};
use sim_server::http::{HttpConfig, router};
use sim_server::mcp::MAX_WAIT;
use sim_server::store::Store;

#[derive(Parser)]
#[command(about = "Empire simulation server: agents connect to seats over MCP")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run a world. Creates it from --scenario if the database is new, otherwise resumes it.
    Run {
        #[arg(long)]
        db: PathBuf,
        #[arg(long)]
        scenario: Option<PathBuf>,
        #[arg(long, default_value_t = 0)]
        seed: u64,
        #[arg(long, default_value = "127.0.0.1:7878")]
        bind: SocketAddr,
        /// Seconds before a tick closes even if some seats are still deciding.
        #[arg(long, default_value_t = 300)]
        tick_seconds: u64,
        /// Advance on the deadline even when no seat has joined.
        #[arg(long)]
        autoplay: bool,
        /// Extra hostnames MCP clients may use to reach this server, e.g. a LAN name.
        #[arg(long = "allowed-host")]
        allowed_hosts: Vec<String>,
        /// Agent assignments: which seats a local agent CLI plays, and how. Seats left out
        /// wait for an external agent or fall back to the rules engine.
        #[arg(long)]
        agents: Option<PathBuf>,
        /// Where agent run logs go. Default: next to the database, as `<db>.runs`.
        #[arg(long)]
        runs: Option<PathBuf>,
    },
    /// Print the JSON Schema for scenario files.
    Schema,
    /// Validate a scenario file.
    Check { scenario: PathBuf },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,sim_server=info".into()),
        )
        .init();

    match Cli::parse().command {
        Command::Schema => {
            let schema = schemars::schema_for!(Scenario);
            println!("{}", serde_json::to_string_pretty(&schema)?);
        }
        Command::Check { scenario } => {
            let yaml = std::fs::read_to_string(&scenario)
                .with_context(|| format!("reading {}", scenario.display()))?;
            let w = World::standard(Scenario::from_yaml(&yaml)?)?;
            let sc = &w.scenario;
            println!(
                "{}: {} empires, {} regions, {} characters, {} seats",
                sc.name,
                sc.empires.len(),
                sc.regions().count(),
                sc.characters().count(),
                sc.seat_ids().count(),
            );
        }
        Command::Run {
            db,
            scenario,
            seed,
            bind,
            tick_seconds,
            autoplay,
            allowed_hosts,
            agents,
            runs,
        } => {
            let agents = agents
                .as_ref()
                .map(|p| -> anyhow::Result<RunnerConfig> {
                    let src = std::fs::read_to_string(p)
                        .with_context(|| format!("reading {}", p.display()))?;
                    Ok(RunnerConfig::from_yaml(&src)?)
                })
                .transpose()?;
            let yaml = scenario
                .as_ref()
                .map(|p| {
                    std::fs::read_to_string(p).with_context(|| format!("reading {}", p.display()))
                })
                .transpose()?;
            let store = Store::open(&db).await?;
            let created = Game::open(
                store,
                yaml.as_deref(),
                seed,
                Config {
                    tick_deadline: Duration::from_secs(tick_seconds),
                    autoplay,
                },
            )
            .await?;
            let w = created.game.world().await;

            println!("\n{} — MCP endpoint: http://{bind}/mcp", w.scenario.name);
            println!("Admin token (for /api): {}\n", created.admin_token);
            println!("Seat tokens:");
            for id in w.scenario.seat_ids() {
                let Some(s) = w.seat(id) else { continue };
                let side = match s.empire.and_then(|e| w.empire(e)) {
                    Some(e) => e.name.as_str(),
                    None => "game master",
                };
                println!(
                    "  {:<12} {}  {} [{side}]",
                    s.id, created.seat_tokens[s.id], s.name
                );
            }
            println!();

            let seat_tokens = created.seat_tokens;
            tokio::spawn(created.game.clone().run_scheduler());
            let app = router(
                created.game,
                HttpConfig {
                    admin_token: created.admin_token,
                    allowed_hosts,
                    max_wait: MAX_WAIT,
                },
            );
            let listener = tokio::net::TcpListener::bind(bind).await?;
            if let Some(config) = agents {
                if let Some(gm) = w.scenario.game_master.as_ref()
                    && config.seats.contains_key(&gm.id)
                {
                    anyhow::bail!(
                        "the runner cannot play the game master seat `{}` yet: it would rule \
                         before the other seats have acted. Connect an agent to it yourself.",
                        gm.id
                    );
                }
                let addr = match listener.local_addr()? {
                    a if a.ip().is_unspecified() && a.is_ipv4() => {
                        SocketAddr::from((std::net::Ipv4Addr::LOCALHOST, a.port()))
                    }
                    a if a.ip().is_unspecified() => {
                        SocketAddr::from((std::net::Ipv6Addr::LOCALHOST, a.port()))
                    }
                    a => a,
                };
                let plan = Plan {
                    tokens: config
                        .seats
                        .keys()
                        .filter_map(|s| Some((s.clone(), seat_tokens.get(s)?.clone())))
                        .collect(),
                    config,
                    mcp_url: format!("http://{addr}/mcp"),
                    log_dir: runs.unwrap_or_else(|| db.with_extension("runs")),
                };
                plan.validate()?;
                let worst = plan.worst_case_tick();
                if worst >= Duration::from_secs(tick_seconds) {
                    tracing::warn!(
                        "{} seats at {} at a time with a {}s turn timeout can take {}s, not \
                         below --tick-seconds ({tick_seconds}): the last seats may miss the tick",
                        plan.config.seats.len(),
                        plan.config.max_concurrent,
                        plan.config.turn_timeout_seconds,
                        worst.as_secs()
                    );
                }
                println!(
                    "Agents play {} seats; run logs in {}\n",
                    plan.config.seats.len(),
                    plan.log_dir.display()
                );
                tokio::spawn(sim_runner::run(plan));
            }
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = tokio::signal::ctrl_c().await;
                })
                .await?;
        }
    }
    Ok(())
}
