use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use chrono::{DateTime, TimeDelta, Utc};
use database::Database;
use reqlog::rollup::{
    self, Attribution, CALLER_PASS_INTERVAL, CALLER_WATERMARK, CATCH_UP_CHUNK, HOUR, MINUTE,
    NETWORK_PASS_INTERVAL, NETWORK_SETTLE, NETWORK_WATERMARK, PRUNE_BATCH, PRUNE_INTERVAL,
    REROLL_WINDOW, ROLLUP_TABLES, Rewritten, SETTLE, Span, TOP_PLAYERS_INTERVAL,
    TRAFFIC_PASS_INTERVAL, TRAFFIC_WATERMARK, floor_to,
};
use serde::Serialize;
use sqlx::{FromRow, PgConnection, Postgres, Transaction};

use crate::error::{AdminError, Context};
use crate::jobs::{JOB_SLICE_TIMEOUT, backfill_window_open};
use crate::query::begin_bounded;
use crate::serde_ts;

mod job;
mod recompute;
#[cfg(test)]
mod tests;
mod top;

pub use job::ROLLUP_JOB;
pub use recompute::{RECOMPUTE_JOB, RecomputeJob};
pub use top::{TOP_PLAYERS_LIMIT, TopList, TopPlayers, TopWindow};

pub const MAX_CHUNKS_PER_PASS: usize = 6;
pub const DIRTY_BATCH: i64 = 1_000;
pub const MAX_PRUNE_BATCHES: usize = 50;

#[derive(Clone)]
pub struct Rollups {
    inner: Arc<Inner>,
}

struct Inner {
    db: Arc<Database>,
    top: TopPlayers,
    running_here: AtomicBool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Pass {
    Traffic,
    Dirty,
    Callers,
    Network,
    TopPlayers,
    Prune,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Done,
    Behind,
    Idle,
    Locked,
    Waiting,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub outcome: Outcome,
    pub covered: Option<Span>,
    pub rewritten: Rewritten,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PassReport {
    #[serde(serialize_with = "serde_ts::micros")]
    pub at: DateTime<Utc>,
    pub duration_ms: u128,
    pub outcome: Outcome,
    #[serde(serialize_with = "serde_ts::micros_opt")]
    pub from: Option<DateTime<Utc>>,
    #[serde(serialize_with = "serde_ts::micros_opt")]
    pub to: Option<DateTime<Utc>>,
    pub written: i64,
    pub removed: i64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RollupState {
    #[serde(serialize_with = "serde_ts::micros_opt")]
    pub log_from: Option<DateTime<Utc>>,
    pub watermarks: Vec<Watermark>,
    pub dirty_ranges: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, FromRow)]
pub struct Watermark {
    pub name: String,
    #[serde(serialize_with = "serde_ts::micros")]
    pub at: DateTime<Utc>,
}

pub struct RunningHere {
    rollups: Rollups,
}

impl Pass {
    pub const ALL: [Self; 6] = [
        Self::Traffic,
        Self::Dirty,
        Self::Callers,
        Self::Network,
        Self::TopPlayers,
        Self::Prune,
    ];

    pub fn interval(self) -> Duration {
        match self {
            Self::Traffic | Self::Dirty => TRAFFIC_PASS_INTERVAL,
            Self::Callers => CALLER_PASS_INTERVAL,
            Self::Network => NETWORK_PASS_INTERVAL,
            Self::TopPlayers => TOP_PLAYERS_INTERVAL,
            Self::Prune => PRUNE_INTERVAL,
        }
    }
}

impl Progress {
    pub fn idle() -> Self {
        Self {
            outcome: Outcome::Idle,
            covered: None,
            rewritten: Rewritten::default(),
        }
    }

    fn covering(self, span: Span, rewritten: Rewritten) -> Self {
        Self {
            outcome: Outcome::Done,
            covered: Some(self.covered.map_or(span, |covered| covered.joined(span))),
            rewritten: self.rewritten + rewritten,
        }
    }

    fn counting(self, rewritten: Rewritten) -> Self {
        Self {
            outcome: Outcome::Done,
            rewritten: self.rewritten + rewritten,
            ..self
        }
    }

    fn locked(self) -> Self {
        Self {
            outcome: Outcome::Locked,
            ..self
        }
    }

    fn waiting() -> Self {
        Self {
            outcome: Outcome::Waiting,
            ..Self::idle()
        }
    }

    fn behind(self) -> Self {
        Self {
            outcome: Outcome::Behind,
            ..self
        }
    }

    fn reaching(self, end: DateTime<Utc>) -> Self {
        match self.covered {
            Some(covered) if covered.to < end => self.behind(),
            _ => self,
        }
    }
}

impl PassReport {
    fn new(at: DateTime<Utc>, took: Duration, result: Result<Progress, AdminError>) -> Self {
        let (progress, error) = match result {
            Ok(progress) => (progress, None),
            Err(error) => (
                Progress {
                    outcome: Outcome::Failed,
                    ..Progress::idle()
                },
                Some(error.to_string()),
            ),
        };
        Self {
            at,
            duration_ms: took.as_millis(),
            outcome: progress.outcome,
            from: progress.covered.map(|span| span.from),
            to: progress.covered.map(|span| span.to),
            written: progress.rewritten.written,
            removed: progress.rewritten.removed,
            error,
        }
    }
}

impl Rollups {
    pub fn new(db: Arc<Database>) -> Self {
        Self {
            inner: Arc::new(Inner {
                db,
                top: TopPlayers::new(),
                running_here: AtomicBool::new(false),
            }),
        }
    }

    pub async fn attribution(&self) -> Result<Attribution, AdminError> {
        let mut conn = self.connection().await?;
        let attribution = Attribution::recorded(&mut conn)
            .await
            .context("read the internal key prefix from recorded requests")?;
        if attribution.internal_prefix().is_none() {
            tracing::warn!(
                "no request has been recorded with the internal key; legacy internal requests roll up as invalid keys"
            );
        }
        Ok(attribution)
    }

    pub fn top_players(&self) -> &TopPlayers {
        &self.inner.top
    }

    pub fn is_running_here(&self) -> bool {
        self.inner.running_here.load(Ordering::Acquire)
    }

    pub fn claim_here(&self) -> RunningHere {
        self.inner.running_here.store(true, Ordering::Release);
        RunningHere {
            rollups: self.clone(),
        }
    }

    pub async fn run_pass(
        &self,
        pass: Pass,
        now: DateTime<Utc>,
        attribution: &Attribution,
    ) -> PassReport {
        let started = Instant::now();
        let result = match pass {
            Pass::Traffic => self.traffic(now, attribution).await,
            Pass::Dirty => self.dirty(attribution).await,
            Pass::Callers => self.callers(now, attribution).await,
            Pass::Network => self.network(now).await,
            Pass::TopPlayers => self.rank_players(now).await,
            Pass::Prune => self.prune(now).await,
        };
        if let Err(error) = &result {
            tracing::error!(?pass, %error, "rollup pass failed; it runs again next interval");
        }
        PassReport::new(now, started.elapsed(), result)
    }

    pub async fn recompute(
        &self,
        span: Span,
        attribution: &Attribution,
    ) -> Result<Rewritten, AdminError> {
        let mut slice = begin_bounded(self.inner.db.pool(), JOB_SLICE_TIMEOUT).await?;
        rollup::wait_for_lock(&mut slice)
            .await
            .context("wait for the rollup lock")?;
        let rewritten = roll_span(&mut slice, span, attribution).await?;
        slice.commit().await.context("commit a recomputed range")?;
        Ok(rewritten)
    }

    pub async fn recompute_span(
        &self,
        db: &Database,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<Span, AdminError> {
        if to > now {
            return Err(AdminError::BadRequest(
                "to must not be in the future".into(),
            ));
        }
        let span = Span::new(from, to)
            .ok_or_else(|| AdminError::BadRequest("from must be before to".into()))?
            .aligned(HOUR);
        let mut conn = db.pool().acquire().await.context("acquire a connection")?;
        let floor = rollup::log_floor(&mut conn)
            .await
            .context("read the oldest request log partition")?
            .ok_or_else(|| AdminError::Rejected("the request log has no partitions".into()))?;
        if span.from < floor {
            return Err(AdminError::BadRequest(format!(
                "rollups can be recomputed from {} onward, the start of the oldest retained request log",
                serde_ts::format(&floor)
            )));
        }
        Ok(span)
    }

    pub async fn state(db: &Database) -> Result<RollupState, AdminError> {
        let mut conn = db.pool().acquire().await.context("acquire a connection")?;
        let log_from = rollup::log_floor(&mut conn)
            .await
            .context("read the oldest request log partition")?;
        let watermarks =
            sqlx::query_as("SELECT name, watermark AS at FROM api_rollup_state ORDER BY name")
                .fetch_all(&mut *conn)
                .await
                .context("read the rollup watermarks")?;
        let dirty_ranges = sqlx::query_scalar("SELECT count(*) FROM api_rollup_dirty")
            .fetch_one(&mut *conn)
            .await
            .context("count the dirty rollup ranges")?;
        Ok(RollupState {
            log_from,
            watermarks,
            dirty_ranges,
        })
    }

    async fn traffic(
        &self,
        now: DateTime<Utc>,
        attribution: &Attribution,
    ) -> Result<Progress, AdminError> {
        let end = floor_to(now - SETTLE, MINUTE);
        let Some(pending) = self.pending(TRAFFIC_WATERMARK, REROLL_WINDOW, end).await? else {
            return Ok(Progress::idle());
        };
        if self.backfill_must_wait(pending, now).await? {
            return Ok(Progress::waiting());
        }
        let mut progress = Progress::idle();
        for chunk in pending.chunks(CATCH_UP_CHUNK).take(MAX_CHUNKS_PER_PASS) {
            let started = Instant::now();
            let Some(mut slice) = self.locked_slice().await? else {
                return Ok(progress.locked());
            };
            let rolled = rollup::roll_traffic(&mut slice, chunk, attribution)
                .await
                .context("roll up traffic")?;
            rollup::set_watermark(&mut slice, TRAFFIC_WATERMARK, chunk.to)
                .await
                .context("advance the traffic watermark")?;
            slice.commit().await.context("commit a traffic rollup")?;
            progress = progress.covering(chunk, rewritten(rolled));
            pause_while_behind(pending, chunk, started).await;
        }
        Ok(progress.reaching(end))
    }

    async fn callers(
        &self,
        now: DateTime<Utc>,
        attribution: &Attribution,
    ) -> Result<Progress, AdminError> {
        let settled = floor_to(now - REROLL_WINDOW, HOUR);
        let end = floor_to(now, HOUR) + HOUR;
        let Some(pending) = self
            .pending(CALLER_WATERMARK, TimeDelta::zero(), end)
            .await?
        else {
            return Ok(Progress::idle());
        };
        let plan = Span {
            from: pending.from.min(settled),
            to: end,
        }
        .aligned(HOUR);
        if self.backfill_must_wait(plan, now).await? {
            return Ok(Progress::waiting());
        }
        let mut progress = Progress::idle();
        for chunk in plan.chunks(HOUR).take(MAX_CHUNKS_PER_PASS) {
            let started = Instant::now();
            let Some(mut slice) = self.locked_slice().await? else {
                return Ok(progress.locked());
            };
            let rolled = rollup::roll_callers(&mut slice, chunk, attribution)
                .await
                .context("roll up callers")?;
            rollup::set_watermark(&mut slice, CALLER_WATERMARK, chunk.to.min(settled))
                .await
                .context("advance the caller watermark")?;
            slice.commit().await.context("commit a caller rollup")?;
            progress = progress.covering(chunk, rewritten(rolled));
            pause_while_behind(plan, chunk, started).await;
        }
        let progress = progress.reaching(end);
        if progress.outcome == Outcome::Done {
            self.record_coverage_once_caught_up(settled).await?;
        }
        Ok(progress)
    }

    async fn network(&self, now: DateTime<Utc>) -> Result<Progress, AdminError> {
        let until = now - NETWORK_SETTLE;
        let Some(pending) = self
            .pending(NETWORK_WATERMARK, TimeDelta::zero(), until)
            .await?
        else {
            return Ok(Progress::idle());
        };
        if self.backfill_must_wait(pending, now).await? {
            return Ok(Progress::waiting());
        }
        let mut progress = Progress::idle();
        for _ in 0..MAX_CHUNKS_PER_PASS {
            let started = Instant::now();
            let Some(mut slice) = self.locked_slice().await? else {
                return Ok(progress.locked());
            };
            let advanced = rollup::advance_network(&mut slice, until)
                .await
                .context("fold request rows into the network tables")?;
            slice.commit().await.context("commit a network fold")?;
            let Some(span) = advanced else {
                break;
            };
            progress = progress.covering(span, Rewritten::default());
            if span.to >= until {
                break;
            }
            tokio::time::sleep(started.elapsed()).await;
        }
        Ok(progress.reaching(until))
    }

    async fn dirty(&self, attribution: &Attribution) -> Result<Progress, AdminError> {
        let mut conn = self.connection().await?;
        let ranges = rollup::dirty_ranges(&mut conn, DIRTY_BATCH)
            .await
            .context("read the dirty rollup ranges")?;
        drop(conn);
        let mut progress = Progress::idle();
        for range in ranges.iter().take(MAX_CHUNKS_PER_PASS) {
            let Some(mut slice) = self.locked_slice().await? else {
                return Ok(progress.locked());
            };
            let rewritten = roll_span(&mut slice, range.span, attribution).await?;
            rollup::clear_dirty(&mut slice, &range.ids)
                .await
                .context("clear the dirty rollup ranges")?;
            slice.commit().await.context("commit a dirty range")?;
            progress = progress.covering(range.span, rewritten);
        }
        let read: usize = ranges.iter().map(|range| range.ids.len()).sum();
        let more_queued = ranges.len() > MAX_CHUNKS_PER_PASS || read as i64 == DIRTY_BATCH;
        match more_queued {
            true => Ok(progress.behind()),
            false => Ok(progress),
        }
    }

    async fn rank_players(&self, now: DateTime<Utc>) -> Result<Progress, AdminError> {
        let mut progress = Progress::idle();
        for window in TopWindow::ALL {
            let span = window.ending_at(now);
            let mut slice = begin_bounded(self.inner.db.pool(), JOB_SLICE_TIMEOUT).await?;
            let players = rollup::top_players(&mut slice, span, TOP_PLAYERS_LIMIT)
                .await
                .context(format!(
                    "rank the players looked up in the last {}",
                    window.label()
                ))?;
            slice.commit().await.context("end the player ranking")?;
            let ranked = Rewritten {
                written: players.len() as i64,
                removed: 0,
            };
            self.inner
                .top
                .store(TopList::new(window, now, players))
                .await;
            progress = progress.covering(span, ranked);
        }
        Ok(progress)
    }

    async fn prune(&self, now: DateTime<Utc>) -> Result<Progress, AdminError> {
        let mut progress = Progress::idle();
        for table in ROLLUP_TABLES {
            for _ in 0..MAX_PRUNE_BATCHES {
                let Some(mut slice) = self.locked_slice().await? else {
                    return Ok(progress.locked());
                };
                let pruned = rollup::prune(&mut slice, table, now)
                    .await
                    .context(format!("prune {}", table.name))?;
                slice.commit().await.context("commit a prune batch")?;
                progress = progress.counting(Rewritten {
                    written: 0,
                    removed: pruned as i64,
                });
                if pruned < PRUNE_BATCH as u64 {
                    break;
                }
            }
        }
        Ok(progress)
    }

    async fn pending(
        &self,
        watermark: &str,
        reroll: TimeDelta,
        end: DateTime<Utc>,
    ) -> Result<Option<Span>, AdminError> {
        let mut conn = self.connection().await?;
        let stored = rollup::watermark(&mut conn, watermark)
            .await
            .context(format!("read the {watermark} watermark"))?;
        let start = match stored {
            Some(at) => at - reroll,
            None => {
                let floor = rollup::log_floor(&mut conn)
                    .await
                    .context("read the oldest request log partition")?;
                let Some(floor) = floor else {
                    return Ok(None);
                };
                floor
            }
        };
        Ok(Span::new(start, end))
    }

    async fn backfill_must_wait(
        &self,
        pending: Span,
        now: DateTime<Utc>,
    ) -> Result<bool, AdminError> {
        if pending.to - pending.from <= catch_up_budget() {
            return Ok(false);
        }
        Ok(!backfill_window_open(&self.inner.db, now).await?)
    }

    async fn record_coverage_once_caught_up(
        &self,
        settled: DateTime<Utc>,
    ) -> Result<(), AdminError> {
        let mut conn = self.connection().await?;
        rollup::record_caught_up_coverage(&mut conn, settled)
            .await
            .context("record where rollups begin")
    }

    async fn locked_slice(&self) -> Result<Option<Transaction<'static, Postgres>>, AdminError> {
        let mut slice = begin_bounded(self.inner.db.pool(), JOB_SLICE_TIMEOUT).await?;
        let taken = rollup::take_lock(&mut slice)
            .await
            .context("take the rollup lock")?;
        Ok(taken.then_some(slice))
    }

    async fn connection(&self) -> Result<sqlx::pool::PoolConnection<Postgres>, AdminError> {
        self.inner
            .db
            .pool()
            .acquire()
            .await
            .context("acquire a rollup connection")
    }
}

impl Drop for RunningHere {
    fn drop(&mut self) {
        self.rollups
            .inner
            .running_here
            .store(false, Ordering::Release);
    }
}

async fn roll_span(
    slice: &mut PgConnection,
    span: Span,
    attribution: &Attribution,
) -> Result<Rewritten, AdminError> {
    let traffic = rollup::roll_traffic(slice, span, attribution)
        .await
        .context("roll up traffic")?;
    let callers = rollup::roll_callers(slice, span, attribution)
        .await
        .context("roll up callers")?;
    if traffic.is_none() {
        tracing::warn!(
            from = %span.from,
            to = %span.to,
            "range is older than the retained request log; its rollups stay as they are"
        );
    }
    Ok(rewritten(traffic) + rewritten(callers))
}

fn rewritten(rolled: Option<rollup::Rolled>) -> Rewritten {
    rolled.map_or_else(Rewritten::default, |rolled| rolled.rewritten)
}

fn catch_up_budget() -> TimeDelta {
    CATCH_UP_CHUNK * MAX_CHUNKS_PER_PASS as i32
}

async fn pause_while_behind(plan: Span, chunk: Span, started: Instant) {
    if chunk.to < plan.to {
        tokio::time::sleep(started.elapsed()).await;
    }
}
