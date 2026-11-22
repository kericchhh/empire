//! The desktop app: hosts one world at a time in-process and lets the user watch and steer it.
//! Agents still reach the world over MCP, exactly as with the headless server.

use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sim_core::perceive::{Sight, describe};
use sim_core::{Effect, EmpireId, SeatId};
use sim_runner::{AgentConfig, AuthorJob, RunnerConfig};
use sim_server::game::{Config, Overview, TurnView};
use sim_server::host::{Hosted, Options, host};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::Mutex;

type Res<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[derive(Default)]
struct App(Mutex<Option<Hosted>>);

/// Worlds live in the app's data directory as `<name>.db`, with their logs beside them.
fn worlds(app: &AppHandle) -> Res<PathBuf> {
    let dir = app.path().app_data_dir().map_err(err)?.join("worlds");
    std::fs::create_dir_all(&dir).map_err(err)?;
    Ok(dir)
}

#[tauri::command]
fn list_worlds(app: AppHandle) -> Res<Vec<String>> {
    let mut names: Vec<String> = std::fs::read_dir(worlds(&app)?)
        .map_err(err)?
        .filter_map(|e| {
            let p = e.ok()?.path();
            (p.extension()? == "db").then(|| p.file_stem()?.to_str().map(str::to_owned))?
        })
        .collect();
    names.sort();
    Ok(names)
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Source {
    /// Reopen a saved world.
    Saved,
    File(PathBuf),
    /// Have the agent write the scenario: the request, and the agent (`claude[:model]`).
    Prompt {
        text: String,
        author: String,
    },
}

#[derive(Deserialize)]
struct OpenWorld {
    name: String,
    source: Source,
    seed: u64,
    tick_seconds: u64,
    autoplay: bool,
    /// `claude[:model]` to let agents play every seat, or none.
    play_all: Option<String>,
    max_concurrent: usize,
}

#[derive(Serialize)]
struct Seat {
    id: SeatId,
    token: String,
    agent: bool,
}

#[derive(Serialize)]
struct Opened {
    name: String,
    mcp_url: String,
    seats: Vec<Seat>,
}

#[tauri::command]
async fn open_world(app: AppHandle, state: State<'_, App>, req: OpenWorld) -> Res<Opened> {
    if req.name.is_empty() || req.name.contains(['/', '\\', '.']) {
        return Err("a world name may not be empty or contain `/`, `\\` or `.`".into());
    }
    let mut slot = state.0.lock().await;
    *slot = None;
    let dir = worlds(&app)?;
    let db = dir.join(format!("{}.db", req.name));
    let runs = dir.join(format!("{}.runs", req.name));
    let parse = |a: &str| a.parse::<AgentConfig>().map_err(err);
    let scenario = match &req.source {
        Source::Saved if !db.exists() => return Err(format!("no saved world `{}`", req.name)),
        Source::Saved => None,
        _ if db.exists() => return Err(format!("a world `{}` already exists", req.name)),
        Source::File(p) => Some(std::fs::read_to_string(p).map_err(err)?),
        Source::Prompt { text, author } => {
            let AgentConfig::Claude(agent) = parse(author)?;
            let a = sim_runner::author(&AuthorJob {
                agent: &agent,
                request: text,
                log_dir: &runs,
                attempts: 3,
                timeout: Duration::from_secs(600),
            })
            .await
            .map_err(err)?;
            std::fs::write(dir.join(format!("{}.scenario.yaml", req.name)), &a.yaml)
                .map_err(err)?;
            Some(a.yaml)
        }
    };
    let agents = req
        .play_all
        .as_deref()
        .map(|a| {
            Ok::<_, String>(RunnerConfig {
                max_concurrent: req.max_concurrent.max(1),
                all: Some(parse(a)?),
                ..RunnerConfig::default()
            })
        })
        .transpose()?;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(err)?;
    let h = host(
        listener,
        Options {
            db,
            scenario,
            seed: req.seed,
            config: Config {
                tick_deadline: Duration::from_secs(req.tick_seconds.max(1)),
                autoplay: req.autoplay,
            },
            allowed_hosts: vec![],
            agents,
            runs,
        },
    )
    .await
    .map_err(err)?;
    // The window refreshes whenever the world records an event.
    let mut events = h.game.subscribe();
    let tx = app.clone();
    tokio::spawn(async move {
        while let Ok((seq, _)) = events.recv().await {
            if tx.emit("world-event", seq).is_err() {
                break;
            }
        }
    });
    let played = |s: &SeatId| {
        h.agents
            .as_ref()
            .is_some_and(|p| p.config.seats.contains_key(s))
    };
    let opened = Opened {
        name: req.name,
        mcp_url: h.mcp_url.clone(),
        seats: h
            .seat_tokens
            .iter()
            .map(|(id, token)| Seat {
                agent: played(id),
                id: id.clone(),
                token: token.clone(),
            })
            .collect(),
    };
    *slot = Some(h);
    Ok(opened)
}

#[tauri::command]
async fn close_world(state: State<'_, App>) -> Res<()> {
    *state.0.lock().await = None;
    Ok(())
}

/// Runs `f` on the open world, or fails if there is none.
async fn with<T>(state: &State<'_, App>, f: impl AsyncFnOnce(&Hosted) -> Res<T>) -> Res<T> {
    match state.0.lock().await.as_ref() {
        Some(h) => f(h).await,
        None => Err("no world is open".into()),
    }
}

#[tauri::command]
async fn overview(state: State<'_, App>) -> Res<Overview> {
    with(&state, async |h| Ok(h.game.overview().await)).await
}

/// The world as one seat's characters know it.
#[tauri::command]
async fn seat_view(state: State<'_, App>, seat: SeatId) -> Res<TurnView> {
    with(&state, async |h| {
        let token = h.seat_tokens.get(&seat).ok_or("unknown seat")?;
        h.game.look(token).await.map_err(err)
    })
    .await
}

#[derive(Serialize)]
struct Entry {
    seq: i64,
    date: String,
    text: String,
}

/// History in words after `after`, as an all-seeing observer reads it, with the last event read:
/// many events describe nothing, so the caller resumes from `last`, not from the last entry.
#[derive(Serialize)]
struct Chronicle {
    last: i64,
    entries: Vec<Entry>,
}

#[tauri::command]
async fn chronicle(state: State<'_, App>, after: i64) -> Res<Chronicle> {
    const PAGE: i64 = 5000;
    with(&state, async |h| {
        let w = h.game.world().await;
        let mut c = Chronicle {
            last: after,
            entries: Vec::new(),
        };
        loop {
            let page = h
                .game
                .store()
                .events_after(c.last, PAGE)
                .await
                .map_err(err)?;
            let full = page.len() as i64 == PAGE;
            for (seq, e) in page {
                c.last = seq;
                if let Some(text) = describe(&w, &e, &Sight::all()) {
                    c.entries.push(Entry {
                        seq,
                        date: w.date(e.tick),
                        text,
                    });
                }
            }
            if !full {
                return Ok(c);
            }
        }
    })
    .await
}

#[tauri::command]
async fn intervene(
    state: State<'_, App>,
    title: String,
    text: String,
    effects: Vec<Effect>,
    empire: Option<EmpireId>,
) -> Res<i64> {
    with(&state, async |h| {
        h.game
            .intervene(&title, &text, effects, empire.as_ref())
            .await
            .map_err(err)
    })
    .await
}

#[tauri::command]
async fn advance_tick(state: State<'_, App>) -> Res<u64> {
    with(&state, async |h| {
        h.game.force_close().await.map(|t| t.0).map_err(err)
    })
    .await
}

/// What the local Claude Code CLI says about itself and its login. Runs no model.
#[derive(Serialize, Default)]
struct ClaudeStatus {
    version: Option<String>,
    logged_in: bool,
    auth_method: Option<String>,
    plan: Option<String>,
}

#[tauri::command]
async fn claude_status() -> ClaudeStatus {
    // A stalled CLI (a keychain prompt, a dead network) must not leave Providers checking forever.
    let run = async |args: &[&str]| {
        let out = tokio::process::Command::new("claude")
            .args(args)
            .kill_on_drop(true)
            .output();
        let out = tokio::time::timeout(std::time::Duration::from_secs(10), out)
            .await
            .ok()?
            .ok()?;
        out.status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
    };
    let Some(version) = run(&["--version"]).await else {
        return ClaudeStatus::default();
    };
    let auth: serde_json::Value = run(&["auth", "status", "--json"])
        .await
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    let field = |k: &str| auth[k].as_str().map(str::to_owned);
    ClaudeStatus {
        version: Some(version),
        logged_in: auth["loggedIn"].as_bool().unwrap_or(false),
        auth_method: field("authMethod"),
        plan: field("subscriptionType"),
    }
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn,sim_server=info,sim_runner=info".into()),
        )
        .init();
    tauri::Builder::default()
        .manage(App::default())
        .invoke_handler(tauri::generate_handler![
            list_worlds,
            open_world,
            close_world,
            overview,
            seat_view,
            chronicle,
            intervene,
            advance_tick,
            claude_status
        ])
        .run(tauri::generate_context!())
        .expect("the app window could not start");
}
