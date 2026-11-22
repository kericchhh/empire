//! One task per agent seat: wait for the seat's turn over MCP, launch a fresh agent run for
//! it, then end the turn so the tick can close.
//!
//! The runner is an MCP client like any external agent, and each task only ever holds one
//! seat's token. Each turn is a new process with no memory of the last: what carries over is
//! what the engine keeps, such as the character's notes.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use rmcp::model::{CallToolRequestParams, ClientConfig};
use rmcp::service::{RoleClient, RunningService, ServiceExt};
use rmcp::transport::StreamableHttpClientTransport;
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use sim_core::{SeatId, Tick};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tracing::Instrument;

use crate::claude::{self, Turn};
use crate::config::{AgentConfig, RunnerConfig};
use crate::fs::private_dir;

type Client = RunningService<RoleClient, ClientConfig>;

/// Everything the runner needs to play its seats.
#[derive(Debug, Clone)]
pub struct Plan {
    pub config: RunnerConfig,
    /// The token of every seat in `config.seats`.
    pub tokens: BTreeMap<SeatId, String>,
    pub mcp_url: String,
    /// Each run's output lands in `<log_dir>/<seat>/tick-<n>.jsonl`, readable only by the
    /// user, since it holds the seat's token and secrets.
    pub log_dir: PathBuf,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum PlanError {
    #[error("seat `{0}` is assigned an agent but has no token: is it in the scenario?")]
    NoToken(SeatId),
    #[error("seat id `{0}` cannot name a log directory")]
    BadSeatId(SeatId),
}

/// Whether a seat id can name its log directory. The runner plays only such seats.
pub fn seat_id_ok(seat: &SeatId) -> bool {
    let id = seat.as_str();
    !(id.is_empty() || id == "." || id == ".." || id.contains(['/', '\\']))
}

impl Plan {
    pub fn validate(&self) -> Result<(), PlanError> {
        for s in self.config.seats.keys() {
            if !seat_id_ok(s) {
                return Err(PlanError::BadSeatId(s.clone()));
            }
            if !self.tokens.contains_key(s) {
                return Err(PlanError::NoToken(s.clone()));
            }
        }
        Ok(())
    }

    /// The longest a tick can wait for its agent runs when every run queues for a slot and
    /// uses its full timeout. A tick deadline shorter than this cuts off the last seats.
    pub fn worst_case_tick(&self) -> Duration {
        let rounds = self.config.seats.len().div_ceil(self.config.max_concurrent);
        self.config.turn_timeout() * u32::try_from(rounds).unwrap_or(u32::MAX)
    }
}

/// Play every configured seat until the future is dropped. Runs share one cap on how many
/// agents run at once.
pub async fn run(plan: Plan) -> Result<(), PlanError> {
    plan.validate()?;
    if let Err(e) = private_dir(&plan.log_dir) {
        tracing::warn!(dir = %plan.log_dir.display(), error = %e, "cannot create the run log directory");
    }
    let slots = Arc::new(Semaphore::new(plan.config.max_concurrent));
    let plan = Arc::new(plan);
    let mut seats = JoinSet::new();
    for seat in plan.config.seats.keys() {
        let span = tracing::info_span!("seat", %seat);
        seats.spawn(play_seat(plan.clone(), seat.clone(), slots.clone()).instrument(span));
    }
    while seats.join_next().await.is_some() {}
    Ok(())
}

const RETRY_MIN: Duration = Duration::from_millis(250);
const RETRY_MAX: Duration = Duration::from_secs(10);
/// How often a running agent's tick is checked, to stop the run once the tick has closed.
const TICK_CHECK: Duration = Duration::from_secs(1);
/// How long a run may go on after its tick closed. A run whose own `end_turn` closed the
/// tick is about to exit and write its summary; one cut off by the deadline is wasting usage.
const AFTER_CLOSE: Duration = Duration::from_secs(5);

/// Keeps playing one seat. A dropped connection or a failed call is logged and retried with
/// a growing pause that resets once the seat has joined again.
async fn play_seat(plan: Arc<Plan>, seat: SeatId, slots: Arc<Semaphore>) {
    let mut pause = RETRY_MIN;
    let mut played: Option<Tick> = None;
    loop {
        let Err(e) = session(&plan, &seat, &slots, &mut played, &mut pause).await;
        tracing::warn!(error = %e, "seat runner: reconnecting");
        tokio::time::sleep(pause).await;
        pause = (pause * 2).min(RETRY_MAX);
    }
}

#[derive(Debug, thiserror::Error)]
enum SessionError {
    #[error("mcp: {0}")]
    Mcp(String),
    #[error("`{tool}` failed: {message}")]
    Tool { tool: &'static str, message: String },
    #[error("unexpected `{tool}` reply: {error}")]
    Reply {
        tool: &'static str,
        error: serde_json::Error,
    },
}

/// The parts of the server's turn view the runner reads.
#[derive(Debug, Deserialize)]
struct View {
    status: Status,
    tick: Tick,
    perception: Option<Seen>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Status {
    YourTurn,
    Waiting,
}

#[derive(Debug, Deserialize)]
struct Seen {
    game_master: bool,
}

/// One MCP connection: join, then take turns until something fails.
async fn session(
    plan: &Plan,
    seat: &SeatId,
    slots: &Semaphore,
    played: &mut Option<Tick>,
    pause: &mut Duration,
) -> Result<std::convert::Infallible, SessionError> {
    let token = &plan.tokens[seat];
    let AgentConfig::Claude(agent) = &plan.config.seats[seat];
    let transport = StreamableHttpClientTransport::from_config(
        StreamableHttpClientTransportConfig::with_uri(plan.mcp_url.clone()),
    );
    let client: Client = ClientConfig::default()
        .serve(transport)
        .await
        .map_err(|e| SessionError::Mcp(e.to_string()))?;
    let token_arg = json!({ "seat_token": token });
    call::<View>(&client, "join", &token_arg).await?;
    *pause = RETRY_MIN;
    loop {
        let view: View = call(&client, "wait_for_turn", &token_arg).await?;
        if view.status != Status::YourTurn {
            continue;
        }
        let tick = view.tick;
        let span = tracing::info_span!("tick", %tick);
        async {
            if *played != Some(tick) {
                *played = Some(tick);
                let _slot = slots
                    .acquire()
                    .await
                    .expect("the semaphore is never closed");
                // Waiting for a slot can outlast the tick.
                let now: View = call(&client, "look", &token_arg).await?;
                if now.tick != tick {
                    tracing::warn!("tick closed before a slot was free; turn skipped");
                    return Ok(());
                }
                let turn = Turn {
                    seat,
                    token,
                    tick,
                    game_master: view.perception.is_some_and(|p| p.game_master),
                    mcp_url: &plan.mcp_url,
                };
                let log = plan
                    .log_dir
                    .join(seat.as_str())
                    .join(format!("tick-{tick}.jsonl"));
                let work = plan.log_dir.join(".work");
                let closed = tick_closed(&client, &token_arg, tick);
                match claude::play(
                    agent,
                    &turn,
                    &log,
                    &work,
                    plan.config.turn_timeout(),
                    closed,
                )
                .await
                {
                    Ok(r) => tracing::info!(
                        outcome = ?r.outcome,
                        turns = r.summary.as_ref().map(|s| s.num_turns),
                        cost_usd = r.summary.as_ref().map(|s| s.total_cost_usd),
                        "agent turn finished"
                    ),
                    Err(e) => tracing::warn!(error = %e, "agent turn failed"),
                }
            }
            // The agent was told to end its turn; make sure it did, even if it crashed or
            // ran out of time. Refusals are expected: it already ended, or the tick closed.
            match call::<Value>(
                &client,
                "end_turn",
                &json!({ "seat_token": token, "tick": tick }),
            )
            .await
            {
                Ok(_) => {}
                Err(SessionError::Tool { message, .. }) => {
                    tracing::debug!(%message, "end_turn refused");
                    // If the tick is somehow still open, the next wait returns it at once;
                    // pause rather than spin on the refusal.
                    tokio::time::sleep(RETRY_MIN).await;
                }
                Err(e) => return Err(e),
            }
            Ok(())
        }
        .instrument(span)
        .await?;
    }
}

/// Completes a little after the world has moved past `tick`.
async fn tick_closed(client: &Client, token_arg: &Value, tick: Tick) {
    loop {
        tokio::time::sleep(TICK_CHECK).await;
        if let Ok(v) = call::<View>(client, "look", token_arg).await
            && v.tick != tick
        {
            tokio::time::sleep(AFTER_CLOSE).await;
            return;
        }
    }
}

async fn call<T: serde::de::DeserializeOwned>(
    client: &Client,
    tool: &'static str,
    args: &Value,
) -> Result<T, SessionError> {
    let args: Map<String, Value> = args.as_object().cloned().unwrap_or_default();
    let r = client
        .call_tool(CallToolRequestParams::new(tool).with_arguments(args))
        .await
        .map_err(|e| SessionError::Mcp(e.to_string()))?;
    if r.is_error == Some(true) {
        let message = r
            .content
            .iter()
            .filter_map(|c| c.as_text().map(|t| t.text.clone()))
            .collect::<Vec<_>>()
            .join(" ");
        return Err(SessionError::Tool { tool, message });
    }
    serde_json::from_value(r.structured_content.unwrap_or_default())
        .map_err(|error| SessionError::Reply { tool, error })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(yaml: &str) -> Plan {
        Plan {
            config: RunnerConfig::from_yaml(yaml).unwrap(),
            tokens: BTreeMap::new(),
            mcp_url: "http://x/mcp".into(),
            log_dir: "runs".into(),
        }
    }

    #[test]
    fn every_agent_seat_needs_a_token_and_a_safe_directory_name() {
        let mut p = plan("seats:\n  a: { claude: {} }\n");
        assert_eq!(p.validate(), Err(PlanError::NoToken("a".into())));
        p.tokens.insert("a".into(), "t".into());
        assert_eq!(p.validate(), Ok(()));
        let mut p = plan("seats:\n  '..': { claude: {} }\n");
        p.tokens.insert("..".into(), "t".into());
        assert_eq!(p.validate(), Err(PlanError::BadSeatId("..".into())));
    }

    #[test]
    fn queued_runs_add_up_to_the_worst_case_tick() {
        let p = plan(
            "max_concurrent: 2\nturn_timeout_seconds: 100\nseats:\n  a: { claude: {} }\n  b: { claude: {} }\n  c: { claude: {} }\n",
        );
        assert_eq!(p.worst_case_tick(), Duration::from_secs(200));
    }
}
