use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Context;
use clap::{Parser, Subcommand};
use sim_core::{Scenario, World};
use sim_runner::{AgentConfig, AuthorJob, Plan, RunnerConfig};
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
#[expect(
    clippy::large_enum_variant,
    reason = "parsed once at startup; boxing would only obscure the clap definitions"
)]
enum Command {
    /// Run a world. Creates it from --scenario or --prompt if the database is new, otherwise
    /// resumes it.
    Run(RunArgs),
    /// Have your agent write a scenario from a description, check it, and save it.
    Create {
        /// What the world should be, in your own words.
        #[arg(long)]
        prompt: String,
        /// Where to save the scenario.
        #[arg(long)]
        out: PathBuf,
        /// The agent that writes it: `claude` or `claude:<model>`.
        #[arg(long, default_value = "claude")]
        author: AgentConfig,
        /// Drafts to try before giving up, each told why the last one was rejected.
        #[arg(long, default_value_t = 3)]
        attempts: u32,
    },
    /// Print the JSON Schema for scenario files.
    Schema,
    /// Validate a scenario file.
    Check { scenario: PathBuf },
}

#[derive(clap::Args)]
struct RunArgs {
    #[arg(long)]
    db: PathBuf,
    #[arg(long, conflicts_with = "prompt")]
    scenario: Option<PathBuf>,
    /// Describe the world in your own words; your agent writes the scenario, saved as
    /// `<db>.scenario.yaml`. Only for a new database.
    #[arg(long)]
    prompt: Option<String>,
    /// The agent that writes the scenario for --prompt: `claude` or `claude:<model>`.
    #[arg(long, default_value = "claude", requires = "prompt")]
    author: AgentConfig,
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
    /// Let one agent play every seat but the game master's: `claude` or
    /// `claude:<model>`. Seats listed in --agents keep their own agent.
    #[arg(long)]
    play_all: Option<AgentConfig>,
    /// Where agent run logs go. Default: next to the database, as `<db>.runs`.
    #[arg(long)]
    runs: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn,sim_server=info,sim_runner=info".into()),
        )
        .init();

    match Cli::parse().command {
        Command::Schema => {
            let schema = schemars::schema_for!(Scenario);
            println!("{}", serde_json::to_string_pretty(&schema)?);
        }
        Command::Check { scenario } => {
            let w = World::standard(Scenario::from_yaml(&read(&scenario)?)?)?;
            println!("{}", summary(&w.scenario));
        }
        Command::Create {
            prompt,
            out,
            author,
            attempts,
        } => {
            if out.exists() {
                anyhow::bail!("{} already exists", out.display());
            }
            let yaml =
                write_scenario(&prompt, &author, &out.with_extension("runs"), attempts).await?;
            write(&out, &yaml)?;
            println!("Saved {}. Run it with --scenario {0}.", out.display());
        }
        Command::Run(args) => run(args).await?,
    }
    Ok(())
}

async fn run(a: RunArgs) -> anyhow::Result<()> {
    let mut agents = a
        .agents
        .as_deref()
        .map(|p| anyhow::Ok(RunnerConfig::from_yaml(&read(p)?)?))
        .transpose()?;
    if let Some(all) = a.play_all {
        agents.get_or_insert_with(RunnerConfig::default).all = Some(all);
    }
    let runs = a.runs.unwrap_or_else(|| a.db.with_extension("runs"));
    // Bind first, so a busy port fails before any agent is paid to write a world.
    let listener = tokio::net::TcpListener::bind(a.bind)
        .await
        .with_context(|| format!("binding {}", a.bind))?;
    let yaml = match (&a.scenario, &a.prompt) {
        (Some(p), _) => Some(read(p)?),
        (None, Some(request)) => Some(prompted_scenario(&a.db, request, &a.author, &runs).await?),
        (None, None) => None,
    };
    // Check agent assignments against a new world before it is created, so a bad
    // assignment never leaves a half-made world behind.
    // A saved world wins over --scenario, so only a new database is checked here.
    let fresh = yaml
        .as_deref()
        .filter(|_| !a.db.exists())
        .map(|y| World::standard(Scenario::from_yaml(y)?))
        .transpose()?;
    if let (Some(c), Some(w)) = (agents.as_mut(), &fresh) {
        assign(c, w)?;
    }
    let config = Config {
        tick_deadline: Duration::from_secs(a.tick_seconds),
        autoplay: a.autoplay,
    };
    let created = Game::open(Store::open(&a.db).await?, yaml.as_deref(), a.seed, config).await?;
    let w = created.game.world().await;

    println!(
        "\n{} — MCP endpoint: http://{}/mcp",
        w.scenario.name, a.bind
    );
    println!("Admin token (for /api): {}\n", created.admin_token);
    println!("Seat tokens:");
    for id in w.scenario.seat_ids() {
        let Some(s) = w.seat(id) else { continue };
        let side = match s.empire.and_then(|e| w.empire(e)) {
            Some(e) => e.name.as_str(),
            None => "game master",
        };
        let token = &created.seat_tokens[s.id];
        println!("  {:<12} {token}  {} [{side}]", s.id, s.name);
    }
    println!();

    tokio::spawn(created.game.clone().run_scheduler());
    let app = router(
        created.game,
        HttpConfig {
            admin_token: created.admin_token,
            allowed_hosts: a.allowed_hosts,
            max_wait: MAX_WAIT,
        },
    );
    if let Some(mut config) = agents {
        if fresh.is_none() {
            assign(&mut config, &w)?;
        }
        let mut addr = listener.local_addr()?;
        if addr.ip().is_unspecified() {
            addr.set_ip(match addr {
                SocketAddr::V4(_) => Ipv4Addr::LOCALHOST.into(),
                SocketAddr::V6(_) => Ipv6Addr::LOCALHOST.into(),
            });
        }
        let tokens = created.seat_tokens;
        let plan = Plan {
            tokens: config
                .seats
                .keys()
                .filter_map(|s| Some((s.clone(), tokens.get(s)?.clone())))
                .collect(),
            config,
            mcp_url: format!("http://{addr}/mcp"),
            log_dir: runs,
        };
        plan.validate()?;
        let worst = plan.worst_case_tick();
        if worst >= Duration::from_secs(a.tick_seconds) {
            tracing::warn!(
                "{} seats at {} at a time with a {}s turn timeout can take {}s, not \
                 below --tick-seconds ({}): the last seats may miss the tick",
                plan.config.seats.len(),
                plan.config.max_concurrent,
                plan.config.turn_timeout_seconds,
                worst.as_secs(),
                a.tick_seconds,
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
    Ok(())
}

/// For `run --prompt`: write a scenario for a new database and save it beside it.
async fn prompted_scenario(
    db: &Path,
    request: &str,
    author: &AgentConfig,
    runs: &Path,
) -> anyhow::Result<String> {
    if db.exists() {
        anyhow::bail!(
            "{} already holds a world; drop --prompt to resume it",
            db.display()
        );
    }
    let saved = db.with_extension("scenario.yaml");
    if saved.exists() {
        anyhow::bail!(
            "{} already exists; start it with --scenario {0}, or move it",
            saved.display()
        );
    }
    let yaml = write_scenario(request, author, runs, 3).await?;
    write(&saved, &yaml)?;
    println!("Saved the scenario as {}.", saved.display());
    Ok(yaml)
}

fn read(p: &Path) -> anyhow::Result<String> {
    std::fs::read_to_string(p).with_context(|| format!("reading {}", p.display()))
}

fn write(p: &Path, contents: &str) -> anyhow::Result<()> {
    std::fs::write(p, contents).with_context(|| format!("writing {}", p.display()))
}

fn summary(sc: &Scenario) -> String {
    format!(
        "{}: {} empires, {} regions, {} characters, {} seats",
        sc.name,
        sc.empires.len(),
        sc.regions().count(),
        sc.characters().count(),
        sc.seat_ids().count(),
    )
}

/// Give `all`'s agent to the world's empire seats, and refuse assignments the runner
/// cannot play: unknown seats, and the game master's.
fn assign(config: &mut RunnerConfig, w: &World) -> anyhow::Result<()> {
    config.resolve(w.scenario.seats().map(|(_, s)| &s.id));
    for id in config.seats.keys() {
        match w.seat(id) {
            None => anyhow::bail!("--agents names seat `{id}`, which this world does not have"),
            Some(s) if s.game_master => anyhow::bail!(
                "the runner cannot play the game master seat `{id}` yet: it would rule before \
                 the other seats have acted. Connect an agent to it yourself."
            ),
            Some(_) => {}
        }
    }
    Ok(())
}

/// Ask the author agent for a scenario and return it once the engine accepts it.
async fn write_scenario(
    request: &str,
    author: &AgentConfig,
    log_dir: &Path,
    attempts: u32,
) -> anyhow::Result<String> {
    let AgentConfig::Claude(agent) = author;
    println!("Writing a scenario with your agent (this takes a minute or two)...");
    let a = sim_runner::author(&AuthorJob {
        agent,
        request,
        log_dir,
        attempts,
        timeout: Duration::from_secs(600),
    })
    .await?;
    println!(
        "{} ({} attempt{}).",
        summary(&a.world.scenario),
        a.attempts,
        if a.attempts == 1 { "" } else { "s" },
    );
    Ok(a.yaml)
}
