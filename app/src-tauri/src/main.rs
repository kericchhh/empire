//! The desktop app: hosts one world at a time in-process and lets the user watch and steer it.
//! Agents still reach the world over MCP, exactly as with the headless server.

use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sim_core::SeatId;
use sim_core::engine::Intervention;
use sim_core::perceive::{Sight, describe};
use sim_runner::{AgentConfig, AuthorJob, RunnerConfig};
use sim_runner::{claude_token, codex_login, world_db, worlds_dir};
use sim_server::game::{Config, Overview, TurnView};
use sim_server::host::{Hosted, Options, host};
use tauri::{AppHandle, Emitter, State};
use tokio::sync::Mutex;

type Res<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[derive(Default)]
struct App(Mutex<Option<Hosted>>);

/// Worlds live in Empire's data directory as `<name>.db`, shared with `sim-server --world`.
#[tauri::command]
fn list_worlds() -> Res<Vec<String>> {
    let mut names: Vec<String> = std::fs::read_dir(worlds_dir().map_err(err)?)
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
    let db = world_db(&req.name).map_err(err)?;
    let mut slot = state.0.lock().await;
    *slot = None;
    let runs = db.with_extension("runs");
    let parse = |a: &str| a.parse::<AgentConfig>().map_err(err);
    let scenario = match &req.source {
        Source::Saved if !db.exists() => return Err(format!("no saved world `{}`", req.name)),
        Source::Saved => None,
        _ if db.exists() => return Err(format!("a world `{}` already exists", req.name)),
        Source::File(p) => Some(std::fs::read_to_string(p).map_err(err)?),
        Source::Prompt { text, author } => {
            let AgentConfig::Claude(agent) = parse(author)? else {
                return Err("only Claude Code writes worlds for now".into());
            };
            let a = sim_runner::author(&AuthorJob {
                agent: &agent,
                login: &claude_token().map_err(err)?,
                bridge: &std::env::current_exe().map_err(err)?,
                request: text,
                log_dir: &runs,
                attempts: 3,
                timeout: Duration::from_secs(600),
            })
            .await
            .map_err(err)?;
            std::fs::write(db.with_extension("scenario.yaml"), &a.yaml).map_err(err)?;
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
    let (claude_token, codex_login) = sim_runner::logins().map_err(err)?;
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
            claude_token,
            codex_login,
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

/// Problems the user must fix for agents to keep playing.
#[tauri::command]
async fn agents_alerts(state: State<'_, App>) -> Res<Vec<&'static str>> {
    with(&state, async |h| {
        Ok(h.agents
            .as_ref()
            .map(|p| p.login_refused.alerts())
            .unwrap_or_default())
    })
    .await
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
async fn intervene(state: State<'_, App>, req: Intervention) -> Res<i64> {
    with(&state, async |h| h.game.intervene(req).await.map_err(err)).await
}

#[tauri::command]
async fn advance_tick(state: State<'_, App>) -> Res<u64> {
    with(&state, async |h| {
        h.game.force_close().await.map(|t| t.0).map_err(err)
    })
    .await
}

/// Which agent CLIs are installed and connected to Empire, and why agent runs cannot be
/// sandboxed on this machine, if they cannot. Runs no model.
#[derive(Serialize)]
struct Providers {
    claude: Provider,
    codex: Provider,
    sandbox_problem: Option<String>,
}

#[derive(Serialize)]
struct Provider {
    version: Option<String>,
    connected: bool,
}

async fn version(program: &str) -> Option<String> {
    // A stalled CLI must not leave Providers checking forever.
    let out = tokio::process::Command::new(program)
        .arg("--version")
        .kill_on_drop(true)
        .output();
    tokio::time::timeout(Duration::from_secs(10), out)
        .await
        .ok()
        .and_then(Result::ok)
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
}

#[tauri::command]
async fn providers() -> Providers {
    let (claude, codex) = tokio::join!(version("claude"), version("codex"));
    Providers {
        claude: Provider {
            version: claude,
            connected: claude_token().is_ok(),
        },
        codex: Provider {
            version: codex,
            connected: codex_login().is_ok(),
        },
        sandbox_problem: sim_runner::check_sandbox().err().map(|e| e.to_string()),
    }
}

/// Log Empire's own Codex home in with `codex login`, which opens the browser.
#[tauri::command]
async fn connect_codex() -> Res<()> {
    let cmd = sim_runner::codex_login_command(std::path::Path::new("codex")).map_err(err)?;
    let status = tokio::process::Command::from(cmd)
        .kill_on_drop(true)
        .status();
    match tokio::time::timeout(Duration::from_secs(600), status).await {
        Ok(Ok(s)) if s.success() => Ok(()),
        Ok(Ok(_)) => Err("`codex login` did not finish the login".into()),
        Ok(Err(e)) => Err(err(e)),
        Err(_) => Err("the Codex login timed out".into()),
    }
}

/// Save the token `claude setup-token` printed, so seats can log in inside their sandbox.
#[tauri::command]
fn save_claude_token(token: String) -> Res<()> {
    sim_runner::save_claude_token(&token).map_err(err)
}

fn main() {
    sim_runner::bridge_if_asked();
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
            providers,
            save_claude_token,
            connect_codex,
            agents_alerts
        ])
        .run(tauri::generate_context!())
        .expect("the app window could not start");
}
