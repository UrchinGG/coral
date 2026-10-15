use std::sync::Arc;

use chrono::{DateTime, TimeDelta, TimeZone, Utc};
use database::Database;
use reqlog::{Form, KeyKind, Service};
use tokio::sync::OnceCell;
use uuid::Uuid;

pub const FIRST_DAY: (i32, u32, u32) = (2020, 2, 1);
pub const DAYS: i64 = 12;
pub const SUBJECT_TABLES: [&str; 2] = ["api_request_log", "api_request_subject"];

static PARTITIONS: OnceCell<()> = OnceCell::const_new();

#[derive(Debug, Clone)]
pub struct Lookup {
    pub ts: DateTime<Utc>,
    pub form: Form,
    pub player: Option<Uuid>,
    pub name: Option<&'static str>,
    pub member_id: Option<i64>,
    pub actor_id: Option<i64>,
    pub key_prefix: Option<&'static str>,
    pub key_kind: KeyKind,
    pub service: Service,
    pub status: i16,
    pub route: &'static str,
    pub method: &'static str,
}

pub fn origin() -> DateTime<Utc> {
    let (year, month, day) = FIRST_DAY;
    Utc.with_ymd_and_hms(year, month, day, 0, 0, 0).unwrap()
}

pub fn end_of_days() -> DateTime<Utc> {
    origin() + TimeDelta::days(DAYS)
}

pub fn lookup(ts: DateTime<Utc>, form: Form, player: Option<Uuid>) -> Lookup {
    Lookup {
        ts,
        form,
        player,
        name: None,
        member_id: None,
        actor_id: None,
        key_prefix: Some("fixture0"),
        key_kind: KeyKind::Invalid,
        service: Service::Unknown,
        status: 200,
        route: "/v3/player/tags",
        method: "GET",
    }
}

pub async fn database() -> Option<Arc<Database>> {
    let db = Arc::new(
        Database::connect(&std::env::var("DATABASE_URL").ok()?)
            .await
            .ok()?,
    );
    PARTITIONS.get_or_init(|| create_partitions(&db)).await;
    Some(db)
}

pub async fn forget_player(db: &Database, player: Uuid, names: &[&str]) {
    sqlx::query(
        "DELETE FROM api_request_log l USING api_request_subject s
         WHERE l.ts = s.ts AND l.request_id = s.request_id
           AND (s.player = $1 OR (s.player IS NULL AND s.name = ANY($2)))",
    )
    .bind(player)
    .bind(names)
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query(
        "DELETE FROM api_request_subject WHERE player = $1 OR (player IS NULL AND name = ANY($2))",
    )
    .bind(player)
    .bind(names)
    .execute(db.pool())
    .await
    .unwrap();
}

pub async fn record(db: &Database, lookups: &[Lookup]) -> Vec<Uuid> {
    let mut ids = Vec::new();
    for lookup in lookups {
        let request_id = reqlog::request_id_for(lookup.ts);
        let query = query_of(lookup);
        sqlx::query(
            "INSERT INTO api_request_log (ts, request_id, ip, member_id, actor_id, key_prefix, key_kind,
               method, path, route, query, status, latency_ms, cause, subjects)
             VALUES ($1, $2, '10.0.0.1', $3, $4, $5, $6, $7, $8, $8, $9, $10, 3, 0, 1)",
        )
        .bind(lookup.ts)
        .bind(request_id)
        .bind(lookup.member_id)
        .bind(lookup.actor_id)
        .bind(lookup.key_prefix)
        .bind(lookup.key_kind.label())
        .bind(lookup.method)
        .bind(lookup.route)
        .bind(query)
        .bind(lookup.status)
        .execute(db.pool())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO api_request_subject (ts, request_id, member_id, actor_id, key_prefix, key_kind,
               service, status, form, player, name)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
        )
        .bind(lookup.ts)
        .bind(request_id)
        .bind(lookup.member_id)
        .bind(lookup.actor_id)
        .bind(lookup.key_prefix)
        .bind(lookup.key_kind.code())
        .bind(lookup.service.code())
        .bind(lookup.status)
        .bind(lookup.form.code())
        .bind(lookup.player)
        .bind(lookup.name)
        .execute(db.pool())
        .await
        .unwrap();
        ids.push(request_id);
    }
    ids
}

fn query_of(lookup: &Lookup) -> Option<String> {
    match (lookup.form, lookup.player, lookup.name) {
        (Form::Undashed, Some(player), _) => Some(format!("uuid={}", player.simple())),
        (Form::Dashed, Some(player), _) => Some(format!("uuid={}", player.hyphenated())),
        (Form::Name, _, Some(name)) => Some(format!("player={name}")),
        _ => None,
    }
}

async fn create_partitions(db: &Database) {
    for offset in 0..DAYS {
        let start = origin() + TimeDelta::days(offset);
        let end = start + TimeDelta::days(1);
        for table in SUBJECT_TABLES {
            sqlx::query(&format!(
                "CREATE TABLE IF NOT EXISTS {table}_{} PARTITION OF {table} FOR VALUES FROM ('{}') TO ('{}')",
                start.format("%Y%m%d"),
                start.format("%Y-%m-%d"),
                end.format("%Y-%m-%d"),
            ))
            .execute(db.pool())
            .await
            .unwrap();
        }
    }
}
