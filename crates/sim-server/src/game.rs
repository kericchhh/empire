use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Duration;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sim_core::engine::{self, close_tick};
use sim_core::perceive::{Sight, describe};
use sim_core::{
    CharacterId, DefinitionError, Effect, EmpireId, Event, Order, Perception, Rejection, Ruling,
    Scenario, SeatId, State, Submission, Tick, TurnInput, World, perceive,
};
use tokio::sync::{Mutex, watch};
use tokio::time::Instant;

use crate::store::{Store, StoreError};

#[derive(Debug, thiserror::Error)]
pub enum GameError {
    #[error("unknown seat token")]
    UnknownToken,
    #[error("you already ended your turn for this tick; call wait_for_turn")]
    TurnEnded,
    #[error("the world has moved on to tick {current}; call wait_for_turn for fresh perception")]
    StaleTick { current: Tick },
    #[error(transparent)]
    Rejected(#[from] Rejection),
    #[error("scenario: {0}")]
    Scenario(#[from] DefinitionError),
    #[error("storage: {0}")]
    Store(#[from] StoreError),
}

#[derive(Debug, Clone)]
pub struct Config {
    /// A tick closes this long after it opened, even if some seats have not ended their turn.
    pub tick_deadline: Duration,
    /// Advance on the deadline even when no seat has joined.
    pub autoplay: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TurnStatus {
    /// Submit orders, then call end_turn.
    YourTurn,
    /// You ended your turn; the tick has not closed yet. Call wait_for_turn again.
    Waiting,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TurnView {
    pub status: TurnStatus,
    pub tick: Tick,
    pub date: String,
    /// Present unless status is `waiting`.
    pub perception: Option<Perception>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OrderResult {
    pub index: usize,
    pub accepted: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EndTurnView {
    pub tick_closed: bool,
    /// Joined seats that have not ended their turn yet.
    pub waiting_for: Vec<SeatId>,
}

/// Full world for observers. Contains every secret, so it is only served to the admin.
#[derive(Debug, Clone, Serialize)]
pub struct Overview {
    pub world: World,
    pub state: State,
    pub date: String,
    pub last_seq: i64,
    pub ended_turn: Vec<SeatId>,
    pub chronicle: Vec<String>,
}

struct Inner {
    w: World,
    state: State,
    tokens: BTreeMap<String, SeatId>,
    queued: BTreeMap<SeatId, Vec<TurnInput>>,
    ended_turn: BTreeSet<SeatId>,
    tick_opened: Instant,
    last_seq: i64,
}

#[derive(Clone)]
pub struct Game {
    inner: Arc<Mutex<Inner>>,
    store: Store,
    config: Config,
    tick_tx: Arc<watch::Sender<Tick>>,
    events_tx: tokio::sync::broadcast::Sender<(i64, Event)>,
}

pub struct Created {
    pub game: Game,
    pub admin_token: String,
    pub seat_tokens: BTreeMap<SeatId, String>,
}

fn token() -> String {
    let bytes: [u8; 16] = rand::random();
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

impl Game {
    /// Resume the world stored in `store`, or create it from `scenario_yaml` if the store is empty.
    pub async fn open(
        store: Store,
        scenario_yaml: Option<&str>,
        seed: u64,
        config: Config,
    ) -> Result<Created, GameError> {
        let (w, admin_token, seat_tokens, events) = match store.load().await? {
            Some(saved) => (
                World::standard(Scenario::from_yaml(&saved.scenario_yaml)?)?,
                saved.admin_token,
                saved.seat_tokens,
                saved.events,
            ),
            None => {
                let yaml = scenario_yaml.ok_or(StoreError::Missing("scenario"))?;
                let w = World::standard(Scenario::from_yaml(yaml)?)?;
                let admin = token();
                let seats: BTreeMap<_, _> = w
                    .scenario
                    .seat_ids()
                    .map(|s| (s.clone(), token()))
                    .collect();
                store.create(yaml, &admin, &seats).await?;
                let events = engine::start(&w, seed);
                store.append(&events).await?;
                (w, admin, seats, events)
            }
        };
        let state = State::replay(&w, &events);
        let (tick_tx, _) = watch::channel(state.tick);
        let (events_tx, _) = tokio::sync::broadcast::channel(1024);
        let inner = Inner {
            tokens: seat_tokens
                .iter()
                .map(|(s, t)| (t.clone(), s.clone()))
                .collect(),
            w,
            state,
            queued: BTreeMap::new(),
            ended_turn: BTreeSet::new(),
            tick_opened: Instant::now(),
            last_seq: events.len() as i64,
        };
        Ok(Created {
            game: Game {
                inner: Arc::new(Mutex::new(inner)),
                store,
                config,
                tick_tx: Arc::new(tick_tx),
                events_tx,
            },
            admin_token,
            seat_tokens,
        })
    }

    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<(i64, Event)> {
        self.events_tx.subscribe()
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    /// Persist first, then apply, so memory never runs ahead of the log.
    async fn commit(&self, inner: &mut Inner, events: Vec<Event>) -> Result<(), GameError> {
        if events.is_empty() {
            return Ok(());
        }
        let last = self.store.append(&events).await?;
        let first = last - events.len() as i64 + 1;
        for (i, e) in events.into_iter().enumerate() {
            inner.state.apply(&e);
            let _ = self.events_tx.send((first + i as i64, e));
        }
        inner.last_seq = last;
        Ok(())
    }

    fn seat(inner: &Inner, token: &str) -> Result<SeatId, GameError> {
        inner
            .tokens
            .get(token)
            .cloned()
            .ok_or(GameError::UnknownToken)
    }

    fn view(inner: &Inner, seat: &SeatId) -> TurnView {
        let status = if inner.ended_turn.contains(seat) {
            TurnStatus::Waiting
        } else {
            TurnStatus::YourTurn
        };
        TurnView {
            status,
            tick: inner.state.tick,
            date: inner.w.date(inner.state.tick),
            perception: (status != TurnStatus::Waiting)
                .then(|| perceive(&inner.w, &inner.state, seat))
                .flatten(),
        }
    }

    pub async fn join(&self, token: &str) -> Result<TurnView, GameError> {
        let mut inner = self.inner.lock().await;
        let seat = Self::seat(&inner, token)?;
        if let Some(e) = engine::join(&inner.w, &inner.state, &seat)? {
            self.commit(&mut inner, vec![e]).await?;
        }
        Ok(Self::view(&inner, &seat))
    }

    pub async fn look(&self, token: &str) -> Result<TurnView, GameError> {
        let inner = self.inner.lock().await;
        let seat = Self::seat(&inner, token)?;
        let mut v = Self::view(&inner, &seat);
        v.perception = perceive(&inner.w, &inner.state, &seat);
        Ok(v)
    }

    /// Returns at once when it is the seat's turn, otherwise waits up to `max_wait` for the
    /// tick to close.
    pub async fn wait_for_turn(
        &self,
        token: &str,
        max_wait: Duration,
    ) -> Result<TurnView, GameError> {
        let deadline = Instant::now() + max_wait;
        let mut rx = self.tick_tx.subscribe();
        loop {
            {
                let inner = self.inner.lock().await;
                let seat = Self::seat(&inner, token)?;
                let v = Self::view(&inner, &seat);
                if v.status != TurnStatus::Waiting {
                    return Ok(v);
                }
                rx.mark_unchanged();
            }
            if tokio::time::timeout_at(deadline, rx.changed())
                .await
                .is_err()
            {
                let inner = self.inner.lock().await;
                let seat = Self::seat(&inner, token)?;
                return Ok(Self::view(&inner, &seat));
            }
        }
    }

    /// Orders, rulings and end_turn name the tick they were decided in, so a decision made
    /// on old perception never lands in a later tick.
    fn check_turn(inner: &Inner, seat: &SeatId, tick: Tick) -> Result<(), GameError> {
        if tick != inner.state.tick {
            return Err(GameError::StaleTick {
                current: inner.state.tick,
            });
        }
        if inner.ended_turn.contains(seat) {
            return Err(GameError::TurnEnded);
        }
        Ok(())
    }

    pub async fn act(
        &self,
        token: &str,
        tick: Tick,
        orders: Vec<Order>,
    ) -> Result<Vec<OrderResult>, GameError> {
        let mut inner = self.inner.lock().await;
        let seat = Self::seat(&inner, token)?;
        Self::check_turn(&inner, &seat, tick)?;
        let limit = inner.w.rules.limits.orders_per_character;
        let mut results = Vec::with_capacity(orders.len());
        for (index, order) in orders.into_iter().enumerate() {
            let used = inner
                .queued
                .get(&seat)
                .into_iter()
                .flatten()
                .filter(|i| matches!(&i.submission, Submission::Order(o) if o.character() == order.character()))
                .count();
            let check = if used >= limit {
                Err(format!(
                    "{} already has {limit} orders this tick",
                    order.character()
                ))
            } else {
                engine::check_order(&inner.w, &inner.state, &seat, &order)
                    .map_err(|e| e.to_string())
            };
            if check.is_ok() {
                inner
                    .queued
                    .entry(seat.clone())
                    .or_default()
                    .push(TurnInput {
                        seat: seat.clone(),
                        submission: Submission::Order(order),
                    });
            }
            results.push(OrderResult {
                index,
                accepted: check.is_ok(),
                error: check.err(),
            });
        }
        Ok(results)
    }

    pub async fn rule(
        &self,
        token: &str,
        tick: Tick,
        rulings: Vec<Ruling>,
    ) -> Result<Vec<OrderResult>, GameError> {
        let mut inner = self.inner.lock().await;
        let seat = Self::seat(&inner, token)?;
        Self::check_turn(&inner, &seat, tick)?;
        let mut results = Vec::with_capacity(rulings.len());
        for (index, ruling) in rulings.into_iter().enumerate() {
            let already = inner.queued.get(&seat).into_iter().flatten().any(
                |i| matches!(&i.submission, Submission::Ruling(r) if r.pending_id == ruling.pending_id),
            );
            let check = if already {
                Err(format!(
                    "`{}` already has a ruling this tick",
                    ruling.pending_id
                ))
            } else {
                engine::check_ruling(&inner.w, &inner.state, &seat, &ruling)
                    .map_err(|e| e.to_string())
            };
            if check.is_ok() {
                inner
                    .queued
                    .entry(seat.clone())
                    .or_default()
                    .push(TurnInput {
                        seat: seat.clone(),
                        submission: Submission::Ruling(ruling),
                    });
            }
            results.push(OrderResult {
                index,
                accepted: check.is_ok(),
                error: check.err(),
            });
        }
        Ok(results)
    }

    pub async fn message(
        &self,
        token: &str,
        from: &CharacterId,
        to: &CharacterId,
        text: &str,
    ) -> Result<(), GameError> {
        let mut inner = self.inner.lock().await;
        let seat = Self::seat(&inner, token)?;
        let e = engine::message(&inner.w, &inner.state, &seat, from, to, text)?;
        self.commit(&mut inner, vec![e]).await
    }

    pub async fn note(
        &self,
        token: &str,
        character: &CharacterId,
        text: &str,
    ) -> Result<(), GameError> {
        let mut inner = self.inner.lock().await;
        let seat = Self::seat(&inner, token)?;
        let e = engine::note(&inner.w, &inner.state, &seat, character, text)?;
        self.commit(&mut inner, vec![e]).await
    }

    pub async fn end_turn(&self, token: &str, tick: Tick) -> Result<EndTurnView, GameError> {
        let mut inner = self.inner.lock().await;
        let seat = Self::seat(&inner, token)?;
        Self::check_turn(&inner, &seat, tick)?;
        if !inner.state.joined.contains(&seat)
            && let Some(e) = engine::join(&inner.w, &inner.state, &seat)?
        {
            self.commit(&mut inner, vec![e]).await?;
        }
        inner.ended_turn.insert(seat.clone());
        let waiting_for = Self::waiting_for(&inner);
        let tick_closed = waiting_for.is_empty();
        if tick_closed && let Err(e) = self.close(&mut inner).await {
            inner.ended_turn.remove(&seat);
            return Err(e);
        }
        Ok(EndTurnView {
            tick_closed,
            waiting_for,
        })
    }

    fn waiting_for(inner: &Inner) -> Vec<SeatId> {
        inner
            .state
            .joined
            .iter()
            .filter(|s| !inner.ended_turn.contains(*s))
            .cloned()
            .collect()
    }

    async fn close(&self, inner: &mut Inner) -> Result<(), GameError> {
        let inputs: Vec<TurnInput> = inner.queued.values().flatten().cloned().collect();
        let events = close_tick(&inner.w, &inner.state, &inputs);
        self.commit(inner, events).await?;
        inner.queued.clear();
        inner.ended_turn.clear();
        inner.tick_opened = Instant::now();
        tracing::info!(tick = %inner.state.tick, "tick closed");
        self.tick_tx.send_replace(inner.state.tick);
        Ok(())
    }

    /// Close the current tick now, whoever is still deciding.
    pub async fn force_close(&self) -> Result<Tick, GameError> {
        let mut inner = self.inner.lock().await;
        self.close(&mut inner).await?;
        Ok(inner.state.tick)
    }

    /// Close the tick if its deadline has passed. Returns whether it closed.
    pub async fn close_if_due(&self) -> Result<bool, GameError> {
        let mut inner = self.inner.lock().await;
        let due = inner.tick_opened.elapsed() >= self.config.tick_deadline;
        let anyone = !inner.state.joined.is_empty() || self.config.autoplay;
        if !due || !anyone {
            return Ok(false);
        }
        self.close(&mut inner).await?;
        Ok(true)
    }

    pub async fn run_scheduler(self) {
        let mut every = tokio::time::interval(Duration::from_millis(500));
        loop {
            every.tick().await;
            if let Err(e) = self.close_if_due().await {
                tracing::error!(error = %e, "scheduled tick close failed");
            }
        }
    }

    /// A user-made event. With `audience`, only that empire's seats hear of it.
    pub async fn intervene(
        &self,
        title: &str,
        text: &str,
        effects: Vec<Effect>,
        audience: Option<&EmpireId>,
    ) -> Result<i64, GameError> {
        let mut inner = self.inner.lock().await;
        let e = engine::intervene(&inner.w, &inner.state, title, text, effects, audience)?;
        self.commit(&mut inner, vec![e]).await?;
        Ok(inner.last_seq)
    }

    pub async fn overview(&self) -> Overview {
        let inner = self.inner.lock().await;
        let chronicle = inner
            .state
            .recent
            .iter()
            .filter_map(|e| {
                describe(&inner.w, e, &Sight::all())
                    .map(|d| format!("[{}] {d}", inner.w.date(e.tick)))
            })
            .collect();
        Overview {
            world: inner.w.clone(),
            date: inner.w.date(inner.state.tick),
            state: inner.state.clone(),
            last_seq: inner.last_seq,
            ended_turn: inner.ended_turn.iter().cloned().collect(),
            chronicle,
        }
    }

    pub async fn world(&self) -> World {
        self.inner.lock().await.w.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim_core::{EventKind, Target};

    const TINY: &str = include_str!("../../sim-core/tests/fixtures/tiny.yaml");

    struct T {
        game: Game,
        tokens: BTreeMap<SeatId, String>,
    }

    impl T {
        async fn new(config: Config) -> Self {
            let store = Store::in_memory().await.unwrap();
            let c = Game::open(store, Some(TINY), 7, config).await.unwrap();
            Self {
                game: c.game,
                tokens: c.seat_tokens,
            }
        }

        async fn tiny() -> Self {
            Self::new(Config {
                tick_deadline: Duration::from_secs(3600),
                autoplay: false,
            })
            .await
        }

        fn tok(&self, seat: &str) -> &str {
            &self.tokens[&SeatId::from(seat)]
        }

        /// A kingdom metric. Tests use culture: in this fixture no standard rule moves it,
        /// so only the orders under test do.
        async fn culture(&self) -> f64 {
            let state = self.game.overview().await.state;
            state.metric(&"kingdom".into(), &"culture".into()).unwrap()
        }

        async fn tick(&self) -> Tick {
            self.game.overview().await.state.tick
        }
    }

    fn action(c: &str, a: &str, target: Option<&str>) -> Order {
        Order::Action {
            character: c.into(),
            action: a.into(),
            target: target.map(str::to_owned),
        }
    }

    #[tokio::test]
    async fn unknown_tokens_are_refused_everywhere() {
        let t = T::tiny().await;
        assert!(matches!(
            t.game.join("nope").await,
            Err(GameError::UnknownToken)
        ));
        assert!(matches!(
            t.game.look("nope").await,
            Err(GameError::UnknownToken)
        ));
        assert!(matches!(
            t.game.act("nope", Tick(0), vec![]).await,
            Err(GameError::UnknownToken)
        ));
        assert!(matches!(
            t.game.end_turn("nope", Tick(0)).await,
            Err(GameError::UnknownToken)
        ));
    }

    #[tokio::test]
    async fn join_returns_the_seats_perception() {
        let t = T::tiny().await;
        let v = t.game.join(t.tok("crown")).await.unwrap();
        assert_eq!(v.status, TurnStatus::YourTurn);
        let p = v.perception.unwrap();
        assert_eq!(p.you_play[0].id, CharacterId::from("queen"));
        assert!(
            p.recent
                .iter()
                .any(|l| l.contains("The Crown has taken their seat"))
        );
    }

    #[tokio::test]
    async fn the_tick_closes_only_when_every_joined_seat_has_ended_its_turn() {
        let t = T::tiny().await;
        t.game.join(t.tok("crown")).await.unwrap();
        t.game.join(t.tok("rival")).await.unwrap();

        let first = t.game.end_turn(t.tok("crown"), Tick(0)).await.unwrap();
        assert!(!first.tick_closed);
        assert_eq!(first.waiting_for, [SeatId::from("rival")]);
        assert_eq!(t.tick().await, Tick(0));

        let second = t.game.end_turn(t.tok("rival"), Tick(0)).await.unwrap();
        assert!(second.tick_closed);
        assert_eq!(t.tick().await, Tick(1));
        assert_eq!(
            t.game.look(t.tok("crown")).await.unwrap().status,
            TurnStatus::YourTurn
        );
    }

    #[tokio::test]
    async fn orders_are_validated_on_submission_and_resolved_at_close() {
        let t = T::tiny().await;
        let r = t
            .game
            .act(
                t.tok("crown"),
                Tick(0),
                vec![
                    action("queen", "public_works", Some("north")),
                    action("queen", "fly", None),
                    action("duke", "public_works", Some("north")),
                ],
            )
            .await
            .unwrap();
        let accepted: Vec<bool> = r.iter().map(|o| o.accepted).collect();
        assert_eq!(accepted, [true, false, false]);
        assert!(
            r[1].error
                .as_deref()
                .unwrap()
                .contains("cannot take action")
        );
        assert!(r[2].error.as_deref().unwrap().contains("does not control"));

        let before = t.culture().await;
        t.game.end_turn(t.tok("crown"), Tick(0)).await.unwrap();
        assert!(
            (t.culture().await - (before + 0.02)).abs() < 1e-9,
            "the works resolve at close"
        );
    }

    #[tokio::test]
    async fn queued_orders_count_toward_the_per_character_limit() {
        let t = T::tiny().await;
        let tok = t.tok("crown");
        t.game
            .act(tok, Tick(0), vec![action("queen", "issue_edict", None)])
            .await
            .unwrap();
        t.game
            .act(tok, Tick(0), vec![action("queen", "relief", Some("north"))])
            .await
            .unwrap();
        let third = t
            .game
            .act(tok, Tick(0), vec![action("queen", "issue_edict", None)])
            .await
            .unwrap();
        assert!(!third[0].accepted);
        assert!(
            third[0]
                .error
                .as_deref()
                .unwrap()
                .contains("already has 2 orders")
        );
    }

    #[tokio::test]
    async fn decisions_made_on_an_old_tick_are_refused() {
        let t = T::tiny().await;
        t.game.force_close().await.unwrap();
        let stale = t
            .game
            .act(
                t.tok("crown"),
                Tick(0),
                vec![action("queen", "issue_edict", None)],
            )
            .await;
        assert!(matches!(
            stale,
            Err(GameError::StaleTick { current: Tick(1) })
        ));
        assert!(matches!(
            t.game.end_turn(t.tok("crown"), Tick(0)).await,
            Err(GameError::StaleTick { .. })
        ));
        assert_eq!(
            t.game.look(t.tok("crown")).await.unwrap().status,
            TurnStatus::YourTurn
        );
        let fresh = t
            .game
            .act(
                t.tok("crown"),
                Tick(1),
                vec![action("queen", "issue_edict", None)],
            )
            .await;
        assert!(fresh.unwrap()[0].accepted);
    }

    #[tokio::test]
    async fn a_seat_cannot_act_after_ending_its_turn() {
        let t = T::tiny().await;
        t.game.join(t.tok("crown")).await.unwrap();
        t.game.join(t.tok("rival")).await.unwrap();
        t.game.end_turn(t.tok("crown"), Tick(0)).await.unwrap();
        assert!(matches!(
            t.game
                .act(
                    t.tok("crown"),
                    Tick(0),
                    vec![action("queen", "issue_edict", None)]
                )
                .await,
            Err(GameError::TurnEnded)
        ));
        assert_eq!(
            t.game.look(t.tok("crown")).await.unwrap().status,
            TurnStatus::Waiting
        );
    }

    #[tokio::test]
    async fn wait_for_turn_times_out_while_others_decide_and_wakes_when_the_tick_closes() {
        let t = T::tiny().await;
        t.game.join(t.tok("crown")).await.unwrap();
        t.game.join(t.tok("rival")).await.unwrap();
        t.game.end_turn(t.tok("crown"), Tick(0)).await.unwrap();

        let waited = t
            .game
            .wait_for_turn(t.tok("crown"), Duration::from_millis(30))
            .await
            .unwrap();
        assert_eq!(waited.status, TurnStatus::Waiting);
        assert!(waited.perception.is_none());

        let game = t.game.clone();
        let crown = t.tok("crown").to_owned();
        let waiter =
            tokio::spawn(async move { game.wait_for_turn(&crown, Duration::from_secs(10)).await });
        tokio::time::sleep(Duration::from_millis(20)).await;
        t.game.end_turn(t.tok("rival"), Tick(0)).await.unwrap();
        let woke = tokio::time::timeout(Duration::from_secs(2), waiter)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(woke.status, TurnStatus::YourTurn);
        assert_eq!(woke.tick, Tick(1));
    }

    #[tokio::test]
    async fn the_deadline_closes_a_tick_only_once_someone_plays_or_autoplay_is_on() {
        let instant = Config {
            tick_deadline: Duration::ZERO,
            autoplay: false,
        };
        let t = T::new(instant.clone()).await;
        assert!(
            !t.game.close_if_due().await.unwrap(),
            "empty world waits for players"
        );
        t.game.join(t.tok("folk")).await.unwrap();
        assert!(t.game.close_if_due().await.unwrap());
        assert_eq!(t.tick().await, Tick(1));

        let auto = T::new(Config {
            autoplay: true,
            ..instant
        })
        .await;
        assert!(auto.game.close_if_due().await.unwrap());

        let slow = T::tiny().await;
        slow.game.join(slow.tok("folk")).await.unwrap();
        assert!(!slow.game.close_if_due().await.unwrap());
    }

    #[tokio::test]
    async fn the_game_master_rules_on_freeform_actions_through_the_service() {
        let t = T::tiny().await;
        t.game
            .act(
                t.tok("crown"),
                Tick(0),
                vec![
                    Order::Freeform {
                        character: "queen".into(),
                        text: "I marry the duke".into(),
                    },
                    action("queen", "relief", Some("south")),
                ],
            )
            .await
            .unwrap();
        t.game.force_close().await.unwrap();

        let gm = t.game.look(t.tok("gm")).await.unwrap().perception.unwrap();
        let pending = &gm.pending_rulings[0];
        let ruling = Ruling {
            pending_id: pending.id.clone(),
            outcome: "A grand wedding".into(),
            public: true,
            effects: vec![Effect {
                target: Target::Metric {
                    empire: "kingdom".into(),
                    metric: "culture".into(),
                },
                delta: 0.2,
                scaled_by: None,
            }],
        };
        let first = t
            .game
            .rule(t.tok("gm"), Tick(1), vec![ruling.clone()])
            .await
            .unwrap();
        assert!(first[0].accepted);
        let dup = t
            .game
            .rule(t.tok("gm"), Tick(1), vec![ruling.clone()])
            .await
            .unwrap();
        assert!(!dup[0].accepted, "one ruling per pending action");
        let not_gm = t
            .game
            .rule(t.tok("crown"), Tick(1), vec![ruling])
            .await
            .unwrap();
        assert!(not_gm[0].error.as_deref().unwrap().contains("game-master"));

        let before = t.culture().await;
        t.game.force_close().await.unwrap();
        assert!((t.culture().await - (before + 0.2)).abs() < 1e-9);
    }

    #[tokio::test]
    async fn private_information_stays_with_its_seat() {
        let t = T::tiny().await;
        t.game
            .note(t.tok("crown"), &"queen".into(), "Poison the duke")
            .await
            .unwrap();
        t.game
            .message(
                t.tok("crown"),
                &"queen".into(),
                &"northfolk".into(),
                "Rise up",
            )
            .await
            .unwrap();
        let rival = serde_json::to_string(&t.game.look(t.tok("rival")).await.unwrap()).unwrap();
        assert!(!rival.contains("Poison"));
        assert!(!rival.contains("Rise up"));
        let folk = serde_json::to_string(&t.game.look(t.tok("folk")).await.unwrap()).unwrap();
        assert!(folk.contains("Rise up"));
        assert!(!folk.contains("Poison"));
    }

    #[tokio::test]
    async fn every_change_is_persisted_and_broadcast_in_log_order() {
        let t = T::tiny().await;
        let mut rx = t.game.subscribe();
        let seq = t
            .game
            .intervene(
                "A comet",
                "",
                vec![Effect {
                    target: Target::Metric {
                        empire: "kingdom".into(),
                        metric: "stability".into(),
                    },
                    delta: -0.1,
                    scaled_by: None,
                }],
                None,
            )
            .await
            .unwrap();
        let (got_seq, e) = rx.recv().await.unwrap();
        assert_eq!(got_seq, seq);
        assert!(matches!(e.kind, EventKind::Intervention { .. }));
        let stored = t.game.store().events_after(seq - 1, 1).await.unwrap();
        assert_eq!(stored, [(seq, e)]);
    }

    #[tokio::test]
    async fn a_reopened_world_resumes_where_it_stopped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("w.db");
        let cfg = Config {
            tick_deadline: Duration::from_secs(60),
            autoplay: false,
        };
        let (tokens, before) = {
            let c = Game::open(
                Store::open(&path).await.unwrap(),
                Some(TINY),
                7,
                cfg.clone(),
            )
            .await
            .unwrap();
            let crown = &c.seat_tokens[&SeatId::from("crown")];
            c.game
                .act(crown, Tick(0), vec![action("queen", "issue_edict", None)])
                .await
                .unwrap();
            c.game.force_close().await.unwrap();
            (c.seat_tokens, c.game.overview().await.state)
        };
        let c = Game::open(Store::open(&path).await.unwrap(), None, 0, cfg)
            .await
            .unwrap();
        assert_eq!(c.seat_tokens, tokens);
        assert_eq!(c.game.overview().await.state, before);
    }

    #[tokio::test]
    async fn a_failed_close_keeps_orders_and_lets_the_seat_retry() {
        let t = T::tiny().await;
        t.game.join(t.tok("crown")).await.unwrap();
        t.game.join(t.tok("rival")).await.unwrap();
        t.game
            .act(
                t.tok("crown"),
                Tick(0),
                vec![action("queen", "issue_edict", None)],
            )
            .await
            .unwrap();
        t.game.end_turn(t.tok("rival"), Tick(0)).await.unwrap();
        t.game.store().close().await;

        assert!(matches!(
            t.game.end_turn(t.tok("crown"), Tick(0)).await,
            Err(GameError::Store(_))
        ));
        assert_eq!(t.tick().await, Tick(0));
        assert_eq!(
            t.game.look(t.tok("crown")).await.unwrap().status,
            TurnStatus::YourTurn
        );
        let more = t
            .game
            .act(
                t.tok("crown"),
                Tick(0),
                vec![
                    action("queen", "issue_edict", None),
                    action("queen", "issue_edict", None),
                ],
            )
            .await
            .unwrap();
        assert_eq!(
            more.iter().map(|r| r.accepted).collect::<Vec<_>>(),
            [true, false],
            "the first decree is still queued"
        );
    }
}
