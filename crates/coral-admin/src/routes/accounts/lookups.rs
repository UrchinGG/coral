use axum::Json;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Path, Query, State};
use chrono::{DateTime, DurationRound, TimeDelta, Utc};
use reqlog::{Form, KeyKind, SubjectSource};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use super::windows::LookupWindow;
use super::{AccountTarget, CursorPage};
use crate::error::{AdminError, Context};
use crate::query::Cursor;
use crate::refs::{EntityRef, Labels};
use crate::routes::ref_labels::label_request;
use crate::serde_ts;
use crate::state::AppState;

pub const LOOKUP_SAMPLE: i64 = 50_000;
const TOP_PLAYERS: i64 = 50;
const STREAM_PAGE: usize = 100;
const MAX_STREAM_PAGE: usize = 500;
const HOUR: TimeDelta = TimeDelta::hours(1);
const SKIP_SEPARATOR: char = '.';
const VIA_KEY: &str = "key";
const VIA_BOT: &str = "bot";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WindowQuery {
    window: LookupWindow,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StreamQuery {
    cursor: Option<String>,
    limit: Option<usize>,
}

#[derive(Serialize)]
pub struct LookupSummary {
    #[serde(serialize_with = "serde_ts::micros")]
    from: DateTime<Utc>,
    #[serde(serialize_with = "serde_ts::micros")]
    to: DateTime<Utc>,
    via_key: Channel,
    via_bot: Channel,
    uncaptured_batches: i64,
}

#[derive(Serialize)]
struct Channel {
    requests: i64,
    player_mentions: i64,
}

#[derive(Serialize)]
pub struct TopLookups {
    sample_size: i64,
    #[serde(serialize_with = "serde_ts::micros_opt")]
    sample_from: Option<DateTime<Utc>>,
    #[serde(serialize_with = "serde_ts::micros")]
    window_from: DateTime<Utc>,
    exact: bool,
    players: Vec<TopPlayer>,
    labels: Labels,
}

#[derive(Serialize)]
struct TopPlayer {
    player: EntityRef,
    requests: i64,
    #[serde(serialize_with = "serde_ts::micros")]
    last: DateTime<Utc>,
    via: Via,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Via {
    Key,
    Bot,
    Both,
}

#[derive(Serialize)]
pub struct LookupRow {
    #[serde(serialize_with = "serde_ts::micros")]
    ts: DateTime<Utc>,
    request: EntityRef,
    player: Option<String>,
    name: Option<String>,
    form: Form,
    via: &'static str,
}

#[derive(FromRow)]
struct SummaryRow {
    key_requests: i64,
    key_mentions: i64,
    bot_requests: i64,
    bot_mentions: i64,
    uncaptured_batches: i64,
}

#[derive(FromRow)]
struct TopRow {
    player: Option<Uuid>,
    name: Option<String>,
    requests: i64,
    last: DateTime<Utc>,
    via_key: bool,
    via_bot: bool,
    sample_size: i64,
    sample_from: Option<DateTime<Utc>>,
}

#[derive(FromRow)]
struct SubjectRow {
    ts: DateTime<Utc>,
    request_id: Uuid,
    player: Option<Uuid>,
    name: Option<String>,
    form: i16,
    via: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StreamCursor {
    at: Cursor,
    skip: usize,
}

pub async fn summary(
    State(state): State<AppState>,
    Path(raw): Path<String>,
    query: Result<Query<WindowQuery>, QueryRejection>,
) -> Result<Json<LookupSummary>, AdminError> {
    let Query(query) = query?;
    let target = AccountTarget::resolve(&state, &raw).await?;
    let to = Utc::now();
    let from = hour_floor(query.window.from(to));
    let row: SummaryRow = sqlx::query_as(
        "SELECT
            coalesce(sum(requests) FILTER (WHERE key_kind <> $4), 0)::int8 AS key_requests,
            coalesce(sum(subjects) FILTER (WHERE key_kind <> $4), 0)::int8 AS key_mentions,
            coalesce(sum(requests) FILTER (WHERE key_kind = $4), 0)::int8 AS bot_requests,
            coalesce(sum(subjects) FILTER (WHERE key_kind = $4), 0)::int8 AS bot_mentions,
            coalesce(sum(requests) FILTER (WHERE route = ANY($5) AND hour < coalesce($6, 'infinity')), 0)::int8
                AS uncaptured_batches
         FROM api_caller_hour
         WHERE (member_id = $1 OR actor_id = $2) AND hour >= $3",
    )
    .bind(target.member_id)
    .bind(target.discord_id)
    .bind(from)
    .bind(KeyKind::Internal.code())
    .bind(batch_templates())
    .bind(state.coverage.current().body_subjects_from)
    .fetch_one(state.db.pool())
    .await
    .context("read the account's lookup totals")?;
    Ok(Json(LookupSummary {
        from,
        to,
        via_key: Channel {
            requests: row.key_requests,
            player_mentions: row.key_mentions,
        },
        via_bot: Channel {
            requests: row.bot_requests,
            player_mentions: row.bot_mentions,
        },
        uncaptured_batches: row.uncaptured_batches,
    }))
}

pub async fn top(
    State(state): State<AppState>,
    Path(raw): Path<String>,
    query: Result<Query<WindowQuery>, QueryRejection>,
) -> Result<Json<TopLookups>, AdminError> {
    let Query(query) = query?;
    let target = AccountTarget::resolve(&state, &raw).await?;
    let window_from = query.window.from(Utc::now());
    let rows: Vec<TopRow> = sqlx::query_as(TOP_SQL)
        .bind(target.member_id)
        .bind(target.discord_id)
        .bind(window_from)
        .bind(LOOKUP_SAMPLE)
        .bind(KeyKind::Internal.code())
        .bind(TOP_PLAYERS)
        .fetch_all(state.db.pool())
        .await
        .context("rank the account's newest lookups")?;
    let sample_size = rows.first().map_or(0, |row| row.sample_size);
    let sample_from = rows.first().and_then(|row| row.sample_from);
    let players: Vec<TopPlayer> = rows.into_iter().map(top_player).collect();
    let labels = state
        .names
        .cached(&label_request(players.iter().map(|top| &top.player)))
        .await?;
    Ok(Json(TopLookups {
        exact: sample_size < LOOKUP_SAMPLE,
        sample_size,
        sample_from,
        window_from,
        players,
        labels,
    }))
}

pub async fn stream(
    State(state): State<AppState>,
    Path(raw): Path<String>,
    query: Result<Query<StreamQuery>, QueryRejection>,
) -> Result<Json<CursorPage<LookupRow>>, AdminError> {
    let Query(query) = query?;
    let limit = stream_limit(query.limit)?;
    let cursor = query
        .cursor
        .as_deref()
        .map(StreamCursor::decode)
        .transpose()?;
    let target = AccountTarget::resolve(&state, &raw).await?;
    let start = cursor.unwrap_or_else(StreamCursor::newest);
    let (rows, next) = read_lookups(state.db.pool(), target, start, limit).await?;
    let items: Vec<LookupRow> = rows.into_iter().map(lookup_row).collect::<Result<_, _>>()?;
    let players: Vec<EntityRef> = items
        .iter()
        .filter_map(|row| {
            row.player
                .as_deref()
                .and_then(|uuid| Uuid::try_parse(uuid).ok())
        })
        .map(|uuid| EntityRef::Player { uuid })
        .collect();
    let labels = state.names.cached(&label_request(&players)).await?;
    Ok(Json(CursorPage {
        items,
        next_cursor: next.map(|cursor| cursor.encode()),
        labels,
    }))
}

const TOP_SQL: &str = "SELECT player, CASE WHEN player IS NULL THEN name END AS name,
            count(*)::int8 AS requests, max(ts) AS last,
            bool_or(NOT bot) AS via_key, bool_or(bot) AS via_bot,
            (sum(count(*)) OVER ())::int8 AS sample_size, min(min(ts)) OVER () AS sample_from
     FROM (
        SELECT ts, player, name, bot FROM (
            (SELECT ts, player, name, false AS bot FROM api_request_subject
             WHERE member_id = $1 AND ts >= $3 ORDER BY ts DESC LIMIT $4)
            UNION ALL
            (SELECT ts, player, name, key_kind = $5 AS bot FROM api_request_subject
             WHERE actor_id = $2 AND ($1::int8 IS NULL OR member_id IS DISTINCT FROM $1) AND ts >= $3
             ORDER BY ts DESC LIMIT $4)
        ) AS arms ORDER BY ts DESC LIMIT $4
     ) AS sample
     GROUP BY player, CASE WHEN player IS NULL THEN name END
     ORDER BY requests DESC, last DESC
     LIMIT $6";

const STREAM_SQL: &str = "SELECT ts, request_id, player, name, form, via FROM (
        (SELECT ts, request_id, player, name, form, 'key'::text AS via FROM api_request_subject
         WHERE member_id = $1 AND ts <= $3 AND (ts, request_id) <= ($3, $4)
         ORDER BY ts DESC, request_id DESC FETCH FIRST $5 ROWS WITH TIES)
        UNION ALL
        (SELECT ts, request_id, player, name, form, CASE WHEN key_kind = $6 THEN 'bot' ELSE 'key' END AS via
         FROM api_request_subject
         WHERE actor_id = $2 AND ($1::int8 IS NULL OR member_id IS DISTINCT FROM $1) AND ts <= $3 AND (ts, request_id) <= ($3, $4)
         ORDER BY ts DESC, request_id DESC FETCH FIRST $5 ROWS WITH TIES)
    ) AS arms
    ORDER BY ts DESC, request_id DESC, form, player NULLS LAST, name NULLS LAST
    LIMIT $5";

async fn read_lookups(
    pool: &PgPool,
    target: AccountTarget,
    start: StreamCursor,
    limit: usize,
) -> Result<(Vec<SubjectRow>, Option<StreamCursor>), AdminError> {
    let fetched: Vec<SubjectRow> = sqlx::query_as(STREAM_SQL)
        .bind(target.member_id)
        .bind(target.discord_id)
        .bind(start.at.ts)
        .bind(start.at.request_id)
        .bind((start.skip + limit + 1) as i64)
        .bind(KeyKind::Internal.code())
        .fetch_all(pool)
        .await
        .context("read the account's lookups")?;
    Ok(page_of(fetched, start, limit))
}

impl StreamCursor {
    fn newest() -> Self {
        Self {
            at: Cursor::new(DateTime::<Utc>::MAX_UTC, Uuid::max()),
            skip: 0,
        }
    }

    fn encode(&self) -> String {
        format!("{}{SKIP_SEPARATOR}{}", self.at.encode(), self.skip)
    }

    fn decode(raw: &str) -> Result<Self, AdminError> {
        let invalid =
            || AdminError::BadRequest(format!("cursor {raw:?} is not a valid lookup cursor"));
        let (at, skip) = raw.split_once(SKIP_SEPARATOR).ok_or_else(invalid)?;
        Ok(Self {
            at: Cursor::decode(at)?,
            skip: skip.parse().map_err(|_| invalid())?,
        })
    }
}

fn page_of(
    fetched: Vec<SubjectRow>,
    start: StreamCursor,
    limit: usize,
) -> (Vec<SubjectRow>, Option<StreamCursor>) {
    let skipped = fetched
        .into_iter()
        .enumerate()
        .filter(|(index, row)| *index >= start.skip || !same_request(row, start.at))
        .map(|(_, row)| row);
    let mut rows: Vec<SubjectRow> = skipped.collect();
    let full = rows.len() > limit;
    rows.truncate(limit);
    if !full {
        return (rows, None);
    }
    let last = rows.last().map(|row| Cursor::new(row.ts, row.request_id));
    let next = last.map(|at| {
        let shown = rows.iter().filter(|row| same_request(row, at)).count();
        let earlier = if at == start.at { start.skip } else { 0 };
        StreamCursor {
            at,
            skip: earlier + shown,
        }
    });
    (rows, next)
}

fn same_request(row: &SubjectRow, at: Cursor) -> bool {
    row.ts == at.ts && row.request_id == at.request_id
}

fn stream_limit(requested: Option<usize>) -> Result<usize, AdminError> {
    let limit = requested.unwrap_or(STREAM_PAGE);
    if !(1..=MAX_STREAM_PAGE).contains(&limit) {
        return Err(AdminError::BadRequest(format!(
            "limit must be between 1 and {MAX_STREAM_PAGE}"
        )));
    }
    Ok(limit)
}

fn lookup_row(row: SubjectRow) -> Result<LookupRow, AdminError> {
    let form = Form::from_code(row.form).ok_or_else(|| {
        AdminError::Internal(format!("subject row has an unknown form code {}", row.form))
    })?;
    Ok(LookupRow {
        ts: row.ts,
        request: EntityRef::Request {
            request_id: row.request_id,
            ts: row.ts,
        },
        player: row.player.map(|player| player.simple().to_string()),
        name: row.name,
        form,
        via: match row.via.as_str() {
            VIA_BOT => VIA_BOT,
            _ => VIA_KEY,
        },
    })
}

fn top_player(row: TopRow) -> TopPlayer {
    let player = match (row.player, row.name) {
        (Some(uuid), _) => EntityRef::Player { uuid },
        (None, Some(name)) => EntityRef::Name { name },
        (None, None) => EntityRef::Anonymous,
    };
    let via = match (row.via_key, row.via_bot) {
        (true, true) => Via::Both,
        (false, true) => Via::Bot,
        _ => Via::Key,
    };
    TopPlayer {
        player,
        requests: row.requests,
        last: row.last,
        via,
    }
}

fn batch_templates() -> Vec<&'static str> {
    reqlog::ROUTE_SUBJECTS
        .iter()
        .filter(|route| {
            route
                .sources
                .iter()
                .any(|source| matches!(source, SubjectSource::BodyArray { .. }))
        })
        .flat_map(|route| route.templates.iter().copied())
        .collect()
}

fn hour_floor(at: DateTime<Utc>) -> DateTime<Utc> {
    at.duration_trunc(HOUR).unwrap_or(at)
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::super::test_db::{at, member, pool, snowflake, unique_key};
    use super::*;

    fn subject(second: u32, request: u128, form: i16) -> SubjectRow {
        SubjectRow {
            ts: Utc.with_ymd_and_hms(2026, 10, 15, 1, 0, second).unwrap(),
            request_id: Uuid::from_u128(request),
            player: Some(Uuid::from_u128(form as u128)),
            name: None,
            form,
            via: VIA_KEY.into(),
        }
    }

    fn ids(rows: &[SubjectRow]) -> Vec<(u128, i16)> {
        rows.iter()
            .map(|row| (row.request_id.as_u128(), row.form))
            .collect()
    }

    #[test]
    fn a_request_split_across_pages_continues_where_it_stopped() {
        let fetched = vec![
            subject(9, 3, 1),
            subject(9, 3, 2),
            subject(9, 3, 3),
            subject(8, 2, 1),
        ];
        let (first, next) = page_of(fetched, StreamCursor::newest(), 2);
        assert_eq!(ids(&first), vec![(3, 1), (3, 2)]);
        let next = next.unwrap();
        assert_eq!(next.skip, 2);
        let refetched = vec![
            subject(9, 3, 1),
            subject(9, 3, 2),
            subject(9, 3, 3),
            subject(8, 2, 1),
        ];
        let (second, last) = page_of(refetched, next, 2);
        assert_eq!(ids(&second), vec![(3, 3), (2, 1)]);
        assert_eq!(last, None);
    }

    #[test]
    fn a_request_longer_than_a_page_keeps_counting_what_was_shown() {
        let fetched = (1..=5).map(|form| subject(9, 3, form)).collect();
        let (_, next) = page_of(fetched, StreamCursor::newest(), 2);
        let next = next.unwrap();
        let refetched = (1..=5).map(|form| subject(9, 3, form)).collect();
        let (second, after) = page_of(refetched, next, 2);
        assert_eq!(ids(&second), vec![(3, 3), (3, 4)]);
        assert_eq!(after.unwrap().skip, 4);
    }

    #[test]
    fn batch_lookups_are_the_routes_whose_body_lists_players() {
        assert_eq!(batch_templates(), vec!["/v3/players"]);
    }

    #[test]
    fn lookup_cursors_round_trip() {
        let cursor = StreamCursor {
            at: Cursor::new(
                Utc.with_ymd_and_hms(2026, 10, 15, 1, 0, 0).unwrap(),
                Uuid::from_u128(7),
            ),
            skip: 3,
        };
        assert_eq!(StreamCursor::decode(&cursor.encode()).unwrap(), cursor);
        assert_eq!(
            StreamCursor::decode("nonsense").unwrap_err().code(),
            "bad_request"
        );
    }

    async fn insert_subject(
        pool: &PgPool,
        minute: i64,
        request: u128,
        member_id: Option<i64>,
        actor_id: Option<i64>,
        key_kind: KeyKind,
    ) {
        sqlx::query(
            "INSERT INTO api_request_subject (ts, request_id, member_id, actor_id, key_kind, service, status, form, player)
             VALUES ($1, $2, $3, $4, $5, 0, 200, 1, $6)",
        )
        .bind(at(minute))
        .bind(Uuid::from_u128(request))
        .bind(member_id)
        .bind(actor_id)
        .bind(key_kind.code())
        .bind(Uuid::new_v4())
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn the_stream_merges_key_and_bot_lookups_and_pages_without_gaps() {
        let Some(pool) = pool().await else {
            return;
        };
        let target = member(&pool, &unique_key()).await;
        let member_id = target.member_id;
        let base = Uuid::new_v4().as_u128();
        for (minute, offset) in [(5, 1), (5, 1), (4, 2), (2, 3)] {
            insert_subject(
                &pool,
                minute,
                base + offset,
                member_id,
                None,
                KeyKind::Personal,
            )
            .await;
        }
        insert_subject(
            &pool,
            3,
            base + 4,
            None,
            Some(target.discord_id),
            KeyKind::Internal,
        )
        .await;
        insert_subject(
            &pool,
            1,
            base + 5,
            member_id,
            Some(target.discord_id),
            KeyKind::Personal,
        )
        .await;
        let mut seen = Vec::new();
        let mut start = StreamCursor::newest();
        loop {
            let (rows, next) = read_lookups(&pool, target, start, 2).await.unwrap();
            seen.extend(rows.iter().map(|row| (row.ts, row.via.clone())));
            let Some(next) = next else {
                break;
            };
            start = next;
        }
        let vias: Vec<&str> = seen.iter().map(|(_, via)| via.as_str()).collect();
        assert_eq!(vias, ["key", "key", "key", "bot", "key", "key"]);
        assert!(seen.windows(2).all(|pair| pair[0].0 >= pair[1].0));
    }

    async fn insert_form(pool: &PgPool, member_id: Option<i64>, request: u128, form: Form) {
        sqlx::query(
            "INSERT INTO api_request_subject (ts, request_id, member_id, key_kind, service, status, form, player)
             VALUES ($1, $2, $3, $4, 0, 200, $5, $6)",
        )
        .bind(at(9))
        .bind(Uuid::from_u128(request))
        .bind(member_id)
        .bind(KeyKind::Personal.code())
        .bind(form.code())
        .bind(Uuid::new_v4())
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn every_subject_of_a_request_cut_by_a_page_appears_exactly_once() {
        let Some(pool) = pool().await else {
            return;
        };
        let target = member(&pool, &unique_key()).await;
        let request = Uuid::new_v4().as_u128();
        let forms = [Form::Body, Form::Name, Form::Dashed, Form::Undashed];
        for form in forms {
            insert_form(&pool, target.member_id, request, form).await;
        }
        let mut seen = Vec::new();
        let mut start = StreamCursor::newest();
        loop {
            let (rows, next) = read_lookups(&pool, target, start, 1).await.unwrap();
            seen.extend(rows.iter().map(|row| row.form));
            let Some(next) = next else {
                break;
            };
            start = next;
        }
        let mut expected: Vec<i16> = forms.iter().map(|form| form.code()).collect();
        expected.sort_unstable();
        assert_eq!(seen, expected);
    }

    #[tokio::test]
    async fn a_discord_user_without_an_account_still_sees_its_bot_lookups() {
        let Some(pool) = pool().await else {
            return;
        };
        let target = AccountTarget {
            discord_id: snowflake(),
            member_id: None,
        };
        let base = Uuid::new_v4().as_u128();
        for (minute, offset) in [(7, 1), (6, 2)] {
            insert_subject(
                &pool,
                minute,
                base + offset,
                None,
                Some(target.discord_id),
                KeyKind::Internal,
            )
            .await;
        }
        let (rows, next) = read_lookups(&pool, target, StreamCursor::newest(), 10)
            .await
            .unwrap();
        let vias: Vec<&str> = rows.iter().map(|row| row.via.as_str()).collect();
        assert_eq!(vias, ["bot", "bot"]);
        assert_eq!(next, None);
        let top: Vec<TopRow> = sqlx::query_as(TOP_SQL)
            .bind(target.member_id)
            .bind(target.discord_id)
            .bind(at(0))
            .bind(LOOKUP_SAMPLE)
            .bind(KeyKind::Internal.code())
            .bind(TOP_PLAYERS)
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(top.len(), 2);
        assert!(top.iter().all(|row| row.via_bot && !row.via_key));
        assert_eq!(top[0].sample_size, 2);
    }
}
