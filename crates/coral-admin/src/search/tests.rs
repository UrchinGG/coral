use std::sync::Arc;

use chrono::{DateTime, TimeZone, Utc};
use coral_redis::RedisPool;
use database::Database;
use serde_json::json;
use tokio::sync::{Mutex, MutexGuard, OnceCell};
use uuid::Uuid;

use super::rank::{
    CURRENT_NAME, DISCORD_NAME_PARTIAL, DISCORD_USER, EXACT_KEY, FORMER_NAME, GUILD_OR_PLUGIN,
    IDENTIFIED, IP_WITH_TRAFFIC, KEY_PREFIX, LOOKUPS_TAB, MEMBER_NUMBER, REQUESTS_NAMING,
    UNPROFILED_PLAYER,
};
use super::*;
use crate::coverage::Coverage;
use crate::names::NameService;

const DAY: (i32, u32, u32) = (2020, 1, 5);
const PARTITIONED: [&str; 3] = [
    "api_request_log",
    "api_request_subject",
    "api_request_detail",
];

const OWNER_MEMBER: i64 = 910_001;
const OWNER_DISCORD: i64 = 940_000_000_000_000_001;
const RETIRED_MEMBER: i64 = 910_002;
const RETIRED_DISCORD: i64 = 940_000_000_000_000_002;
const FORMER_MEMBER: i64 = 910_003;
const FORMER_DISCORD: i64 = 940_000_000_000_000_003;
const FAN_DISCORD: i64 = 940_000_000_000_000_004;
const STARFISH_DISCORD: i64 = 940_000_000_000_000_005;
const BOT_USER_DISCORD: i64 = 940_000_000_000_000_006;
const MEMBERS: [(i64, i64); 3] = [
    (OWNER_MEMBER, OWNER_DISCORD),
    (RETIRED_MEMBER, RETIRED_DISCORD),
    (FORMER_MEMBER, FORMER_DISCORD),
];

const CURRENT_KEY: &str = "5ea4c4e1-1000-4000-8000-000000000001";
const RETIRED_KEY: &str = "5ea4c4e2-2000-4000-8000-000000000002";
const CURRENT_PREFIX: &str = "5ea4c4e1";
const UNKNOWN_PREFIX: &str = "5ea4dead";
const INVALID_KEY_KIND: i16 = 5;

const CURRENT_HOLDER: &str = "a11ce000000040008000000000000a01";
const FORMER_HOLDER: &str = "a11ce000000040008000000000000a02";
const EARLY_HOLDER: &str = "a11ce000000040008000000000000a03";
const HEAVY_HOLDER: &str = "a11ce000000040008000000000000a04";
const UNPROFILED: &str = "a11ce000000040008000000000000a09";
const NAME: &str = "SearchTestName";
const FAN_NAME: &str = "searchtestnamefan";
const RENAMED_TO: &str = "SearchTestOld";
const HEAVY_NAME: &str = "SearchTestHeavy";
const HEAVY_SNAPSHOTS: i32 = 600;
const EARLY_RENAMED_TO: &str = "SearchTestEarly";
const GHOST_NAME: &str = "searchtestghost";
const REMOTE_NAME: &str = "SearchTestRemote";
const MISSING_NAME: &str = "SearchTestMissing";

const IP: &str = "198.51.100.77";
const IP_COUNTRY: &str = "NL";
const GUILD_ID: &str = "a11ce0000000000000000001";
const GUILD_NAME: &str = "SearchTestGuild";
const PLUGIN_SLUG: &str = "searchtest-plugin";
const STORED_REQUEST: &str = "a11ce000-0000-4000-8000-000000000b04";
const TIMED_REQUEST: &str = "016f7525-a900-7000-8000-000000000b07";

static SEEDED: OnceCell<()> = OnceCell::const_new();
static SERIAL: Mutex<()> = Mutex::const_new(());

struct Run {
    _serial: MutexGuard<'static, ()>,
    db: Arc<Database>,
    redis: RedisPool,
    names: NameService,
    limits: SearchLimits,
}

impl Run {
    fn searcher(&self) -> Searcher<'_> {
        Searcher {
            db: &self.db,
            redis: &self.redis,
            names: &self.names,
            limits: &self.limits,
            coverage: coverage(),
        }
    }

    async fn search(&self, q: &str) -> SearchResults {
        self.searcher().search(q, MAX_HIT_LIMIT).await.unwrap()
    }
}

fn at(hour: u32, minute: u32) -> DateTime<Utc> {
    let (year, month, day) = DAY;
    Utc.with_ymd_and_hms(year, month, day, hour, minute, 0)
        .unwrap()
}

fn coverage() -> Coverage {
    Coverage {
        attribution_from: Some(at(0, 0)),
        subjects_from: Some(at(0, 0)),
        subject_partitions_from: Some(at(0, 0)),
        rollups_from: Some(at(0, 0)),
        ..Coverage::default()
    }
}

fn uuid(hex: &str) -> Uuid {
    Uuid::parse_str(hex).unwrap()
}

fn player(hex: &str) -> EntityRef {
    EntityRef::Player { uuid: uuid(hex) }
}

fn account(discord_id: i64, member_id: Option<i64>) -> EntityRef {
    EntityRef::Account {
        discord_id,
        member_id,
    }
}

fn hit_for<'a>(results: &'a SearchResults, entity: &EntityRef) -> &'a SearchHit {
    results
        .hits
        .iter()
        .find(|hit| &hit.entity == entity)
        .unwrap_or_else(|| panic!("no hit for {entity:?} in {:#?}", results.hits))
}

fn holds(results: &SearchResults, entity: &EntityRef) -> bool {
    results.hits.iter().any(|hit| &hit.entity == entity)
}

async fn run() -> Option<Run> {
    let db = Arc::new(
        Database::connect(&std::env::var("DATABASE_URL").ok()?)
            .await
            .ok()?,
    );
    let redis = RedisPool::connect(&std::env::var("REDIS_URL").ok()?)
        .await
        .ok()?;
    let serial = SERIAL.lock().await;
    SEEDED
        .get_or_init(|| async {
            seed_database(&db).await;
            seed_redis(&redis).await;
        })
        .await;
    let (names, _workers) = NameService::new(db.clone(), redis.clone(), None);
    Some(Run {
        _serial: serial,
        db,
        redis,
        names,
        limits: SearchLimits::new(),
    })
}

async fn seed_database(db: &Database) {
    create_partitions(db).await;
    clear(db).await;
    seed_accounts(db).await;
    seed_players(db).await;
    seed_requests(db).await;
    seed_catalog(db).await;
}

async fn create_partitions(db: &Database) {
    let (year, month, day) = DAY;
    for table in PARTITIONED {
        sqlx::query(&format!(
            "CREATE TABLE IF NOT EXISTS {table}_{year:04}{month:02}{day:02} PARTITION OF {table}
             FOR VALUES FROM ('{year:04}-{month:02}-{day:02}') TO ('{year:04}-{month:02}-{next:02}')",
            next = day + 1,
        ))
        .execute(db.pool())
        .await
        .unwrap();
    }
}

async fn clear(db: &Database) {
    for table in PARTITIONED {
        sqlx::query(&format!("DELETE FROM {table} WHERE ts >= $1 AND ts < $2"))
            .bind(at(0, 0))
            .bind(at(23, 59))
            .execute(db.pool())
            .await
            .unwrap();
    }
    let members: Vec<i64> = MEMBERS.iter().map(|(member, _)| *member).collect();
    let discord: Vec<i64> = vec![
        OWNER_DISCORD,
        RETIRED_DISCORD,
        FORMER_DISCORD,
        FAN_DISCORD,
        STARFISH_DISCORD,
    ];
    let players = [
        CURRENT_HOLDER,
        FORMER_HOLDER,
        EARLY_HOLDER,
        HEAVY_HOLDER,
        UNPROFILED,
    ];
    sqlx::query("DELETE FROM plugins WHERE slug = $1")
        .bind(PLUGIN_SLUG)
        .execute(db.pool())
        .await
        .unwrap();
    for (statement, ids) in [
        ("DELETE FROM members WHERE id = ANY($1)", &members),
        (
            "DELETE FROM discord_username_cache WHERE discord_id = ANY($1)",
            &discord,
        ),
        (
            "DELETE FROM starfish_users WHERE discord_id = ANY($1)",
            &discord,
        ),
    ] {
        sqlx::query(statement)
            .bind(ids)
            .execute(db.pool())
            .await
            .unwrap();
    }
    for statement in [
        "DELETE FROM player_snapshots WHERE uuid = ANY($1)",
        "DELETE FROM players WHERE uuid = ANY($1)",
    ] {
        sqlx::query(statement)
            .bind(players)
            .execute(db.pool())
            .await
            .unwrap();
    }
    sqlx::query("DELETE FROM guild_current WHERE guild_id = $1")
        .bind(GUILD_ID)
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("DELETE FROM api_caller_hour WHERE key_prefix = $1")
        .bind(UNKNOWN_PREFIX)
        .execute(db.pool())
        .await
        .unwrap();
}

async fn seed_accounts(db: &Database) {
    for (member, discord_id) in MEMBERS {
        sqlx::query("INSERT INTO members (id, discord_id) VALUES ($1, $2)")
            .bind(member)
            .bind(discord_id)
            .execute(db.pool())
            .await
            .unwrap();
    }
    sqlx::query("UPDATE members SET uuid = $2, api_key = $3 WHERE id = $1")
        .bind(OWNER_MEMBER)
        .bind(CURRENT_HOLDER)
        .bind(CURRENT_KEY)
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO member_api_keys (member_id, kind, api_key, created_at, retired_at) VALUES
         ($1, 'personal', $2, '2019-06-01', NULL),
         ($3, 'personal', $4, '2019-06-01', '2019-09-01')",
    )
    .bind(OWNER_MEMBER)
    .bind(CURRENT_KEY)
    .bind(RETIRED_MEMBER)
    .bind(RETIRED_KEY)
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO member_mc_links (member_id, uuid, relation, linked_at) VALUES ($1, $2, 'primary', '2019-01-01')",
    )
    .bind(FORMER_MEMBER)
    .bind(FORMER_HOLDER)
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO discord_username_cache (discord_id, username) VALUES
         ($1, lower($2)), ($3, $4)",
    )
    .bind(OWNER_DISCORD)
    .bind(NAME)
    .bind(FAN_DISCORD)
    .bind(FAN_NAME)
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query("INSERT INTO starfish_users (discord_id) VALUES ($1)")
        .bind(STARFISH_DISCORD)
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO api_key_ips (member_id, ip_address, first_seen, last_seen, requests, country, proxied)
         VALUES ($1, $2::inet, '2019-12-01', '2020-01-04', 12, $3, true)",
    )
    .bind(OWNER_MEMBER)
    .bind(IP)
    .bind(IP_COUNTRY)
    .execute(db.pool())
    .await
    .unwrap();
}

async fn seed_players(db: &Database) {
    sqlx::query(
        "INSERT INTO players (uuid, first_seen) VALUES ($1, '2019-01-01'), ($2, '2019-01-01')",
    )
    .bind(CURRENT_HOLDER)
    .bind(FORMER_HOLDER)
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO player_snapshots (uuid, timestamp, username, data)
         SELECT $1, timestamptz '2019-11-01' + make_interval(days => n), $3, '{}'::jsonb
         FROM generate_series(1, 10) AS n
         UNION ALL
         SELECT $1, timestamptz '2019-12-01' + make_interval(days => n), $4, '{}'::jsonb
         FROM generate_series(1, 5) AS n
         UNION ALL
         SELECT $2, timestamptz '2019-12-20' + make_interval(days => n), $3, '{}'::jsonb
         FROM generate_series(1, 5) AS n",
    )
    .bind(FORMER_HOLDER)
    .bind(CURRENT_HOLDER)
    .bind(NAME)
    .bind(RENAMED_TO)
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO player_snapshots (uuid, timestamp, username, data)
         SELECT $1, timestamptz '2018-03-01' + make_interval(days => n), $3, '{}'::jsonb
         FROM generate_series(1, 3) AS n
         UNION ALL
         SELECT $1, timestamptz '2018-06-01' + make_interval(days => n), $5, '{}'::jsonb
         FROM generate_series(1, 2) AS n
         UNION ALL
         SELECT $2, timestamptz '2019-01-01' + make_interval(hours => n), $3, '{}'::jsonb
         FROM generate_series(1, $4) AS n",
    )
    .bind(EARLY_HOLDER)
    .bind(HEAVY_HOLDER)
    .bind(HEAVY_NAME)
    .bind(HEAVY_SNAPSHOTS)
    .bind(EARLY_RENAMED_TO)
    .execute(db.pool())
    .await
    .unwrap();
}

async fn seed_requests(db: &Database) {
    let looked_up = at(10, 0);
    sqlx::query(
        "INSERT INTO api_request_log (ts, request_id, method, path, route, status, key_kind, actor_id, latency_ms)
         VALUES ($1, $2, 'GET', '/v3/player/tags', '/v3/player/tags', 200, 'internal', $3, 1),
                ($4, $5, 'POST', '/v3/players', '/v3/players', 200, 'personal', NULL, 1)",
    )
    .bind(looked_up)
    .bind(uuid(TIMED_REQUEST))
    .bind(BOT_USER_DISCORD)
    .bind(at(11, 0))
    .bind(uuid(STORED_REQUEST))
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO api_request_detail (request_id, ts, headers, body_bytes) VALUES ($1, $2, '{}', 0)",
    )
    .bind(uuid(STORED_REQUEST))
    .bind(at(11, 0))
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO api_request_subject (ts, request_id, key_kind, service, status, form, player, name) VALUES
         ($1, $2, 1, 0, 200, 1, $3, NULL),
         ($1, $2, 1, 0, 200, 3, NULL, $4)",
    )
    .bind(looked_up)
    .bind(uuid(TIMED_REQUEST))
    .bind(uuid(UNPROFILED))
    .bind(GHOST_NAME)
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO api_caller_hour (hour, key_kind, key_prefix, service, method, status, cause, requests, latency_sum_ms, subjects)
         VALUES ($1, $2, $3, 0, 'GET', 403, 4, 3, 3, 0)",
    )
    .bind(at(10, 0))
    .bind(INVALID_KEY_KIND)
    .bind(UNKNOWN_PREFIX)
    .execute(db.pool())
    .await
    .unwrap();
}

async fn seed_catalog(db: &Database) {
    sqlx::query(
        "INSERT INTO guild_current (guild_id, name, level, experience, member_count, raw)
         VALUES ($1, $2, 1, 1, 1, '{}')",
    )
    .bind(GUILD_ID)
    .bind(GUILD_NAME)
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO plugins (slug, owner_user_id, repo, github_repo_id, display_name, description)
         SELECT $1, id, 'searchtest/plugin', -910001, 'SearchTest Plugin', 'test'
         FROM starfish_users WHERE discord_id = $2",
    )
    .bind(PLUGIN_SLUG)
    .bind(STARFISH_DISCORD)
    .execute(db.pool())
    .await
    .unwrap();
}

async fn seed_redis(redis: &RedisPool) {
    let lower = |name: &str| name.to_ascii_lowercase();
    let _: () = redis::pipe()
        .del(format!("mj:id:{}", lower(NAME)))
        .del(format!("mj:stale:{}", lower(NAME)))
        .del(format!("mj:id:{GHOST_NAME}"))
        .del(format!("mj:uuid:{CURRENT_HOLDER}"))
        .del(format!("mj:uuid:{FORMER_HOLDER}"))
        .set(
            format!("mj:id:{}", lower(REMOTE_NAME)),
            json!({ "uuid": CURRENT_HOLDER, "username": REMOTE_NAME }).to_string(),
        )
        .set(format!("mj:missing:{}", lower(MISSING_NAME)), "1")
        .query_async(&mut redis.connection())
        .await
        .unwrap();
}

#[tokio::test]
async fn a_name_finds_its_current_and_former_holders_and_merges_the_owning_account() {
    let Some(run) = run().await else {
        return;
    };
    let results = run.search(NAME).await;

    let merged = hit_for(&results, &account(OWNER_DISCORD, Some(OWNER_MEMBER)));
    assert_eq!(merged.score, IDENTIFIED);
    let current = hit_for(&results, &player(CURRENT_HOLDER));
    assert_eq!(current.score, CURRENT_NAME);
    assert_eq!(current.account_id, Some(OWNER_DISCORD));

    let former = hit_for(&results, &player(FORMER_HOLDER));
    assert_eq!(former.score, FORMER_NAME);
    assert_eq!(former.account_id, Some(FORMER_DISCORD));
    assert!(
        former.reason.contains("last seen with it 2019-11-11"),
        "{}",
        former.reason
    );

    let fan = hit_for(&results, &account(FAN_DISCORD, None));
    assert_eq!(fan.score, DISCORD_NAME_PARTIAL);
    assert!(!results.remote_pending);
    assert!(results.failures.is_empty());
    assert_eq!(results.interpretations, vec!["name"]);
}

#[tokio::test]
async fn a_renamed_player_holds_its_new_name() {
    let Some(run) = run().await else {
        return;
    };
    let results = run.search(&format!("p:{RENAMED_TO}")).await;
    let holder = hit_for(&results, &player(FORMER_HOLDER));
    assert_eq!(holder.score, CURRENT_NAME);
    assert_eq!(results.hits.len(), 1);
}

#[tokio::test]
async fn a_name_heavier_than_the_scan_still_finds_its_first_holder() {
    let Some(run) = run().await else {
        return;
    };
    let results = run.search(&format!("p:{HEAVY_NAME}")).await;
    assert_eq!(hit_for(&results, &player(HEAVY_HOLDER)).score, CURRENT_NAME);
    let early = hit_for(&results, &player(EARLY_HOLDER));
    assert_eq!(early.score, FORMER_NAME);
    assert!(
        early.reason.contains("first seen with it 2018-03-02"),
        "{}",
        early.reason
    );
}

#[tokio::test]
async fn uuids_in_either_form_find_the_player_and_unprofiled_ones_find_their_requests() {
    let Some(run) = run().await else {
        return;
    };
    let dashed = uuid(CURRENT_HOLDER).hyphenated().to_string();
    for (input, form) in [(CURRENT_HOLDER, "Undashed"), (dashed.as_str(), "Dashed")] {
        let results = run.search(input).await;
        let known = hit_for(&results, &player(CURRENT_HOLDER));
        assert_eq!(known.score, IDENTIFIED);
        assert_eq!(known.account_id, Some(OWNER_DISCORD));
        assert!(known.reason.starts_with(form), "{}", known.reason);
    }

    let results = run.search(UNPROFILED).await;
    let unprofiled = hit_for(&results, &player(UNPROFILED));
    assert_eq!(unprofiled.score, UNPROFILED_PLAYER);
    assert!(
        unprofiled
            .reason
            .starts_with(&format!("Requests for {UNPROFILED} (no profile)"))
    );
}

#[tokio::test]
async fn a_full_key_opens_its_account_without_echoing_the_key() {
    let Some(run) = run().await else {
        return;
    };
    let results = run.search(CURRENT_KEY).await;
    let hit = hit_for(&results, &account(OWNER_DISCORD, Some(OWNER_MEMBER)));
    assert_eq!(hit.score, EXACT_KEY);
    assert!(hit.exact);

    let retired = run.search(RETIRED_KEY).await;
    let key = &retired.hits[0];
    assert!(
        matches!(&key.entity, EntityRef::Key { key_id: Some(_), prefix, .. } if prefix == &RETIRED_KEY[..8])
    );
    assert_eq!(key.score, EXACT_KEY);
    assert_eq!(key.account_id, Some(RETIRED_DISCORD));

    for results in [results, retired] {
        let body = serde_json::to_string(&results).unwrap();
        for full in [CURRENT_KEY, RETIRED_KEY] {
            assert!(!body.contains(full));
            assert!(!body.contains(&uuid(full).simple().to_string()));
        }
    }
}

#[tokio::test]
async fn key_prefixes_find_stored_keys_or_the_requests_that_presented_them() {
    let Some(run) = run().await else {
        return;
    };
    let stored = run.search(CURRENT_PREFIX).await;
    let key = stored
        .hits
        .iter()
        .find(|hit| matches!(hit.entity, EntityRef::Key { .. }))
        .unwrap();
    assert_eq!(key.score, KEY_PREFIX);
    assert_eq!(key.account_id, Some(OWNER_DISCORD));

    let presented = run.search(UNKNOWN_PREFIX).await;
    let unknown = presented
        .hits
        .iter()
        .find(|hit| matches!(hit.entity, EntityRef::Key { .. }))
        .unwrap();
    assert_eq!(
        unknown.entity,
        EntityRef::Key {
            key_id: None,
            prefix: UNKNOWN_PREFIX.into(),
            key_kind: reqlog::KeyKind::Invalid,
        }
    );
    assert!(
        unknown.reason.contains("2020-01-05 10:00 UTC"),
        "{}",
        unknown.reason
    );
}

#[tokio::test]
async fn discord_ids_find_members_bot_users_and_starfish_users() {
    let Some(run) = run().await else {
        return;
    };
    let member = run.search(&format!("<@!{OWNER_DISCORD}>")).await;
    assert_eq!(
        hit_for(&member, &account(OWNER_DISCORD, Some(OWNER_MEMBER))).score,
        IDENTIFIED
    );

    let bot = run.search(&BOT_USER_DISCORD.to_string()).await;
    let lookups = hit_for(&bot, &account(BOT_USER_DISCORD, None));
    assert_eq!(lookups.tab, Some(LOOKUPS_TAB));

    let starfish = run.search(&STARFISH_DISCORD.to_string()).await;
    assert_eq!(
        hit_for(&starfish, &account(STARFISH_DISCORD, None)).score,
        DISCORD_USER
    );

    let by_number = run.search(&format!("a:{OWNER_MEMBER}")).await;
    assert_eq!(
        hit_for(&by_number, &account(OWNER_DISCORD, Some(OWNER_MEMBER))).score,
        MEMBER_NUMBER
    );
}

#[tokio::test]
async fn ips_guilds_plugins_and_requests_are_found_by_their_ids() {
    let Some(run) = run().await else {
        return;
    };
    let ip = run.search(IP).await;
    let traffic = &ip.hits[0];
    assert_eq!(traffic.score, IP_WITH_TRAFFIC);
    assert_eq!(
        traffic.entity,
        EntityRef::Ip {
            ip: IP.into(),
            country: Some(IP_COUNTRY.into()),
            proxied: true,
        }
    );

    let guild = run.search(GUILD_ID).await;
    assert_eq!(guild.hits[0].score, GUILD_OR_PLUGIN);
    assert!(
        matches!(&guild.hits[0].entity, EntityRef::Guild { guild_id, .. } if guild_id == GUILD_ID)
    );

    let plugin = run.search("pl:searchtest").await;
    assert!(
        matches!(&plugin.hits[0].entity, EntityRef::Plugin { slug, .. } if slug == PLUGIN_SLUG)
    );

    let request = run.search(TIMED_REQUEST).await;
    assert_eq!(
        request.hits[0].entity,
        EntityRef::Request {
            request_id: uuid(TIMED_REQUEST),
            ts: at(10, 0),
        }
    );

    let stored = run.search(STORED_REQUEST).await;
    assert!(holds(
        &stored,
        &EntityRef::Request {
            request_id: uuid(STORED_REQUEST),
            ts: at(11, 0),
        }
    ));
}

#[tokio::test]
async fn names_without_a_player_yield_their_requests_and_ask_mojang() {
    let Some(run) = run().await else {
        return;
    };
    let results = run.search(GHOST_NAME).await;
    let naming = hit_for(
        &results,
        &EntityRef::Name {
            name: GHOST_NAME.into(),
        },
    );
    assert_eq!(naming.score, REQUESTS_NAMING);
    assert!(results.remote_pending);

    let forced = run.search(&format!("n:{GHOST_NAME}")).await;
    assert_eq!(forced.hits.len(), 1);
    assert!(!forced.remote_pending);
}

#[tokio::test]
async fn a_failed_lookup_is_reported_beside_the_hits_that_succeeded() {
    let Some(run) = run().await else {
        return;
    };
    let mut locker = run.db.pool().begin().await.unwrap();
    sqlx::query("LOCK TABLE plugins IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *locker)
        .await
        .unwrap();
    let partial = run.search(NAME).await;
    let total = run.searcher().search("pl:searchtest", MAX_HIT_LIMIT).await;
    locker.rollback().await.unwrap();

    assert!(holds(&partial, &player(FORMER_HOLDER)));
    assert_eq!(partial.failures.len(), 1);
    assert_eq!(partial.failures[0].lookup, "plugin");
    assert_eq!(partial.failures[0].code, "timeout");
    assert_eq!(total.unwrap_err().code(), "timeout");
}

#[tokio::test]
async fn mojang_answers_come_from_its_cache_or_its_vacancies() {
    let Some(run) = run().await else {
        return;
    };
    let hits = run.searcher().remote(REMOTE_NAME).await.unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].entity, player(CURRENT_HOLDER));
    assert_eq!(hits[0].score, CURRENT_NAME);
    assert_eq!(hits[0].account_id, Some(OWNER_DISCORD));

    assert!(
        run.searcher()
            .remote(MISSING_NAME)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        run.searcher().remote("no spaces").await.unwrap_err().code(),
        "bad_request"
    );
}
