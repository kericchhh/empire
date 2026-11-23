//! Hosting a world: its game, tick scheduler, MCP and admin server, and the agents that play
//! its seats. The headless binary and the desktop app both start worlds through `host`.

use std::collections::BTreeMap;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::PathBuf;

use sim_core::{DefinitionError, Scenario, SeatId, World};
use sim_runner::{Plan, PlanError, RunnerConfig, SandboxError};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

use crate::game::{Config, Game, GameError};
use crate::http::{HttpConfig, router};
use crate::mcp::MAX_WAIT;
use crate::store::{Store, StoreError};

pub struct Options {
    pub db: PathBuf,
    /// The scenario for a new database. A saved world ignores it.
    pub scenario: Option<String>,
    pub seed: u64,
    pub config: Config,
    pub allowed_hosts: Vec<String>,
    pub agents: Option<RunnerConfig>,
    /// Where agent run logs go.
    pub runs: PathBuf,
    /// The token from `claude setup-token` that Claude seats log in with.
    pub claude_token: Option<String>,
}

/// A running world. Dropping it stops its server, scheduler and agents.
pub struct Hosted {
    pub game: Game,
    pub admin_token: String,
    pub seat_tokens: BTreeMap<SeatId, String>,
    pub mcp_url: String,
    /// The seats agents play, and how.
    pub agents: Option<Plan>,
    tasks: Vec<JoinHandle<()>>,
}

impl Drop for Hosted {
    fn drop(&mut self) {
        self.tasks.iter().for_each(JoinHandle::abort);
    }
}

#[derive(Debug, thiserror::Error)]
pub enum HostError {
    #[error(transparent)]
    Game(#[from] GameError),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Definition(#[from] DefinitionError),
    #[error(transparent)]
    Plan(#[from] PlanError),
    #[error(transparent)]
    Sandbox(#[from] SandboxError),
    #[error("{0}")]
    Agents(String),
    #[error("listener: {0}")]
    Io(#[from] std::io::Error),
}

/// Open or create the world in `o.db` and serve it on `listener`.
pub async fn host(listener: TcpListener, o: Options) -> Result<Hosted, HostError> {
    let mut agents = o.agents;
    // Check assignments against a new world before it is created, so a bad assignment never
    // leaves a half-made world behind. A saved world wins over the scenario.
    if let (Some(c), Some(y)) = (agents.as_mut(), o.scenario.as_deref())
        && !o.db.exists()
    {
        assign(c, &World::standard(Scenario::from_yaml(y)?)?)?;
    }
    let created = Game::open(
        Store::open(&o.db).await?,
        o.scenario.as_deref(),
        o.seed,
        o.config.clone(),
    )
    .await?;
    let mut addr = listener.local_addr()?;
    if addr.ip().is_unspecified() {
        addr.set_ip(match addr {
            SocketAddr::V4(_) => Ipv4Addr::LOCALHOST.into(),
            SocketAddr::V6(_) => Ipv6Addr::LOCALHOST.into(),
        });
    }
    let mcp_url = format!("http://{addr}/mcp");
    let app = router(
        created.game.clone(),
        HttpConfig {
            admin_token: created.admin_token.clone(),
            allowed_hosts: o.allowed_hosts,
            max_wait: MAX_WAIT,
        },
    );
    let mut tasks = vec![
        tokio::spawn(created.game.clone().run_scheduler()),
        tokio::spawn(async move {
            if let Err(e) = axum::serve(listener, app).await {
                tracing::error!(error = %e, "server stopped");
            }
        }),
    ];
    let agents = match agents {
        None => None,
        Some(mut config) => {
            assign(&mut config, &created.game.world().await)?;
            let plan = Plan {
                tokens: config
                    .seats
                    .keys()
                    .filter_map(|s| Some((s.clone(), created.seat_tokens.get(s)?.clone())))
                    .collect(),
                config,
                mcp_url: mcp_url.clone(),
                log_dir: o.runs,
                claude_token: o.claude_token,
                login_refused: Default::default(),
                bridge: std::env::current_exe()?,
            };
            plan.validate()?;
            sim_runner::check_sandbox()?;
            let worst = plan.worst_case_tick();
            if worst >= o.config.tick_deadline {
                tracing::warn!(
                    "{} seats at {} at a time with a {}s turn timeout can take {}s, not below \
                     the {}s tick deadline: the last seats may miss the tick",
                    plan.config.seats.len(),
                    plan.config.max_concurrent,
                    plan.config.turn_timeout_seconds,
                    worst.as_secs(),
                    o.config.tick_deadline.as_secs(),
                );
            }
            let runner = plan.clone();
            tasks.push(tokio::spawn(async move {
                if let Err(e) = sim_runner::run(runner).await {
                    tracing::error!(error = %e, "agent runner stopped");
                }
            }));
            Some(plan)
        }
    };
    Ok(Hosted {
        game: created.game,
        admin_token: created.admin_token,
        seat_tokens: created.seat_tokens,
        mcp_url,
        agents,
        tasks,
    })
}

/// Give `all`'s agent to the world's empire seats, and refuse assignments the runner
/// cannot play: unknown seats, and the game master's.
fn assign(config: &mut RunnerConfig, w: &World) -> Result<(), HostError> {
    config.resolve(w.scenario.seats().map(|(_, s)| &s.id));
    for id in config.seats.keys() {
        match w.seat(id) {
            None => {
                return Err(HostError::Agents(format!(
                    "agents are assigned seat `{id}`, which this world does not have"
                )));
            }
            Some(s) if s.game_master => {
                return Err(HostError::Agents(format!(
                    "the runner cannot play the game master seat `{id}` yet: it would rule \
                     before the other seats have acted. Connect an agent to it yourself."
                )));
            }
            Some(_) => {}
        }
    }
    Ok(())
}
