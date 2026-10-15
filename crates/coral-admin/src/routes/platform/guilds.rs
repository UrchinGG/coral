use axum::Json;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Path, Query, State};
use chrono::{DateTime, Utc};
use database::GuildCacheRepository;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{FromRow, Postgres, QueryBuilder};

use super::page::{ContainsText, RowCursor, RowPage, page_size, split_page};
use crate::error::{AdminError, Context};
use crate::refs::{EntityRef, Labels};
use crate::serde_ts;
use crate::state::AppState;

const GUILD_ID_LEN: usize = 24;
const GUILD_COLUMNS: &str = "guild_id, name, tag, level, member_count, experience, updated_at";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GuildSort {
    #[default]
    Updated,
    Members,
    Level,
    Experience,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuildsQuery {
    q: Option<String>,
    #[serde(default)]
    sort: GuildSort,
    cursor: Option<RowCursor>,
    limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AtQuery {
    ts: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct GuildRow {
    guild: EntityRef,
    tag: Option<String>,
    level: i32,
    member_count: i32,
    experience: i64,
    #[serde(serialize_with = "serde_ts::micros")]
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct GuildDetail {
    guild: EntityRef,
    summary: Option<GuildRow>,
    current: Option<Value>,
    snapshots: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct GuildSnapshot {
    #[serde(serialize_with = "serde_ts::micros")]
    at: DateTime<Utc>,
    snapshot: Value,
}

#[derive(FromRow)]
struct GuildRecord {
    guild_id: String,
    name: String,
    tag: Option<String>,
    level: i32,
    member_count: i32,
    experience: i64,
    updated_at: DateTime<Utc>,
}

impl GuildSort {
    fn column(self) -> &'static str {
        match self {
            Self::Updated => "updated_at",
            Self::Members => "member_count",
            Self::Level => "level",
            Self::Experience => "experience",
        }
    }

    fn cursor_of(self, record: &GuildRecord) -> RowCursor {
        let key = match self {
            Self::Updated => record.updated_at.timestamp_micros(),
            Self::Members => i64::from(record.member_count),
            Self::Level => i64::from(record.level),
            Self::Experience => record.experience,
        };
        RowCursor::ranked(key, record.guild_id.clone())
    }

    fn push_after(
        self,
        cursor: &RowCursor,
        sql: &mut QueryBuilder<'_, Postgres>,
    ) -> Result<(), AdminError> {
        sql.push(format!(" AND ({}, guild_id) < (", self.column()));
        match self {
            Self::Updated => sql.push_bind(cursor.ts()?),
            Self::Members | Self::Level => sql.push_bind(narrow(cursor.key)?),
            Self::Experience => sql.push_bind(cursor.key),
        };
        sql.push(", ").push_bind(cursor.tie.clone()).push(")");
        Ok(())
    }
}

impl GuildRecord {
    fn into_row(self) -> GuildRow {
        GuildRow {
            guild: EntityRef::Guild {
                guild_id: self.guild_id,
                name: Some(self.name),
            },
            tag: self.tag,
            level: self.level,
            member_count: self.member_count,
            experience: self.experience,
            updated_at: self.updated_at,
        }
    }
}

pub async fn list(
    State(state): State<AppState>,
    query: Result<Query<GuildsQuery>, QueryRejection>,
) -> Result<Json<RowPage<GuildRow>>, AdminError> {
    let Query(query) = query?;
    let limit = page_size(query.limit)?;
    let records = guild_records(&state, &query, limit).await?;
    let (records, next_cursor) = split_page(records, limit, |record| query.sort.cursor_of(record));
    Ok(Json(RowPage {
        items: records.into_iter().map(GuildRecord::into_row).collect(),
        next_cursor,
        labels: Labels::default(),
    }))
}

pub async fn detail(
    State(state): State<AppState>,
    Path(guild_id): Path<String>,
) -> Result<Json<GuildDetail>, AdminError> {
    let guild_id = parse_guild_id(&guild_id)?;
    let cache = GuildCacheRepository::new(state.db.pool());
    let (summary, current, mut snapshots) = tokio::try_join!(
        guild_summary(&state, &guild_id),
        async {
            cache
                .get_current(&guild_id)
                .await
                .context("rebuild the guild's latest snapshot")
        },
        async {
            cache
                .list_snapshot_timestamps(&guild_id, None, None)
                .await
                .context("list the guild's snapshots")
        },
    )?;
    if summary.is_none() && current.is_none() {
        return Err(AdminError::NotFound(format!(
            "guild {guild_id} has never been cached"
        )));
    }
    snapshots.reverse();
    let name = summary
        .as_ref()
        .map(|record| record.name.clone())
        .or_else(|| snapshot_name(current.as_ref()));
    Ok(Json(GuildDetail {
        guild: EntityRef::Guild { guild_id, name },
        summary: summary.map(GuildRecord::into_row),
        current,
        snapshots: snapshots.iter().map(serde_ts::format).collect(),
    }))
}

pub async fn at(
    State(state): State<AppState>,
    Path(guild_id): Path<String>,
    query: Result<Query<AtQuery>, QueryRejection>,
) -> Result<Json<GuildSnapshot>, AdminError> {
    let Query(query) = query?;
    let guild_id = parse_guild_id(&guild_id)?;
    let snapshot = GuildCacheRepository::new(state.db.pool())
        .get_at(&guild_id, query.ts)
        .await
        .context("rebuild the guild snapshot")?
        .ok_or_else(|| {
            AdminError::NotFound(format!(
                "guild {guild_id} has no snapshot at or before {}",
                serde_ts::format(&query.ts)
            ))
        })?;
    Ok(Json(GuildSnapshot {
        at: query.ts,
        snapshot,
    }))
}

async fn guild_records(
    state: &AppState,
    query: &GuildsQuery,
    limit: i64,
) -> Result<Vec<GuildRecord>, AdminError> {
    let mut sql = QueryBuilder::<Postgres>::new(format!(
        "SELECT {GUILD_COLUMNS} FROM guild_current WHERE TRUE"
    ));
    if let Some(text) = query.q.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
        let contains = ContainsText::new(text);
        sql.push(" AND (");
        contains.push_match(&mut sql, "name");
        sql.push(" OR ");
        contains.push_match(&mut sql, "tag");
        sql.push(" OR guild_id = ")
            .push_bind(text.to_ascii_lowercase())
            .push(")");
    }
    if let Some(cursor) = &query.cursor {
        query.sort.push_after(cursor, &mut sql)?;
    }
    let column = query.sort.column();
    sql.push(format!(" ORDER BY {column} DESC, guild_id DESC LIMIT "))
        .push_bind(limit + 1);
    sql.build_query_as()
        .fetch_all(state.db.pool())
        .await
        .context("read cached guilds")
}

async fn guild_summary(
    state: &AppState,
    guild_id: &str,
) -> Result<Option<GuildRecord>, AdminError> {
    sqlx::query_as(&format!(
        "SELECT {GUILD_COLUMNS} FROM guild_current WHERE guild_id = $1"
    ))
    .bind(guild_id)
    .fetch_optional(state.db.pool())
    .await
    .context("read the cached guild")
}

fn snapshot_name(snapshot: Option<&Value>) -> Option<String> {
    snapshot?.get("name")?.as_str().map(str::to_owned)
}

fn parse_guild_id(raw: &str) -> Result<String, AdminError> {
    let id = raw.trim().to_ascii_lowercase();
    let valid = id.len() == GUILD_ID_LEN && id.bytes().all(|byte| byte.is_ascii_hexdigit());
    if !valid {
        return Err(AdminError::BadRequest(format!(
            "{raw:?} is not a guild id ({GUILD_ID_LEN} hex characters)"
        )));
    }
    Ok(id)
}

fn narrow(key: i64) -> Result<i32, AdminError> {
    i32::try_from(key)
        .map_err(|_| AdminError::BadRequest("the cursor holds an out-of-range key".into()))
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn record() -> GuildRecord {
        GuildRecord {
            guild_id: "5b3e8f1a9c2d4e6f7a8b9c0d".into(),
            name: "Tidal Order".into(),
            tag: Some("TIDE".into()),
            level: 96,
            member_count: 125,
            experience: 81_000_000,
            updated_at: Utc.with_ymd_and_hms(2026, 10, 14, 12, 0, 0).unwrap(),
        }
    }

    #[test]
    fn each_sort_pages_by_its_own_column() {
        let record = record();
        assert_eq!(GuildSort::Members.cursor_of(&record).key, 125);
        assert_eq!(GuildSort::Level.cursor_of(&record).key, 96);
        assert_eq!(GuildSort::Experience.cursor_of(&record).key, 81_000_000);
        assert_eq!(
            GuildSort::Updated.cursor_of(&record).ts().unwrap(),
            record.updated_at
        );
        assert_eq!(GuildSort::Level.cursor_of(&record).tie, record.guild_id);
    }

    #[test]
    fn guild_ids_are_24_hex_characters() {
        assert_eq!(
            parse_guild_id(" 5B3E8F1A9C2D4E6F7A8B9C0D ").unwrap(),
            "5b3e8f1a9c2d4e6f7a8b9c0d"
        );
        assert_eq!(parse_guild_id("tidal").unwrap_err().code(), "bad_request");
        assert_eq!(
            parse_guild_id("5b3e8f1a9c2d4e6f7a8b9c0z")
                .unwrap_err()
                .code(),
            "bad_request"
        );
    }

    #[test]
    fn the_snapshot_name_is_read_from_its_name_field() {
        let snapshot = serde_json::json!({ "name": "Kelp Forest" });
        assert_eq!(
            snapshot_name(Some(&snapshot)).as_deref(),
            Some("Kelp Forest")
        );
        assert_eq!(snapshot_name(None), None);
    }
}
