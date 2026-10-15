use axum::Json;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Query, State};
use chrono::{DateTime, TimeDelta, Utc};
use reqlog::Cause;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool, Postgres, QueryBuilder};

use super::CursorPage;
use super::profile::player_ref;
use crate::error::{AdminError, Context};
use crate::refs::EntityRef;
use crate::routes::ref_labels::label_request;
use crate::serde_ts;
use crate::state::AppState;

const DEFAULT_PAGE: usize = 50;
const MAX_PAGE: usize = 200;
const DAY: TimeDelta = TimeDelta::hours(24);
const CURSOR_SEPARATOR: char = '.';
const LICENSE_INACTIVE: &str = "inactive";

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListQuery {
    rank: Option<i16>,
    locked: Option<bool>,
    has_key: Option<bool>,
    has_dev_key: Option<bool>,
    starfish: Option<bool>,
    sort: Option<SortColumn>,
    dir: Option<Direction>,
    cursor: Option<String>,
    limit: Option<usize>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortColumn {
    #[default]
    Member,
    Joined,
    Requests,
    Access,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Asc,
    #[default]
    Desc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ListCursor {
    sort: SortColumn,
    value: i64,
    member_id: i64,
}

#[derive(Serialize)]
pub struct AccountRow {
    account: EntityRef,
    minecraft: Option<EntityRef>,
    alts: i64,
    access_level: i16,
    key_locked: bool,
    tagging_disabled: bool,
    has_key: bool,
    has_dev_key: bool,
    starfish_license: Option<String>,
    active_strikes: i64,
    request_count: i64,
    requests_24h: i64,
    rate_limited_24h: i64,
    #[serde(serialize_with = "serde_ts::micros_opt")]
    last_active: Option<DateTime<Utc>>,
    #[serde(serialize_with = "serde_ts::micros")]
    joined: DateTime<Utc>,
}

#[derive(FromRow)]
struct ListedMember {
    id: i64,
    discord_id: i64,
    uuid: Option<String>,
    alts: i64,
    access_level: i16,
    key_locked: bool,
    tagging_disabled: bool,
    has_key: bool,
    has_dev_key: bool,
    starfish_license: Option<String>,
    active_strikes: i64,
    request_count: i64,
    requests_24h: i64,
    rate_limited_24h: i64,
    last_active: Option<DateTime<Utc>>,
    join_date: DateTime<Utc>,
    sort_value: i64,
}

pub async fn accounts(
    State(state): State<AppState>,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Json<CursorPage<AccountRow>>, AdminError> {
    let Query(query) = query?;
    let limit = page_limit(query.limit)?;
    let sort = query.sort.unwrap_or_default();
    let cursor = query
        .cursor
        .as_deref()
        .map(|raw| ListCursor::decode(raw, sort))
        .transpose()?;
    let mut rows = list_members(state.db.pool(), &query, cursor, limit).await?;
    let full = rows.len() > limit;
    rows.truncate(limit);
    let next_cursor = rows
        .last()
        .filter(|_| full)
        .map(|row| ListCursor::after(sort, row).encode());
    let items: Vec<AccountRow> = rows.into_iter().map(account_row).collect();
    let mentioned = items
        .iter()
        .flat_map(|row| std::iter::once(&row.account).chain(&row.minecraft));
    let labels = state.names.cached(&label_request(mentioned)).await?;
    Ok(Json(CursorPage {
        items,
        next_cursor,
        labels,
    }))
}

async fn list_members(
    pool: &PgPool,
    query: &ListQuery,
    cursor: Option<ListCursor>,
    limit: usize,
) -> Result<Vec<ListedMember>, AdminError> {
    let sort = query.sort.unwrap_or_default();
    let dir = query.dir.unwrap_or_default();
    list_sql(query, sort, dir, cursor, limit)
        .build_query_as()
        .fetch_all(pool)
        .await
        .context("list accounts")
}

fn list_sql(
    query: &ListQuery,
    sort: SortColumn,
    dir: Direction,
    cursor: Option<ListCursor>,
    limit: usize,
) -> QueryBuilder<'static, Postgres> {
    let mut sql = QueryBuilder::new(
        "SELECT m.id, m.discord_id, m.uuid, m.access_level, m.key_locked, m.tagging_disabled,
                m.request_count, m.join_date, m.has_key, m.sort_value,
                (SELECT count(*) FROM member_mc_links l
                  WHERE l.member_id = m.id AND l.relation = 'alt' AND l.unlinked_at IS NULL) AS alts,
                EXISTS (SELECT 1 FROM developer_keys d WHERE d.member_id = m.id) AS has_dev_key,
                (SELECT u.license_status FROM starfish_users u WHERE u.discord_id = m.discord_id) AS starfish_license,
                (SELECT count(*) FROM member_strike s WHERE s.member_id = m.id AND s.revoked_at IS NULL
                  AND (s.expires_at IS NULL OR s.expires_at > now())) AS active_strikes,
                traffic.requests_24h, traffic.rate_limited_24h, traffic.last_active
         FROM (",
    );
    push_page(&mut sql, query, sort, dir, cursor, limit);
    sql.push(
        ") AS m
         CROSS JOIN LATERAL (
            SELECT coalesce(sum(c.requests), 0)::int8 AS requests_24h,
                   coalesce(sum(c.requests) FILTER (WHERE c.cause IN (",
    )
    .push_bind(Cause::CallerLimit.code())
    .push(", ")
    .push_bind(Cause::UpstreamLimit.code())
    .push(
        ")), 0)::int8 AS rate_limited_24h, max(c.hour) AS last_active
            FROM api_caller_hour c
            WHERE (c.member_id = m.id OR c.actor_id = m.discord_id) AND c.hour >= ",
    )
    .push_bind(Utc::now() - DAY)
    .push(format!(
        ") AS traffic ORDER BY m.sort_value {dir}, m.id {dir}",
        dir = dir.keyword()
    ));
    sql
}

fn push_page(
    sql: &mut QueryBuilder<'static, Postgres>,
    query: &ListQuery,
    sort: SortColumn,
    dir: Direction,
    cursor: Option<ListCursor>,
    limit: usize,
) {
    sql.push(format!(
        "SELECT m.id, m.discord_id, m.uuid, m.access_level, m.key_locked, m.tagging_disabled,
                m.request_count, m.join_date, m.api_key IS NOT NULL AS has_key, {} AS sort_value
         FROM members m WHERE true",
        sort.expression()
    ));
    push_filters(sql, query);
    if let Some(cursor) = cursor {
        sql.push(format!(
            " AND ({}, m.id) {} (",
            sort.expression(),
            dir.after()
        ))
        .push_bind(cursor.value)
        .push(", ")
        .push_bind(cursor.member_id)
        .push(")");
    }
    sql.push(format!(
        " ORDER BY {} {dir}, m.id {dir} LIMIT ",
        sort.expression(),
        dir = dir.keyword()
    ))
    .push_bind((limit + 1) as i64);
}

fn push_filters(sql: &mut QueryBuilder<'static, Postgres>, query: &ListQuery) {
    if let Some(rank) = query.rank {
        sql.push(" AND m.access_level >= ").push_bind(rank);
    }
    if let Some(locked) = query.locked {
        sql.push(" AND m.key_locked = ").push_bind(locked);
    }
    if let Some(has_key) = query.has_key {
        sql.push(" AND (m.api_key IS NOT NULL) = ")
            .push_bind(has_key);
    }
    if let Some(has_dev_key) = query.has_dev_key {
        sql.push(" AND EXISTS (SELECT 1 FROM developer_keys d WHERE d.member_id = m.id) = ")
            .push_bind(has_dev_key);
    }
    if let Some(starfish) = query.starfish {
        sql.push(" AND EXISTS (SELECT 1 FROM starfish_users u WHERE u.discord_id = m.discord_id AND u.license_status <> ")
            .push_bind(LICENSE_INACTIVE)
            .push(") = ")
            .push_bind(starfish);
    }
}

impl SortColumn {
    fn expression(self) -> &'static str {
        match self {
            Self::Member => "m.id",
            Self::Joined => "(extract(epoch FROM m.join_date) * 1000000)::int8",
            Self::Requests => "m.request_count",
            Self::Access => "m.access_level::int8",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Member => "member",
            Self::Joined => "joined",
            Self::Requests => "requests",
            Self::Access => "access",
        }
    }
}

impl Direction {
    fn keyword(self) -> &'static str {
        match self {
            Self::Asc => "ASC",
            Self::Desc => "DESC",
        }
    }

    fn after(self) -> &'static str {
        match self {
            Self::Asc => ">",
            Self::Desc => "<",
        }
    }
}

impl ListCursor {
    fn after(sort: SortColumn, row: &ListedMember) -> Self {
        Self {
            sort,
            value: row.sort_value,
            member_id: row.id,
        }
    }

    fn encode(&self) -> String {
        format!(
            "{}{CURSOR_SEPARATOR}{}{CURSOR_SEPARATOR}{}",
            self.sort.label(),
            self.value,
            self.member_id
        )
    }

    fn decode(raw: &str, sort: SortColumn) -> Result<Self, AdminError> {
        let invalid =
            || AdminError::BadRequest(format!("cursor {raw:?} is not an account list cursor"));
        let mut parts = raw.split(CURSOR_SEPARATOR);
        let (Some(label), Some(value), Some(member_id), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(invalid());
        };
        if label != sort.label() {
            return Err(AdminError::BadRequest(format!(
                "cursor {raw:?} belongs to another sort order than {}",
                sort.label()
            )));
        }
        Ok(Self {
            sort,
            value: value.parse().map_err(|_| invalid())?,
            member_id: member_id.parse().map_err(|_| invalid())?,
        })
    }
}

fn page_limit(requested: Option<usize>) -> Result<usize, AdminError> {
    let limit = requested.unwrap_or(DEFAULT_PAGE);
    if !(1..=MAX_PAGE).contains(&limit) {
        return Err(AdminError::BadRequest(format!(
            "limit must be between 1 and {MAX_PAGE}"
        )));
    }
    Ok(limit)
}

fn account_row(row: ListedMember) -> AccountRow {
    AccountRow {
        account: EntityRef::Account {
            discord_id: row.discord_id,
            member_id: Some(row.id),
        },
        minecraft: row.uuid.as_deref().map(player_ref),
        alts: row.alts,
        access_level: row.access_level,
        key_locked: row.key_locked,
        tagging_disabled: row.tagging_disabled,
        has_key: row.has_key,
        has_dev_key: row.has_dev_key,
        starfish_license: row.starfish_license,
        active_strikes: row.active_strikes,
        request_count: row.request_count,
        requests_24h: row.requests_24h,
        rate_limited_24h: row.rate_limited_24h,
        last_active: row.last_active,
        joined: row.join_date,
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_db::{member, pool, unique_key};
    use super::*;

    #[test]
    fn list_cursors_round_trip_within_their_sort_order() {
        let cursor = ListCursor {
            sort: SortColumn::Requests,
            value: 1_730_031,
            member_id: 21111,
        };
        assert_eq!(
            ListCursor::decode(&cursor.encode(), SortColumn::Requests).unwrap(),
            cursor
        );
        assert_eq!(
            ListCursor::decode(&cursor.encode(), SortColumn::Joined)
                .unwrap_err()
                .code(),
            "bad_request"
        );
        for raw in ["", "requests.1", "requests.a.1", "requests.1.2.3"] {
            assert_eq!(
                ListCursor::decode(raw, SortColumn::Requests)
                    .unwrap_err()
                    .code(),
                "bad_request",
                "{raw}"
            );
        }
    }

    #[test]
    fn descending_pages_continue_below_the_cursor() {
        assert_eq!(Direction::Desc.after(), "<");
        assert_eq!(Direction::Asc.after(), ">");
    }

    #[tokio::test]
    async fn pages_by_lifetime_requests_continue_without_repeats() {
        let Some(pool) = pool().await else {
            return;
        };
        let top: i64 = sqlx::query_scalar("SELECT coalesce(max(request_count), 0) FROM members")
            .fetch_one(&pool)
            .await
            .unwrap();
        let mut inserted = Vec::new();
        for offset in [3, 2, 1] {
            let target = member(&pool, &unique_key()).await;
            sqlx::query("UPDATE members SET request_count = $2 WHERE id = $1")
                .bind(target.member_id)
                .bind(top + offset)
                .execute(&pool)
                .await
                .unwrap();
            inserted.push(target.member_id.unwrap());
        }
        let query = ListQuery {
            sort: Some(SortColumn::Requests),
            ..ListQuery::default()
        };
        let mut cursor = None;
        let mut seen = Vec::new();
        for _ in 0..inserted.len() {
            let rows = list_members(&pool, &query, cursor, 1).await.unwrap();
            let row = rows.first().unwrap();
            seen.push(row.id);
            cursor = Some(ListCursor::after(SortColumn::Requests, row));
        }
        assert_eq!(seen, inserted);
    }
}
