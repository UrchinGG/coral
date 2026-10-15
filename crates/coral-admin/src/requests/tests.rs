use std::sync::Arc;

use chrono::{DateTime, TimeDelta, TimeZone, Utc};
use coral_redis::RedisPool;
use database::Database;
use reqlog::{Cause, Form, KeyKind};
use serde_json::{Value, json};
use tokio::sync::OnceCell;
use uuid::Uuid;

use super::caller::{self, KeyState};
use super::detail::DetailState;
use super::*;
use crate::coverage::Coverage;
use crate::error::AdminError;
use crate::names::NameService;
use crate::query::{LogQuery, PlayerSearch, RequestFilter, ScanRates};
use crate::refs::{EntityRef, Page, RequestRow};

const LIST_DAY: (i32, u32, u32) = (2020, 2, 1);
const DETAIL_DAY: (i32, u32, u32) = (2020, 2, 2);
const PARTITION_DAYS: [(i32, u32, u32); 2] = [LIST_DAY, DETAIL_DAY];
const PARTITIONED: [&str; 3] = [
    "api_request_log",
    "api_request_detail",
    "api_request_subject",
];
const GROUP_DAY: (i32, u32, u32) = (2099, 1, 1);
const OJUGG: &str = "94589c87999b4a7ab3add2ae50711e64";
const OJUGG_DASHED: &str = "94589c87-999b-4a7a-b3ad-d2ae50711e64";
const TECHNO: &str = "b876ec32e396476ba1158438d83c67d4";
const LIST_MEMBER: i64 = 910_000_001;
const LIST_DISCORD: i64 = 930_000_000_000_000_001;
const DETAIL_MEMBER: i64 = 910_000_002;
const DETAIL_DISCORD: i64 = 930_000_000_000_000_002;
const COUNTRY_MEMBER: i64 = 910_000_003;
const CURRENT_KEY: &str = "5e1ec7ed-0000-4000-8000-000000000001";
const RETIRED_KEY: &str = "5e1ec7ed-0000-4000-8000-000000000002";
const UNKNOWN_KEY: &str = "0badc0de-0000-4000-8000-000000000003";
const DERIVED_ID_SEARCH: &str =
    "SELECT g FROM generate_series(9100000000::int8, 9100001000::int8) g
     WHERE substr(md5('arl:' || g), 13, 1) = $1 LIMIT 1";
const TIME_ORDERED_NIBBLE: &str = "7";
const RANDOM_NIBBLE: &str = "4";
const UNVERSIONED_ID: &str = "1c4b2a90-5d6e-1f70-8a91-b2c3d4e5f607";

static TEST_PARTITIONS: OnceCell<()> = OnceCell::const_new();

struct Fixture {
    db: Arc<Database>,
    names: NameService,
    players: PlayerSearch,
    rates: ScanRates,
}

#[derive(Clone)]
struct Row {
    id: Option<i64>,
    ts: DateTime<Utc>,
    request_id: Option<Uuid>,
    method: &'static str,
    path: &'static str,
    route: Option<&'static str>,
    query: Option<&'static str>,
    status: i16,
    key_kind: &'static str,
    key_prefix: Option<&'static str>,
    member_id: Option<i64>,
    ip: Option<&'static str>,
    country: Option<&'static str>,
    subjects: Option<i16>,
    error: Option<&'static str>,
}

struct Detail {
    headers: Value,
    body: Option<String>,
    server_error: Option<&'static str>,
}

fn on((year, month, day): (i32, u32, u32), hour: u32, minute: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(year, month, day, hour, minute, 0)
        .unwrap()
}

fn uuid(raw: &str) -> Uuid {
    Uuid::parse_str(raw).unwrap()
}

fn account(discord_id: i64, member_id: i64) -> EntityRef {
    EntityRef::Account {
        discord_id,
        member_id: Some(member_id),
    }
}

fn row(ts: DateTime<Utc>) -> Row {
    Row {
        id: None,
        ts,
        request_id: Some(reqlog::request_id_for(ts)),
        method: "GET",
        path: "/v3/player/tags",
        route: Some("/v3/player/tags"),
        query: None,
        status: 200,
        key_kind: "personal",
        key_prefix: Some("5e1ec7ed"),
        member_id: Some(LIST_MEMBER),
        ip: Some("203.0.113.9"),
        country: Some("NL"),
        subjects: Some(0),
        error: None,
    }
}

fn legacy(ts: DateTime<Utc>) -> Row {
    Row {
        request_id: None,
        route: None,
        key_kind: "key",
        country: None,
        subjects: None,
        ..row(ts)
    }
}

fn detail(headers: Value) -> Detail {
    Detail {
        headers,
        body: None,
        server_error: None,
    }
}

fn coverage() -> Coverage {
    Coverage {
        log_from: Some(on(LIST_DAY, 0, 0)),
        detail_since: Some(on(LIST_DAY, 0, 0)),
        detail_from: Some(on(LIST_DAY, 0, 0)),
        attribution_from: Some(on(LIST_DAY, 0, 0)),
        subjects_from: Some(on(LIST_DAY, 0, 0)),
        subject_partitions_from: Some(on(LIST_DAY, 0, 0)),
        rollups_from: Some(on(GROUP_DAY, 0, 0)),
        ..Coverage::default()
    }
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
    TEST_PARTITIONS.get_or_init(|| prepare(&db)).await;
    let (names, _workers) = NameService::new(db.clone(), redis.clone(), None);
    Some(Fixture {
        players: PlayerSearch::new(db.clone(), redis),
        rates: ScanRates::new(),
        names,
        db,
    })
}

async fn prepare(db: &Database) {
    for day in PARTITION_DAYS {
        create_partitions(db, day).await;
        clear_day(db, day).await;
    }
    for (member, discord) in [(LIST_MEMBER, LIST_DISCORD), (DETAIL_MEMBER, DETAIL_DISCORD)] {
        sqlx::query(
            "INSERT INTO members (id, discord_id) VALUES ($1, $2) ON CONFLICT (id) DO NOTHING",
        )
        .bind(member)
        .bind(discord)
        .execute(db.pool())
        .await
        .unwrap();
    }
    for (key, retired) in [(CURRENT_KEY, false), (RETIRED_KEY, true)] {
        sqlx::query(
            "INSERT INTO member_api_keys (member_id, kind, api_key, created_at, retired_at)
             VALUES ($1, 'personal', $2, now(), CASE WHEN $3 THEN now() END)
             ON CONFLICT (api_key) DO NOTHING",
        )
        .bind(DETAIL_MEMBER)
        .bind(key)
        .bind(retired)
        .execute(db.pool())
        .await
        .unwrap();
    }
}

async fn create_partitions(db: &Database, (year, month, day): (i32, u32, u32)) {
    for table in PARTITIONED {
        execute(
            db,
            &format!(
                "CREATE TABLE IF NOT EXISTS {table}_{year:04}{month:02}{day:02} PARTITION OF {table}
                 FOR VALUES FROM ('{year:04}-{month:02}-{day:02}') TO ('{year:04}-{month:02}-{next:02}')",
                next = day + 1,
            ),
        )
        .await;
    }
}

async fn clear_day(db: &Database, (year, month, day): (i32, u32, u32)) {
    for table in PARTITIONED {
        execute(
            db,
            &format!("DELETE FROM {table}_{year:04}{month:02}{day:02}"),
        )
        .await;
    }
}

async fn execute(db: &Database, sql: &str) {
    sqlx::query(sql).execute(db.pool()).await.unwrap();
}

async fn insert(db: &Database, row: &Row) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO api_request_log (id, ts, request_id, method, path, route, query, status,
           key_kind, key_prefix, member_id, ip, country, subjects, error, latency_ms)
         VALUES (coalesce($1, nextval('api_request_log_id_seq')), $2, $3, $4, $5, $6, $7, $8,
           $9, $10, $11, $12, $13, $14, $15, 3)
         RETURNING coalesce(request_id, md5('arl:' || id)::uuid)",
    )
    .bind(row.id)
    .bind(row.ts)
    .bind(row.request_id)
    .bind(row.method)
    .bind(row.path)
    .bind(row.route)
    .bind(row.query)
    .bind(row.status)
    .bind(row.key_kind)
    .bind(row.key_prefix)
    .bind(row.member_id)
    .bind(row.ip)
    .bind(row.country)
    .bind(row.subjects)
    .bind(row.error)
    .fetch_one(db.pool())
    .await
    .unwrap()
}

async fn insert_detail(db: &Database, row: &Row, detail: Detail) {
    let body_bytes = detail.body.as_ref().map_or(0, String::len) as i32;
    sqlx::query(
        "INSERT INTO api_request_detail (request_id, ts, member_id, headers, body, body_bytes,
           body_complete, server_error)
         VALUES ($1, $2, $3, $4, $5, $6, true, $7)",
    )
    .bind(row.request_id.unwrap())
    .bind(row.ts)
    .bind(row.member_id)
    .bind(detail.headers)
    .bind(detail.body)
    .bind(body_bytes)
    .bind(detail.server_error)
    .execute(db.pool())
    .await
    .unwrap();
}

async fn insert_subject(db: &Database, row: &Row, form: Form, player: Uuid) {
    sqlx::query(
        "INSERT INTO api_request_subject (ts, request_id, member_id, key_kind, service, status,
           form, player)
         VALUES ($1, $2, $3, 1, 0, $4, $5, $6)",
    )
    .bind(row.ts)
    .bind(row.request_id.unwrap())
    .bind(row.member_id)
    .bind(row.status)
    .bind(form.code())
    .bind(player)
    .execute(db.pool())
    .await
    .unwrap();
}

async fn insert_caller_hour(db: &Database, hour: u32, member: i64, status: i16, requests: i32) {
    let cause = match status {
        429 => Cause::CallerLimit,
        _ => Cause::Ok,
    };
    sqlx::query(
        "INSERT INTO api_caller_hour (hour, key_kind, member_id, service, route, method, status,
           cause, requests, latency_sum_ms, subjects)
         VALUES ($1, $2, $3, 0, '/v3/player/tags', 'GET', $4, $5, $6, 0, $6)",
    )
    .bind(on(GROUP_DAY, hour, 0))
    .bind(KeyKind::Personal.code())
    .bind(member)
    .bind(status)
    .bind(cause.code())
    .bind(requests)
    .execute(db.pool())
    .await
    .unwrap();
}

async fn derived_row_id(db: &Database, nibble: &str) -> i64 {
    sqlx::query_scalar(DERIVED_ID_SEARCH)
        .bind(nibble)
        .fetch_one(db.pool())
        .await
        .unwrap()
}

async fn insert_without_request_id(db: &Database, nibble: &str, ts: DateTime<Utc>) -> Uuid {
    let id = derived_row_id(db, nibble).await;
    sqlx::query("DELETE FROM api_request_log WHERE id = $1")
        .bind(id)
        .execute(db.pool())
        .await
        .unwrap();
    insert(
        db,
        &Row {
            id: Some(id),
            ..legacy(ts)
        },
    )
    .await
}

async fn list(fixture: &Fixture, body: Value) -> Page<RequestRow> {
    let coverage = coverage();
    let filter: RequestFilter = serde_json::from_value(body).unwrap();
    let page = LogQuery::new(&fixture.db, &coverage, &fixture.players, &fixture.rates)
        .run(filter)
        .await
        .unwrap();
    RequestRows {
        db: &fixture.db,
        names: &fixture.names,
        coverage: &coverage,
    }
    .page(page)
    .await
    .unwrap()
}

async fn open(
    fixture: &Fixture,
    request_id: Uuid,
    t: Option<DateTime<Utc>>,
    coverage: &Coverage,
) -> Result<RequestFull, DetailError> {
    read_request(&fixture.db, &fixture.names, coverage, request_id, t).await
}

async fn groups(fixture: &Fixture, body: Value) -> Result<GroupPage, AdminError> {
    let filter: GroupFilter = serde_json::from_value(body).unwrap();
    read_groups(&fixture.db, &fixture.names, &coverage(), filter).await
}

fn admin_error(result: Result<RequestFull, DetailError>) -> AdminError {
    match result {
        Err(DetailError::Admin(error)) => error,
        Err(DetailError::NeedsTimestamp(id)) => panic!("{id} unexpectedly needs a timestamp"),
        Ok(full) => panic!("{} unexpectedly opened", full.request.request_id),
    }
}

#[tokio::test]
async fn listed_rows_carry_their_subjects_body_counts_and_client_networks() {
    let Some(fixture) = fixture().await else {
        return;
    };
    let tags = Row {
        query: Some("player=%20oJugg+&uuid=94589c87-999b-4a7a-b3ad-d2ae50711e64"),
        subjects: Some(2),
        ..row(on(LIST_DAY, 10, 0))
    };
    let batch = Row {
        method: "POST",
        path: "/v3/players",
        route: Some("/v3/players"),
        subjects: Some(3),
        ip: Some("198.51.100.7"),
        country: None,
        ..row(on(LIST_DAY, 10, 1))
    };
    insert(&fixture.db, &tags).await;
    insert(&fixture.db, &batch).await;
    let derived = insert(&fixture.db, &legacy(on(LIST_DAY, 10, 2))).await;
    insert_detail(
        &fixture.db,
        &batch,
        detail(json!({ "cf-worker": "proxy.example", "cf-ipcountry": "DE" })),
    )
    .await;

    let page = list(
        &fixture,
        json!({ "from": on(LIST_DAY, 9, 0), "to": on(LIST_DAY, 11, 0), "member": LIST_MEMBER }),
    )
    .await;
    let [legacy_row, batch_row, tags_row] = page.items.as_slice() else {
        panic!("expected three rows, got {}", page.items.len());
    };

    assert_eq!(legacy_row.request_id, derived);
    assert_eq!(legacy_row.key_kind, KeyKind::Legacy);
    assert_eq!(legacy_row.route.as_deref(), Some("/v3/player/tags"));

    assert!(batch_row.subjects.is_empty());
    assert_eq!(batch_row.body_subjects, 3);
    assert_eq!(batch_row.country.as_deref(), Some("DE"));
    assert_eq!(
        batch_row.ip,
        Some(EntityRef::Ip {
            ip: "198.51.100.7".into(),
            country: Some("DE".into()),
            proxied: true,
        })
    );

    let raws: Vec<(&str, Form)> = tags_row
        .subjects
        .iter()
        .map(|subject| (subject.raw.as_str(), subject.form))
        .collect();
    assert_eq!(
        raws,
        vec![("oJugg", Form::Name), (OJUGG_DASHED, Form::Dashed)]
    );
    assert_eq!(tags_row.body_subjects, 0);
    assert_eq!(
        tags_row.ip,
        Some(EntityRef::Ip {
            ip: "203.0.113.9".into(),
            country: Some("NL".into()),
            proxied: false,
        })
    );
    assert_eq!(tags_row.requester, account(LIST_DISCORD, LIST_MEMBER));
}

#[tokio::test]
async fn the_country_filter_reads_attributed_rows_only() {
    let Some(fixture) = fixture().await else {
        return;
    };
    let base = on(LIST_DAY, 18, 0);
    for (offset, country) in [(0, Some("NL")), (1, Some("DE")), (2, None)] {
        let ts = base + TimeDelta::seconds(offset);
        insert(
            &fixture.db,
            &Row {
                member_id: Some(COUNTRY_MEMBER),
                country,
                ..row(ts)
            },
        )
        .await;
    }
    let page = list(
        &fixture,
        json!({ "from": base, "to": base + TimeDelta::minutes(1), "country": "de" }),
    )
    .await;
    let countries: Vec<Option<&str>> = page
        .items
        .iter()
        .map(|row| row.country.as_deref())
        .collect();
    assert_eq!(countries, vec![Some("DE")]);
}

#[tokio::test]
async fn a_request_opens_with_every_header_its_body_its_params_and_its_caller() {
    let Some(fixture) = fixture().await else {
        return;
    };
    let earlier = Row {
        member_id: Some(DETAIL_MEMBER),
        ..row(on(DETAIL_DAY, 9, 58))
    };
    let opened = Row {
        method: "POST",
        path: "/v3/players",
        route: Some("/v3/players"),
        query: Some("key=5e1ec7ed-0000-4000-8000-000000000002"),
        status: 500,
        member_id: Some(DETAIL_MEMBER),
        subjects: Some(2),
        error: Some(r#"{"error":"Internal server error"}"#),
        ..row(on(DETAIL_DAY, 10, 0))
    };
    let later = Row {
        member_id: Some(DETAIL_MEMBER),
        ..row(on(DETAIL_DAY, 10, 3))
    };
    let nearby = Row {
        query: Some("uuid=94589c87999b4a7ab3add2ae50711e64"),
        ..row(on(DETAIL_DAY, 10, 2))
    };
    for seeded in [&earlier, &opened, &later, &nearby] {
        insert(&fixture.db, seeded).await;
    }
    let body = format!(r#"{{"uuids":["{OJUGG}","{TECHNO}"]}}"#);
    insert_detail(
        &fixture.db,
        &opened,
        Detail {
            body: Some(body.clone()),
            server_error: Some("Internal(\"database: connection reset\")"),
            ..detail(json!({ "host": "api.urchin.gg", "user-agent": "curl/8.5.0", "cf-ipcountry": "NL" }))
        },
    )
    .await;
    insert_subject(&fixture.db, &opened, Form::Body, uuid(OJUGG)).await;
    insert_subject(&fixture.db, &opened, Form::Body, uuid(TECHNO)).await;
    insert_subject(&fixture.db, &nearby, Form::Undashed, uuid(OJUGG)).await;

    let full = open(&fixture, opened.request_id.unwrap(), None, &coverage())
        .await
        .unwrap();

    assert_eq!(full.detail_state, DetailState::Present);
    assert_eq!(
        full.url,
        "https://api.urchin.gg/v3/players?key=5e1ec7ed-0000-4000-8000-000000000002"
    );
    assert_eq!(full.headers.as_ref().map(|headers| headers.len()), Some(3));
    assert_eq!(full.body.as_deref(), Some(body.as_str()));
    assert_eq!(full.body_json, Some(json!({ "uuids": [OJUGG, TECHNO] })));
    assert_eq!(
        full.server_error.as_deref(),
        Some("Internal(\"database: connection reset\")")
    );
    assert_eq!(full.request.cause, Cause::Internal);

    let players: Vec<Option<Uuid>> = full
        .subjects
        .iter()
        .map(|involved| involved.subject.player)
        .collect();
    assert_eq!(players, vec![Some(uuid(OJUGG)), Some(uuid(TECHNO))]);

    let [key_param] = full.query_params.as_slice() else {
        panic!("expected one query parameter");
    };
    assert_eq!((key_param.name.as_str(), &key_param.entity), ("key", &None));

    let key = full.caller.key.as_ref().unwrap();
    assert_eq!(key.key, RETIRED_KEY);
    assert_eq!(key.state, KeyState::Retired);
    assert_eq!(key.owner, Some(account(DETAIL_DISCORD, DETAIL_MEMBER)));
    assert_eq!(
        full.caller.account,
        Some(account(DETAIL_DISCORD, DETAIL_MEMBER))
    );

    let request_ref = |seeded: &Row| EntityRef::Request {
        request_id: seeded.request_id.unwrap(),
        ts: seeded.ts,
    };
    assert_eq!(
        full.neighbors.prev_same_requester,
        Some(request_ref(&earlier))
    );
    assert_eq!(
        full.neighbors.next_same_requester,
        Some(request_ref(&later))
    );
    assert_eq!(full.neighbors.same_player_5m, 1);
}

#[tokio::test]
async fn keys_are_current_retired_or_unknown() {
    let Some(fixture) = fixture().await else {
        return;
    };
    let current = caller::key_detail(&fixture.db, CURRENT_KEY.into())
        .await
        .unwrap();
    assert_eq!(current.state, KeyState::Current);
    assert!(current.key_id.is_some());
    let unknown = caller::key_detail(&fixture.db, UNKNOWN_KEY.into())
        .await
        .unwrap();
    assert_eq!(unknown.state, KeyState::Unknown);
    assert_eq!((unknown.key_id, unknown.owner), (None, None));
}

#[tokio::test]
async fn rows_without_a_request_id_open_with_their_timestamp() {
    let Some(fixture) = fixture().await else {
        return;
    };
    let before_detail = Coverage {
        detail_since: Some(on(DETAIL_DAY, 12, 0)),
        ..coverage()
    };
    let mut derived_ids = Vec::new();
    for (minute, nibble) in [(20, RANDOM_NIBBLE), (21, TIME_ORDERED_NIBBLE)] {
        let ts = on(DETAIL_DAY, 11, minute) + TimeDelta::microseconds(412_318);
        let derived = insert_without_request_id(&fixture.db, nibble, ts).await;
        let full = open(&fixture, derived, Some(ts), &before_detail)
            .await
            .unwrap();
        assert_eq!((full.request.request_id, full.request.ts), (derived, ts));
        assert_eq!(full.detail_state, DetailState::NotCaptured);
        assert_eq!(full.headers, None);
        derived_ids.push(derived);
    }
    let error = admin_error(open(&fixture, derived_ids[0], None, &before_detail).await);
    assert_eq!(error.code(), "not_found");
    let unversioned = uuid(UNVERSIONED_ID);
    assert!(matches!(
        open(&fixture, unversioned, None, &before_detail).await,
        Err(DetailError::NeedsTimestamp(id)) if id == unversioned
    ));
}

#[tokio::test]
async fn random_ids_without_a_timestamp_are_found_through_their_detail() {
    let Some(fixture) = fixture().await else {
        return;
    };
    let random = Uuid::new_v4();
    let seeded = Row {
        request_id: Some(random),
        route: None,
        key_kind: "key",
        ..row(on(DETAIL_DAY, 14, 0) + TimeDelta::microseconds(7))
    };
    insert(&fixture.db, &seeded).await;
    insert_detail(&fixture.db, &seeded, detail(json!({ "cf-worker": "w" }))).await;
    let full = open(&fixture, random, None, &coverage()).await.unwrap();
    assert_eq!(full.request.ts, seeded.ts);
    assert!(matches!(
        full.request.ip,
        Some(EntityRef::Ip { proxied: true, .. })
    ));
}

#[tokio::test]
async fn missing_detail_is_expired_once_its_partition_is_gone() {
    let Some(fixture) = fixture().await else {
        return;
    };
    let seeded = row(on(DETAIL_DAY, 15, 0) + TimeDelta::microseconds(99));
    insert(&fixture.db, &seeded).await;
    let dropped = Coverage {
        detail_from: Some(on(DETAIL_DAY, 16, 0)),
        ..coverage()
    };
    let full = open(&fixture, seeded.request_id.unwrap(), None, &dropped)
        .await
        .unwrap();
    assert_eq!(full.detail_state, DetailState::Expired);
}

#[tokio::test]
async fn requester_groups_come_from_whole_caller_hours() {
    let Some(fixture) = fixture().await else {
        return;
    };
    sqlx::query("DELETE FROM api_caller_hour WHERE hour >= $1 AND hour < $2")
        .bind(on(GROUP_DAY, 0, 0))
        .bind(on(GROUP_DAY, 23, 0))
        .execute(fixture.db.pool())
        .await
        .unwrap();
    insert_caller_hour(&fixture.db, 10, LIST_MEMBER, 200, 40).await;
    insert_caller_hour(&fixture.db, 11, LIST_MEMBER, 429, 5).await;
    insert_caller_hour(&fixture.db, 11, DETAIL_MEMBER, 200, 70).await;

    let page = groups(
        &fixture,
        json!({ "from": on(GROUP_DAY, 10, 30), "to": on(GROUP_DAY, 11, 15) }),
    )
    .await
    .unwrap();
    assert_eq!(
        (page.from, page.to),
        (on(GROUP_DAY, 10, 0), on(GROUP_DAY, 12, 0))
    );
    let totals: Vec<(i64, i64, i64)> = page
        .groups
        .iter()
        .map(|group| (group.requests, group.rate_limited, group.subjects))
        .collect();
    assert_eq!(totals, vec![(70, 0, 70), (45, 5, 45)]);
    assert_eq!(page.groups[1].requester, account(LIST_DISCORD, LIST_MEMBER));
    assert!(!page.truncated);

    let limited = groups(
        &fixture,
        json!({
            "from": on(GROUP_DAY, 10, 0),
            "to": on(GROUP_DAY, 12, 0),
            "sort": "rate_limited",
            "limit": 1,
        }),
    )
    .await
    .unwrap();
    assert!(limited.truncated);
    assert_eq!(limited.groups[0].rate_limited, 5);

    let member = groups(
        &fixture,
        json!({ "from": on(GROUP_DAY, 10, 0), "to": on(GROUP_DAY, 12, 0), "member": DETAIL_MEMBER }),
    )
    .await
    .unwrap();
    assert_eq!(member.groups.len(), 1);
    assert_eq!(member.groups[0].requests, 70);

    let unrolled = serde_json::from_value::<GroupFilter>(json!({ "ip": "203.0.113.9" }));
    assert!(unrolled.is_err());
}
