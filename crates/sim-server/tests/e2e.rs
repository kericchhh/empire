use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::time::Duration;

use rmcp::ServiceExt;
use rmcp::model::{CallToolRequestParams, CallToolResult, ClientConfig};
use rmcp::service::{RoleClient, RunningService};
use rmcp::transport::StreamableHttpClientTransport;
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use serde_json::{Value, json};
use sim_core::SeatId;
use sim_server::http::LoggedEvent;

mod common;

struct Server {
    addr: SocketAddr,
    admin: String,
    tokens: BTreeMap<SeatId, String>,
    http: reqwest::Client,
}

type Client = RunningService<RoleClient, ClientConfig>;

impl Server {
    async fn start() -> Self {
        Self::of(common::TINY).await
    }

    async fn of(scenario: &str) -> Self {
        let (created, addr) = common::serve(scenario, Duration::from_secs(3600)).await;
        Self {
            addr,
            admin: created.admin_token,
            tokens: created.seat_tokens,
            http: reqwest::Client::new(),
        }
    }

    fn tok(&self, seat: &str) -> String {
        self.tokens[&SeatId::from(seat)].clone()
    }

    async fn mcp(&self) -> Client {
        let transport = StreamableHttpClientTransport::from_config(
            StreamableHttpClientTransportConfig::with_uri(format!("http://{}/mcp", self.addr)),
        );
        ClientConfig::default().serve(transport).await.unwrap()
    }

    fn api(&self, path: &str) -> String {
        format!("http://{}/api{path}", self.addr)
    }

    async fn get(&self, path: &str) -> reqwest::Response {
        self.http
            .get(self.api(path))
            .bearer_auth(&self.admin)
            .send()
            .await
            .unwrap()
    }

    async fn post(&self, path: &str, body: Value) -> reqwest::Response {
        self.http
            .post(self.api(path))
            .bearer_auth(&self.admin)
            .json(&body)
            .send()
            .await
            .unwrap()
    }
}

async fn call(client: &Client, tool: &'static str, args: Value) -> CallToolResult {
    let Value::Object(map) = args else {
        panic!("args must be an object")
    };
    client
        .call_tool(CallToolRequestParams::new(tool).with_arguments(map))
        .await
        .unwrap()
}

async fn look(client: &Client, token: &str) -> Value {
    ok(call(client, "look", json!({ "seat_token": token })).await)
}

fn ok(r: CallToolResult) -> Value {
    assert_ne!(r.is_error, Some(true), "tool failed: {r:?}");
    r.structured_content.expect("structured output")
}

#[tokio::test]
async fn the_server_advertises_the_seat_tools() {
    let s = Server::start().await;
    let c = s.mcp().await;
    let mut names: Vec<String> = c
        .list_all_tools()
        .await
        .unwrap()
        .into_iter()
        .map(|t| t.name.to_string())
        .collect();
    names.sort();
    assert_eq!(
        names,
        [
            "act",
            "end_turn",
            "join",
            "look",
            "message",
            "note",
            "rule",
            "wait_for_turn"
        ]
    );
    let info = c.peer_info().unwrap();
    assert!(
        info.instructions
            .as_deref()
            .unwrap_or("")
            .contains("wait_for_turn")
    );
}

#[tokio::test]
async fn a_bad_token_is_a_tool_error_not_a_protocol_failure() {
    let s = Server::start().await;
    let c = s.mcp().await;
    let r = call(&c, "join", json!({ "seat_token": "forged" })).await;
    assert_eq!(r.is_error, Some(true));
    assert!(format!("{:?}", r.content).contains("unknown seat token"));
}

#[tokio::test]
async fn two_agents_play_a_tick_over_mcp() {
    let s = Server::start().await;
    let crown = s.mcp().await;
    let rival = s.mcp().await;
    let (ct, rt) = (s.tok("crown"), s.tok("rival"));

    let joined = ok(call(&crown, "join", json!({ "seat_token": ct })).await);
    assert_eq!(joined["status"], "your_turn");
    assert_eq!(joined["perception"]["you_play"][0]["id"], "queen");
    ok(call(&rival, "join", json!({ "seat_token": rt })).await);

    let acted = ok(call(
        &crown,
        "act",
        json!({
            "seat_token": ct,
            "tick": 0,
            "orders": [
                { "kind": "action", "character": "queen", "action": "issue_edict" },
                { "kind": "action", "character": "queen", "action": "relief" }
            ]
        }),
    )
    .await);
    assert_eq!(acted["results"][0]["accepted"], true);
    assert_eq!(acted["results"][1]["accepted"], false);
    assert!(
        acted["results"][1]["error"]
            .as_str()
            .unwrap()
            .contains("needs a target")
    );

    ok(call(
        &crown,
        "message",
        json!({ "seat_token": ct, "from": "queen", "to": "duke", "text": "Kneel" }),
    )
    .await);
    let seen = look(&rival, &rt).await;
    assert!(seen.to_string().contains("«Kneel»"));

    let first = ok(call(&crown, "end_turn", json!({ "seat_token": ct, "tick": 0 })).await);
    assert_eq!(first["tick_closed"], false);
    assert_eq!(first["waiting_for"], json!(["rival"]));
    let waiting = ok(call(&crown, "wait_for_turn", json!({ "seat_token": ct })).await);
    assert_eq!(waiting["status"], "waiting");

    let second = ok(call(&rival, "end_turn", json!({ "seat_token": rt, "tick": 0 })).await);
    assert_eq!(second["tick_closed"], true);
    let next = ok(call(&crown, "wait_for_turn", json!({ "seat_token": ct })).await);
    assert_eq!(next["status"], "your_turn");
    assert_eq!(next["tick"], 1);
    assert!(next.to_string().contains("Queen Ada: Issue an edict"));
}

#[tokio::test]
async fn admin_endpoints_require_the_admin_token() {
    let s = Server::start().await;
    let anon = s.http.get(s.api("/overview")).send().await.unwrap();
    assert_eq!(anon.status(), 401);
    let seat = s
        .http
        .get(s.api("/overview"))
        .bearer_auth(s.tok("gm"))
        .send()
        .await
        .unwrap();
    assert_eq!(seat.status(), 401, "a seat token is not an admin token");
    assert_eq!(s.get("/overview").await.status(), 200);
}

#[tokio::test]
async fn an_intervention_changes_the_world_and_enters_the_log() {
    let s = Server::start().await;
    let r = s
        .post(
            "/intervene",
            json!({
                "title": "A dragon appears",
                "text": "It sleeps on the gold",
                "effects": [{
                    "target": { "metric": { "empire": "kingdom", "metric": "treasury" } },
                    "delta": -50
                }]
            }),
        )
        .await;
    assert_eq!(r.status(), 200);
    let seq = r.json::<Value>().await.unwrap()["seq"].as_i64().unwrap();

    let overview: Value = s.get("/overview").await.json().await.unwrap();
    assert_eq!(
        overview["state"]["empires"]["kingdom"]["metrics"]["treasury"],
        50.0
    );
    assert!(
        overview["chronicle"]
            .to_string()
            .contains("A dragon appears")
    );

    let page: Vec<LoggedEvent> = s
        .get(&format!("/events?after={}", seq - 1))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].seq, seq);
}

#[tokio::test]
async fn invalid_interventions_are_rejected_with_a_reason() {
    let s = Server::start().await;
    let r = s
        .post(
            "/intervene",
            json!({ "title": "Mana storm", "effects": [{
                "target": { "metric": { "empire": "kingdom", "metric": "mana" } },
                "delta": 1
            }] }),
        )
        .await;
    assert_eq!(r.status(), 422);
    assert!(r.text().await.unwrap().contains("unknown target"));
    let r = s
        .post(
            "/intervene",
            json!({ "title": "A storm", "empire": "atlantis" }),
        )
        .await;
    assert_eq!(r.status(), 422);
    assert!(r.text().await.unwrap().contains("unknown empire"));
}

#[tokio::test]
async fn an_intervention_can_be_announced_to_one_empire() {
    let s = Server::start().await;
    let r = s
        .post(
            "/intervene",
            json!({ "title": "A storm wrecks the fleet", "empire": "isles" }),
        )
        .await;
    assert_eq!(r.status(), 200);
    let pirates = s.mcp().await;
    let crown = s.mcp().await;
    let p = look(&pirates, &s.tok("pirates")).await;
    let c = look(&crown, &s.tok("crown")).await;
    assert!(p.to_string().contains("A storm wrecks the fleet"));
    assert!(!c.to_string().contains("A storm wrecks the fleet"));
}

#[tokio::test]
async fn an_intervention_introduces_two_empires_that_had_never_met() {
    let s = Server::of(common::THREE).await;
    let c = s.mcp().await;
    let token = s.tok("marsh");
    let marsh = || look(&c, &token);
    assert!(!marsh().await.to_string().contains("Lord Aren"));
    let r = s
        .post("/introduce", json!({ "a": "marsh", "b": "vale" }))
        .await;
    assert_eq!(r.status(), 200);
    let seen = marsh().await.to_string();
    assert!(seen.contains("Lord Aren"), "{seen}");
    assert!(
        seen.contains("The Marsh and The Vale have made contact."),
        "{seen}"
    );
    assert!(
        !look(&c, &s.tok("ridge"))
            .await
            .to_string()
            .contains("The Marsh")
    );
    let again = s
        .post("/introduce", json!({ "a": "vale", "b": "marsh" }))
        .await;
    assert_eq!(again.status(), 422);
    let body: Value = again.json().await.unwrap();
    assert_eq!(body["error"], "`vale` and `marsh` already know each other");
}

#[tokio::test]
async fn forcing_a_tick_advances_time_for_everyone() {
    let s = Server::start().await;
    let r: Value = s.post("/tick", json!({})).await.json().await.unwrap();
    assert_eq!(r["tick"], 1);
    let page: Vec<LoggedEvent> = s
        .get("/events?after=0&limit=1000")
        .await
        .json()
        .await
        .unwrap();
    assert!(page.windows(2).all(|w| w[0].seq < w[1].seq));
    assert!(
        page.iter()
            .any(|e| matches!(e.event.kind, sim_core::EventKind::TickEnded { .. }))
    );
}

#[tokio::test]
async fn notes_are_remembered_across_turns() {
    let s = Server::start().await;
    let c = s.mcp().await;
    let ct = s.tok("crown");
    ok(call(
        &c,
        "note",
        json!({ "seat_token": ct, "character": "queen", "text": "The duke lies" }),
    )
    .await);
    ok(call(&c, "end_turn", json!({ "seat_token": ct, "tick": 0 })).await);
    let next = ok(call(&c, "wait_for_turn", json!({ "seat_token": ct })).await);
    assert_eq!(
        next["perception"]["you_play"][0]["notes"],
        json!(["The duke lies"])
    );
    let foreign = call(
        &c,
        "note",
        json!({ "seat_token": ct, "character": "duke", "text": "x" }),
    )
    .await;
    assert_eq!(foreign.is_error, Some(true));
}

#[tokio::test]
async fn the_game_master_rules_on_a_secret_plan_over_mcp() {
    let s = Server::start().await;
    let (crown, rival, gm) = (s.mcp().await, s.mcp().await, s.mcp().await);
    let (ct, rt, gt) = (s.tok("crown"), s.tok("rival"), s.tok("gm"));
    ok(call(
        &crown,
        "act",
        json!({ "seat_token": ct, "tick": 0, "orders": [
            { "kind": "freeform", "character": "queen", "text": "Poison the duke" }
        ]}),
    )
    .await);
    s.post("/tick", json!({})).await;

    let rival_view = look(&rival, &rt).await;
    assert!(
        !rival_view.to_string().contains("Poison"),
        "plans stay secret from their target"
    );

    let gm_view = look(&gm, &gt).await;
    let pending = gm_view["perception"]["pending_rulings"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let ruled = ok(call(
        &gm,
        "rule",
        json!({ "seat_token": gt, "tick": 1, "rulings": [{
            "pending_id": pending,
            "outcome": "The duke falls ill",
            "effects": [{
                "target": { "metric": { "empire": "kingdom", "metric": "stability" } },
                "delta": -0.1
            }]
        }]}),
    )
    .await);
    assert_eq!(ruled["results"][0]["accepted"], true);
    let by_seat = call(
        &crown,
        "rule",
        json!({ "seat_token": ct, "tick": 1, "rulings": [{ "pending_id": pending, "outcome": "I win" }] }),
    )
    .await;
    assert_eq!(ok(by_seat)["results"][0]["accepted"], false);

    s.post("/tick", json!({})).await;
    let crown_view = look(&crown, &ct).await;
    assert!(crown_view.to_string().contains("«The duke falls ill»"));
    let rival_view = look(&rival, &rt).await;
    assert!(
        !rival_view.to_string().contains("falls ill"),
        "private ruling by default"
    );
}

#[tokio::test]
async fn a_head_of_state_appoints_over_mcp_and_the_appointee_gains_the_office() {
    let s = Server::start().await;
    let (crown, rival) = (s.mcp().await, s.mcp().await);
    let (ct, rt) = (s.tok("crown"), s.tok("rival"));
    let queen = look(&crown, &ct).await["perception"]["you_play"][0].clone();
    let appoint = queen["actions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == "appoint")
        .unwrap();
    assert_eq!(appoint["target"], "own_character");
    assert_eq!(appoint["choice"]["options"]["commander"], "Commander");

    let order = |choice: Value| json!({ "kind": "action", "character": "queen", "action": "appoint", "target": "duke", "choice": choice });
    let acted = ok(call(
        &crown,
        "act",
        json!({ "seat_token": ct, "tick": 0, "orders": [order(json!(null)), order(json!("commander"))] }),
    )
    .await);
    assert!(
        acted["results"][0]["error"]
            .as_str()
            .unwrap()
            .contains("needs a choice, one of: official, commander")
    );
    assert_eq!(acted["results"][1]["accepted"], true);
    s.post("/tick", json!({})).await;

    let duke = look(&rival, &rt).await["perception"].clone();
    assert_eq!(duke["you_play"][0]["office"], "Commander");
    assert_eq!(duke["you_play"][0]["actions"][0]["id"], "campaign");
    assert!(
        duke["recent"]
            .to_string()
            .contains("Duke Bors becomes Commander (formerly Official).")
    );
}
