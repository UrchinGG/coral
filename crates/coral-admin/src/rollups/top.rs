use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, TimeDelta, Utc};
use moka::future::Cache;
use reqlog::rollup::{Span, TOP_PLAYERS_INTERVAL, TopPlayer};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{serde_id, serde_ts};

pub const TOP_PLAYERS_LIMIT: i64 = 100;

const TOP_LIST_TTL: Duration = Duration::from_secs(2 * TOP_PLAYERS_INTERVAL.as_secs());

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TopWindow {
    #[serde(rename = "1h")]
    LastHour,
    #[serde(rename = "24h")]
    LastDay,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TopList {
    pub window: TopWindow,
    #[serde(serialize_with = "serde_ts::micros")]
    pub computed_at: DateTime<Utc>,
    pub players: Vec<RankedPlayer>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RankedPlayer {
    #[serde(serialize_with = "serde_id::uuid_undashed")]
    pub player: Uuid,
    pub requests: i64,
    #[serde(serialize_with = "serde_ts::micros")]
    pub last_seen: DateTime<Utc>,
}

#[derive(Clone)]
pub struct TopPlayers {
    lists: Cache<TopWindow, Arc<TopList>>,
}

impl TopWindow {
    pub const ALL: [Self; 2] = [Self::LastHour, Self::LastDay];

    pub fn length(self) -> TimeDelta {
        match self {
            Self::LastHour => TimeDelta::hours(1),
            Self::LastDay => TimeDelta::days(1),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::LastHour => "1h",
            Self::LastDay => "24h",
        }
    }

    pub fn ending_at(self, now: DateTime<Utc>) -> Span {
        Span {
            from: now - self.length(),
            to: now,
        }
    }
}

impl TopList {
    pub fn new(window: TopWindow, computed_at: DateTime<Utc>, players: Vec<TopPlayer>) -> Self {
        Self {
            window,
            computed_at,
            players: players
                .into_iter()
                .map(|ranked| RankedPlayer {
                    player: ranked.player,
                    requests: ranked.requests,
                    last_seen: ranked.last_seen,
                })
                .collect(),
        }
    }
}

impl TopPlayers {
    pub fn new() -> Self {
        Self {
            lists: Cache::builder()
                .max_capacity(TopWindow::ALL.len() as u64)
                .time_to_live(TOP_LIST_TTL)
                .build(),
        }
    }

    pub async fn get(&self, window: TopWindow) -> Option<Arc<TopList>> {
        self.lists.get(&window).await
    }

    pub async fn store(&self, list: TopList) {
        self.lists.insert(list.window, Arc::new(list)).await;
    }
}
