use chrono::{NaiveDate, TimeZone};
use coral_redis::RedisPool;
use reqlog::Form;
use reqlog::rollup::{
    CALLER_HOUR, NETWORK_WATERMARK, ROLLUP_LOCK_KEY, ROLLUPS_FEATURE, RollupTable, TRAFFIC_HOUR,
    TRAFFIC_MINUTE,
};
use tokio::sync::Mutex;
use uuid::Uuid;

use super::*;
use crate::jobs::{DUMP_APPLICATION, JobRegistry, JobState};

const PASS_DAY: (i32, u32, u32) = (2020, 4, 1);
const PARTITIONED: [&str; 3] = [
    "api_request_log",
    "api_request_detail",
    "api_request_subject",
];
const STATE_NAMES: [&str; 3] = [TRAFFIC_WATERMARK, CALLER_WATERMARK, NETWORK_WATERMARK];
const ROW_STEP_SECS: i64 = 30;
const JOB_POLLS: usize = 200;
const JOB_POLL: Duration = Duration::from_millis(25);

static ROLLUP_STATE: Mutex<()> = Mutex::const_new(());

struct Fixture {
    db: Arc<Database>,
    rollups: Rollups,
}

impl Fixture {
    async fn run(&self, pass: Pass, now: DateTime<Utc>) -> PassReport {
        self.rollups
            .run_pass(pass, now, &Attribution::without_internal_key())
            .await
    }
}

fn at(hour: u32, minute: u32, second: u32) -> DateTime<Utc> {
    let (year, month, day) = PASS_DAY;
    Utc.with_ymd_and_hms(year, month, day, hour, minute, second)
        .unwrap()
}

fn span(from: DateTime<Utc>, to: DateTime<Utc>) -> Span {
    Span::new(from, to).unwrap()
}

async fn fixture() -> Option<Fixture> {
    let db = Arc::new(
        Database::connect(&std::env::var("DATABASE_URL").ok()?)
            .await
            .ok()?,
    );
    create_day_partitions(&db).await;
    let day = span(at(0, 0, 0), at(0, 0, 0) + TimeDelta::days(1));
    clear(&db, day).await;
    Some(Fixture {
        rollups: Rollups::new(db.clone()),
        db,
    })
}

async fn create_day_partitions(db: &Database) {
    let (year, month, day) = PASS_DAY;
    let date = NaiveDate::from_ymd_opt(year, month, day).unwrap();
    for table in PARTITIONED {
        sqlx::query(&format!(
            "CREATE TABLE IF NOT EXISTS {table}_{suffix} PARTITION OF {table}
             FOR VALUES FROM ('{date}') TO ('{next}')",
            suffix = date.format("%Y%m%d"),
            next = date.succ_opt().unwrap(),
        ))
        .execute(db.pool())
        .await
        .unwrap();
    }
}

async fn clear(db: &Database, day: Span) {
    for table in PARTITIONED {
        sqlx::query(&format!("DELETE FROM {table} WHERE ts >= $1 AND ts < $2"))
            .bind(day.from)
            .bind(day.to)
            .execute(db.pool())
            .await
            .unwrap();
    }
    for table in ROLLUP_TABLES {
        sqlx::query(&format!(
            "DELETE FROM {name} WHERE {time} >= $1 AND {time} < $2",
            name = table.name,
            time = table.time
        ))
        .bind(day.from - HOUR)
        .bind(day.to)
        .execute(db.pool())
        .await
        .unwrap();
    }
    sqlx::query("DELETE FROM api_rollup_state WHERE name = ANY($1)")
        .bind(&STATE_NAMES[..])
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("DELETE FROM api_rollup_dirty")
        .execute(db.pool())
        .await
        .unwrap();
}

async fn insert_rows(db: &Database, rows: Span, member_id: Option<i64>) -> i64 {
    let count = (rows.to - rows.from).num_seconds() / ROW_STEP_SECS;
    sqlx::query(
        "INSERT INTO api_request_log (ts, request_id, method, path, route, status, cause, key_kind,
           member_id, ip, user_agent, latency_ms, subjects)
         SELECT $1 + make_interval(secs => g * $2), uuidv7(), 'GET', '/v3/player/tags',
                '/v3/player/tags', 200, 0, 'personal', $4, '198.51.100.9', 'pass-test', 5, 1
         FROM generate_series(0, $3 - 1) AS g",
    )
    .bind(rows.from)
    .bind(ROW_STEP_SECS as f64)
    .bind(count)
    .bind(member_id)
    .execute(db.pool())
    .await
    .unwrap();
    count
}

async fn set_state(db: &Database, name: &str, at: DateTime<Utc>) {
    let mut conn = db.pool().acquire().await.unwrap();
    rollup::set_watermark(&mut conn, name, at).await.unwrap();
}

async fn state_of(db: &Database, name: &str) -> Option<DateTime<Utc>> {
    let mut conn = db.pool().acquire().await.unwrap();
    rollup::watermark(&mut conn, name).await.unwrap()
}

async fn raw_count(db: &Database, rows: Span) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM api_request_log WHERE ts >= $1 AND ts < $2")
        .bind(rows.from)
        .bind(rows.to)
        .fetch_one(db.pool())
        .await
        .unwrap()
}

async fn rolled_count(db: &Database, table: &RollupTable, rows: Span) -> i64 {
    sqlx::query_scalar(&format!(
        "SELECT coalesce(sum(requests), 0)::bigint FROM {name} WHERE {time} >= $1 AND {time} < $2",
        name = table.name,
        time = table.time
    ))
    .bind(rows.from)
    .bind(rows.to)
    .fetch_one(db.pool())
    .await
    .unwrap()
}

async fn rename_connection(conn: &mut PgConnection, name: &str) -> String {
    let previous = sqlx::query_scalar("SELECT current_setting('application_name')")
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    sqlx::query("SELECT set_config('application_name', $1, false)")
        .bind(name)
        .execute(&mut *conn)
        .await
        .unwrap();
    previous
}

async fn coverage_since(db: &Database) -> Option<DateTime<Utc>> {
    sqlx::query_scalar("SELECT since FROM api_coverage WHERE feature = $1")
        .bind(ROLLUPS_FEATURE)
        .fetch_optional(db.pool())
        .await
        .unwrap()
}

async fn new_member(db: &Database) -> i64 {
    let discord_id = 880_400_000_000_000_000 + (Uuid::new_v4().as_u128() % 1_000_000_000) as i64;
    sqlx::query_scalar("INSERT INTO members (discord_id) VALUES ($1) RETURNING id")
        .bind(discord_id)
        .fetch_one(db.pool())
        .await
        .unwrap()
}

async fn remove_member(db: &Database, member_id: i64) {
    sqlx::query("DELETE FROM members WHERE id = $1")
        .bind(member_id)
        .execute(db.pool())
        .await
        .unwrap();
}

async fn insert_subjects(db: &Database, ts: DateTime<Utc>, player: Uuid, form: Form, count: i64) {
    sqlx::query(
        "INSERT INTO api_request_subject (ts, request_id, key_kind, service, status, form, player)
         SELECT $1 + make_interval(secs => g), uuidv7(), 1, 0, 200, $2, $3
         FROM generate_series(0, $4 - 1) AS g",
    )
    .bind(ts)
    .bind(form.code())
    .bind(player)
    .bind(count)
    .execute(db.pool())
    .await
    .unwrap();
}

#[tokio::test]
async fn a_traffic_pass_catches_up_from_its_watermark_and_then_only_rerolls() {
    let _state = ROLLUP_STATE.lock().await;
    let Some(fixture) = fixture().await else {
        return;
    };
    insert_rows(&fixture.db, span(at(5, 0, 0), at(9, 0, 0)), None).await;
    set_state(&fixture.db, TRAFFIC_WATERMARK, at(6, 0, 0)).await;
    let now = at(8, 0, 30);

    let first = fixture.run(Pass::Traffic, now).await;
    assert_eq!(first.outcome, Outcome::Done, "{first:?}");
    assert_eq!(
        (first.from, first.to),
        (Some(at(5, 50, 0)), Some(at(7, 59, 0)))
    );
    let rolled = span(at(5, 50, 0), at(7, 59, 0));
    let raw = raw_count(&fixture.db, rolled).await;
    assert_eq!(
        rolled_count(&fixture.db, &TRAFFIC_MINUTE, rolled).await,
        raw
    );
    assert_eq!(
        rolled_count(
            &fixture.db,
            &TRAFFIC_MINUTE,
            span(at(5, 0, 0), at(5, 50, 0))
        )
        .await,
        0
    );
    assert_eq!(
        rolled_count(&fixture.db, &TRAFFIC_HOUR, span(at(6, 0, 0), at(7, 0, 0))).await,
        raw_count(&fixture.db, span(at(6, 0, 0), at(7, 0, 0))).await
    );
    assert_eq!(
        state_of(&fixture.db, TRAFFIC_WATERMARK).await,
        Some(at(7, 59, 0))
    );

    let again = fixture.run(Pass::Traffic, now).await;
    assert_eq!(again.outcome, Outcome::Done);
    assert_eq!(
        (again.from, again.to),
        (Some(at(7, 49, 0)), Some(at(7, 59, 0)))
    );
    assert_eq!((again.written, again.removed), (0, 0));
}

#[tokio::test]
async fn a_long_catch_up_stops_after_its_chunk_budget_and_reports_behind() {
    let _state = ROLLUP_STATE.lock().await;
    let Some(fixture) = fixture().await else {
        return;
    };
    insert_rows(&fixture.db, span(at(10, 0, 0), at(19, 0, 0)), None).await;
    set_state(&fixture.db, TRAFFIC_WATERMARK, at(10, 10, 0)).await;
    let now = at(18, 0, 30);

    let first = fixture.run(Pass::Traffic, now).await;
    assert_eq!(first.outcome, Outcome::Behind);
    assert_eq!(
        (first.from, first.to),
        (Some(at(10, 0, 0)), Some(at(16, 0, 0)))
    );
    let second = fixture.run(Pass::Traffic, now).await;
    assert_eq!(second.outcome, Outcome::Done);
    assert_eq!(second.to, Some(at(17, 59, 0)));
    let rolled = span(at(10, 0, 0), at(17, 59, 0));
    assert_eq!(
        rolled_count(&fixture.db, &TRAFFIC_MINUTE, rolled).await,
        raw_count(&fixture.db, rolled).await
    );
}

#[tokio::test]
async fn a_backlog_waits_for_the_quiet_window_and_for_a_running_dump_to_finish() {
    let _state = ROLLUP_STATE.lock().await;
    let Some(fixture) = fixture().await else {
        return;
    };
    insert_rows(&fixture.db, span(at(0, 0, 0), at(9, 0, 0)), None).await;
    set_state(&fixture.db, TRAFFIC_WATERMARK, at(0, 10, 0)).await;
    let backlog = span(at(0, 0, 0), at(9, 0, 0));

    let daytime = fixture.run(Pass::Traffic, at(8, 0, 30)).await;
    assert_eq!(daytime.outcome, Outcome::Waiting, "{daytime:?}");
    assert_eq!(
        state_of(&fixture.db, TRAFFIC_WATERMARK).await,
        Some(at(0, 10, 0))
    );
    assert_eq!(rolled_count(&fixture.db, &TRAFFIC_MINUTE, backlog).await, 0);

    let mut dump = fixture.db.pool().acquire().await.unwrap();
    let own_name = rename_connection(&mut dump, DUMP_APPLICATION).await;
    let dumping = fixture.run(Pass::Traffic, at(23, 0, 0)).await;
    assert_eq!(dumping.outcome, Outcome::Waiting, "{dumping:?}");
    rename_connection(&mut dump, &own_name).await;
    drop(dump);

    let quiet = fixture.run(Pass::Traffic, at(23, 0, 0)).await;
    assert_eq!(quiet.outcome, Outcome::Behind, "{quiet:?}");
    assert_eq!(
        state_of(&fixture.db, TRAFFIC_WATERMARK).await,
        Some(at(6, 0, 0))
    );
}

#[tokio::test]
async fn a_caller_pass_rerolls_unsettled_hours_and_holds_its_watermark_at_the_settled_one() {
    let _state = ROLLUP_STATE.lock().await;
    let Some(fixture) = fixture().await else {
        return;
    };
    insert_rows(&fixture.db, span(at(5, 0, 0), at(9, 0, 0)), None).await;
    set_state(&fixture.db, CALLER_WATERMARK, at(6, 0, 0)).await;

    let first = fixture.run(Pass::Callers, at(8, 5, 0)).await;
    assert_eq!(first.outcome, Outcome::Done, "{first:?}");
    assert_eq!(
        (first.from, first.to),
        (Some(at(6, 0, 0)), Some(at(9, 0, 0)))
    );
    assert_eq!(
        state_of(&fixture.db, CALLER_WATERMARK).await,
        Some(at(7, 0, 0))
    );
    let rolled = span(at(6, 0, 0), at(9, 0, 0));
    assert_eq!(
        rolled_count(&fixture.db, &CALLER_HOUR, rolled).await,
        raw_count(&fixture.db, rolled).await
    );

    let later = fixture.run(Pass::Callers, at(8, 20, 0)).await;
    assert_eq!(
        (later.from, later.to),
        (Some(at(7, 0, 0)), Some(at(9, 0, 0)))
    );
    assert_eq!((later.written, later.removed), (0, 0));
    assert_eq!(
        state_of(&fixture.db, CALLER_WATERMARK).await,
        Some(at(8, 0, 0))
    );
}

#[tokio::test]
async fn the_network_pass_folds_up_to_its_settle_point_across_invocations() {
    let _state = ROLLUP_STATE.lock().await;
    let Some(fixture) = fixture().await else {
        return;
    };
    let member_id = new_member(&fixture.db).await;
    let folded = insert_rows(
        &fixture.db,
        span(at(10, 0, 0), at(17, 50, 0)),
        Some(member_id),
    )
    .await;
    insert_rows(
        &fixture.db,
        span(at(17, 50, 0), at(19, 0, 0)),
        Some(member_id),
    )
    .await;
    set_state(&fixture.db, NETWORK_WATERMARK, at(10, 0, 0)).await;
    let now = at(18, 0, 0);

    let first = fixture.run(Pass::Network, now).await;
    assert_eq!(first.outcome, Outcome::Behind);
    assert_eq!(
        (first.from, first.to),
        (Some(at(10, 0, 0)), Some(at(16, 0, 0)))
    );
    let second = fixture.run(Pass::Network, now).await;
    assert_eq!(second.outcome, Outcome::Done);
    assert_eq!(
        (second.from, second.to),
        (Some(at(16, 0, 0)), Some(at(17, 50, 0)))
    );
    let third = fixture.run(Pass::Network, now).await;
    assert_eq!(third.outcome, Outcome::Idle);

    let counted: i64 = sqlx::query_scalar("SELECT requests FROM api_key_ips WHERE member_id = $1")
        .bind(member_id)
        .fetch_one(fixture.db.pool())
        .await
        .unwrap();
    assert_eq!(counted, folded);
    remove_member(&fixture.db, member_id).await;
}

#[tokio::test]
async fn passes_skip_their_work_while_another_instance_holds_the_rollup_lock() {
    let _state = ROLLUP_STATE.lock().await;
    let Some(fixture) = fixture().await else {
        return;
    };
    set_state(&fixture.db, TRAFFIC_WATERMARK, at(6, 0, 0)).await;
    let mut holder = fixture.db.pool().begin().await.unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(ROLLUP_LOCK_KEY)
        .execute(&mut *holder)
        .await
        .unwrap();

    let report = fixture.run(Pass::Traffic, at(8, 0, 30)).await;
    assert_eq!(report.outcome, Outcome::Locked);
    assert_eq!(
        state_of(&fixture.db, TRAFFIC_WATERMARK).await,
        Some(at(6, 0, 0))
    );
    holder.rollback().await.unwrap();
}

#[tokio::test]
async fn dirty_ranges_are_recomputed_as_whole_hours_and_cleared() {
    let _state = ROLLUP_STATE.lock().await;
    let Some(fixture) = fixture().await else {
        return;
    };
    let hour = span(at(10, 0, 0), at(11, 0, 0));
    let raw = insert_rows(&fixture.db, hour, None).await;
    for (from, to) in [
        (at(10, 5, 0), at(10, 10, 0)),
        (at(10, 20, 0), at(10, 20, 0)),
    ] {
        sqlx::query("INSERT INTO api_rollup_dirty (from_ts, to_ts) VALUES ($1, $2)")
            .bind(from)
            .bind(to)
            .execute(fixture.db.pool())
            .await
            .unwrap();
    }

    let report = fixture.run(Pass::Dirty, at(12, 0, 0)).await;
    assert_eq!(report.outcome, Outcome::Done, "{report:?}");
    assert_eq!((report.from, report.to), (Some(hour.from), Some(hour.to)));
    assert_eq!(rolled_count(&fixture.db, &TRAFFIC_MINUTE, hour).await, raw);
    assert_eq!(rolled_count(&fixture.db, &CALLER_HOUR, hour).await, raw);
    let state = Rollups::state(&fixture.db).await.unwrap();
    assert_eq!(state.dirty_ranges, 0);
}

#[tokio::test]
async fn a_full_dirty_batch_reports_behind_even_when_it_merges_into_one_range() {
    let _state = ROLLUP_STATE.lock().await;
    let Some(fixture) = fixture().await else {
        return;
    };
    let hour = span(at(10, 0, 0), at(11, 0, 0));
    insert_rows(&fixture.db, hour, None).await;
    sqlx::query(
        "INSERT INTO api_rollup_dirty (from_ts, to_ts)
         SELECT $1, $1 FROM generate_series(1, $2 + 1)",
    )
    .bind(at(10, 30, 0))
    .bind(DIRTY_BATCH)
    .execute(fixture.db.pool())
    .await
    .unwrap();

    let full = fixture.run(Pass::Dirty, at(12, 0, 0)).await;
    assert_eq!(full.outcome, Outcome::Behind, "{full:?}");
    let rest = fixture.run(Pass::Dirty, at(12, 0, 0)).await;
    assert_eq!(rest.outcome, Outcome::Done, "{rest:?}");
    let drained = fixture.run(Pass::Dirty, at(12, 0, 0)).await;
    assert_eq!(drained.outcome, Outcome::Idle, "{drained:?}");
}

#[tokio::test]
async fn rollup_coverage_is_recorded_only_once_traffic_and_callers_have_caught_up() {
    let _state = ROLLUP_STATE.lock().await;
    let Some(fixture) = fixture().await else {
        return;
    };
    sqlx::query("DELETE FROM api_coverage WHERE feature = $1")
        .bind(ROLLUPS_FEATURE)
        .execute(fixture.db.pool())
        .await
        .unwrap();
    insert_rows(&fixture.db, span(at(5, 0, 0), at(9, 0, 0)), None).await;
    set_state(&fixture.db, CALLER_WATERMARK, at(6, 0, 0)).await;
    set_state(&fixture.db, TRAFFIC_WATERMARK, at(5, 0, 0)).await;

    let callers_only = fixture.run(Pass::Callers, at(8, 5, 0)).await;
    assert_eq!(callers_only.outcome, Outcome::Done, "{callers_only:?}");
    assert_eq!(coverage_since(&fixture.db).await, None);

    set_state(&fixture.db, TRAFFIC_WATERMARK, at(8, 4, 0)).await;
    let both = fixture.run(Pass::Callers, at(8, 6, 0)).await;
    assert_eq!(both.outcome, Outcome::Done, "{both:?}");
    let mut conn = fixture.db.pool().acquire().await.unwrap();
    let floor = rollup::log_floor(&mut conn).await.unwrap();
    assert_eq!(coverage_since(&fixture.db).await, floor);
}

#[tokio::test]
async fn top_players_rank_lookups_per_window_without_batch_bodies() {
    let _state = ROLLUP_STATE.lock().await;
    let Some(fixture) = fixture().await else {
        return;
    };
    let (often, sometimes, batched) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    insert_subjects(&fixture.db, at(11, 10, 0), often, Form::Undashed, 3).await;
    insert_subjects(&fixture.db, at(11, 20, 0), sometimes, Form::Name, 2).await;
    insert_subjects(&fixture.db, at(11, 30, 0), batched, Form::Body, 5).await;
    insert_subjects(&fixture.db, at(9, 0, 0), sometimes, Form::Dashed, 2).await;

    let report = fixture.run(Pass::TopPlayers, at(12, 0, 0)).await;
    assert_eq!(report.outcome, Outcome::Done, "{report:?}");
    let ranking = |list: &TopList| -> Vec<(Uuid, i64)> {
        list.players
            .iter()
            .map(|ranked| (ranked.player, ranked.requests))
            .collect()
    };
    let top = fixture.rollups.top_players();
    let hour = top.get(TopWindow::LastHour).await.unwrap();
    assert_eq!(ranking(&hour), vec![(often, 3), (sometimes, 2)]);
    let day = top.get(TopWindow::LastDay).await.unwrap();
    assert_eq!(ranking(&day), vec![(sometimes, 4), (often, 3)]);
}

#[tokio::test]
async fn recompute_spans_must_end_by_now_and_start_inside_the_retained_log() {
    let _state = ROLLUP_STATE.lock().await;
    let Some(fixture) = fixture().await else {
        return;
    };
    let now = at(12, 0, 0);
    let rejected = [
        (at(10, 0, 0), at(12, 30, 0)),
        (at(11, 0, 0), at(11, 0, 0)),
        (
            Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap(),
            at(11, 0, 0),
        ),
    ];
    for (from, to) in rejected {
        let error = fixture
            .rollups
            .recompute_span(&fixture.db, from, to, now)
            .await
            .unwrap_err();
        assert_eq!(error.code(), "bad_request", "{from} {to}: {error}");
    }
    let accepted = fixture
        .rollups
        .recompute_span(&fixture.db, at(10, 20, 0), at(11, 10, 0), now)
        .await
        .unwrap();
    assert_eq!(accepted, span(at(10, 0, 0), at(12, 0, 0)));
}

#[tokio::test]
async fn a_recompute_job_rewrites_nothing_over_rows_that_are_already_rolled_up() {
    let _state = ROLLUP_STATE.lock().await;
    let Some(fixture) = fixture().await else {
        return;
    };
    let Ok(redis_url) = std::env::var("REDIS_URL") else {
        return;
    };
    let redis = RedisPool::connect(&redis_url).await.unwrap();
    let range = span(at(13, 0, 0), at(15, 0, 0));
    insert_rows(&fixture.db, range, None).await;
    let first = fixture
        .rollups
        .recompute(range, &Attribution::without_internal_key())
        .await
        .unwrap();
    assert!(first.written > 0);

    let registry = JobRegistry::new(fixture.db.clone(), redis);
    let job = RecomputeJob::new(fixture.rollups.clone(), range);
    registry.start(Arc::new(job)).await.unwrap();
    let mut finished = None;
    for _ in 0..JOB_POLLS {
        let status = registry.status(RECOMPUTE_JOB).await.unwrap().unwrap();
        if status.finished_at.is_some() {
            finished = Some(status);
            break;
        }
        tokio::time::sleep(JOB_POLL).await;
    }
    let status = finished.expect("the recompute job finished");
    assert_eq!(status.state, JobState::Succeeded, "{status:?}");
    assert_eq!(status.progress["written"], 0);
    assert_eq!(status.progress["removed"], 0);
    assert_eq!(status.progress["done_until"], serde_ts::format(&range.to));
}

#[tokio::test]
async fn the_rollup_state_lists_every_watermark_and_the_dirty_queue() {
    let _state = ROLLUP_STATE.lock().await;
    let Some(fixture) = fixture().await else {
        return;
    };
    set_state(&fixture.db, TRAFFIC_WATERMARK, at(6, 0, 0)).await;
    set_state(&fixture.db, CALLER_WATERMARK, at(5, 0, 0)).await;
    sqlx::query("INSERT INTO api_rollup_dirty (from_ts, to_ts) VALUES ($1, $1)")
        .bind(at(4, 0, 0))
        .execute(fixture.db.pool())
        .await
        .unwrap();

    let state = Rollups::state(&fixture.db).await.unwrap();
    assert!(state.log_from.is_some());
    assert!(state.watermarks.contains(&Watermark {
        name: TRAFFIC_WATERMARK.into(),
        at: at(6, 0, 0),
    }));
    assert!(state.watermarks.contains(&Watermark {
        name: CALLER_WATERMARK.into(),
        at: at(5, 0, 0),
    }));
    assert_eq!(state.dirty_ranges, 1);
    clear(
        &fixture.db,
        span(at(0, 0, 0), at(0, 0, 0) + TimeDelta::days(1)),
    )
    .await;
}
