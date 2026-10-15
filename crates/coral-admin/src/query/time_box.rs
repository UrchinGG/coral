use std::time::{Duration, Instant};

use chrono::{DateTime, TimeDelta, Utc};
use moka::future::Cache;

use super::cursor::Cursor;
use super::filter::{Bounds, Condition};
use super::row::LogRow;
use super::{LogPage, MemberScope, Plan, Source};
use crate::error::AdminError;

pub const TIME_BOX_BUDGET: Duration = Duration::from_millis(400);
pub const FIRST_SLICE_SPAN: TimeDelta = TimeDelta::hours(1);
pub const MIN_SLICE_SPAN: TimeDelta = TimeDelta::minutes(1);
pub const MAX_SLICE_SPAN: TimeDelta = TimeDelta::days(1);
pub const MIN_SLICE_BUDGET: Duration = Duration::from_millis(20);
pub const SLICE_HEADROOM: f64 = 0.8;
pub const OPENING_SLICE_SHARE: f64 = 0.4;
pub const TIMEOUT_BACKOFF: f64 = 4.0;
pub const SCAN_RATES_CAPACITY: u64 = 1_000;
pub const SCAN_RATES_TTL: Duration = Duration::from_secs(10 * 60);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ScanKey {
    source: Source,
    conditions: Vec<Condition>,
    member: Option<MemberScope>,
    include_display: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScanRate {
    log_time_per_elapsed: f64,
}

#[derive(Clone)]
pub struct ScanRates {
    learnt: Cache<ScanKey, ScanRate>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Slice {
    pub lo: DateTime<Utc>,
    pub hi: Option<DateTime<Utc>>,
    pub budget: Duration,
}

#[derive(Debug)]
pub enum SliceOutcome {
    Scanned(Vec<LogRow>),
    Canceled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScanState {
    Scanning,
    Finished,
    OutOfTime,
    Stalled,
}

pub struct Scan {
    started: Instant,
    top: DateTime<Utc>,
    edge: DateTime<Utc>,
    upper: Option<DateTime<Utc>>,
    floor: Option<DateTime<Utc>>,
    limit: usize,
    resume: Cursor,
    rows: Vec<LogRow>,
    rate: Option<ScanRate>,
    state: ScanState,
}

impl ScanKey {
    pub(super) fn of(plan: &Plan) -> Self {
        Self {
            source: plan.source.clone(),
            conditions: plan.conditions.clone(),
            member: plan.member.clone(),
            include_display: plan.include_display,
        }
    }
}

impl ScanRate {
    pub fn measured(span: TimeDelta, took: Duration) -> Self {
        Self {
            log_time_per_elapsed: span.as_seconds_f64() / took.as_secs_f64(),
        }
    }

    pub fn span_within(self, budget: Duration) -> TimeDelta {
        let seconds = (self.log_time_per_elapsed * budget.as_secs_f64()).clamp(
            MIN_SLICE_SPAN.as_seconds_f64(),
            MAX_SLICE_SPAN.as_seconds_f64(),
        );
        TimeDelta::from_std(Duration::from_secs_f64(seconds))
            .expect("a slice span fits in a TimeDelta")
    }

    fn slowest(self, other: Self) -> Self {
        Self {
            log_time_per_elapsed: self.log_time_per_elapsed.min(other.log_time_per_elapsed),
        }
    }

    fn backed_off(self) -> Self {
        Self {
            log_time_per_elapsed: self.log_time_per_elapsed / TIMEOUT_BACKOFF,
        }
    }
}

impl ScanRates {
    pub fn new() -> Self {
        Self {
            learnt: Cache::builder()
                .max_capacity(SCAN_RATES_CAPACITY)
                .time_to_live(SCAN_RATES_TTL)
                .build(),
        }
    }

    pub async fn get(&self, key: &ScanKey) -> Option<ScanRate> {
        self.learnt.get(key).await
    }

    pub async fn remember(&self, key: ScanKey, rate: ScanRate) {
        self.learnt.insert(key, rate).await;
    }
}

impl Slice {
    pub fn bounds(&self) -> Bounds {
        Bounds {
            lo: Some(self.lo),
            hi: self.hi,
        }
    }
}

impl Scan {
    pub fn new(plan: &Plan, now: DateTime<Utc>, rate: Option<ScanRate>) -> Self {
        let top = plan.top(now);
        Self {
            started: Instant::now(),
            top,
            edge: top,
            upper: plan.ceiling,
            floor: plan.floor,
            limit: plan.limit,
            resume: plan.resume(top),
            rows: Vec::new(),
            rate,
            state: ScanState::Scanning,
        }
    }

    pub fn next_slice(&self) -> Option<Slice> {
        if self.state != ScanState::Scanning || self.rows.len() > self.limit {
            return None;
        }
        let budget = TIME_BOX_BUDGET.saturating_sub(self.started.elapsed());
        if budget < MIN_SLICE_BUDGET {
            return None;
        }
        let reach = self.edge - self.span_within(budget);
        Some(Slice {
            lo: self.floor.map_or(reach, |floor| floor.max(reach)),
            hi: self.upper,
            budget,
        })
    }

    pub fn found(&self) -> usize {
        self.rows.len()
    }

    pub fn rate(&self) -> Option<ScanRate> {
        self.rate
    }

    pub fn record(&mut self, slice: Slice, outcome: SliceOutcome, took: Duration) {
        let span = self.edge - slice.lo;
        match outcome {
            SliceOutcome::Scanned(rows) => self.advance(slice.lo, rows, span, took),
            SliceOutcome::Canceled => self.stop_after_cancel(span, took),
        }
    }

    pub fn into_page(self, plan: &Plan) -> Result<LogPage, AdminError> {
        match self.state {
            ScanState::Stalled => Err(AdminError::Timeout(format!(
                "a {}-minute slice of the request log did not finish within {} ms",
                MIN_SLICE_SPAN.num_minutes(),
                TIME_BOX_BUDGET.as_millis()
            ))),
            ScanState::Finished => Ok(plan.page(self.rows, None)),
            ScanState::Scanning | ScanState::OutOfTime => {
                Ok(plan.page(self.rows, Some(self.resume)))
            }
        }
    }

    fn span_within(&self, budget: Duration) -> TimeDelta {
        let Some(rate) = self.rate else {
            return FIRST_SLICE_SPAN;
        };
        let share = match self.progressed() {
            true => SLICE_HEADROOM,
            false => OPENING_SLICE_SHARE,
        };
        rate.span_within(budget.mul_f64(share))
    }

    fn progressed(&self) -> bool {
        self.edge < self.top
    }

    fn advance(&mut self, lo: DateTime<Utc>, rows: Vec<LogRow>, span: TimeDelta, took: Duration) {
        self.rows.extend(rows);
        self.resume = Cursor::before(lo);
        self.edge = lo;
        self.upper = Some(lo);
        if self.floor.is_some_and(|floor| lo <= floor) {
            self.state = ScanState::Finished;
            return;
        }
        if self.rows.len() <= self.limit {
            self.rate = Some(ScanRate::measured(span, took));
        }
    }

    fn stop_after_cancel(&mut self, span: TimeDelta, took: Duration) {
        let measured = ScanRate::measured(span, took);
        let slowest = self.rate.map_or(measured, |rate| rate.slowest(measured));
        self.rate = Some(slowest.backed_off());
        self.state = match self.progressed() || span > MIN_SLICE_SPAN {
            true => ScanState::OutOfTime,
            false => ScanState::Stalled,
        };
    }
}
