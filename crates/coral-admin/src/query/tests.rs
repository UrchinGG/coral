use std::time::Duration;

use chrono::{TimeDelta, TimeZone};
use coral_redis::RedisPool;
use database::PoolSettings;
use reqlog::{Cause, Form};
use serde_json::json;
use tokio::sync::OnceCell;
use uuid::Uuid;

use super::time_box::{
    FIRST_SLICE_SPAN, MAX_SLICE_SPAN, MIN_SLICE_SPAN, OPENING_SLICE_SHARE, ScanRate,
    TIME_BOX_BUDGET,
};
use super::*;
use crate::refs::Match;

const TEST_DAY: (i32, u32, u32) = (2020, 1, 1);
const BLOCKED_DAY: (i32, u32, u32) = (2020, 1, 3);
const STALLED_DAY: (i32, u32, u32) = (2020, 1, 4);
const PARTITION_DAYS: [(i32, u32, u32); 3] = [TEST_DAY, BLOCKED_DAY, STALLED_DAY];
const PARTITIONS: [&str; 2] = ["api_request_log", "api_request_subject"];

static TEST_PARTITIONS: OnceCell<()> = OnceCell::const_new();

struct Fixture {
    db: Arc<Database>,
    players: PlayerSearch,
    rates: ScanRates,
}

#[derive(Clone)]
struct Seed {
    ts: DateTime<Utc>,
    request_id: Option<Uuid>,
    method: &'static str,
    path: &'static str,
    route: Option<&'static str>,
    status: i16,
    key_kind: &'static str,
    member_id: Option<i64>,
    actor_id: Option<i64>,
    key_prefix: Option<&'static str>,
    ip: Option<&'static str>,
    query: Option<String>,
    user_agent: Option<&'static str>,
}

fn at(hour: u32, minute: u32) -> DateTime<Utc> {
    on(TEST_DAY, hour, minute)
}

fn on((year, month, day): (i32, u32, u32), hour: u32, minute: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(year, month, day, hour, minute, 0)
        .unwrap()
}

fn coverage(attribution: DateTime<Utc>) -> Coverage {
    Coverage {
        log_from: Some(at(0, 0)),
        attribution_from: Some(attribution),
        subjects_from: Some(at(0, 0)),
        subject_partitions_from: Some(at(0, 0)),
        ..Coverage::default()
    }
}

fn seed(ts: DateTime<Utc>) -> Seed {
    Seed {
        ts,
        request_id: Some(reqlog::request_id_for(ts)),
        method: "GET",
        path: "/v3/player/tags",
        route: Some("/v3/player/tags"),
        status: 200,
        key_kind: "personal",
        member_id: None,
        actor_id: None,
        key_prefix: None,
        ip: None,
        query: None,
        user_agent: None,
    }
}

fn legacy(ts: DateTime<Utc>) -> Seed {
    Seed {
        request_id: None,
        route: None,
        key_kind: "key",
        ..seed(ts)
    }
}

fn window(from: DateTime<Utc>, to: DateTime<Utc>) -> serde_json::Value {
    json!({ "from": from, "to": to })
}

fn filter(body: serde_json::Value) -> RequestFilter {
    serde_json::from_value(body).unwrap()
}

fn with(mut body: serde_json::Value, extra: serde_json::Value) -> serde_json::Value {
    let fields = body.as_object_mut().unwrap();
    for (key, value) in extra.as_object().unwrap() {
        fields.insert(key.clone(), value.clone());
    }
    body
}

async fn fixture() -> Option<Fixture> {
    let db = Arc::new(
        Database::connect(&std::env::var("DATABASE_URL").ok()?)
            .await
            .ok()?,
    );
    let redis = RedisPool::connect(&std::env::var("REDIS_URL").ok()?)
        .await
        .ok()?;
    TEST_PARTITIONS
        .get_or_init(|| create_test_partitions(&db))
        .await;
    Some(Fixture {
        players: PlayerSearch::new(db.clone(), redis),
        rates: ScanRates::new(),
        db,
    })
}

async fn create_test_partitions(db: &Database) {
    for (year, month, day) in PARTITION_DAYS {
        for table in PARTITIONS {
            sqlx::query(&format!(
                "CREATE TABLE IF NOT EXISTS {} PARTITION OF {table}
                 FOR VALUES FROM ('{year:04}-{month:02}-{day:02}') TO ('{year:04}-{month:02}-{next:02}')",
                partition(table, (year, month, day)),
                next = day + 1,
            ))
            .execute(db.pool())
            .await
            .unwrap();
        }
    }
}

fn partition(table: &str, (year, month, day): (i32, u32, u32)) -> String {
    format!("{table}_{year:04}{month:02}{day:02}")
}

async fn lock_log_partition(
    db: &Database,
    day: (i32, u32, u32),
) -> sqlx::Transaction<'static, sqlx::Postgres> {
    let mut locker = db.pool().begin().await.unwrap();
    sqlx::query(&format!(
        "LOCK TABLE {} IN ACCESS EXCLUSIVE MODE",
        partition("api_request_log", day)
    ))
    .execute(&mut *locker)
    .await
    .unwrap();
    locker
}

async fn clear(db: &Database, from: DateTime<Utc>, to: DateTime<Utc>) {
    for table in PARTITIONS {
        sqlx::query(&format!("DELETE FROM {table} WHERE ts >= $1 AND ts < $2"))
            .bind(from)
            .bind(to)
            .execute(db.pool())
            .await
            .unwrap();
    }
}

async fn insert(db: &Database, seeds: &[Seed]) {
    for seed in seeds {
        sqlx::query(
            "INSERT INTO api_request_log (ts, request_id, method, path, route, status, key_kind,
               member_id, actor_id, key_prefix, ip, query, user_agent, latency_ms)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, 1)",
        )
        .bind(seed.ts)
        .bind(seed.request_id)
        .bind(seed.method)
        .bind(seed.path)
        .bind(seed.route)
        .bind(seed.status)
        .bind(seed.key_kind)
        .bind(seed.member_id)
        .bind(seed.actor_id)
        .bind(seed.key_prefix)
        .bind(seed.ip)
        .bind(&seed.query)
        .bind(seed.user_agent)
        .execute(db.pool())
        .await
        .unwrap();
    }
}

async fn insert_subject(
    db: &Database,
    seed: &Seed,
    player: Option<Uuid>,
    name: Option<&str>,
    form: Form,
) {
    sqlx::query(
        "INSERT INTO api_request_subject (ts, request_id, key_kind, service, status, form, player, name)
         VALUES ($1, $2, 1, 0, $3, $4, $5, $6)",
    )
    .bind(seed.ts)
    .bind(seed.request_id.unwrap())
    .bind(seed.status)
    .bind(form.code())
    .bind(player)
    .bind(name)
    .execute(db.pool())
    .await
    .unwrap();
}

async fn run(fixture: &Fixture, coverage: &Coverage, body: serde_json::Value) -> LogPage {
    try_run(fixture, coverage, body).await.unwrap()
}

async fn try_run(
    fixture: &Fixture,
    coverage: &Coverage,
    body: serde_json::Value,
) -> Result<LogPage, AdminError> {
    LogQuery::new(&fixture.db, coverage, &fixture.players, &fixture.rates)
        .run(filter(body))
        .await
}

async fn collect_pages(
    fixture: &Fixture,
    coverage: &Coverage,
    body: serde_json::Value,
) -> Vec<LogRow> {
    let mut rows = Vec::new();
    let mut cursor: Option<Cursor> = None;
    loop {
        let request = match cursor {
            Some(cursor) => with(body.clone(), json!({ "cursor": cursor })),
            None => body.clone(),
        };
        let page = run(fixture, coverage, request).await;
        rows.extend(page.rows);
        match page.next_cursor {
            Some(next) => cursor = Some(next),
            None => return rows,
        }
    }
}

fn timestamps(rows: &[LogRow]) -> Vec<DateTime<Utc>> {
    rows.iter().map(|row| row.ts).collect()
}

fn plan(body: serde_json::Value, coverage: &Coverage) -> Plan {
    let selection = Selection::parse(filter(body)).unwrap();
    Plan::build(selection, Source::Log, None, coverage)
}

#[test]
fn eras_split_the_window_at_attribution_since() {
    let window = Bounds {
        lo: Some(at(1, 0)),
        hi: Some(at(3, 0)),
    };
    assert_eq!(
        era_ranges(window, Some(at(2, 0))),
        vec![
            EraRange {
                era: Era::Legacy,
                bounds: Bounds {
                    lo: Some(at(1, 0)),
                    hi: Some(at(2, 0))
                }
            },
            EraRange {
                era: Era::Attributed,
                bounds: Bounds {
                    lo: Some(at(2, 0)),
                    hi: Some(at(3, 0))
                }
            },
        ]
    );
    assert_eq!(era_ranges(window, Some(at(4, 0)))[0].era, Era::Legacy);
    assert_eq!(era_ranges(window, Some(at(4, 0))).len(), 1);
    assert_eq!(era_ranges(window, None)[0].bounds, window);
}

#[test]
fn attribution_only_filters_clamp_the_legacy_era_away() {
    let coverage = coverage(at(2, 0));
    let clamped = plan(json!({ "actor": "111" }), &coverage);
    assert_eq!(clamped.clamped_from, Some(at(2, 0)));
    assert!(
        clamped
            .eras
            .iter()
            .all(|range| range.era == Era::Attributed)
    );

    let after_since = plan(
        with(window(at(2, 30), at(3, 0)), json!({ "actor": "111" })),
        &coverage,
    );
    assert_eq!(after_since.clamped_from, None);

    let unclamped = plan(json!({ "ip": "203.0.113.1" }), &coverage);
    assert_eq!(unclamped.clamped_from, None);
    assert_eq!(unclamped.eras.len(), 2);
}

#[test]
fn indexed_single_filters_run_directly_and_everything_else_is_time_boxed() {
    let coverage = coverage(at(2, 0));
    for body in [
        json!({}),
        json!({ "ip": "203.0.113.1" }),
        json!({ "key_prefix": "1a2b3c4d" }),
        json!({ "actor": "111" }),
        json!({ "route": "/v3/players" }),
        json!({ "after": Cursor::before(at(5, 0)), "q": "x" }),
    ] {
        assert_eq!(plan(body.clone(), &coverage).mode, Mode::Direct, "{body}");
    }
    for body in [
        json!({ "q": "oJugg" }),
        json!({ "status": 429 }),
        json!({ "class": "5xx" }),
        json!({ "method": "POST" }),
        json!({ "ip": "203.0.113.1", "method": "GET" }),
        json!({ "route": "/v3/resolve/{identifier}" }),
    ] {
        assert_eq!(
            plan(body.clone(), &coverage).mode,
            Mode::TimeBoxed,
            "{body}"
        );
    }
    let attributed_only = with(
        window(at(3, 0), at(4, 0)),
        json!({ "route": "/v3/resolve/{identifier}" }),
    );
    assert_eq!(plan(attributed_only, &coverage).mode, Mode::Direct);
}

#[test]
fn member_streams_reading_legacy_prefix_rows_are_time_boxed() {
    let coverage = coverage(at(2, 0));
    let member = |prefixes: Vec<String>| MemberScope {
        identity: MemberIdentity {
            id: 7,
            discord_id: Some(70),
            prefixes,
        },
        role: MemberRole::Driver,
    };
    let build = |body: serde_json::Value, scope: MemberScope| {
        let selection = Selection::parse(filter(body)).unwrap();
        Plan::build(selection, Source::Log, Some(scope), &coverage)
    };
    let spanning = json!({ "member": 7 });
    assert_eq!(
        build(spanning.clone(), member(vec!["1a2b3c4d".into()])).mode,
        Mode::TimeBoxed
    );
    assert_eq!(build(spanning, member(Vec::new())).mode, Mode::Direct);
    let attributed = with(window(at(3, 0), at(4, 0)), json!({ "member": 7 }));
    assert_eq!(
        build(attributed, member(vec!["1a2b3c4d".into()])).mode,
        Mode::Direct
    );
}

#[test]
fn scan_rates_size_slices_from_the_measured_cost() {
    let fast = ScanRate::measured(FIRST_SLICE_SPAN, Duration::from_millis(10));
    assert_eq!(fast.span_within(Duration::from_millis(300)), MAX_SLICE_SPAN);
    let slow = ScanRate::measured(FIRST_SLICE_SPAN, Duration::from_millis(200));
    assert_eq!(
        slow.span_within(Duration::from_millis(100)),
        TimeDelta::minutes(30)
    );
    assert_eq!(slow.span_within(Duration::from_millis(1)), MIN_SLICE_SPAN);
}

#[test]
fn an_opening_slice_spans_the_first_hour_below_the_top() {
    let coverage = coverage(at(2, 0));
    let plan = plan(json!({ "q": "x" }), &coverage);
    let scan = Scan::new(&plan, at(5, 0), None);
    let slice = scan.next_slice().unwrap();
    assert_eq!(slice.lo, at(5, 0) - FIRST_SLICE_SPAN);
    assert_eq!(slice.hi, None);
}

#[test]
fn a_canceled_opening_slice_keeps_the_resume_point_and_backs_off() {
    let coverage = coverage(at(2, 0));
    let plan = plan(json!({ "q": "x" }), &coverage);
    let mut scan = Scan::new(&plan, at(5, 0), None);
    let slice = scan.next_slice().unwrap();

    scan.record(slice, SliceOutcome::Canceled, TIME_BOX_BUDGET);

    assert!(scan.next_slice().is_none());
    let opening = scan
        .rate()
        .unwrap()
        .span_within(TIME_BOX_BUDGET.mul_f64(OPENING_SLICE_SHARE));
    assert!(opening < FIRST_SLICE_SPAN, "{opening}");
    let page = scan.into_page(&plan).unwrap();
    assert!(page.rows.is_empty());
    assert_eq!(page.next_cursor, Some(Cursor::before(at(5, 0))));
    assert_eq!(page.scanned_to, Some(at(5, 0)));
}

#[test]
fn a_cancel_after_progress_returns_the_scanned_part() {
    let coverage = coverage(at(2, 0));
    let plan = plan(json!({ "q": "x" }), &coverage);
    let mut scan = Scan::new(&plan, at(5, 0), None);
    let first = scan.next_slice().unwrap();
    scan.record(
        first,
        SliceOutcome::Scanned(Vec::new()),
        Duration::from_millis(100),
    );
    let second = scan.next_slice().unwrap();
    assert_eq!(second.hi, Some(at(4, 0)));
    assert!(second.lo < at(4, 0));

    scan.record(second, SliceOutcome::Canceled, Duration::from_millis(300));

    let page = scan.into_page(&plan).unwrap();
    assert_eq!(page.next_cursor, Some(Cursor::before(at(4, 0))));
    assert_eq!(page.scanned_to, Some(at(4, 0)));
}

#[test]
fn a_canceled_minimal_opening_slice_is_a_timeout() {
    let coverage = coverage(at(2, 0));
    let plan = plan(json!({ "q": "x" }), &coverage);
    let crawling = ScanRate::measured(MIN_SLICE_SPAN, TIME_BOX_BUDGET * 10);
    let mut scan = Scan::new(&plan, at(5, 0), Some(crawling));
    let slice = scan.next_slice().unwrap();
    assert_eq!(slice.lo, at(5, 0) - MIN_SLICE_SPAN);

    scan.record(slice, SliceOutcome::Canceled, TIME_BOX_BUDGET);

    let error = scan.into_page(&plan).unwrap_err();
    assert_eq!(error.code(), "timeout");
}

#[test]
fn slices_that_reach_the_floor_or_fill_the_page_teach_no_rate() {
    let coverage = coverage(at(2, 0));
    let windowed = plan(
        with(window(at(4, 30), at(5, 0)), json!({ "q": "x", "limit": 1 })),
        &coverage,
    );
    let learnt = ScanRate::measured(FIRST_SLICE_SPAN, Duration::from_millis(100));
    let mut floored = Scan::new(&windowed, at(5, 0), Some(learnt));
    let slice = floored.next_slice().unwrap();
    assert_eq!(slice.lo, at(4, 30));
    floored.record(
        slice,
        SliceOutcome::Scanned(Vec::new()),
        Duration::from_millis(1),
    );
    assert_eq!(floored.rate(), Some(learnt));
    assert!(floored.next_slice().is_none());
    assert_eq!(floored.into_page(&windowed).unwrap().next_cursor, None);

    let open = plan(json!({ "q": "x", "limit": 1 }), &coverage);
    let mut filled = Scan::new(&open, at(5, 0), Some(learnt));
    let slice = filled.next_slice().unwrap();
    let rows = vec![row::tests::row(), row::tests::row()];
    filled.record(slice, SliceOutcome::Scanned(rows), Duration::from_millis(1));
    assert_eq!(filled.rate(), Some(learnt));
    assert!(filled.next_slice().is_none());
}

#[test]
fn full_pages_continue_from_their_last_row() {
    let coverage = coverage(at(2, 0));
    let plan = plan(json!({ "limit": 2 }), &coverage);
    let rows: Vec<LogRow> = [3, 2, 1]
        .into_iter()
        .map(|minute| LogRow {
            ts: at(4, minute),
            ..row::tests::row()
        })
        .collect();
    let page = plan.page(rows, Some(Cursor::before(at(0, 0))));
    assert_eq!(timestamps(&page.rows), vec![at(4, 3), at(4, 2)]);
    assert_eq!(page.next_cursor, Some(Cursor::new(at(4, 2), Uuid::nil())));
    assert_eq!(page.scanned_to, None);
}

#[tokio::test]
async fn pages_walk_newest_first_without_gaps_or_repeats() {
    let Some(fixture) = fixture().await else {
        return;
    };
    let (from, to) = (at(1, 0), at(2, 0));
    clear(&fixture.db, from, to).await;
    let mut seeds: Vec<Seed> = (0..5).map(|minute| seed(at(1, minute * 7))).collect();
    let tied = at(1, 30);
    seeds.extend([seed(tied), seed(tied), legacy(tied), legacy(at(1, 2))]);
    insert(&fixture.db, &seeds).await;
    let coverage = coverage(at(0, 0));

    let rows = collect_pages(
        &fixture,
        &coverage,
        with(window(from, to), json!({ "limit": 2 })),
    )
    .await;

    assert_eq!(rows.len(), seeds.len());
    let keys: Vec<Cursor> = rows.iter().map(LogRow::cursor).collect();
    let mut sorted = keys.clone();
    sorted.sort_by(|a, b| b.cmp(a));
    sorted.dedup();
    assert_eq!(keys, sorted);
}

#[tokio::test]
async fn legacy_rows_keep_a_stable_synthesized_id() {
    let Some(fixture) = fixture().await else {
        return;
    };
    let (from, to) = (at(3, 0), at(4, 0));
    clear(&fixture.db, from, to).await;
    insert(&fixture.db, &[legacy(at(3, 5))]).await;
    let coverage = coverage(at(0, 0));

    let first = run(&fixture, &coverage, window(from, to)).await;
    let second = run(&fixture, &coverage, window(from, to)).await;

    let row = &first.rows[0];
    assert!(row.legacy);
    assert_ne!(row.request_key, Uuid::nil());
    assert_eq!(row.request_key, second.rows[0].request_key);
    let opened: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM api_request_log WHERE ts = $1 AND md5('arl:' || id)::uuid = $2",
    )
    .bind(row.ts)
    .bind(row.request_key)
    .fetch_optional(fixture.db.pool())
    .await
    .unwrap();
    assert!(opened.is_some());
}

#[tokio::test]
async fn actor_filters_only_see_rows_after_attribution_began() {
    let Some(fixture) = fixture().await else {
        return;
    };
    let (from, to) = (at(5, 0), at(6, 0));
    clear(&fixture.db, from, to).await;
    let actor = 930_000_000_000_000_005_i64;
    let before = Seed {
        actor_id: Some(actor),
        ..legacy(at(5, 10))
    };
    let after = Seed {
        actor_id: Some(actor),
        key_kind: "internal",
        ..seed(at(5, 40))
    };
    insert(&fixture.db, &[before, after.clone()]).await;
    let coverage = coverage(at(5, 30));

    let page = run(
        &fixture,
        &coverage,
        with(window(from, to), json!({ "actor": actor.to_string() })),
    )
    .await;

    assert_eq!(timestamps(&page.rows), vec![after.ts]);
    assert_eq!(page.clamped_from, Some(at(5, 30)));
}

#[tokio::test]
async fn legacy_routes_match_by_path_pattern_and_new_rows_by_template() {
    let Some(fixture) = fixture().await else {
        return;
    };
    let (from, to) = (at(6, 0), at(7, 0));
    clear(&fixture.db, from, to).await;
    let legacy_resolve = Seed {
        path: "/v3/resolve/oJugg",
        ..legacy(at(6, 5))
    };
    let legacy_other = Seed {
        path: "/v3/resolve/a/b",
        ..legacy(at(6, 6))
    };
    let new_resolve = Seed {
        path: "/v3/resolve/Technoblade",
        route: Some("/v3/resolve/{identifier}"),
        ..seed(at(6, 45))
    };
    insert(
        &fixture.db,
        &[legacy_resolve.clone(), legacy_other, new_resolve.clone()],
    )
    .await;
    let coverage = coverage(at(6, 30));

    let page = run(
        &fixture,
        &coverage,
        with(
            window(from, to),
            json!({ "route": "/v3/resolve/{identifier}" }),
        ),
    )
    .await;

    assert_eq!(
        timestamps(&page.rows),
        vec![new_resolve.ts, legacy_resolve.ts]
    );
    assert_eq!(page.clamped_from, None);
    assert_eq!(page.next_cursor, None);
}

#[tokio::test]
async fn member_streams_join_own_acted_and_legacy_prefix_rows_once_each() {
    let Some(fixture) = fixture().await else {
        return;
    };
    let (from, to) = (at(7, 0), at(8, 0));
    clear(&fixture.db, from, to).await;
    let discord_id = 930_000_000_000_000_007_i64;
    sqlx::query("DELETE FROM members WHERE discord_id = $1")
        .bind(discord_id)
        .execute(fixture.db.pool())
        .await
        .unwrap();
    let member_id: i64 =
        sqlx::query_scalar("INSERT INTO members (discord_id) VALUES ($1) RETURNING id")
            .bind(discord_id)
            .fetch_one(fixture.db.pool())
            .await
            .unwrap();
    sqlx::query(
        "INSERT INTO member_api_keys (member_id, kind, api_key) VALUES ($1, 'personal', $2)",
    )
    .bind(member_id)
    .bind(format!("m7prefix{member_id}"))
    .execute(fixture.db.pool())
    .await
    .unwrap();
    let own_legacy = Seed {
        member_id: Some(member_id),
        ..legacy(at(7, 1))
    };
    let prefix_only = Seed {
        key_prefix: Some("m7prefix"),
        status: 403,
        ..legacy(at(7, 2))
    };
    let acted_legacy = Seed {
        actor_id: Some(discord_id),
        ..legacy(at(7, 3))
    };
    let own = Seed {
        member_id: Some(member_id),
        ..seed(at(7, 40))
    };
    let session = Seed {
        member_id: Some(member_id),
        actor_id: Some(discord_id),
        key_kind: "session",
        ..seed(at(7, 41))
    };
    let acted = Seed {
        actor_id: Some(discord_id),
        key_kind: "internal",
        user_agent: Some("coral-bot"),
        ..seed(at(7, 42))
    };
    let stranger = Seed {
        key_prefix: Some("m7prefix"),
        key_kind: "invalid",
        ..seed(at(7, 43))
    };
    insert(
        &fixture.db,
        &[
            own_legacy.clone(),
            prefix_only.clone(),
            acted_legacy,
            own.clone(),
            session.clone(),
            acted.clone(),
            stranger,
        ],
    )
    .await;
    let coverage = coverage(at(7, 30));

    let page = run(
        &fixture,
        &coverage,
        with(window(from, to), json!({ "member": member_id })),
    )
    .await;

    assert_eq!(
        timestamps(&page.rows),
        vec![acted.ts, session.ts, own.ts, prefix_only.ts, own_legacy.ts]
    );
    assert_eq!(
        page.rows[0].requester(),
        crate::refs::EntityRef::Internal {
            service: reqlog::Service::Bot
        }
    );
    assert_eq!(
        page.rows[0].actor(),
        Some(crate::refs::EntityRef::Account {
            discord_id,
            member_id: Some(member_id)
        })
    );
    assert_eq!(page.rows[1].actor(), None);
}

#[tokio::test]
async fn player_searches_match_uuids_resolved_names_and_held_names() {
    let Some(fixture) = fixture().await else {
        return;
    };
    let (from, to) = (at(10, 0), at(11, 0));
    clear(&fixture.db, from, to).await;
    let player = Uuid::parse_str("7300000a999b4a7ab3add2ae50711e64").unwrap();
    let hex = player.simple().to_string();
    sqlx::query("DELETE FROM player_snapshots WHERE uuid = $1")
        .bind(&hex)
        .execute(fixture.db.pool())
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO player_snapshots (uuid, timestamp, username, is_baseline, data) VALUES
         ($1, $2, 'HeldBefore', true, '{}'), ($1, $3, 'HeldAfter', true, '{}')",
    )
    .bind(&hex)
    .bind(at(0, 0) - TimeDelta::days(30))
    .bind(at(10, 30))
    .execute(fixture.db.pool())
    .await
    .unwrap();
    let by_uuid = seed(at(10, 5));
    let by_resolved_name = seed(at(10, 10));
    let by_old_name_in_window = seed(at(10, 15));
    let by_old_name_too_late = seed(at(10, 45));
    let by_display = seed(at(10, 20));
    let by_resolved_path = Seed {
        path: "/v3/resolve/HeldAfter",
        route: Some("/v3/resolve/{identifier}"),
        ..seed(at(10, 40))
    };
    let both_forms = seed(at(10, 50));
    let all = [
        by_uuid.clone(),
        by_resolved_name.clone(),
        by_old_name_in_window.clone(),
        by_old_name_too_late.clone(),
        by_display.clone(),
        by_resolved_path.clone(),
        both_forms.clone(),
    ];
    insert(&fixture.db, &all).await;
    insert_subject(&fixture.db, &by_uuid, Some(player), None, Form::Undashed).await;
    insert_subject(
        &fixture.db,
        &by_resolved_path,
        Some(player),
        Some("heldafter"),
        Form::Path,
    )
    .await;
    insert_subject(
        &fixture.db,
        &by_resolved_name,
        Some(player),
        Some("heldafter"),
        Form::Name,
    )
    .await;
    insert_subject(
        &fixture.db,
        &by_old_name_in_window,
        None,
        Some("heldbefore"),
        Form::Name,
    )
    .await;
    insert_subject(
        &fixture.db,
        &by_old_name_too_late,
        None,
        Some("heldbefore"),
        Form::Name,
    )
    .await;
    insert_subject(
        &fixture.db,
        &by_display,
        None,
        Some("heldbefore"),
        Form::Display,
    )
    .await;
    insert_subject(
        &fixture.db,
        &both_forms,
        Some(player),
        Some("heldafter"),
        Form::Name,
    )
    .await;
    insert_subject(&fixture.db, &both_forms, Some(player), None, Form::Dashed).await;
    let coverage = coverage(at(0, 0));
    let request = with(window(from, to), json!({ "player": hex }));

    let page = run(&fixture, &coverage, request.clone()).await;

    let matches: Vec<(DateTime<Utc>, Option<Match>)> = page
        .rows
        .iter()
        .map(|row| (row.ts, row.matched()))
        .collect();
    assert_eq!(
        matches,
        vec![
            (both_forms.ts, Some(Match::Uuid)),
            (by_resolved_path.ts, Some(Match::ResolvedName)),
            (by_old_name_in_window.ts, Some(Match::NameOnly)),
            (by_resolved_name.ts, Some(Match::ResolvedName)),
            (by_uuid.ts, Some(Match::Uuid)),
        ]
    );

    let with_display = run(
        &fixture,
        &coverage,
        with(request.clone(), json!({ "include_display": true })),
    )
    .await;
    assert!(
        with_display
            .rows
            .iter()
            .any(|row| row.ts == by_display.ts && row.matched() == Some(Match::DisplayName))
    );

    let paged = collect_pages(&fixture, &coverage, with(request, json!({ "limit": 1 }))).await;
    assert_eq!(timestamps(&paged), timestamps(&page.rows));
}

#[tokio::test]
async fn filters_that_no_era_admits_return_an_empty_finished_page() {
    let Some(fixture) = fixture().await else {
        return;
    };
    let mut coverage = coverage(at(0, 0));
    coverage.attribution_from = None;
    let page = run(
        &fixture,
        &coverage,
        with(
            window(at(21, 0), at(22, 0)),
            json!({ "actor": "111", "q": "x" }),
        ),
    )
    .await;
    assert!(page.rows.is_empty());
    assert_eq!(page.next_cursor, None);
}

#[tokio::test]
async fn player_searches_before_subject_coverage_are_clamped() {
    let Some(fixture) = fixture().await else {
        return;
    };
    let mut coverage = coverage(at(0, 0));
    coverage.subjects_from = Some(at(12, 0));
    let page = run(
        &fixture,
        &coverage,
        with(
            window(at(11, 0), at(13, 0)),
            json!({ "player": "nobody_has_this_name" }),
        ),
    )
    .await;
    assert_eq!(page.clamped_from, Some(at(12, 0)));
}

#[tokio::test]
async fn time_boxed_text_search_pages_through_every_match() {
    let Some(fixture) = fixture().await else {
        return;
    };
    let (from, to) = (at(13, 0), at(19, 0));
    clear(&fixture.db, from, to).await;
    let needle = Uuid::new_v4().simple().to_string();
    let matching: Vec<Seed> = [13, 14, 16, 18]
        .into_iter()
        .map(|hour| Seed {
            query: Some(format!("player={needle}")),
            ..seed(at(hour, 20))
        })
        .collect();
    let noise: Vec<Seed> = (13..19).map(|hour| seed(at(hour, 10))).collect();
    insert(&fixture.db, &matching).await;
    insert(&fixture.db, &noise).await;
    let coverage = coverage(at(0, 0));
    let request = with(window(from, to), json!({ "q": needle, "limit": 3 }));

    let rows = collect_pages(&fixture, &coverage, request).await;

    let mut expected: Vec<DateTime<Utc>> = matching.iter().map(|seed| seed.ts).collect();
    expected.reverse();
    assert_eq!(timestamps(&rows), expected);
}

#[tokio::test]
async fn status_filters_use_the_semantic_status() {
    let Some(fixture) = fixture().await else {
        return;
    };
    let (from, to) = (at(19, 0), at(20, 0));
    clear(&fixture.db, from, to).await;
    let limited = seed(at(19, 40));
    insert(&fixture.db, std::slice::from_ref(&limited)).await;
    sqlx::query("UPDATE api_request_log SET outcome_status = 429, cause = $2 WHERE ts = $1")
        .bind(limited.ts)
        .bind(Cause::CallerLimit.code())
        .execute(fixture.db.pool())
        .await
        .unwrap();
    insert(
        &fixture.db,
        &[
            Seed {
                status: 429,
                ..legacy(at(19, 10))
            },
            seed(at(19, 50)),
        ],
    )
    .await;
    let coverage = coverage(at(19, 30));

    let page = run(
        &fixture,
        &coverage,
        with(window(from, to), json!({ "status": 429 })),
    )
    .await;

    assert_eq!(timestamps(&page.rows), vec![at(19, 40), at(19, 10)]);
    assert_eq!(page.rows[0].cause(), Cause::CallerLimit);
    assert_eq!(page.rows[1].cause(), Cause::CallerLimit);
}

#[tokio::test]
async fn a_tail_behind_by_more_than_a_page_reads_forward_without_gaps() {
    let Some(fixture) = fixture().await else {
        return;
    };
    let (from, to) = (at(22, 0), at(23, 0));
    clear(&fixture.db, from, to).await;
    let seen = seed(at(22, 1));
    let arrived: Vec<Seed> = (10..15).map(|minute| seed(at(22, minute))).collect();
    insert(&fixture.db, std::slice::from_ref(&seen)).await;
    insert(&fixture.db, &arrived).await;
    let coverage = coverage(at(0, 0));
    let mut after = Cursor::new(seen.ts, seen.request_id.unwrap());
    let mut polls = Vec::new();

    loop {
        let page = run(
            &fixture,
            &coverage,
            with(window(from, to), json!({ "after": after, "limit": 2 })),
        )
        .await;
        let newest_first = timestamps(&page.rows);
        let mut sorted = newest_first.clone();
        sorted.sort_by(|a, b| b.cmp(a));
        assert_eq!(newest_first, sorted);
        assert_eq!(page.scanned_to, None);
        polls.push(newest_first);
        match page.next_cursor {
            Some(next) => after = next,
            None => break,
        }
    }

    let expected: Vec<Vec<DateTime<Utc>>> = vec![
        vec![at(22, 11), at(22, 10)],
        vec![at(22, 13), at(22, 12)],
        vec![at(22, 14)],
    ];
    assert_eq!(polls, expected);
}

#[tokio::test]
async fn a_scan_blocked_past_its_budget_comes_back_smaller_and_moves_on() {
    let Some(fixture) = fixture().await else {
        return;
    };
    let (from, to) = (on(BLOCKED_DAY, 0, 0), on(BLOCKED_DAY, 6, 0));
    clear(&fixture.db, from, to).await;
    let needle = Uuid::new_v4().simple().to_string();
    let matching: Vec<Seed> = [1, 5]
        .into_iter()
        .map(|hour| Seed {
            query: Some(format!("player={needle}")),
            ..seed(on(BLOCKED_DAY, hour, 30))
        })
        .collect();
    insert(&fixture.db, &matching).await;
    let coverage = coverage(at(0, 0));
    let request = with(window(from, to), json!({ "q": needle }));
    let key = {
        let query = LogQuery::new(&fixture.db, &coverage, &fixture.players, &fixture.rates);
        ScanKey::of(
            &query
                .plan(Selection::parse(filter(request.clone())).unwrap())
                .await
                .unwrap(),
        )
    };

    let locker = lock_log_partition(&fixture.db, BLOCKED_DAY).await;
    let blocked = run(&fixture, &coverage, request.clone()).await;
    locker.rollback().await.unwrap();

    assert!(blocked.rows.is_empty());
    assert_eq!(blocked.next_cursor, Some(Cursor::before(to)));
    assert_eq!(blocked.scanned_to, Some(to));
    let opening = fixture
        .rates
        .get(&key)
        .await
        .unwrap()
        .span_within(TIME_BOX_BUDGET.mul_f64(OPENING_SLICE_SHARE));
    assert!(opening < FIRST_SLICE_SPAN, "{opening}");

    let resumed = run(
        &fixture,
        &coverage,
        with(request, json!({ "cursor": blocked.next_cursor })),
    )
    .await;
    let progressed = match resumed.next_cursor {
        Some(_) => resumed.scanned_to.is_some_and(|scanned| scanned < to),
        None => true,
    };
    assert!(progressed, "{:?}", resumed.scanned_to);
    assert!(
        resumed
            .rows
            .iter()
            .all(|row| row.ts < to && row.query.as_deref() == matching[0].query.as_deref())
    );
}

#[tokio::test]
async fn a_minimal_slice_that_cannot_finish_fails_with_a_timeout() {
    let Some(fixture) = fixture().await else {
        return;
    };
    let (from, to) = (on(STALLED_DAY, 0, 0), on(STALLED_DAY, 6, 0));
    let coverage = coverage(at(0, 0));
    let request = with(window(from, to), json!({ "q": "never matches" }));
    let key = {
        let query = LogQuery::new(&fixture.db, &coverage, &fixture.players, &fixture.rates);
        ScanKey::of(
            &query
                .plan(Selection::parse(filter(request.clone())).unwrap())
                .await
                .unwrap(),
        )
    };
    let crawling = ScanRate::measured(MIN_SLICE_SPAN, TIME_BOX_BUDGET * 10);
    fixture.rates.remember(key, crawling).await;

    let locker = lock_log_partition(&fixture.db, STALLED_DAY).await;
    let outcome = try_run(&fixture, &coverage, request).await;
    locker.rollback().await.unwrap();

    assert_eq!(outcome.unwrap_err().code(), "timeout");
}

#[tokio::test]
async fn a_scan_without_a_free_connection_fails_instead_of_returning_an_empty_page() {
    let Some(fixture) = fixture().await else {
        return;
    };
    let single = Database::connect_with(
        &std::env::var("DATABASE_URL").unwrap(),
        PoolSettings {
            max_connections: 1,
            acquire_timeout: Duration::from_secs(2),
            statement_timeout: Duration::from_secs(5),
            application_name: "coral-admin-test",
        },
    )
    .await
    .unwrap();
    let held = single.pool().acquire().await.unwrap();
    let coverage = coverage(at(0, 0));

    let outcome = LogQuery::new(&single, &coverage, &fixture.players, &fixture.rates)
        .run(filter(with(
            window(at(13, 0), at(19, 0)),
            json!({ "q": "anything" }),
        )))
        .await;
    drop(held);

    let error = outcome.unwrap_err();
    assert_eq!(error.code(), "timeout");
    assert!(
        error.message().contains("no database connection"),
        "{error}"
    );
}

#[tokio::test]
async fn live_tail_returns_only_rows_newer_than_after() {
    let Some(fixture) = fixture().await else {
        return;
    };
    let (from, to) = (at(20, 0), at(21, 0));
    clear(&fixture.db, from, to).await;
    let seen = seed(at(20, 10));
    let newer = seed(at(20, 20));
    insert(&fixture.db, &[seen.clone(), newer.clone()]).await;
    let coverage = coverage(at(0, 0));
    let after = Cursor::new(seen.ts, seen.request_id.unwrap());

    let page = run(
        &fixture,
        &coverage,
        with(window(from, to), json!({ "after": after })),
    )
    .await;

    assert_eq!(timestamps(&page.rows), vec![newer.ts]);
}
