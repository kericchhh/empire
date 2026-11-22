use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context;
use clap::{Parser, Subcommand};
use sim_core::Scenario;
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
            let sc = Scenario::from_yaml(&yaml)?;
            println!(
                "{}: {} regions, {} characters, {} seats, {} actions, {} rules",
                sc.name,
                sc.regions.len(),
                sc.characters.len(),
                sc.seats.len(),
                sc.actions.len(),
                sc.rules.len()
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
        } => {
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
            let sc = created.game.scenario().await;

            println!("\n{} — MCP endpoint: http://{bind}/mcp", sc.name);
            println!("Admin token (for /api): {}\n", created.admin_token);
            println!("Seat tokens:");
            for s in &sc.seats {
                let gm = if s.game_master { " [game master]" } else { "" };
                println!(
                    "  {:<12} {}  {}{gm}",
                    s.id, created.seat_tokens[&s.id], s.name
                );
            }
            println!();

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
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = tokio::signal::ctrl_c().await;
                })
                .await?;
        }
    }
    Ok(())
}
