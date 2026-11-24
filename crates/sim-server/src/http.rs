use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Query, Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sim_core::{Effect, EmpireId, Event};

use crate::game::{Game, GameError, Overview};
use crate::mcp::EmpireMcp;

#[derive(Clone)]
struct Api {
    game: Game,
    admin_token: Arc<str>,
}

pub struct HttpConfig {
    pub admin_token: String,
    /// Hostnames accepted in the `Host` header of MCP requests, beyond loopback.
    pub allowed_hosts: Vec<String>,
    pub max_wait: Duration,
}

pub fn router(game: Game, cfg: HttpConfig) -> Router {
    let mut mcp_cfg = StreamableHttpServerConfig::default();
    mcp_cfg.allowed_hosts.extend(cfg.allowed_hosts);
    let (mcp_game, max_wait) = (game.clone(), cfg.max_wait);
    let mcp = StreamableHttpService::new(
        move || Ok(EmpireMcp::new(mcp_game.clone(), max_wait)),
        Arc::new(LocalSessionManager::default()),
        mcp_cfg,
    );

    let api = Api {
        game,
        admin_token: cfg.admin_token.into(),
    };
    let admin = Router::new()
        .route("/overview", get(overview))
        .route("/events", get(events))
        .route("/intervene", post(intervene))
        .route("/introduce", post(introduce))
        .route("/tick", post(tick))
        .route_layer(middleware::from_fn_with_state(api.clone(), require_admin))
        .with_state(api);

    Router::new().nest_service("/mcp", mcp).nest("/api", admin)
}

async fn require_admin(State(api): State<Api>, req: Request, next: Next) -> Response {
    let presented = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    match presented {
        Some(t) if constant_time_eq(t.as_bytes(), api.admin_token.as_bytes()) => {
            next.run(req).await
        }
        _ => (StatusCode::UNAUTHORIZED, "admin token required").into_response(),
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

impl IntoResponse for GameError {
    fn into_response(self) -> Response {
        let status = match &self {
            GameError::Rejected(_) | GameError::TurnEnded | GameError::StaleTick { .. } => {
                StatusCode::UNPROCESSABLE_ENTITY
            }
            GameError::UnknownToken => StatusCode::UNAUTHORIZED,
            GameError::Scenario(_) | GameError::Store(_) => {
                tracing::error!(error = %self, "request failed");
                StatusCode::INTERNAL_SERVER_ERROR
            }
        };
        (status, Json(json!({ "error": self.to_string() }))).into_response()
    }
}

async fn overview(State(api): State<Api>) -> Json<Overview> {
    Json(api.game.overview().await)
}

#[derive(Deserialize)]
struct EventsQuery {
    #[serde(default)]
    after: i64,
    #[serde(default = "default_limit")]
    limit: i64,
}

fn default_limit() -> i64 {
    500
}

#[derive(Serialize, Deserialize)]
pub struct LoggedEvent {
    pub seq: i64,
    pub event: Event,
}

async fn events(
    State(api): State<Api>,
    Query(q): Query<EventsQuery>,
) -> Result<Json<Vec<LoggedEvent>>, GameError> {
    let page = api
        .game
        .store()
        .events_after(q.after, q.limit.clamp(1, 5000))
        .await?;
    Ok(Json(
        page.into_iter()
            .map(|(seq, event)| LoggedEvent { seq, event })
            .collect(),
    ))
}

#[derive(Deserialize)]
struct InterveneBody {
    title: String,
    #[serde(default)]
    text: String,
    #[serde(default)]
    effects: Vec<Effect>,
    /// Announce only to this empire's seats. Omitted: every seat hears of it.
    #[serde(default)]
    empire: Option<EmpireId>,
}

async fn intervene(
    State(api): State<Api>,
    Json(b): Json<InterveneBody>,
) -> Result<Json<Value>, GameError> {
    let seq = api
        .game
        .intervene(&b.title, &b.text, b.effects, b.empire.as_ref())
        .await?;
    Ok(Json(json!({ "seq": seq })))
}

#[derive(Deserialize)]
struct IntroduceBody {
    a: EmpireId,
    b: EmpireId,
}

async fn introduce(
    State(api): State<Api>,
    Json(b): Json<IntroduceBody>,
) -> Result<Json<Value>, GameError> {
    let seq = api.game.introduce(&b.a, &b.b).await?;
    Ok(Json(json!({ "seq": seq })))
}

async fn tick(State(api): State<Api>) -> Result<Json<Value>, GameError> {
    let tick = api.game.force_close().await?;
    Ok(Json(json!({ "tick": tick })))
}
