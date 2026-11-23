use std::io::IsTerminal;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Context;
use clap::{Parser, Subcommand};
use sim_core::{Scenario, World};
use sim_runner::{AgentConfig, AuthorJob, RunnerConfig};
use sim_server::game::Config;
use sim_server::host::{Options, host};

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
    /// Connect an agent CLI to Empire, on your own subscription. For Claude Code, run
    /// `claude setup-token` first and paste the token it prints; Codex opens its own login.
    Login {
        #[arg(value_enum, default_value = "claude")]
        agent: Provider,
    },
    /// Print the JSON Schema for scenario files.
    Schema,
    /// Validate a scenario file.
    Check { scenario: PathBuf },
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum Provider {
    Claude,
    Codex,
}

#[derive(clap::Args)]
struct RunArgs {
    /// The world's name. It lives in Empire's data directory, shared with the desktop app.
    #[arg(long, required_unless_present = "db")]
    world: Option<String>,
    /// A database file of your choosing instead of a named world.
    #[arg(long, conflicts_with = "world")]
    db: Option<PathBuf>,
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
    sim_runner::bridge_if_asked();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn,sim_server=info,sim_runner=info".into()),
        )
        .init();

    match Cli::parse().command {
        Command::Login {
            agent: Provider::Codex,
        } => {
            let status = sim_runner::codex_login_command(Path::new("codex"))?
                .status()
                .context("starting `codex`")?;
            anyhow::ensure!(status.success(), "`codex login` failed");
            println!("Codex seats now log in as this account.");
        }
        Command::Login {
            agent: Provider::Claude,
        } => {
            println!("Paste the token `claude setup-token` printed (it won't show):");
            let echo = |on: bool| {
                if std::io::stdin().is_terminal() {
                    let _ = std::process::Command::new("stty")
                        .arg(if on { "echo" } else { "-echo" })
                        .status();
                }
            };
            echo(false);
            let mut token = String::new();
            let read = std::io::stdin().read_line(&mut token);
            echo(true);
            read?;
            sim_runner::save_claude_token(&token)?;
            println!("Saved. Claude Code seats now log in with it.");
        }
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
    let db = match a.db {
        Some(db) => db,
        None => sim_runner::world_db(a.world.as_deref().expect("clap requires --world or --db"))?,
    };
    let mut agents = a
        .agents
        .as_deref()
        .map(|p| anyhow::Ok(RunnerConfig::from_yaml(&read(p)?)?))
        .transpose()?;
    if let Some(all) = a.play_all {
        agents.get_or_insert_with(RunnerConfig::default).all = Some(all);
    }
    let (claude_token, codex_login) = sim_runner::logins()?;
    let runs = a.runs.unwrap_or_else(|| db.with_extension("runs"));
    // Bind first, so a busy port fails before any agent is paid to write a world.
    let listener = tokio::net::TcpListener::bind(a.bind)
        .await
        .with_context(|| format!("binding {}", a.bind))?;
    let scenario = match (&a.scenario, &a.prompt) {
        (Some(p), _) => Some(read(p)?),
        (None, Some(request)) => Some(prompted_scenario(&db, request, &a.author, &runs).await?),
        (None, None) => None,
    };
    let h = host(
        listener,
        Options {
            db,
            scenario,
            seed: a.seed,
            config: Config {
                tick_deadline: Duration::from_secs(a.tick_seconds),
                autoplay: a.autoplay,
            },
            allowed_hosts: a.allowed_hosts,
            agents,
            runs,
            claude_token,
            codex_login,
        },
    )
    .await?;
    let w = h.game.world().await;
    println!("\n{} — MCP endpoint: {}", w.scenario.name, h.mcp_url);
    println!("Admin token (for /api): {}\n", h.admin_token);
    println!("Seat tokens:");
    for id in w.scenario.seat_ids() {
        let Some(s) = w.seat(id) else { continue };
        let side = match s.empire.and_then(|e| w.empire(e)) {
            Some(e) => e.name.as_str(),
            None => "game master",
        };
        let token = &h.seat_tokens[s.id];
        println!("  {:<12} {token}  {} [{side}]", s.id, s.name);
    }
    if let Some(p) = &h.agents {
        println!(
            "\nAgents play {} seats; run logs in {}",
            p.config.seats.len(),
            p.log_dir.display()
        );
    }
    println!();
    tokio::signal::ctrl_c().await?;
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

/// Ask the author agent for a scenario and return it once the engine accepts it.
async fn write_scenario(
    request: &str,
    author: &AgentConfig,
    log_dir: &Path,
    attempts: u32,
) -> anyhow::Result<String> {
    let AgentConfig::Claude(agent) = author else {
        anyhow::bail!("only Claude Code writes scenarios for now");
    };
    let login = sim_runner::claude_token()?;
    println!("Writing a scenario with your agent (this takes a minute or two)...");
    let a = sim_runner::author(&AuthorJob {
        agent,
        login: &login,
        bridge: &std::env::current_exe()?,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &str) -> Result<Cli, clap::Error> {
        Cli::try_parse_from(args.split(' '))
    }

    #[test]
    fn a_world_is_named_or_given_as_a_file_but_not_both() {
        assert!(parse("sim-server run --world rome").is_ok());
        assert!(parse("sim-server run --db rome.db").is_ok());
        assert!(parse("sim-server run --scenario x.yaml").is_err());
        assert!(parse("sim-server run --world rome --db rome.db").is_err());
        assert!(matches!(
            parse("sim-server login").unwrap().command,
            Command::Login {
                agent: Provider::Claude
            }
        ));
        assert!(matches!(
            parse("sim-server login codex").unwrap().command,
            Command::Login {
                agent: Provider::Codex
            }
        ));
    }
}
