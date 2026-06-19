use std::collections::HashSet;

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Extension, Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use utoipa::{IntoParams, ToSchema};

use database::*;

use crate::{
    auth::{AuthenticatedMember, DeveloperKeyAuth, InternalAuth},
    error::ApiError,
    routes::session::{parse_duration, parse_timestamp},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/guild/sessions/daily", get(guild_daily))
        .route("/guild/sessions/weekly", get(guild_weekly))
        .route("/guild/sessions/monthly", get(guild_monthly))
        .route("/guild/sessions/yearly", get(guild_yearly))
        .route("/guild/sessions/custom", get(guild_custom))
        .route("/guild/sessions/snapshots", get(guild_snapshots))
}

#[derive(Deserialize, IntoParams)]
pub struct GuildQuery {
    pub guild: String,
}

#[derive(Deserialize, IntoParams)]
pub struct GuildCustomQuery {
    pub guild: String,
    #[serde(default)]
    pub duration: Option<String>,
    #[serde(default)]
    pub from: Option<String>,
}

#[derive(Deserialize, IntoParams)]
pub struct GuildSnapshotQuery {
    pub guild: String,
    #[serde(default)]
    pub before: Option<String>,
    #[serde(default)]
    pub after: Option<String>,
    #[serde(default)]
    pub at: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct GuildSessionResponse {
    pub guild_id: String,
    pub name: String,
    pub from: i64,
    pub from_readable: String,
    #[schema(value_type = Value)]
    pub guild: Value,
    #[schema(value_type = Value)]
    pub members: Value,
}

#[derive(Serialize, ToSchema)]
pub struct GuildSnapshotListResponse {
    pub guild_id: String,
    pub name: String,
    #[schema(value_type = Vec<Value>)]
    pub snapshots: Vec<Value>,
}

#[derive(Serialize, ToSchema)]
pub struct GuildSnapshotDataResponse {
    pub guild_id: String,
    pub name: String,
    pub timestamp: i64,
    #[schema(value_type = Value)]
    pub data: Value,
}

macro_rules! guild_period {
    ($name:ident, $period:ident, $path:literal) => {
        #[utoipa::path(
            get, path = $path,
            description = "Returns everything that changed for a guild over the period: guild-level stats (experience, level, roster), each member's rank/roster changes, and each member's GEXP for the period broken down by day. You must be linked to an account in the guild.",
            params(GuildQuery),
            responses(
                (status = 200, body = GuildSessionResponse),
                (status = 403, body = crate::error::ErrorResponse),
                (status = 404, body = crate::error::ErrorResponse),
            ),
            tag = "Guild",
            security(("api_key" = []))
        )]
        pub async fn $name(
            State(state): State<AppState>,
            member: Option<Extension<AuthenticatedMember>>,
            internal: Option<Extension<InternalAuth>>,
            dev_auth: Option<Extension<DeveloperKeyAuth>>,
            Query(query): Query<GuildQuery>,
        ) -> Result<Json<GuildSessionResponse>, ApiError> {
            let ctx = authorize(&state, &query.guild, member, internal, dev_auth).await?;
            guild_session(&state, ctx, Period::$period.last_reset(Utc::now())).await
        }
    };
}

guild_period!(guild_daily, Daily, "/v3/guild/sessions/daily");
guild_period!(guild_weekly, Weekly, "/v3/guild/sessions/weekly");
guild_period!(guild_monthly, Monthly, "/v3/guild/sessions/monthly");
guild_period!(guild_yearly, Yearly, "/v3/guild/sessions/yearly");

#[utoipa::path(
    get,
    path = "/v3/guild/sessions/custom",
    description = "Returns everything that changed for a guild since a starting point that you specify. Provide exactly one of `duration` (for example `48h`, `10d`, or `2w`) or `from` (a Unix millisecond timestamp or RFC 3339 string). You must be linked to an account in the guild.",
    params(GuildCustomQuery),
    responses(
        (status = 200, body = GuildSessionResponse),
        (status = 400, body = crate::error::ErrorResponse),
        (status = 403, body = crate::error::ErrorResponse),
        (status = 404, body = crate::error::ErrorResponse),
    ),
    tag = "Guild",
    security(("api_key" = []))
)]
pub async fn guild_custom(
    State(state): State<AppState>,
    member: Option<Extension<AuthenticatedMember>>,
    internal: Option<Extension<InternalAuth>>,
    dev_auth: Option<Extension<DeveloperKeyAuth>>,
    Query(query): Query<GuildCustomQuery>,
) -> Result<Json<GuildSessionResponse>, ApiError> {
    let ctx = authorize(&state, &query.guild, member, internal, dev_auth).await?;

    let from = match (&query.duration, &query.from) {
        (Some(d), None) => {
            Utc::now()
                - parse_duration(d).ok_or_else(|| {
                    ApiError::BadRequest("'duration' must be like 48h, 10d, or 2w".into())
                })?
        }
        (None, Some(ts)) => parse_timestamp(ts)?,
        _ => {
            return Err(ApiError::BadRequest(
                "specify exactly one of 'duration' or 'from'".into(),
            ));
        }
    };

    guild_session(&state, ctx, from).await
}

#[utoipa::path(
    get,
    path = "/v3/guild/sessions/snapshots",
    description = "Lists a guild's snapshot timestamps, or returns a single snapshot in full when `at` is provided. Use `before` and `after` to bound the list; each accepts a Unix millisecond timestamp or an RFC 3339 string. You must be linked to an account in the guild.",
    params(GuildSnapshotQuery),
    responses(
        (status = 200, body = GuildSnapshotListResponse),
        (status = 403, body = crate::error::ErrorResponse),
        (status = 404, body = crate::error::ErrorResponse),
    ),
    tag = "Guild",
    security(("api_key" = []))
)]
pub async fn guild_snapshots(
    State(state): State<AppState>,
    member: Option<Extension<AuthenticatedMember>>,
    internal: Option<Extension<InternalAuth>>,
    dev_auth: Option<Extension<DeveloperKeyAuth>>,
    Query(query): Query<GuildSnapshotQuery>,
) -> Result<Json<Value>, ApiError> {
    let ctx = authorize(&state, &query.guild, member, internal, dev_auth).await?;
    let cache = GuildCacheRepository::new(state.db.pool());

    if let Some(ref at) = query.at {
        let ts = parse_timestamp(at)?;
        let data = cache
            .get_at(&ctx.guild_id, ts)
            .await?
            .ok_or_else(|| ApiError::NotFound("no snapshot data for this guild".into()))?;
        return Ok(Json(json!({
            "guild_id": ctx.guild_id,
            "name": ctx.name,
            "timestamp": ts.timestamp_millis(),
            "readable": ts.format("%b %d, %Y %H:%M UTC").to_string(),
            "data": data,
        })));
    }

    let before = query
        .before
        .as_ref()
        .map(|s| parse_timestamp(s))
        .transpose()?;
    let after = query
        .after
        .as_ref()
        .map(|s| parse_timestamp(s))
        .transpose()?;

    let snapshots: Vec<Value> = cache
        .list_snapshot_timestamps(&ctx.guild_id, before, after)
        .await?
        .into_iter()
        .map(|ts| {
            json!({
                "timestamp": ts.timestamp_millis(),
                "readable": ts.format("%b %d, %Y %H:%M UTC").to_string(),
            })
        })
        .collect();

    Ok(Json(json!({
        "guild_id": ctx.guild_id,
        "name": ctx.name,
        "snapshots": snapshots,
    })))
}

struct GuildCtx {
    guild_id: String,
    name: String,
}

async fn guild_session(
    state: &AppState,
    ctx: GuildCtx,
    from: DateTime<Utc>,
) -> Result<Json<GuildSessionResponse>, ApiError> {
    let cache = GuildCacheRepository::new(state.db.pool());
    let now = Utc::now();

    let mut start = cache
        .get_at_keyed(&ctx.guild_id, from)
        .await?
        .ok_or_else(|| ApiError::NotFound("no snapshot data for this guild".into()))?;
    let mut end = cache
        .get_current_keyed(&ctx.guild_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("no current data".into()))?;

    strip_exp_history(&mut start);
    strip_exp_history(&mut end);

    let mut guild = session_delta(&start, &end).unwrap_or(Value::Object(Map::new()));
    let mut members = guild
        .as_object_mut()
        .and_then(|o| o.remove("members"))
        .unwrap_or(Value::Object(Map::new()));
    let members_obj = members.as_object_mut().unwrap();

    for (uuid, daily) in cache.member_gexp(&ctx.guild_id, from, now).await? {
        let total: i64 = daily.values().sum();
        let entry = members_obj
            .entry(uuid)
            .or_insert_with(|| Value::Object(Map::new()));
        if let Some(obj) = entry.as_object_mut() {
            obj.insert(
                "gexp".into(),
                serde_json::json!({ "total": total, "daily": daily }),
            );
        }
    }

    Ok(Json(GuildSessionResponse {
        guild_id: ctx.guild_id,
        name: ctx.name,
        from: from.timestamp_millis(),
        from_readable: from.format("%b %d, %Y %H:%M UTC").to_string(),
        guild,
        members,
    }))
}

fn strip_exp_history(guild: &mut Value) {
    let Some(members) = guild.get_mut("members").and_then(Value::as_object_mut) else {
        return;
    };
    for member in members.values_mut() {
        if let Some(obj) = member.as_object_mut() {
            obj.remove("expHistory");
        }
    }
}

async fn authorize(
    state: &AppState,
    guild: &str,
    member: Option<Extension<AuthenticatedMember>>,
    internal: Option<Extension<InternalAuth>>,
    dev_auth: Option<Extension<DeveloperKeyAuth>>,
) -> Result<GuildCtx, ApiError> {
    let raw = resolve_guild(state, guild).await?;
    let guild_id = raw
        .get("_id")
        .and_then(Value::as_str)
        .ok_or_else(|| ApiError::NotFound("guild not tracked".into()))?
        .to_string();
    let name = raw
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    let bypass =
        internal.is_some() || dev_auth.is_some_and(|Extension(d)| d.has(permissions::ALL_SESSIONS));

    if !bypass {
        let Extension(member) = member
            .ok_or_else(|| ApiError::Forbidden("you must be a member of this guild".into()))?;
        if !is_member(state, &raw, member.0.discord_id).await? {
            return Err(ApiError::Forbidden(
                "you must be a member of this guild".into(),
            ));
        }
    }

    Ok(GuildCtx { guild_id, name })
}

async fn resolve_guild(state: &AppState, guild: &str) -> Result<Value, ApiError> {
    let repo = GuildCurrentRepository::new(state.db.pool());
    if let Some((raw, _)) = repo.get(guild).await? {
        return Ok(raw);
    }
    if let Some((raw, _)) = repo.get_by_name(guild).await? {
        return Ok(raw);
    }
    Err(ApiError::NotFound("guild not tracked".into()))
}

async fn is_member(state: &AppState, raw: &Value, discord_id: i64) -> Result<bool, ApiError> {
    let members: HashSet<String> = raw
        .get("members")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m.get("uuid").and_then(Value::as_str))
                .map(normalize_uuid)
                .collect()
        })
        .unwrap_or_default();

    let uuids = AccountRepository::new(state.db.pool())
        .list_uuids(discord_id)
        .await?;
    Ok(uuids.iter().any(|u| members.contains(&normalize_uuid(u))))
}

fn normalize_uuid(uuid: &str) -> String {
    uuid.replace('-', "").to_lowercase()
}
