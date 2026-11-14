use std::sync::Arc;
use std::time::Instant;

use chrono::{DateTime, Utc};
use database::Database;
use reqlog::KEY_PREFIX_CHARS;
use sqlx::{Acquire, FromRow};

use crate::coverage::Coverage;
use crate::error::{AdminError, Context, is_statement_timeout};

mod attribution;
mod bounded;
mod cursor;
mod filter;
mod players;
mod row;
mod sql;
mod time_box;

pub use attribution::Attribution;
pub use bounded::{SearchLimits, begin_bounded};
pub use bounded::{acquire_within, bound_statements};
pub use cursor::Cursor;
pub use filter::{Bounds, Condition, Keyset, RequestFilter, Selection, StatusClass};
pub use players::{HeldName, cached_identities};
pub use players::{PlayerInput, PlayerKeys, PlayerSearch};
pub use row::LogRow;
pub use sql::{REQUEST_KEY, log_row_select};
pub use time_box::ScanRates;

use sql::{Era, EraRange, MemberArm};
use time_box::{Scan, ScanKey, Slice, SliceOutcome};

pub struct LogQuery<'a> {
    db: &'a Database,
    coverage: &'a Coverage,
    players: &'a PlayerSearch,
    rates: &'a ScanRates,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogPage {
    pub rows: Vec<LogRow>,
    pub next_cursor: Option<Cursor>,
    pub scanned_to: Option<DateTime<Utc>>,
    pub clamped_from: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, FromRow)]
pub struct MemberIdentity {
    pub id: i64,
    pub discord_id: Option<i64>,
    pub prefixes: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum MemberRole {
    Driver,
    Residual,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct MemberScope {
    identity: MemberIdentity,
    role: MemberRole,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Source {
    Log,
    Subjects(Arc<PlayerKeys>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Direct,
    TimeBoxed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Plan {
    source: Source,
    conditions: Vec<Condition>,
    member: Option<MemberScope>,
    eras: Vec<EraRange>,
    keyset: Keyset,
    limit: usize,
    include_display: bool,
    floor: Option<DateTime<Utc>>,
    ceiling: Option<DateTime<Utc>>,
    mode: Mode,
    clamped_from: Option<DateTime<Utc>>,
}

impl<'a> LogQuery<'a> {
    pub fn new(
        db: &'a Database,
        coverage: &'a Coverage,
        players: &'a PlayerSearch,
        rates: &'a ScanRates,
    ) -> Self {
        Self {
            db,
            coverage,
            players,
            rates,
        }
    }

    pub async fn run(&self, filter: RequestFilter) -> Result<LogPage, AdminError> {
        let plan = self.plan(Selection::parse(filter)?).await?;
        match plan.mode {
            Mode::Direct => self.direct(&plan).await,
            Mode::TimeBoxed => self.time_boxed(&plan, Utc::now()).await,
        }
    }

    async fn plan(&self, selection: Selection) -> Result<Plan, AdminError> {
        let source = match &selection.player {
            Some(input) => Source::Subjects(self.players.keys(input).await?),
            None => Source::Log,
        };
        let member = match selection.member {
            Some(id) => Some(MemberScope {
                identity: member_identity(self.db, id).await?,
                role: match source {
                    Source::Log => MemberRole::Driver,
                    Source::Subjects(_) => MemberRole::Residual,
                },
            }),
            None => None,
        };
        Ok(Plan::build(selection, source, member, self.coverage))
    }

    async fn direct(&self, plan: &Plan) -> Result<LogPage, AdminError> {
        let rows = match sql::render(plan, Bounds::OPEN, page_fetch(plan.limit)) {
            Some(mut query) => query
                .build_query_as::<LogRow>()
                .fetch_all(self.db.pool())
                .await
                .context("read the request log")?,
            None => Vec::new(),
        };
        Ok(plan.page(rows, None))
    }

    async fn time_boxed(&self, plan: &Plan, now: DateTime<Utc>) -> Result<LogPage, AdminError> {
        if plan.eras.is_empty() {
            return Ok(plan.page(Vec::new(), None));
        }
        let key = ScanKey::of(plan);
        let mut scan = Scan::new(plan, now, self.rates.get(&key).await);
        while let Some(slice) = scan.next_slice() {
            let started = Instant::now();
            let outcome = self.slice(plan, &slice, scan.found()).await?;
            scan.record(slice, outcome, started.elapsed());
        }
        if let Some(rate) = scan.rate() {
            self.rates.remember(key, rate).await;
        }
        scan.into_page(plan)
    }

    async fn slice(
        &self,
        plan: &Plan,
        slice: &Slice,
        found: usize,
    ) -> Result<SliceOutcome, AdminError> {
        let Some(mut query) = sql::render(plan, slice.bounds(), page_fetch(plan.limit - found))
        else {
            return Ok(SliceOutcome::Scanned(Vec::new()));
        };
        let started = Instant::now();
        let mut conn = bounded::acquire_within(self.db.pool(), slice.budget).await?;
        let mut tx = conn.begin().await.context("begin a request log slice")?;
        bounded::bound_statements(&mut tx, slice.budget.saturating_sub(started.elapsed())).await?;
        let rows = match query.build_query_as::<LogRow>().fetch_all(&mut *tx).await {
            Ok(rows) => rows,
            Err(error) if is_statement_timeout(&error) => return Ok(SliceOutcome::Canceled),
            Err(error) => return Err(error).context("scan a slice of the request log"),
        };
        tx.commit().await.context("finish a request log slice")?;
        Ok(SliceOutcome::Scanned(rows))
    }
}

impl Plan {
    fn build(
        selection: Selection,
        source: Source,
        member: Option<MemberScope>,
        coverage: &Coverage,
    ) -> Self {
        let source_floor = match source {
            Source::Log => coverage.log_from,
            Source::Subjects(_) => coverage.searchable_subjects_from(),
        };
        let keyset_floor = match selection.keyset {
            Keyset::After(cursor) => Some(cursor.ts),
            Keyset::Newest | Keyset::Before(_) => None,
        };
        let window = Bounds {
            lo: [selection.window.lo, source_floor, keyset_floor]
                .into_iter()
                .flatten()
                .max(),
            hi: selection.window.hi,
        };
        let has_source = source_floor.is_some();
        let source_clamp = source_floor
            .filter(|_| matches!(source, Source::Subjects(_)))
            .filter(|floor| selection.window.starts_before(*floor));
        let mut plan = Self {
            source,
            conditions: selection.conditions,
            member,
            eras: Vec::new(),
            keyset: selection.keyset,
            limit: selection.limit,
            include_display: selection.include_display,
            floor: window.lo,
            ceiling: window.hi,
            mode: Mode::Direct,
            clamped_from: None,
        };
        let era_clamp = match has_source {
            true => plan.admit_eras(window, coverage.attribution_from),
            false => None,
        };
        plan.clamped_from = source_clamp.max(era_clamp);
        plan.mode = plan.choose_mode();
        plan
    }

    fn admit_eras(
        &mut self,
        window: Bounds,
        attribution_since: Option<DateTime<Utc>>,
    ) -> Option<DateTime<Utc>> {
        let (admitted, excluded): (Vec<EraRange>, Vec<EraRange>) =
            era_ranges(window, attribution_since)
                .into_iter()
                .partition(|range| self.admits(range.era));
        self.eras = admitted;
        excluded
            .iter()
            .any(|range| range.era == Era::Legacy)
            .then_some(attribution_since)
            .flatten()
    }

    fn admits(&self, era: Era) -> bool {
        self.conditions
            .iter()
            .all(|condition| condition.admits(era))
    }

    fn choose_mode(&self) -> Mode {
        if matches!(self.keyset, Keyset::After(_)) {
            return Mode::Direct;
        }
        if self.conditions.iter().any(Condition::is_text) {
            return Mode::TimeBoxed;
        }
        let driven = match (&self.source, &self.member) {
            (Source::Subjects(_), _) => self.residuals_are_empty(),
            (Source::Log, Some(member)) => {
                self.conditions.is_empty() && !self.reads_unattributed_prefixes(member)
            }
            (Source::Log, None) => match self.conditions.as_slice() {
                [] => true,
                [single] => self.is_indexed(single),
                _ => false,
            },
        };
        match driven {
            true => Mode::Direct,
            false => Mode::TimeBoxed,
        }
    }

    fn reads_unattributed_prefixes(&self, member: &MemberScope) -> bool {
        self.eras.iter().any(|range| {
            member
                .identity
                .arms(range.era)
                .contains(&MemberArm::Prefixes)
        })
    }

    fn is_indexed(&self, condition: &Condition) -> bool {
        match condition {
            Condition::Actor(_)
            | Condition::ActorPlayer(_)
            | Condition::Ip(_)
            | Condition::KeyPrefix(_) => true,
            Condition::Route(template) => {
                reqlog::is_static_template(template)
                    || self.eras.iter().all(|range| range.era == Era::Attributed)
            }
            _ => false,
        }
    }

    fn residuals_are_empty(&self) -> bool {
        self.conditions.is_empty() && self.member.is_none()
    }

    fn top(&self, now: DateTime<Utc>) -> DateTime<Utc> {
        let newest = match self.keyset {
            Keyset::Before(cursor) => cursor.ts,
            Keyset::Newest | Keyset::After(_) => now,
        };
        self.ceiling.map_or(newest, |ceiling| ceiling.min(newest))
    }

    fn resume(&self, top: DateTime<Utc>) -> Cursor {
        match self.keyset {
            Keyset::Before(cursor) => cursor,
            Keyset::Newest | Keyset::After(_) => Cursor::before(top),
        }
    }

    fn page(&self, mut rows: Vec<LogRow>, unscanned: Option<Cursor>) -> LogPage {
        let full = rows.len() > self.limit;
        rows.truncate(self.limit);
        let next_cursor = match full {
            true => rows.last().map(LogRow::cursor),
            false => unscanned,
        };
        if self.keyset.reads_forward() {
            rows.reverse();
        }
        LogPage {
            scanned_to: next_cursor.filter(|_| !full).map(|cursor| cursor.ts),
            next_cursor,
            rows,
            clamped_from: self.clamped_from,
        }
    }
}

async fn member_identity(db: &Database, id: i64) -> Result<MemberIdentity, AdminError> {
    let identity = sqlx::query_as(
        "SELECT $1::int8 AS id,
                (SELECT discord_id FROM members WHERE id = $1) AS discord_id,
                ARRAY(SELECT DISTINCT left(api_key, $2) FROM member_api_keys WHERE member_id = $1) AS prefixes",
    )
    .bind(id)
    .bind(KEY_PREFIX_CHARS as i32)
    .fetch_one(db.pool())
    .await
    .context("read the member's identity")?;
    Ok(identity)
}

fn era_ranges(window: Bounds, attribution_since: Option<DateTime<Utc>>) -> Vec<EraRange> {
    let Some(since) = attribution_since else {
        return vec![EraRange {
            era: Era::Legacy,
            bounds: window,
        }];
    };
    [
        EraRange {
            era: Era::Legacy,
            bounds: Bounds {
                lo: None,
                hi: Some(since),
            },
        },
        EraRange {
            era: Era::Attributed,
            bounds: Bounds {
                lo: Some(since),
                hi: None,
            },
        },
    ]
    .into_iter()
    .map(|range| EraRange {
        bounds: range.bounds.intersect(window),
        ..range
    })
    .filter(|range| !range.bounds.is_empty())
    .collect()
}

fn page_fetch(limit: usize) -> i64 {
    limit as i64 + 1
}

#[cfg(test)]
mod tests;
