use axum::Json;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Query, State};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Postgres, QueryBuilder};

use super::page::{Accounts, ContainsText, Mentions, RowCursor, RowPage, page_size, split_page};
use crate::error::{AdminError, Context};
use crate::refs::EntityRef;
use crate::serde_ts;
use crate::state::AppState;

const USER_COLUMNS: &str = "u.id, u.discord_id, u.license_status, u.github_username, u.updated_at,
    (SELECT count(*) FROM starfish_hwids h WHERE h.user_id = u.id) AS hwids,
    EXISTS (SELECT 1 FROM starfish_sessions s WHERE s.user_id = u.id AND s.expires_at > now()) AS active_session,
    (SELECT max(s.last_heartbeat_at) FROM starfish_sessions s WHERE s.user_id = u.id) AS last_heartbeat_at,
    (SELECT count(*) FROM plugin_installs pi WHERE pi.user_id = u.id) AS plugins_installed,
    (SELECT count(*) FROM plugins p WHERE p.owner_user_id = u.id) AS plugins_owned";
const INACTIVE_LICENSE: &str = "inactive";
const SIGNED_IN: &str =
    "EXISTS (SELECT 1 FROM starfish_sessions s WHERE s.user_id = u.id AND s.expires_at > now())";
const OVERVIEW_SQL: &str = "SELECT
    count(*) FILTER (WHERE license_status = 'active') AS licensed,
    (SELECT count(DISTINCT user_id) FROM starfish_sessions WHERE expires_at > now()) AS signed_in
  FROM starfish_users";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UsersQuery {
    q: Option<String>,
    license: Option<LicenseFilter>,
    session: Option<SessionFilter>,
    cursor: Option<RowCursor>,
    limit: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LicenseFilter {
    Active,
    Inactive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionFilter {
    SignedIn,
    SignedOut,
}

#[derive(Debug, Serialize, FromRow)]
pub struct StarfishOverview {
    licensed: i64,
    signed_in: i64,
}

#[derive(Debug, Serialize)]
pub struct StarfishUser {
    user_id: i64,
    account: EntityRef,
    license_status: String,
    github_username: Option<String>,
    hwids: i64,
    active_session: bool,
    #[serde(serialize_with = "serde_ts::micros_opt")]
    last_heartbeat_at: Option<DateTime<Utc>>,
    plugins_installed: i64,
    plugins_owned: i64,
    #[serde(serialize_with = "serde_ts::micros")]
    updated_at: DateTime<Utc>,
}

#[derive(FromRow)]
struct UserRecord {
    id: i64,
    discord_id: i64,
    license_status: String,
    github_username: Option<String>,
    updated_at: DateTime<Utc>,
    hwids: i64,
    active_session: bool,
    last_heartbeat_at: Option<DateTime<Utc>>,
    plugins_installed: i64,
    plugins_owned: i64,
}

pub async fn overview(State(state): State<AppState>) -> Result<Json<StarfishOverview>, AdminError> {
    sqlx::query_as(OVERVIEW_SQL)
        .fetch_one(state.db.pool())
        .await
        .map(Json)
        .context("count Starfish users")
}

pub async fn users(
    State(state): State<AppState>,
    query: Result<Query<UsersQuery>, QueryRejection>,
) -> Result<Json<RowPage<StarfishUser>>, AdminError> {
    let Query(query) = query?;
    let limit = page_size(query.limit)?;
    let records = user_records(&state, &query, limit).await?;
    let (records, next_cursor) = split_page(records, limit, |record| {
        RowCursor::at(record.updated_at, record.id)
    });
    let mut mentions = Mentions::default();
    records
        .iter()
        .for_each(|record| mentions.account(record.discord_id));
    let accounts = Accounts::load(&state, &mentions.discord_ids()).await?;
    let labels = mentions.labels(&state).await?;
    Ok(Json(RowPage {
        items: records
            .into_iter()
            .map(|record| starfish_user(record, &accounts))
            .collect(),
        next_cursor,
        labels,
    }))
}

async fn user_records(
    state: &AppState,
    query: &UsersQuery,
    limit: i64,
) -> Result<Vec<UserRecord>, AdminError> {
    let mut sql = QueryBuilder::<Postgres>::new(format!(
        "SELECT {USER_COLUMNS} FROM starfish_users u
         LEFT JOIN discord_username_cache c ON c.discord_id = u.discord_id
         WHERE u.license_status <> "
    ));
    sql.push_bind(INACTIVE_LICENSE);
    if let Some(text) = query.q.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
        push_search(&mut sql, &ContainsText::new(text));
    }
    if let Some(license) = query.license {
        sql.push(" AND u.license_status = ")
            .push_bind(license.status());
    }
    if let Some(session) = query.session {
        sql.push(session.condition());
    }
    if let Some(cursor) = &query.cursor {
        sql.push(" AND (u.updated_at, u.id) < (")
            .push_bind(cursor.ts()?)
            .push(", ")
            .push_bind(cursor.id()?)
            .push(")");
    }
    sql.push(" ORDER BY u.updated_at DESC, u.id DESC LIMIT ")
        .push_bind(limit + 1);
    sql.build_query_as()
        .fetch_all(state.db.pool())
        .await
        .context("read Starfish users")
}

fn push_search(sql: &mut QueryBuilder<'_, Postgres>, contains: &ContainsText) {
    sql.push(" AND (");
    contains.push_match(sql, "u.discord_id::text");
    sql.push(" OR ");
    contains.push_match(sql, "c.username");
    sql.push(" OR ");
    contains.push_match(sql, "u.github_username");
    sql.push(" OR EXISTS (SELECT 1 FROM starfish_hwids h WHERE h.user_id = u.id AND ");
    contains.push_match(sql, "h.hwid_hash");
    sql.push("))");
}

impl LicenseFilter {
    fn status(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Inactive => "inactive",
        }
    }
}

impl SessionFilter {
    fn condition(self) -> String {
        match self {
            Self::SignedIn => format!(" AND {SIGNED_IN}"),
            Self::SignedOut => format!(" AND NOT {SIGNED_IN}"),
        }
    }
}

fn starfish_user(record: UserRecord, accounts: &Accounts) -> StarfishUser {
    StarfishUser {
        user_id: record.id,
        account: accounts.of(record.discord_id),
        license_status: record.license_status,
        github_username: record.github_username,
        hwids: record.hwids,
        active_session: record.active_session,
        last_heartbeat_at: record.last_heartbeat_at,
        plugins_installed: record.plugins_installed,
        plugins_owned: record.plugins_owned,
        updated_at: record.updated_at,
    }
}
