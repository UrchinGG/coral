use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use chrono::{DateTime, NaiveDate, Utc};
use redis::aio::ConnectionManager;
use sqlx::{FromRow, PgConnection, PgPool};
use uuid::Uuid;

use crate::TIME_ORDERED_VERSION;
use crate::entry::{KeyKind, Service};
use crate::partitions::create_partition;
use crate::rollup::Attribution;
use crate::rollup::rows::{
    known_prefixes, legacy_key_kind, legacy_route, recorded_key_kind, service_from_agent, smallint,
};
use crate::routes::route_subjects;
use crate::subjects::{RequestParts, Subject, SubjectSources, extract, resolve_names};
use crate::writer::names;
use crate::writer::store::{SubjectColumns, SubjectOwner, insert_subject_columns};

pub const BACKFILL_SOURCES: SubjectSources = SubjectSources::All;
pub const SNAPSHOT_NAME_GROUP: usize = 1_000;

const SUBJECT_TABLE: &str = "api_request_subject";

static SOURCE_ROWS: LazyLock<String> = LazyLock::new(|| {
    format!(
        "WITH {known},
classified AS (
  SELECT l.id, l.ts, l.request_id, l.method, l.path, l.route, l.query, l.status, l.outcome_status,
         l.user_agent, l.key_kind, l.member_id, l.actor_id, l.key_prefix,
         l.request_id IS NULL OR uuid_extract_version(l.request_id) <> {TIME_ORDERED_VERSION} AS legacy,
         k.prefix IS NOT NULL AS known_prefix, k.member_id AS known_member, k.kind AS known_kind
  FROM api_request_log l
  LEFT JOIN known k ON k.prefix = l.key_prefix
  WHERE l.ts >= $1 AND l.ts < $2
    AND l.method IS NOT NULL AND l.path IS NOT NULL AND l.status IS NOT NULL
),
attributed AS (
  SELECT c.ts, coalesce(c.request_id, md5('arl:' || c.id)::uuid) AS request_id,
         c.method, c.path, c.query, c.status, c.actor_id, c.key_prefix, c.user_agent,
         CASE WHEN c.legacy THEN {legacy_route} ELSE c.route END AS route,
         coalesce(c.member_id, CASE WHEN c.legacy THEN c.known_member END) AS member_id,
         coalesce(c.outcome_status, c.status) AS semantic_status,
         CASE WHEN c.legacy THEN {legacy_kind} ELSE {recorded_kind} END AS key_kind
  FROM classified c
)
SELECT a.ts, a.request_id, a.method, a.path, a.route, a.query, a.status, a.member_id, a.actor_id,
       CASE WHEN a.member_id IS NULL THEN a.key_prefix END AS key_prefix,
       a.key_kind,
       CASE WHEN a.key_kind = {internal} THEN {service} ELSE {unknown_service} END AS service,
       a.semantic_status
FROM attributed a",
        known = known_prefixes(),
        legacy_route = legacy_route("c.path"),
        legacy_kind = legacy_key_kind(),
        recorded_kind = recorded_key_kind("c.key_kind"),
        internal = smallint(KeyKind::Internal.code()),
        service = service_from_agent("a.user_agent"),
        unknown_service = smallint(Service::Unknown.code()),
    )
});

#[derive(Debug)]
pub enum BackfillError {
    Database(sqlx::Error),
    Redis(redis::RedisError),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SliceOutcome {
    pub rows: u64,
    pub subjects: u64,
    pub named: u64,
    pub by_name: u64,
}

#[derive(Debug, Default)]
pub struct DayNames {
    known: HashMap<String, Option<Uuid>>,
}

#[derive(Debug, Clone, FromRow)]
struct SourceRow {
    ts: DateTime<Utc>,
    request_id: Uuid,
    method: String,
    path: String,
    route: Option<String>,
    query: Option<String>,
    status: i16,
    member_id: Option<i64>,
    actor_id: Option<i64>,
    key_prefix: Option<String>,
    key_kind: i16,
    service: i16,
    semantic_status: i16,
}

#[derive(FromRow)]
struct SnapshotName {
    name: String,
    uuid: String,
}

impl std::fmt::Display for BackfillError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Database(error) => write!(formatter, "database: {error}"),
            Self::Redis(error) => write!(formatter, "redis: {error}"),
        }
    }
}

impl std::error::Error for BackfillError {}

impl From<sqlx::Error> for BackfillError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

impl From<redis::RedisError> for BackfillError {
    fn from(error: redis::RedisError) -> Self {
        Self::Redis(error)
    }
}

impl SliceOutcome {
    pub fn by_name_share(self) -> f64 {
        match self.named {
            0 => 0.0,
            named => self.by_name as f64 / named as f64,
        }
    }

    fn count(&mut self, subjects: &[Subject]) {
        self.subjects += subjects.len() as u64;
        let named = subjects
            .iter()
            .filter(|subject| subject.name.is_some() && subject.form.resolves_by_name());
        for subject in named {
            self.named += 1;
            self.by_name += u64::from(subject.player.is_none());
        }
    }
}

impl std::ops::AddAssign for SliceOutcome {
    fn add_assign(&mut self, other: Self) {
        self.rows += other.rows;
        self.subjects += other.subjects;
        self.named += other.named;
        self.by_name += other.by_name;
    }
}

impl DayNames {
    async fn resolve(
        &mut self,
        conn: &mut PgConnection,
        redis: &mut ConnectionManager,
        wanted: HashSet<String>,
    ) -> Result<HashMap<String, Uuid>, BackfillError> {
        let unknown: Vec<String> = wanted
            .iter()
            .filter(|name| !self.known.contains_key(*name))
            .cloned()
            .collect();
        if !unknown.is_empty() {
            self.learn(conn, redis, unknown).await?;
        }
        Ok(wanted
            .into_iter()
            .filter_map(|name| {
                let uuid = (*self.known.get(&name)?)?;
                Some((name, uuid))
            })
            .collect())
    }

    async fn learn(
        &mut self,
        conn: &mut PgConnection,
        redis: &mut ConnectionManager,
        unknown: Vec<String>,
    ) -> Result<(), BackfillError> {
        let mut found = names::resolve(redis, &unknown).await?;
        let misses: Vec<String> = unknown
            .iter()
            .filter(|name| !found.contains_key(*name))
            .cloned()
            .collect();
        for group in misses.chunks(SNAPSHOT_NAME_GROUP) {
            found.extend(snapshot_holders(conn, group).await?);
        }
        for name in unknown {
            let uuid = found.get(&name).copied();
            self.known.insert(name, uuid);
        }
        Ok(())
    }
}

pub async fn ensure_subject_partition(pool: &PgPool, day: NaiveDate) -> Result<(), BackfillError> {
    create_partition(pool, SUBJECT_TABLE, day).await?;
    Ok(())
}

pub async fn backfill_slice(
    conn: &mut PgConnection,
    redis: &mut ConnectionManager,
    day_names: &mut DayNames,
    attribution: &Attribution,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<SliceOutcome, BackfillError> {
    let rows: Vec<SourceRow> = sqlx::query_as(&SOURCE_ROWS)
        .bind(from)
        .bind(to)
        .bind(attribution.internal_prefix())
        .fetch_all(&mut *conn)
        .await?;
    let bodies = bodies_of(conn, &rows, from, to).await?;
    let extracted: Vec<Vec<Subject>> = rows
        .iter()
        .map(|row| subjects_of(row, bodies.get(&row.request_id).map(String::as_str)))
        .collect();
    let names = day_names
        .resolve(conn, redis, names_to_resolve(&extracted))
        .await?;
    let mut outcome = SliceOutcome {
        rows: rows.len() as u64,
        ..SliceOutcome::default()
    };
    let mut columns = SubjectColumns::default();
    for (row, subjects) in rows.iter().zip(extracted) {
        let resolved = resolve_names(subjects, &names);
        outcome.count(&resolved);
        let owner = row.owner();
        for subject in &resolved {
            columns.push(&owner, subject);
        }
    }
    sqlx::query("DELETE FROM api_request_subject WHERE ts >= $1 AND ts < $2")
        .bind(from)
        .bind(to)
        .execute(&mut *conn)
        .await?;
    insert_subject_columns(conn, columns).await?;
    Ok(outcome)
}

impl SourceRow {
    fn parts<'a>(&'a self, body: Option<&'a str>) -> RequestParts<'a> {
        RequestParts {
            method: &self.method,
            status: self.status as u16,
            route: self.route.as_deref(),
            path: &self.path,
            query: self.query.as_deref(),
            body,
        }
    }

    fn reads_body(&self) -> bool {
        self.route
            .as_deref()
            .and_then(|route| route_subjects(&self.method, route))
            .is_some_and(|mapping| mapping.sources.iter().any(|source| source.reads_body()))
    }

    fn owner(&self) -> SubjectOwner<'_> {
        SubjectOwner {
            ts: self.ts,
            request_id: self.request_id,
            member_id: self.member_id,
            actor_id: self.actor_id,
            key_prefix: self.key_prefix.as_deref(),
            key_kind: self.key_kind,
            service: self.service,
            status: self.semantic_status,
        }
    }
}

fn subjects_of(row: &SourceRow, body: Option<&str>) -> Vec<Subject> {
    extract(&row.parts(body), BACKFILL_SOURCES).subjects
}

fn names_to_resolve(extracted: &[Vec<Subject>]) -> HashSet<String> {
    extracted
        .iter()
        .flatten()
        .filter(|subject| subject.needs_resolution())
        .filter_map(|subject| subject.name.clone())
        .collect()
}

async fn bodies_of(
    conn: &mut PgConnection,
    rows: &[SourceRow],
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<HashMap<Uuid, String>, sqlx::Error> {
    let ids: Vec<Uuid> = rows
        .iter()
        .filter(|row| row.reads_body())
        .map(|row| row.request_id)
        .collect();
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let bodies: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT request_id, body FROM api_request_detail
         WHERE ts >= $1 AND ts < $2 AND request_id = ANY($3) AND body IS NOT NULL",
    )
    .bind(from)
    .bind(to)
    .bind(&ids)
    .fetch_all(conn)
    .await?;
    Ok(bodies.into_iter().collect())
}

async fn snapshot_holders(
    conn: &mut PgConnection,
    names: &[String],
) -> Result<HashMap<String, Uuid>, sqlx::Error> {
    let holders: Vec<SnapshotName> = sqlx::query_as(
        "SELECT DISTINCT ON (lower(username)) lower(username) AS name, uuid
         FROM player_snapshots
         WHERE lower(username) = ANY($1)
         ORDER BY lower(username), timestamp DESC",
    )
    .bind(names)
    .fetch_all(conn)
    .await?;
    Ok(holders
        .into_iter()
        .filter_map(|holder| Some((holder.name, Uuid::try_parse(&holder.uuid).ok()?)))
        .collect())
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;
    use crate::KEY_PREFIX_CHARS;
    use crate::subjects::Form;
    use crate::test_support::{create_day_partitions, test_pool};

    const BACKFILL_DAY: (i32, u32, u32) = (2020, 3, 9);
    const UNPARTITIONED_DAY: (i32, u32, u32) = (2020, 3, 10);
    const UNKNOWN_PREFIX: &str = "zz00ff11";

    #[derive(Debug, FromRow, PartialEq, Eq)]
    struct StoredSubject {
        request_id: Uuid,
        member_id: Option<i64>,
        key_prefix: Option<String>,
        key_kind: i16,
        form: i16,
        player: Option<Uuid>,
        name: Option<String>,
    }

    struct Fixture {
        pool: sqlx::PgPool,
        redis: ConnectionManager,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        member_id: i64,
        prefix: String,
        player: Uuid,
        holder: Uuid,
        name: String,
    }

    async fn test_redis() -> Option<ConnectionManager> {
        let client = redis::Client::open(std::env::var("REDIS_URL").ok()?).ok()?;
        ConnectionManager::new(client).await.ok()
    }

    async fn fixture() -> Option<Fixture> {
        let pool = test_pool().await?;
        let redis = test_redis().await?;
        let (year, month, day) = BACKFILL_DAY;
        create_day_partitions(&pool, NaiveDate::from_ymd_opt(year, month, day).unwrap()).await;
        let from = Utc.with_ymd_and_hms(year, month, day, 6, 0, 0).unwrap();
        let key = Uuid::new_v4().simple().to_string();
        let member_id: i64 = sqlx::query_scalar(
            "INSERT INTO members (discord_id, api_key) VALUES ($1, $2) RETURNING id",
        )
        .bind((Uuid::new_v4().as_u64_pair().0 >> 2) as i64)
        .bind(&key)
        .fetch_one(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO member_api_keys (member_id, kind, api_key) VALUES ($1, 'personal', $2)",
        )
        .bind(member_id)
        .bind(&key)
        .execute(&pool)
        .await
        .unwrap();
        let name = format!("bf{}", &Uuid::new_v4().simple().to_string()[..10]);
        let holder = Uuid::new_v4();
        sqlx::query("INSERT INTO player_snapshots (uuid, username, is_baseline, data) VALUES ($1, $2, true, '{}')")
            .bind(holder.simple().to_string())
            .bind(&name)
            .execute(&pool)
            .await
            .unwrap();
        Some(Fixture {
            pool,
            redis,
            from,
            to: from + chrono::TimeDelta::minutes(15),
            member_id,
            prefix: key[..KEY_PREFIX_CHARS].to_string(),
            player: Uuid::new_v4(),
            holder,
            name,
        })
    }

    async fn insert_legacy_row(
        fixture: &Fixture,
        offset_s: i64,
        path: &str,
        query: Option<&str>,
        prefix: &str,
    ) -> Uuid {
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO api_request_log (ts, member_id, key_kind, key_prefix, method, path, status, query, latency_ms)
             VALUES ($1, NULL, 'key', $2, 'GET', $3, 200, $4, 5) RETURNING id",
        )
        .bind(fixture.from + chrono::TimeDelta::seconds(offset_s))
        .bind(prefix)
        .bind(path)
        .bind(query)
        .fetch_one(&fixture.pool)
        .await
        .unwrap();
        sqlx::query_scalar("SELECT md5('arl:' || $1)::uuid")
            .bind(id)
            .fetch_one(&fixture.pool)
            .await
            .unwrap()
    }

    async fn stored(fixture: &Fixture) -> Vec<StoredSubject> {
        sqlx::query_as(
            "SELECT request_id, member_id, key_prefix, key_kind, form, player, name FROM api_request_subject
             WHERE ts >= $1 AND ts < $2 ORDER BY ts",
        )
        .bind(fixture.from)
        .bind(fixture.to)
        .fetch_all(&fixture.pool)
        .await
        .unwrap()
    }

    async fn run(fixture: &mut Fixture) -> SliceOutcome {
        let mut conn = fixture.pool.acquire().await.unwrap();
        backfill_slice(
            &mut conn,
            &mut fixture.redis,
            &mut DayNames::default(),
            &Attribution::without_internal_key(),
            fixture.from,
            fixture.to,
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn legacy_rows_get_attributed_subjects_under_their_shim_request_id() {
        let Some(mut fixture) = fixture().await else {
            return;
        };
        for table in ["api_request_log", "api_request_subject"] {
            sqlx::query(&format!("DELETE FROM {table} WHERE ts >= $1 AND ts < $2"))
                .bind(fixture.from)
                .bind(fixture.to)
                .execute(&fixture.pool)
                .await
                .unwrap();
        }
        let query = format!("uuid={}", fixture.player.simple());
        let prefix = fixture.prefix.clone();
        let keyed = insert_legacy_row(&fixture, 1, "/v3/player/tags", Some(&query), &prefix).await;
        let path = format!("/v3/resolve/{}", fixture.name);
        let invalid = insert_legacy_row(&fixture, 2, &path, None, UNKNOWN_PREFIX).await;

        let first = run(&mut fixture).await;
        let again = run(&mut fixture).await;

        assert_eq!(first, again);
        assert_eq!(
            (first.rows, first.subjects, first.named, first.by_name),
            (2, 2, 1, 0)
        );
        assert_eq!(
            stored(&fixture).await,
            vec![
                StoredSubject {
                    request_id: keyed,
                    member_id: Some(fixture.member_id),
                    key_prefix: None,
                    key_kind: KeyKind::Personal.code(),
                    form: Form::Undashed.code(),
                    player: Some(fixture.player),
                    name: None,
                },
                StoredSubject {
                    request_id: invalid,
                    member_id: None,
                    key_prefix: Some(UNKNOWN_PREFIX.to_string()),
                    key_kind: KeyKind::Invalid.code(),
                    form: Form::Path.code(),
                    player: Some(fixture.holder),
                    name: Some(fixture.name.clone()),
                },
            ]
        );
    }

    #[tokio::test]
    async fn a_day_older_than_its_subject_partitions_gets_one_before_its_slices() {
        let (Some(pool), Some(mut redis)) = (test_pool().await, test_redis().await) else {
            return;
        };
        let (year, month, day) = UNPARTITIONED_DAY;
        let day = NaiveDate::from_ymd_opt(year, month, day).unwrap();
        create_day_partitions(&pool, day).await;
        let partition = format!("{SUBJECT_TABLE}_{}", day.format("%Y%m%d"));
        sqlx::raw_sql(&format!("DROP TABLE IF EXISTS {partition}"))
            .execute(&pool)
            .await
            .unwrap();
        let from = Utc.from_utc_datetime(&day.and_hms_opt(6, 0, 0).unwrap());
        let to = from + chrono::TimeDelta::minutes(15);
        sqlx::query("DELETE FROM api_request_log WHERE ts >= $1 AND ts < $2")
            .bind(from)
            .bind(to)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO api_request_log (ts, key_kind, key_prefix, method, path, status, query, latency_ms)
             VALUES ($1, 'key', $2, 'GET', '/v3/player/tags', 200, $3, 5)",
        )
        .bind(from)
        .bind(UNKNOWN_PREFIX)
        .bind(format!("uuid={}", Uuid::new_v4().simple()))
        .execute(&pool)
        .await
        .unwrap();

        ensure_subject_partition(&pool, day).await.unwrap();
        ensure_subject_partition(&pool, day).await.unwrap();
        let mut conn = pool.acquire().await.unwrap();
        let outcome = backfill_slice(
            &mut conn,
            &mut redis,
            &mut DayNames::default(),
            &Attribution::without_internal_key(),
            from,
            to,
        )
        .await
        .unwrap();

        assert_eq!((outcome.rows, outcome.subjects), (1, 1));
    }

    fn row(method: &str, route: Option<&str>, path: &str, query: Option<&str>) -> SourceRow {
        SourceRow {
            ts: Utc::now(),
            request_id: Uuid::nil(),
            method: method.into(),
            path: path.into(),
            route: route.map(str::to_owned),
            query: query.map(str::to_owned),
            status: 200,
            member_id: None,
            actor_id: None,
            key_prefix: None,
            key_kind: KeyKind::Legacy.code(),
            service: Service::Unknown.code(),
            semantic_status: 200,
        }
    }

    #[test]
    fn query_and_path_subjects_are_extracted_like_the_writer_does() {
        let tags = row(
            "GET",
            Some("/v3/player/tags"),
            "/v3/player/tags",
            Some("uuid=94589c87999b4a7ab3add2ae50711e64"),
        );
        let subjects = subjects_of(&tags, None);
        assert_eq!(subjects.len(), 1);
        assert_eq!(subjects[0].form, Form::Undashed);
        let resolve = row(
            "GET",
            Some("/v3/resolve/{identifier}"),
            "/v3/resolve/oJugg",
            None,
        );
        let subjects = subjects_of(&resolve, None);
        assert_eq!(subjects[0].name.as_deref(), Some("ojugg"));
        assert!(subjects[0].needs_resolution());
    }

    #[test]
    fn only_routes_whose_mapping_reads_a_body_fetch_one() {
        assert!(row("POST", Some("/v3/players"), "/v3/players", None).reads_body());
        assert!(!row("GET", Some("/v3/player/tags"), "/v3/player/tags", None).reads_body());
        assert!(!row("GET", None, "/wp-login.php", None).reads_body());
    }

    #[test]
    fn the_by_name_share_counts_unresolved_name_forms() {
        let mut outcome = SliceOutcome::default();
        let resolved = Subject {
            form: Form::Name,
            player: Some(Uuid::nil()),
            name: Some("ojugg".into()),
            raw: "ojugg".into(),
        };
        let unresolved = Subject {
            form: Form::Path,
            player: None,
            name: Some("ghost".into()),
            raw: "ghost".into(),
        };
        let display = Subject {
            form: Form::Display,
            player: None,
            name: Some("shown".into()),
            raw: "shown".into(),
        };
        outcome.count(&[resolved, unresolved, display]);
        assert_eq!(
            (outcome.subjects, outcome.named, outcome.by_name),
            (3, 2, 1)
        );
        assert_eq!(outcome.by_name_share(), 0.5);
        assert_eq!(SliceOutcome::default().by_name_share(), 0.0);
    }
}
