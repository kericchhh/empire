use std::collections::BTreeMap;
use std::path::Path;
use std::str::FromStr;

use sim_core::{Event, SeatId};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error(transparent)]
    Sql(#[from] sqlx::Error),
    #[error("stored event {seq} is corrupt: {source}")]
    Corrupt { seq: i64, source: serde_json::Error },
    #[error("could not encode event: {0}")]
    Encode(serde_json::Error),
    #[error("database has no `{0}`; it was not created by this server")]
    Missing(&'static str),
}

/// One SQLite file per world: the scenario, seat tokens and the append-only event log.
#[derive(Clone)]
pub struct Store {
    pool: SqlitePool,
}

pub struct Saved {
    pub scenario_yaml: String,
    pub admin_token: String,
    pub seat_tokens: BTreeMap<SeatId, String>,
    pub events: Vec<Event>,
}

impl Store {
    pub async fn open(path: &Path) -> Result<Self, StoreError> {
        let opts = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal);
        Self::connect(opts).await
    }

    pub async fn in_memory() -> Result<Self, StoreError> {
        Self::connect(SqliteConnectOptions::from_str("sqlite::memory:")?).await
    }

    async fn connect(opts: SqliteConnectOptions) -> Result<Self, StoreError> {
        // One connection serialises writes, and keeps an in-memory database shared.
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await?;
        for ddl in [
            "CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
            "CREATE TABLE IF NOT EXISTS seats (seat TEXT PRIMARY KEY, token TEXT NOT NULL UNIQUE)",
            "CREATE TABLE IF NOT EXISTS events (seq INTEGER PRIMARY KEY, tick INTEGER NOT NULL, body TEXT NOT NULL)",
        ] {
            sqlx::query(ddl).execute(&pool).await?;
        }
        Ok(Self { pool })
    }

    /// Close the connection. Later operations fail with a storage error.
    pub async fn close(&self) {
        self.pool.close().await;
    }

    pub async fn load(&self) -> Result<Option<Saved>, StoreError> {
        let meta: BTreeMap<String, String> = sqlx::query("SELECT key, value FROM meta")
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(|r| (r.get("key"), r.get("value")))
            .collect();
        if meta.is_empty() {
            return Ok(None);
        }
        let get = |k: &'static str| meta.get(k).cloned().ok_or(StoreError::Missing(k));
        let seat_tokens = sqlx::query("SELECT seat, token FROM seats")
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(|r| (SeatId(r.get("seat")), r.get("token")))
            .collect();
        let events = self
            .events_after(0, i64::MAX)
            .await?
            .into_iter()
            .map(|(_, e)| e)
            .collect();
        Ok(Some(Saved {
            scenario_yaml: get("scenario")?,
            admin_token: get("admin_token")?,
            seat_tokens,
            events,
        }))
    }

    pub async fn create(
        &self,
        scenario_yaml: &str,
        admin_token: &str,
        seat_tokens: &BTreeMap<SeatId, String>,
    ) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await?;
        for (k, v) in [("scenario", scenario_yaml), ("admin_token", admin_token)] {
            sqlx::query("INSERT INTO meta (key, value) VALUES (?, ?)")
                .bind(k)
                .bind(v)
                .execute(&mut *tx)
                .await?;
        }
        for (seat, token) in seat_tokens {
            sqlx::query("INSERT INTO seats (seat, token) VALUES (?, ?)")
                .bind(seat.as_str())
                .bind(token)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Append atomically; returns the sequence number of the last event.
    pub async fn append(&self, events: &[Event]) -> Result<i64, StoreError> {
        let mut tx = self.pool.begin().await?;
        let mut last = 0;
        for e in events {
            let body = serde_json::to_string(e).map_err(StoreError::Encode)?;
            last = sqlx::query("INSERT INTO events (tick, body) VALUES (?, ?)")
                .bind(e.tick.0 as i64)
                .bind(body)
                .execute(&mut *tx)
                .await?
                .last_insert_rowid();
        }
        tx.commit().await?;
        Ok(last)
    }

    pub async fn events_after(
        &self,
        seq: i64,
        limit: i64,
    ) -> Result<Vec<(i64, Event)>, StoreError> {
        sqlx::query("SELECT seq, body FROM events WHERE seq > ? ORDER BY seq LIMIT ?")
            .bind(seq)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(|r| {
                let seq: i64 = r.get("seq");
                let body: String = r.get("body");
                serde_json::from_str(&body)
                    .map(|e| (seq, e))
                    .map_err(|source| StoreError::Corrupt { seq, source })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim_core::{Cause, EventKind, Tick, Visibility};

    fn ev(tick: u64) -> Event {
        Event {
            tick: Tick(tick),
            cause: Cause::System,
            visibility: Visibility::Public,
            kind: EventKind::TickStarted,
        }
    }

    #[tokio::test]
    async fn a_world_survives_reopening_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("world.db");
        let tokens = BTreeMap::from([(SeatId::from("crown"), "t1".to_owned())]);
        {
            let s = Store::open(&path).await.unwrap();
            assert!(s.load().await.unwrap().is_none());
            s.create("name: x", "admin", &tokens).await.unwrap();
            s.append(&[ev(0), ev(1)]).await.unwrap();
        }
        let saved = Store::open(&path)
            .await
            .unwrap()
            .load()
            .await
            .unwrap()
            .unwrap();
        assert_eq!(saved.scenario_yaml, "name: x");
        assert_eq!(saved.admin_token, "admin");
        assert_eq!(saved.seat_tokens, tokens);
        assert_eq!(saved.events, [ev(0), ev(1)]);
    }

    #[tokio::test]
    async fn events_are_paged_by_sequence() {
        let s = Store::in_memory().await.unwrap();
        let last = s.append(&[ev(0), ev(1), ev(2)]).await.unwrap();
        assert_eq!(last, 3);
        let page = s.events_after(1, 1).await.unwrap();
        assert_eq!(page, [(2, ev(1))]);
    }
}
