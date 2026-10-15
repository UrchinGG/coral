use chrono::{DateTime, Days, TimeDelta, TimeZone, Utc};
use coral_redis::minute_history::{
    CHANGED_SNAPSHOTS, FIRST_SNAPSHOTS, HYPIXEL_ERRORS, HYPIXEL_HISTORY, HYPIXEL_REQUESTS,
    SNAPSHOT_HISTORY,
};
use coral_redis::{PERSONAL_RATE_LIMIT, RedisPool, key};
use database::Database;
use redis::AsyncCommands;
use reqlog::rollup::Span;
use reqlog::{Cause, KeyKind, Service};
use sqlx::PgPool;
use tokio::sync::Mutex;

use super::by_route::routes;
use super::callers::{CallerSort, top_callers};
use super::class::{RouteFilter, StatusClass};
use super::history::UpstreamHistory;
use super::pressure::{Limiter, pressure};
use super::range::{RangeQuery, TrafficRange, Window};
use super::rows::TrafficFilter;
use super::series::{SeriesFilter, series};
use super::summary::{Classes, summary};
use super::upstream::{HYPIXEL_COUNTERS, upstream};
use crate::refs::EntityRef;

const DAY: (i32, u32, u32) = (2019, 3, 1);
const MEMBER_DISCORD: i64 = 1_190_000_000_000_000_001;
const PRESSED_DISCORD: i64 = 1_190_000_000_000_000_002;
const PRESSED_KEY: &str = "pressure-test-0000-0000-0000-000000000002";
const PRESSED_USES: i64 = 500;
const BUCKET_COUNT: usize = 12;
const HISTORY_DAY: (i32, u32, u32) = (1970, 1, 10);

static FIXTURE: Mutex<()> = Mutex::const_new(());

const MINUTES: [Minute; 5] = [
    Minute {
        hour: 10,
        minute: 0,
        route: Some("/v3/players"),
        method: "POST",
        status: 200,
        cause: Cause::Ok,
        requests: 10,
        bucket: 1,
        max_ms: 20,
    },
    Minute {
        hour: 10,
        minute: 1,
        route: Some("/v3/players"),
        method: "POST",
        status: 429,
        cause: Cause::CallerLimit,
        requests: 3,
        bucket: 0,
        max_ms: 2,
    },
    Minute {
        hour: 10,
        minute: 5,
        route: None,
        method: "GET",
        status: 404,
        cause: Cause::Unmatched,
        requests: 2,
        bucket: 0,
        max_ms: 1,
    },
    Minute {
        hour: 11,
        minute: 30,
        route: Some("/v3/tags"),
        method: "GET",
        status: 500,
        cause: Cause::Internal,
        requests: 1,
        bucket: 8,
        max_ms: 4_000,
    },
    Minute {
        hour: 11,
        minute: 31,
        route: Some("/v3/tags"),
        method: "GET",
        status: 429,
        cause: Cause::UpstreamLimit,
        requests: 4,
        bucket: 0,
        max_ms: 3,
    },
];

struct Minute {
    hour: u32,
    minute: u32,
    route: Option<&'static str>,
    method: &'static str,
    status: i16,
    cause: Cause,
    requests: i32,
    bucket: usize,
    max_ms: i32,
}

struct CallerFixture {
    member: Option<i64>,
    kind: KeyKind,
    prefix: Option<&'static str>,
    status: i16,
    cause: Cause,
    requests: i32,
    subjects: i64,
}

fn at(hour: u32, minute: u32) -> DateTime<Utc> {
    let (year, month, day) = DAY;
    Utc.with_ymd_and_hms(year, month, day, hour, minute, 0)
        .unwrap()
}

fn now() -> DateTime<Utc> {
    at(12, 0) + TimeDelta::days(1)
}

fn range(from: DateTime<Utc>, to: DateTime<Utc>) -> TrafficRange {
    TrafficRange::between(from, to, now()).unwrap()
}

async fn fixture() -> Option<(PgPool, i64)> {
    let db = Database::connect(&std::env::var("DATABASE_URL").ok()?)
        .await
        .ok()?;
    let pool = db.pool().clone();
    clear(&pool).await;
    for row in &MINUTES {
        insert_row(
            &pool,
            "api_traffic_minute",
            "minute",
            at(row.hour, row.minute),
            row,
        )
        .await;
        insert_row(&pool, "api_traffic_hour", "hour", at(row.hour, 0), row).await;
    }
    let member_id = insert_member(&pool).await;
    insert_callers(&pool, member_id).await;
    Some((pool, member_id))
}

async fn clear(pool: &PgPool) {
    let day = Span::new(at(0, 0), at(0, 0) + TimeDelta::days(1)).unwrap();
    for (table, time) in [
        ("api_traffic_minute", "minute"),
        ("api_traffic_hour", "hour"),
        ("api_caller_hour", "hour"),
    ] {
        sqlx::query(&format!(
            "DELETE FROM {table} WHERE {time} >= $1 AND {time} < $2"
        ))
        .bind(day.from)
        .bind(day.to)
        .execute(pool)
        .await
        .unwrap();
    }
    sqlx::query("DELETE FROM members WHERE discord_id = $1")
        .bind(MEMBER_DISCORD)
        .execute(pool)
        .await
        .unwrap();
}

async fn insert_row(pool: &PgPool, table: &str, time: &str, at: DateTime<Utc>, row: &Minute) {
    let mut buckets = [0_i32; BUCKET_COUNT];
    buckets[row.bucket] = row.requests;
    sqlx::query(&format!(
        "INSERT INTO {table} ({time}, route, method, status, cause, key_kind, requests,
           latency_sum_ms, latency_max_ms, latency_buckets)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"
    ))
    .bind(at)
    .bind(row.route)
    .bind(row.method)
    .bind(row.status)
    .bind(row.cause.code())
    .bind(KeyKind::Personal.code())
    .bind(row.requests)
    .bind(i64::from(row.requests * row.max_ms))
    .bind(row.max_ms)
    .bind(&buckets[..])
    .execute(pool)
    .await
    .unwrap();
}

async fn insert_member(pool: &PgPool) -> i64 {
    sqlx::query_scalar("INSERT INTO members (discord_id) VALUES ($1) RETURNING id")
        .bind(MEMBER_DISCORD)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn insert_callers(pool: &PgPool, member_id: i64) {
    let rows = [
        CallerFixture {
            member: Some(member_id),
            kind: KeyKind::Personal,
            prefix: None,
            status: 200,
            cause: Cause::Ok,
            requests: 10,
            subjects: 900,
        },
        CallerFixture {
            member: Some(member_id),
            kind: KeyKind::Personal,
            prefix: None,
            status: 429,
            cause: Cause::CallerLimit,
            requests: 3,
            subjects: 0,
        },
        CallerFixture {
            member: None,
            kind: KeyKind::Invalid,
            prefix: Some("1a2b3c4d"),
            status: 401,
            cause: Cause::Auth,
            requests: 6,
            subjects: 6,
        },
    ];
    for row in rows {
        sqlx::query(
            "INSERT INTO api_caller_hour (hour, key_kind, member_id, actor_id, key_prefix, service,
               route, method, status, cause, requests, latency_sum_ms, subjects)
             VALUES ($1, $2, $3, NULL, $4, $5, '/v3/players', 'POST', $6, $7, $8, 0, $9)",
        )
        .bind(at(10, 0))
        .bind(row.kind.code())
        .bind(row.member)
        .bind(row.prefix)
        .bind(Service::Unknown.code())
        .bind(row.status)
        .bind(row.cause.code())
        .bind(row.requests)
        .bind(row.subjects)
        .execute(pool)
        .await
        .unwrap();
    }
}

#[tokio::test]
async fn summaries_read_whole_hours_from_the_hour_table_and_edges_from_minutes() {
    let _guard = FIXTURE.lock().await;
    let Some((pool, _)) = fixture().await else {
        return;
    };
    let whole = summary(&pool, range(at(10, 0), at(12, 0)), false, now())
        .await
        .unwrap();
    assert_eq!(whole.totals.requests, 20);
    assert_eq!(
        whole.totals.classes,
        Classes {
            c2xx: 10,
            c4xx: 2,
            c429_caller: 3,
            c429_upstream: 4,
            c5xx: 1
        }
    );
    assert_eq!(whole.totals.errors, 3);
    assert_eq!(whole.causes[&Cause::Unmatched], 2);
    assert_eq!(whole.totals.p99, Some(3_700));
    let edges = summary(&pool, range(at(10, 1), at(11, 31)), false, now())
        .await
        .unwrap();
    assert_eq!(edges.totals.requests, 6);
    let mixed = summary(&pool, range(at(10, 1), at(12, 0)), true, now())
        .await
        .unwrap();
    assert_eq!(mixed.totals.requests, 10);
    assert_eq!(mixed.prev.unwrap().requests, 10);
    clear(&pool).await;
}

#[tokio::test]
async fn series_fill_every_bucket_and_apply_their_filters() {
    let _guard = FIXTURE.lock().await;
    let Some((pool, member_id)) = fixture().await else {
        return;
    };
    let hour = series(&pool, range(at(10, 0), at(11, 0)), SeriesFilter::default())
        .await
        .unwrap();
    assert_eq!(hour.step_s, 60);
    assert_eq!(hour.points.len(), 60);
    assert_eq!(hour.points[0].requests, 10);
    assert!(hour.points[0].p95.unwrap() <= 25);
    assert_eq!(hour.points[1].classes.c429_caller, 3);
    assert_eq!(hour.points[5].classes.c4xx, 2);
    assert_eq!(hour.points[2].requests, 0);
    let unmatched = SeriesFilter {
        traffic: TrafficFilter {
            route: Some(RouteFilter::Unmatched),
            ..TrafficFilter::default()
        },
        ..SeriesFilter::default()
    };
    let only_unmatched = series(&pool, range(at(10, 0), at(11, 0)), unmatched)
        .await
        .unwrap();
    assert_eq!(
        only_unmatched
            .points
            .iter()
            .map(|point| point.requests)
            .sum::<i64>(),
        2
    );
    let by_member = SeriesFilter {
        member: Some(member_id),
        traffic: TrafficFilter {
            class: Some(StatusClass::RateLimited),
            ..TrafficFilter::default()
        },
        ..SeriesFilter::default()
    };
    let member = series(&pool, range(at(10, 15), at(11, 45)), by_member)
        .await
        .unwrap();
    assert_eq!(member.step_s, 3_600);
    assert_eq!(member.points.len(), 2);
    assert_eq!(member.points[0].requests, 3);
    assert_eq!(member.points[0].p95, None);
    clear(&pool).await;
}

#[tokio::test]
async fn routes_rank_by_requests_with_their_errors_limits_and_latency() {
    let _guard = FIXTURE.lock().await;
    let Some((pool, _)) = fixture().await else {
        return;
    };
    let routes = routes(&pool, range(at(10, 0), at(12, 0))).await.unwrap();
    assert_eq!(routes.len(), 3);
    assert_eq!(routes[0].route.as_deref(), Some("/v3/players"));
    assert_eq!(
        (routes[0].requests, routes[0].rate_limited, routes[0].errors),
        (13, 3, 0)
    );
    assert_eq!(routes[0].spark.iter().sum::<i64>(), 13);
    let tags = routes
        .iter()
        .find(|route| route.route.as_deref() == Some("/v3/tags"))
        .unwrap();
    assert_eq!((tags.requests, tags.errors, tags.rate_limited), (5, 1, 4));
    let unmatched = routes.iter().find(|route| route.route.is_none()).unwrap();
    assert_eq!(unmatched.error_rate, 1.0);
    clear(&pool).await;
}

#[tokio::test]
async fn callers_are_ranked_by_the_chosen_measure_over_whole_hours() {
    let _guard = FIXTURE.lock().await;
    let Some((pool, member_id)) = fixture().await else {
        return;
    };
    let by_requests = top_callers(
        &pool,
        range(at(10, 20), at(10, 40)),
        TrafficFilter::default(),
        CallerSort::Requests,
        10,
    )
    .await
    .unwrap();
    assert_eq!(by_requests.range.from, at(10, 0));
    assert_eq!(by_requests.range.to, at(11, 0));
    assert_eq!(by_requests.callers.len(), 2);
    let member = &by_requests.callers[0];
    assert_eq!(
        member.requester,
        EntityRef::Account {
            discord_id: MEMBER_DISCORD,
            member_id: Some(member_id)
        }
    );
    assert_eq!(
        (member.requests, member.rate_limited, member.subjects),
        (13, 3, 900)
    );
    let by_errors = top_callers(
        &pool,
        range(at(10, 20), at(10, 40)),
        TrafficFilter::default(),
        CallerSort::Errors,
        1,
    )
    .await
    .unwrap();
    assert_eq!(by_errors.callers.len(), 1);
    assert_eq!(
        by_errors.callers[0].requester,
        EntityRef::Key {
            key_id: None,
            prefix: "1a2b3c4d".into(),
            key_kind: KeyKind::Invalid
        }
    );
    clear(&pool).await;
}

async fn redis() -> Option<RedisPool> {
    RedisPool::connect(&std::env::var("REDIS_URL").ok()?)
        .await
        .ok()
}

async fn create_log_partition(pool: &PgPool, day: DateTime<Utc>) {
    let from = day.date_naive();
    sqlx::query(&format!(
        "CREATE TABLE IF NOT EXISTS api_request_log_{} PARTITION OF api_request_log
         FOR VALUES FROM ('{from}') TO ('{to}')",
        from.format("%Y%m%d"),
        to = from + Days::new(1),
    ))
    .execute(pool)
    .await
    .unwrap();
}

async fn clear_pressed(pool: &PgPool, redis: &RedisPool) {
    sqlx::query(
        "DELETE FROM api_request_log WHERE ts >= $1 AND ts < $2
           AND member_id IN (SELECT id FROM members WHERE discord_id = $3)",
    )
    .bind(now() - TimeDelta::hours(1))
    .bind(now())
    .bind(PRESSED_DISCORD)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("DELETE FROM members WHERE discord_id = $1")
        .bind(PRESSED_DISCORD)
        .execute(pool)
        .await
        .unwrap();
    let () = redis.connection().del(key(PRESSED_KEY)).await.unwrap();
}

#[tokio::test]
async fn pressure_counts_the_limits_of_recent_callers_and_keeps_those_near_one() {
    let _guard = FIXTURE.lock().await;
    let (Some((pool, _)), Some(redis)) = (fixture().await, redis().await) else {
        return;
    };
    create_log_partition(&pool, now()).await;
    clear_pressed(&pool, &redis).await;
    let member_id: i64 = sqlx::query_scalar(
        "INSERT INTO members (discord_id, api_key) VALUES ($1, $2) RETURNING id",
    )
    .bind(PRESSED_DISCORD)
    .bind(PRESSED_KEY)
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO api_request_log (ts, method, path, status, key_kind, member_id, latency_ms)
         VALUES ($1, 'GET', '/v3/player', 200, 'personal', $2, 1)",
    )
    .bind(now() - TimeDelta::minutes(1))
    .bind(member_id)
    .execute(&pool)
    .await
    .unwrap();
    let scored = Utc::now().timestamp();
    let entries: Vec<(i64, String)> = (0..PRESSED_USES)
        .map(|entry| (scored, format!("{scored}:{entry}")))
        .collect();
    let () = redis
        .connection()
        .zadd_multiple(key(PRESSED_KEY), &entries)
        .await
        .unwrap();
    let found = pressure(&pool, &redis, now()).await;
    clear_pressed(&pool, &redis).await;
    clear(&pool).await;
    let found = found.unwrap();
    let pressed = found
        .accounts
        .iter()
        .find(|account| account.discord_id == PRESSED_DISCORD)
        .unwrap();
    let worst = pressed.worst().unwrap();
    assert_eq!(worst.limiter, Limiter::Personal);
    assert_eq!(
        (worst.used, worst.limit),
        (PRESSED_USES, PERSONAL_RATE_LIMIT)
    );
    assert!(found.candidates >= 1);
}

#[tokio::test]
async fn upstream_buckets_the_minute_histories_of_the_range() {
    let Some(redis) = redis().await else {
        return;
    };
    let (year, month, day) = HISTORY_DAY;
    let start = Utc.with_ymd_and_hms(year, month, day, 10, 0, 0).unwrap();
    let keys = [
        HYPIXEL_HISTORY.slot(start).key,
        SNAPSHOT_HISTORY.slot(start).key,
    ];
    let mut conn = redis.connection();
    let () = conn.del(&keys).await.unwrap();
    for (offset, counters) in [
        (0, &[HYPIXEL_REQUESTS, HYPIXEL_ERRORS][..]),
        (1, &[HYPIXEL_REQUESTS]),
        (59, &[HYPIXEL_REQUESTS]),
    ] {
        let at = start + TimeDelta::minutes(offset);
        HYPIXEL_HISTORY
            .record(&mut conn, at, counters)
            .await
            .unwrap();
    }
    let at = start + TimeDelta::minutes(2);
    SNAPSHOT_HISTORY
        .record(&mut conn, at, &[FIRST_SNAPSHOTS, CHANGED_SNAPSHOTS])
        .await
        .unwrap();
    let now = start + TimeDelta::minutes(59) + TimeDelta::seconds(30);
    let range = RangeQuery {
        window: Some(Window::Hour),
        ..RangeQuery::default()
    }
    .resolve(now)
    .unwrap();
    let found = upstream(&redis, &UpstreamHistory::new(), range, now).await;
    let () = conn.del(&keys).await.unwrap();
    let found = found.unwrap();
    assert_eq!(found.step_s, 60);
    assert_eq!(found.series.len(), 60);
    assert_eq!(found.series[0].t, start);
    assert_eq!((found.series[0].requests, found.series[0].errors), (1, 1));
    assert_eq!(found.series[1].requests, 1);
    assert_eq!(found.series[59].requests, 1);
    assert_eq!(
        found.series.iter().map(|point| point.requests).sum::<i64>(),
        3
    );
    assert_eq!((found.snapshots[2].first, found.snapshots[2].delta), (1, 1));
}

#[tokio::test]
async fn closed_history_days_are_read_once_and_open_days_every_time() {
    let Some(redis) = redis().await else {
        return;
    };
    let closed = Utc.with_ymd_and_hms(1970, 1, 12, 10, 0, 0).unwrap();
    let open = closed + TimeDelta::days(1);
    let now = open + TimeDelta::hours(1);
    let keys = [
        HYPIXEL_HISTORY.slot(closed).key,
        HYPIXEL_HISTORY.slot(open).key,
    ];
    let mut conn = redis.connection();
    let () = conn.del(&keys).await.unwrap();
    for at in [closed, open] {
        HYPIXEL_HISTORY
            .record(&mut conn, at, &[HYPIXEL_REQUESTS])
            .await
            .unwrap();
    }
    let history = UpstreamHistory::new();
    let span = (closed - TimeDelta::hours(1), now);
    let read = || {
        history.read(
            &redis,
            HYPIXEL_HISTORY,
            HYPIXEL_COUNTERS,
            span.0,
            span.1,
            now,
        )
    };
    let first = read()
        .await
        .unwrap()
        .total(HYPIXEL_REQUESTS, span.0, span.1);
    for at in [closed, open] {
        HYPIXEL_HISTORY
            .record(&mut conn, at, &[HYPIXEL_REQUESTS])
            .await
            .unwrap();
    }
    let second = read()
        .await
        .unwrap()
        .total(HYPIXEL_REQUESTS, span.0, span.1);
    let () = conn.del(&keys).await.unwrap();
    assert_eq!(first, 2);
    assert_eq!(second, 3);
}
