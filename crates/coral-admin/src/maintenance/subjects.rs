use std::collections::BTreeMap;
use std::time::Instant;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, TimeDelta, Utc};
use coral_redis::RedisPool;
use database::Database;
use reqlog::backfill::{DayNames, SliceOutcome, backfill_slice, ensure_subject_partition};
use reqlog::rollup::{Attribution, Span, floor_to, log_floor};
use serde::Serialize;
use serde_json::json;
use sqlx::FromRow;

use crate::coverage::BY_NAME_SHARE_KEY;
use crate::error::{AdminError, Context};
use crate::jobs::{Job, JobContext};
use crate::serde_ts;

pub const SUBJECTS_JOB: &str = "subjects";
pub const SLICE_SPAN: TimeDelta = TimeDelta::minutes(15);
pub const GAP_SETTLE: TimeDelta = TimeDelta::minutes(10);

const DAY: TimeDelta = TimeDelta::days(1);
const SUBJECTS_FEATURE: &str = "subjects";
const MOVED_BACK_NOTE: &str = "moved back by the subjects backfill";
const SHARE_DAY_FORMAT: &str = "%Y-%m-%d";
const SLICE_UNIT: &str = "slices";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BackfillPlan {
    spans: Vec<PlannedSpan>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
struct PlannedSpan {
    #[serde(serialize_with = "serde_ts::micros")]
    from: DateTime<Utc>,
    #[serde(serialize_with = "serde_ts::micros")]
    to: DateTime<Utc>,
    kind: SpanKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum SpanKind {
    BeforeCoverage,
    Gap,
}

pub struct SubjectsJob {
    plan: BackfillPlan,
}

#[derive(FromRow)]
struct Gap {
    from_ts: DateTime<Utc>,
    to_ts: DateTime<Utc>,
}

struct Run<'a> {
    ctx: &'a JobContext,
    attribution: Attribution,
    total: usize,
    done: usize,
    totals: SliceOutcome,
    days: BTreeMap<NaiveDate, SliceOutcome>,
}

impl BackfillPlan {
    pub async fn load(db: &Database, now: DateTime<Utc>) -> Result<Self, AdminError> {
        let mut conn = db.pool().acquire().await.context("acquire a connection")?;
        let log_from = log_floor(&mut conn)
            .await
            .context("read the oldest request log partition")?;
        let since: Option<DateTime<Utc>> =
            sqlx::query_scalar("SELECT since FROM api_coverage WHERE feature = $1")
                .bind(SUBJECTS_FEATURE)
                .fetch_optional(&mut *conn)
                .await
                .context("read the subjects coverage marker")?;
        let gaps: Vec<Gap> = sqlx::query_as(
            "SELECT from_ts, to_ts FROM api_coverage_gap
             WHERE feature = $1 AND to_ts <= $2 ORDER BY from_ts",
        )
        .bind(SUBJECTS_FEATURE)
        .bind(now - GAP_SETTLE)
        .fetch_all(&mut *conn)
        .await
        .context("read the subjects coverage gaps")?;
        Ok(Self::from_coverage(log_from, since, &gaps))
    }

    pub fn is_empty(&self) -> bool {
        self.spans.is_empty()
    }

    fn from_coverage(
        log_from: Option<DateTime<Utc>>,
        since: Option<DateTime<Utc>>,
        gaps: &[Gap],
    ) -> Self {
        let Some(log_from) = log_from else {
            return Self { spans: Vec::new() };
        };
        let before =
            since.and_then(|since| PlannedSpan::new(log_from, since, SpanKind::BeforeCoverage));
        let gaps = gaps.iter().filter_map(|gap| {
            PlannedSpan::new(gap.from_ts.max(log_from), gap.to_ts, SpanKind::Gap)
        });
        Self {
            spans: before.into_iter().chain(gaps).collect(),
        }
    }

    fn slice_count(&self) -> usize {
        self.spans.iter().map(|span| span.slices().count()).sum()
    }
}

impl PlannedSpan {
    fn new(from: DateTime<Utc>, to: DateTime<Utc>, kind: SpanKind) -> Option<Self> {
        Span::new(from, to).map(|_| Self { from, to, kind })
    }

    fn days(self) -> impl Iterator<Item = Span> {
        let end = self.to;
        let first = Span::new(self.from, next_midnight(self.from).min(end));
        std::iter::successors(first, move |day| {
            Span::new(day.to, next_midnight(day.to).min(end))
        })
    }

    fn slices(self) -> impl Iterator<Item = Span> {
        self.days().flat_map(|day| day.chunks(SLICE_SPAN))
    }
}

impl SubjectsJob {
    pub async fn planned(db: &Database, now: DateTime<Utc>) -> Result<Self, AdminError> {
        let plan = BackfillPlan::load(db, now).await?;
        if plan.is_empty() {
            return Err(AdminError::Rejected(
                "nothing to backfill: every retained request already has its subjects, or the writer has not recorded subjects coverage yet".into(),
            ));
        }
        Ok(Self { plan })
    }
}

#[async_trait]
impl Job for SubjectsJob {
    fn name(&self) -> &'static str {
        SUBJECTS_JOB
    }

    async fn run(&self, ctx: &JobContext) -> Result<(), AdminError> {
        let mut run = Run::start(ctx, self.plan.slice_count()).await?;
        for span in &self.plan.spans {
            run.backfill(*span).await?;
            record_coverage(ctx.db(), *span).await?;
        }
        prune_shares(ctx.db(), ctx.redis()).await
    }
}

impl<'a> Run<'a> {
    async fn start(ctx: &'a JobContext, total: usize) -> Result<Self, AdminError> {
        let mut conn = ctx
            .db()
            .pool()
            .acquire()
            .await
            .context("acquire a connection")?;
        let attribution = Attribution::recorded(&mut conn)
            .await
            .context("read the internal key prefix")?;
        Ok(Self {
            ctx,
            attribution,
            total,
            done: 0,
            totals: SliceOutcome::default(),
            days: BTreeMap::new(),
        })
    }

    async fn backfill(&mut self, span: PlannedSpan) -> Result<(), AdminError> {
        for day in span.days() {
            let date = day.from.date_naive();
            self.attach_partition(date).await?;
            let mut names = DayNames::default();
            for slice in day.chunks(SLICE_SPAN) {
                self.slice(&mut names, slice).await?;
            }
            self.record_share(date).await?;
        }
        Ok(())
    }

    async fn attach_partition(&self, day: NaiveDate) -> Result<(), AdminError> {
        self.ctx.wait_for_quiet_window().await?;
        ensure_subject_partition(self.ctx.db().pool(), day)
            .await
            .context(format!("attach the subject partition of {day}"))
    }

    async fn slice(&mut self, names: &mut DayNames, slice: Span) -> Result<(), AdminError> {
        self.ctx.wait_for_quiet_window().await?;
        let started = Instant::now();
        let mut redis = self.ctx.redis().connection();
        let mut tx = self.ctx.slice().await?;
        let outcome = backfill_slice(
            &mut tx,
            &mut redis,
            names,
            &self.attribution,
            slice.from,
            slice.to,
        )
        .await
        .context(format!(
            "backfill subjects from {}",
            serde_ts::format(&slice.from)
        ))?;
        tx.commit().await.context("commit a subjects slice")?;
        self.done += 1;
        self.totals += outcome;
        *self.days.entry(slice.from.date_naive()).or_default() += outcome;
        self.ctx.report(self.progress(slice)).await?;
        tokio::time::sleep(started.elapsed()).await;
        Ok(())
    }

    async fn record_share(&self, day: NaiveDate) -> Result<(), AdminError> {
        let Some(outcome) = self.days.get(&day) else {
            return Ok(());
        };
        redis::cmd("HSET")
            .arg(BY_NAME_SHARE_KEY)
            .arg(day.format(SHARE_DAY_FORMAT).to_string())
            .arg(outcome.by_name_share())
            .query_async::<()>(&mut self.ctx.redis().connection())
            .await
            .context(format!("record the by-name share of {day}"))
    }

    fn progress(&self, slice: Span) -> serde_json::Value {
        json!({
            "done": self.done,
            "total": self.total,
            "unit": SLICE_UNIT,
            "through": serde_ts::format(&slice.to),
            "rows": self.totals.rows,
            "subjects": self.totals.subjects,
            "by_name_share": self.totals.by_name_share(),
        })
    }
}

fn next_midnight(at: DateTime<Utc>) -> DateTime<Utc> {
    floor_to(at, DAY) + DAY
}

async fn record_coverage(db: &Database, span: PlannedSpan) -> Result<(), AdminError> {
    let query = match span.kind {
        SpanKind::BeforeCoverage => sqlx::query(
            "UPDATE api_coverage SET since = $2, note = $3 WHERE feature = $1 AND since > $2",
        )
        .bind(SUBJECTS_FEATURE)
        .bind(span.from)
        .bind(MOVED_BACK_NOTE),
        SpanKind::Gap => sqlx::query(
            "DELETE FROM api_coverage_gap WHERE feature = $1 AND from_ts <= $2 AND to_ts = $3",
        )
        .bind(SUBJECTS_FEATURE)
        .bind(span.from)
        .bind(span.to),
    };
    query
        .execute(db.pool())
        .await
        .context("record the backfilled subjects coverage")?;
    Ok(())
}

async fn prune_shares(db: &Database, redis: &RedisPool) -> Result<(), AdminError> {
    let mut conn = db.pool().acquire().await.context("acquire a connection")?;
    let Some(log_from) = log_floor(&mut conn)
        .await
        .context("read the oldest request log partition")?
    else {
        return Ok(());
    };
    let days: Vec<String> = redis::cmd("HKEYS")
        .arg(BY_NAME_SHARE_KEY)
        .query_async(&mut redis.connection())
        .await
        .context(format!("list {BY_NAME_SHARE_KEY}"))?;
    let expired = expired_days(&days, log_from.date_naive());
    if expired.is_empty() {
        return Ok(());
    }
    redis::cmd("HDEL")
        .arg(BY_NAME_SHARE_KEY)
        .arg(&expired)
        .query_async::<()>(&mut redis.connection())
        .await
        .context(format!("drop expired days from {BY_NAME_SHARE_KEY}"))
}

fn expired_days(days: &[String], oldest: NaiveDate) -> Vec<String> {
    days.iter()
        .filter(|day| {
            NaiveDate::parse_from_str(day, SHARE_DAY_FORMAT).is_ok_and(|parsed| parsed < oldest)
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn at(day: u32, hour: u32, minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, day, hour, minute, 0)
            .unwrap()
    }

    #[test]
    fn the_plan_covers_retained_days_before_the_marker_then_settled_gaps() {
        let gaps = [Gap {
            from_ts: at(14, 9, 0),
            to_ts: at(14, 9, 40),
        }];
        let plan = BackfillPlan::from_coverage(Some(at(1, 0, 0)), Some(at(3, 6, 10)), &gaps);
        assert_eq!(
            plan.spans,
            vec![
                PlannedSpan {
                    from: at(1, 0, 0),
                    to: at(3, 6, 10),
                    kind: SpanKind::BeforeCoverage
                },
                PlannedSpan {
                    from: at(14, 9, 0),
                    to: at(14, 9, 40),
                    kind: SpanKind::Gap
                },
            ]
        );
        assert_eq!(plan.slice_count(), 2 * 96 + 25 + 3);
    }

    #[test]
    fn nothing_is_planned_without_a_marker_or_a_log() {
        assert!(BackfillPlan::from_coverage(Some(at(1, 0, 0)), None, &[]).is_empty());
        assert!(BackfillPlan::from_coverage(None, Some(at(3, 0, 0)), &[]).is_empty());
        assert!(BackfillPlan::from_coverage(Some(at(3, 0, 0)), Some(at(3, 0, 0)), &[]).is_empty());
    }

    #[test]
    fn slices_never_cross_a_day() {
        let span = PlannedSpan {
            from: at(1, 23, 50),
            to: at(2, 0, 20),
            kind: SpanKind::Gap,
        };
        let slices: Vec<Span> = span.slices().collect();
        assert_eq!(slices.len(), 3);
        assert!(
            slices.iter().all(|slice| slice.from.date_naive()
                == (slice.to - TimeDelta::microseconds(1)).date_naive())
        );
    }

    #[test]
    fn shares_of_days_older_than_the_log_are_expired() {
        let days = [
            "2026-09-30".to_string(),
            "2026-10-01".to_string(),
            "junk".to_string(),
        ];
        let oldest = NaiveDate::from_ymd_opt(2026, 10, 1).unwrap();
        assert_eq!(expired_days(&days, oldest), vec!["2026-09-30".to_string()]);
    }
}
