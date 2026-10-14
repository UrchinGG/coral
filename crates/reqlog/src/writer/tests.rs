use std::time::Duration as StdDuration;

use chrono::Duration;
use serde_json::json;
use sqlx::PgPool;

use super::*;
use crate::entry::{Caller, Cause, Client, Detail, KeyKind};

const WRITE_WAIT: StdDuration = StdDuration::from_secs(10);
const INCOMPRESSIBLE_PATH_SEGMENTS: usize = 200;
const FAR_FUTURE_ROWS: usize = 8;

fn entry(path: &str, route: Option<&str>, query: Option<&str>) -> LogEntry {
    let ts = crate::now();
    LogEntry {
        request_id: crate::request_id_for(ts),
        ts,
        method: "GET".into(),
        path: path.into(),
        route: route.map(str::to_owned),
        query: query.map(str::to_owned),
        client: Client {
            ip: Some("203.0.113.9".parse().unwrap()),
            country: Some("NL".into()),
            user_agent: Some("writer-test".into()),
            key_prefix: Some("abcdefgh".into()),
        },
        caller: Caller::anonymous(KeyKind::Invalid),
        status: 401,
        outcome_status: None,
        cause: Cause::Auth,
        latency_ms: 3,
        error: None,
        detail: Detail {
            headers: json!({"user-agent": "writer-test"}),
            body: None,
            body_bytes: 0,
            body_complete: true,
            server_error: None,
        },
    }
}

fn with_body(bytes: usize) -> LogEntry {
    let mut entry = entry("/v3/players", Some("/v3/players"), None);
    entry.detail.body = Some("x".repeat(bytes));
    entry
}

fn queued_bytes(log: &LogReceiver) -> usize {
    log.backlog.queued_bytes.load(Ordering::Relaxed)
}

#[tokio::test]
async fn batches_hold_at_most_the_batch_limit() {
    let (log, mut receiver) = channel();
    for _ in 0..MAX_BATCH + 250 {
        log.submit(entry("/x", None, None));
    }
    drop(log);
    let batch_sizes = [MAX_BATCH, 250];
    for size in batch_sizes {
        assert_eq!(next_batch(&mut receiver).await.unwrap().entries.len(), size);
    }
    assert!(next_batch(&mut receiver).await.is_none());
}

#[tokio::test]
async fn batches_close_once_they_reach_the_byte_budget() {
    let (log, mut receiver) = channel();
    let quarter_budget = MAX_BATCH_BYTES / 4;
    for _ in 0..6 {
        log.submit(with_body(quarter_budget));
    }
    drop(log);
    let first = next_batch(&mut receiver).await.unwrap();
    assert_eq!(first.entries.len(), 4);
    assert!(first.bytes >= MAX_BATCH_BYTES);
    assert_eq!(next_batch(&mut receiver).await.unwrap().entries.len(), 2);
}

#[tokio::test(start_paused = true)]
async fn a_batch_closes_after_the_flush_interval() {
    let (log, mut receiver) = channel();
    log.submit(entry("/a", None, None));
    let late = log.clone();
    tokio::spawn(async move {
        tokio::time::sleep(FLUSH_INTERVAL * 2).await;
        late.submit(entry("/b", None, None));
    });
    let first = next_batch(&mut receiver).await.unwrap();
    assert_eq!(first.entries.len(), 1);
    assert_eq!(first.entries[0].path, "/a");
    let second = next_batch(&mut receiver).await.unwrap();
    assert_eq!(second.entries[0].path, "/b");
}

#[tokio::test]
async fn submissions_beyond_capacity_are_counted_as_overflow() {
    let (log, receiver) = channel();
    for _ in 0..CHANNEL_CAPACITY + 3 {
        log.submit(entry("/x", None, None));
    }
    assert_eq!(receiver.backlog.drops.pending(DropReason::Overflow), 3);
}

#[tokio::test]
async fn submissions_beyond_the_byte_budget_are_counted_as_overflow() {
    let (log, mut receiver) = channel();
    assert!(receiver.backlog.reserve(CHANNEL_BYTES));
    log.submit(entry("/x", None, None));
    assert_eq!(receiver.backlog.drops.pending(DropReason::Overflow), 1);
    receiver.backlog.release(CHANNEL_BYTES);

    let queued = entry("/x", None, None);
    let bytes = queued.text_bytes();
    log.submit(queued);
    assert_eq!(queued_bytes(&receiver), bytes);
    receiver.recv().await.unwrap();
    assert_eq!(queued_bytes(&receiver), 0);
    assert_eq!(receiver.backlog.drops.pending(DropReason::Overflow), 1);
}

#[test]
fn the_backlog_admits_bytes_up_to_its_budget() {
    let backlog = Backlog::default();
    assert!(backlog.reserve(CHANNEL_BYTES - 10));
    assert!(!backlog.reserve(11));
    assert!(backlog.reserve(10));
    backlog.release(CHANNEL_BYTES);
    assert!(backlog.reserve(CHANNEL_BYTES));
}

#[test]
fn sqlstates_are_classified_by_row_specificity() {
    assert!(matches!(
        classify_sqlstate(Some("54000")),
        Failure::RowSpecific
    ));
    assert!(matches!(
        classify_sqlstate(Some("22021")),
        Failure::RowSpecific
    ));
    assert!(matches!(
        classify_sqlstate(Some("23505")),
        Failure::RowSpecific
    ));
    assert!(matches!(
        classify_sqlstate(Some("23514")),
        Failure::MissingPartition
    ));
    assert!(matches!(
        classify_sqlstate(Some("57014")),
        Failure::Transient
    ));
    assert!(matches!(
        classify_sqlstate(Some("40P01")),
        Failure::Transient
    ));
    assert!(matches!(classify_sqlstate(None), Failure::Transient));
}

struct Harness {
    pool: PgPool,
    redis: ConnectionManager,
}

async fn harness() -> Option<Harness> {
    dotenvy::dotenv().ok();
    let pool = PgPool::connect(&std::env::var("DATABASE_URL").ok()?)
        .await
        .ok()?;
    let client = redis::Client::open(std::env::var("REDIS_URL").ok()?).ok()?;
    let redis = ConnectionManager::new(client).await.ok()?;
    partitions::create_upcoming(&pool).await.unwrap();
    Some(Harness { pool, redis })
}

impl Harness {
    fn writer(&self) -> Writer {
        Writer {
            pool: self.pool.clone(),
            redis: self.redis.clone(),
            sources: SubjectSources::All,
            backlog: Arc::new(Backlog::default()),
            started: crate::now(),
            attribution_recorded: true,
            writer_seen_at: Some(Instant::now()),
        }
    }

    async fn logged(&self, ids: &[Uuid]) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM api_request_log WHERE request_id = ANY($1)")
            .bind(ids)
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }

    async fn detailed(&self, ids: &[Uuid]) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM api_request_detail WHERE request_id = ANY($1)")
            .bind(ids)
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }
}

fn prepared(entries: Vec<LogEntry>) -> Vec<PreparedEntry> {
    entries
        .into_iter()
        .map(|entry| PreparedEntry {
            entry,
            subjects: Some(Vec::new()),
        })
        .collect()
}

#[tokio::test]
async fn a_bad_row_is_isolated_by_bisection() {
    let Some(harness) = harness().await else {
        return;
    };
    let mut entries: Vec<LogEntry> = (0..20).map(|_| entry("/fine", None, None)).collect();
    entries[13].path = (0..INCOMPRESSIBLE_PATH_SEGMENTS)
        .map(|_| format!("/{}", Uuid::new_v4().simple()))
        .collect();
    let bad = entries[13].request_id;
    let good: Vec<Uuid> = entries
        .iter()
        .map(|e| e.request_id)
        .filter(|id| *id != bad)
        .collect();
    let mut writer = harness.writer();
    writer
        .persist(&prepared(entries), &mut PartitionRepair::Pending)
        .await;

    assert_eq!(harness.logged(&good).await, 19);
    assert_eq!(harness.detailed(&good).await, 19);
    assert_eq!(harness.logged(&[bad]).await, 0);
    assert_eq!(harness.detailed(&[bad]).await, 0);
    assert_eq!(writer.backlog.drops.pending(DropReason::RowError), 1);
}

#[tokio::test]
async fn bodies_with_nul_bytes_are_written_with_their_log_row() {
    let Some(harness) = harness().await else {
        return;
    };
    let mut nul = entry("/v3/player/tags", Some("/v3/player/tags"), None);
    nul.status = 200;
    nul.error = Some(crate::StoredText::decode(b"bad\0error").text);
    nul.detail = Detail::capture(json!({}), b"a\0b", 3, true, Some("chain\0".into()));
    let id = nul.request_id;
    let mut writer = harness.writer();
    writer
        .persist(&prepared(vec![nul]), &mut PartitionRepair::Pending)
        .await;

    assert_eq!(harness.logged(&[id]).await, 1);
    let stored: (Option<String>, bool, Option<String>) = sqlx::query_as(
        "SELECT body, body_complete, server_error FROM api_request_detail WHERE request_id = $1",
    )
    .bind(id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(
        stored,
        (
            Some("a\u{FFFD}b".to_owned()),
            false,
            Some("chain\u{FFFD}".to_owned())
        )
    );
    assert_eq!(writer.backlog.drops.pending(DropReason::RowError), 0);
}

#[tokio::test]
async fn rows_outside_every_partition_are_dropped_after_one_repair() {
    let Some(harness) = harness().await else {
        return;
    };
    let mut entries: Vec<LogEntry> = (0..FAR_FUTURE_ROWS)
        .map(|_| {
            let mut far = entry("/future", None, None);
            far.ts += Duration::days(30);
            far.request_id = crate::request_id_for(far.ts);
            far
        })
        .collect();
    let far: Vec<Uuid> = entries.iter().map(|e| e.request_id).collect();
    let near = entry("/now", None, None);
    let near_id = near.request_id;
    entries.push(near);
    let mut writer = harness.writer();
    let mut repair = PartitionRepair::Pending;
    writer.persist(&prepared(entries), &mut repair).await;

    assert_eq!(repair, PartitionRepair::Done);
    assert_eq!(harness.logged(&[near_id]).await, 1);
    assert_eq!(harness.logged(&far).await, 0);
    assert_eq!(
        writer.backlog.drops.pending(DropReason::RowError),
        FAR_FUTURE_ROWS as u64
    );
}

#[tokio::test]
async fn a_repaired_batch_isolates_rows_without_repairing_again() {
    let Some(harness) = harness().await else {
        return;
    };
    let mut far = entry("/future", None, None);
    far.ts += Duration::days(30);
    far.request_id = crate::request_id_for(far.ts);
    let mut writer = harness.writer();
    let mut repair = PartitionRepair::Done;
    writer.persist(&prepared(vec![far]), &mut repair).await;

    assert_eq!(repair, PartitionRepair::Done);
    assert_eq!(writer.backlog.drops.pending(DropReason::RowError), 1);
}

#[tokio::test]
async fn late_batches_mark_dirty_ranges_and_fold_network_rows() {
    let Some(harness) = harness().await else {
        return;
    };
    let discord_id = 880_000_000_000_000_000 + crate::now().timestamp_subsec_micros() as i64;
    let member_id: i64 =
        sqlx::query_scalar("INSERT INTO members (discord_id) VALUES ($1) RETURNING id")
            .bind(discord_id)
            .fetch_one(&harness.pool)
            .await
            .unwrap();
    sqlx::query(
        "INSERT INTO api_rollup_state (name, watermark) VALUES ('network', now())
         ON CONFLICT (name) DO UPDATE SET watermark = excluded.watermark",
    )
    .execute(&harness.pool)
    .await
    .unwrap();

    let mut late = entry("/v3/player/tags", Some("/v3/player/tags"), None);
    late.ts -= Duration::minutes(10);
    late.request_id = crate::request_id_for(late.ts);
    late.caller = Caller::member(KeyKind::Personal, member_id);
    late.status = 200;
    let mut session = entry("/api/v1/starfish/users/me", None, None);
    session.ts -= Duration::minutes(10);
    session.request_id = crate::request_id_for(session.ts);
    session.caller = Caller::session(None, discord_id);
    session.client.ip = Some("2001:db8::1".parse().unwrap());
    session.detail.headers = json!({"cf-worker": "example"});
    let late_ts = late.ts;
    let mut writer = harness.writer();
    writer
        .persist(
            &prepared(vec![late, session]),
            &mut PartitionRepair::Pending,
        )
        .await;

    let dirty: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM api_rollup_dirty WHERE from_ts <= $1 AND to_ts >= $1",
    )
    .bind(late_ts)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert!(dirty >= 1);
    let ips: Vec<(String, i64, Option<String>, bool)> = sqlx::query_as(
        "SELECT host(ip_address), requests, country, proxied FROM api_key_ips
         WHERE member_id = $1 ORDER BY 1",
    )
    .bind(member_id)
    .fetch_all(&harness.pool)
    .await
    .unwrap();
    assert_eq!(
        ips,
        vec![
            ("2001:db8::1".to_owned(), 1, Some("NL".to_owned()), true),
            ("203.0.113.9".to_owned(), 1, Some("NL".to_owned()), false),
        ]
    );
    let agents: i64 = sqlx::query_scalar(
        "SELECT requests FROM api_member_agents WHERE member_id = $1 AND user_agent = 'writer-test'",
    )
    .bind(member_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(agents, 2);

    sqlx::query("DELETE FROM api_rollup_state WHERE name = 'network'")
        .execute(&harness.pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM members WHERE id = $1")
        .bind(member_id)
        .execute(&harness.pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn the_writer_stores_resolved_subjects_with_each_request() {
    let Some(mut harness) = harness().await else {
        return;
    };
    let player = Uuid::new_v4();
    let name = format!("w{}", &player.simple().to_string()[..12]);
    let _: () = redis::cmd("SET")
        .arg(format!("mj:id:{name}"))
        .arg(json!({"uuid": player.simple().to_string(), "username": name}).to_string())
        .arg("EX")
        .arg(60)
        .query_async(&mut harness.redis)
        .await
        .unwrap();
    let mut by_name = entry(
        "/v3/player/tags",
        Some("/v3/player/tags"),
        Some(&format!("player={}", name.to_uppercase())),
    );
    by_name.status = 200;
    let unmatched = entry("/player/whatever", None, Some("player=Somebody"));
    let ids = [by_name.request_id, unmatched.request_id];

    let (log, receiver) = channel();
    let (ready, partitions_ready) = watch::channel(true);
    let handle = spawn_writer(
        WriterConfig {
            pool: harness.pool.clone(),
            redis: harness.redis.clone(),
            sources: SubjectSources::All,
            partitions_ready,
        },
        receiver,
    );
    log.submit(by_name);
    log.submit(unmatched);
    drop(log);
    drop(ready);
    tokio::time::timeout(WRITE_WAIT, handle)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(harness.logged(&ids).await, 2);
    let counts: Vec<(Uuid, Option<i16>)> = sqlx::query_as(
        "SELECT request_id, subjects FROM api_request_log WHERE request_id = ANY($1)",
    )
    .bind(&ids[..])
    .fetch_all(&harness.pool)
    .await
    .unwrap();
    assert!(counts.contains(&(ids[0], Some(1))));
    assert!(counts.contains(&(ids[1], Some(0))));
    let subject: (Option<Uuid>, Option<String>, i16, i16, i16) = sqlx::query_as(
        "SELECT player, name, form, status, key_kind FROM api_request_subject WHERE request_id = $1",
    )
    .bind(ids[0])
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(
        subject,
        (
            Some(player),
            Some(name.clone()),
            crate::Form::Name.code(),
            200,
            KeyKind::Invalid.code()
        )
    );
}
