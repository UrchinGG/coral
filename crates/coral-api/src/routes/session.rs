use axum::extract::{Path, Query, State};
use axum::routing::{get, patch};
use axum::{Extension, Json, Router};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use utoipa::{IntoParams, ToSchema};

use database::*;

use crate::{
    auth::{AuthenticatedMember, DeveloperKeyAuth},
    error::ApiError,
    responses::SuccessResponse,
    routes::player,
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/player/sessions/daily", get(session_daily))
        .route("/player/sessions/weekly", get(session_weekly))
        .route("/player/sessions/monthly", get(session_monthly))
        .route("/player/sessions/yearly", get(session_yearly))
        .route("/player/sessions/custom", get(session_custom))
        .route(
            "/player/sessions/markers",
            get(list_markers).post(create_marker),
        )
        .route(
            "/player/sessions/markers/{name}",
            patch(rename_marker).delete(delete_marker),
        )
        .route("/player/sessions/snapshots", get(list_snapshots))
}

#[derive(Deserialize, IntoParams)]
pub struct PlayerQuery {
    pub player: String,
}

#[derive(Deserialize, IntoParams)]
pub struct CustomSessionQuery {
    pub player: String,
    #[serde(default)]
    pub duration: Option<String>,
    #[serde(default)]
    pub from: Option<String>,
    #[serde(default)]
    pub marker: Option<String>,
}

#[derive(Deserialize, IntoParams)]
pub struct SnapshotQuery {
    pub player: String,
    #[serde(default)]
    pub before: Option<String>,
    #[serde(default)]
    pub after: Option<String>,
    #[serde(default)]
    pub at: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct CreateMarkerRequest {
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct RenameMarkerRequest {
    pub new_name: String,
}

#[derive(Serialize, ToSchema)]
pub struct SessionDeltaResponse {
    pub uuid: String,
    pub from: i64,
    pub from_readable: String,
    #[schema(value_type = Value)]
    pub delta: Value,
}

#[derive(Serialize, ToSchema)]
pub struct MarkerResponse {
    pub id: i64,
    pub name: String,
    pub snapshot_timestamp: i64,
    pub snapshot_readable: String,
    pub created_at: i64,
    pub created_readable: String,
}

#[derive(Serialize, ToSchema)]
pub struct MarkerListResponse {
    pub uuid: String,
    pub markers: Vec<MarkerResponse>,
}

#[derive(Serialize, ToSchema)]
pub struct SnapshotListResponse {
    pub uuid: String,
    pub snapshots: Vec<i64>,
}

#[derive(Serialize, ToSchema)]
pub struct SnapshotDataResponse {
    pub uuid: String,
    pub timestamp: i64,
    #[schema(value_type = Value)]
    pub data: Value,
}

macro_rules! period_handler {
    ($name:ident, $period:ident, $path:literal) => {
        #[utoipa::path(
            get, path = $path,
            description = "Returns the change in a player's stats since the start of the period: the latest snapshot diffed against the most recent snapshot at or before the period's reset. The `delta` is a recursive diff — unchanged fields are omitted, a changed numeric stat is the bare difference (new minus old, e.g. `50` or `-3`), and a field that appeared, disappeared, or changed non-numerically is `{ \"old\": <previous or null>, \"new\": <current or null> }`, with `old` null for a stat absent from the baseline snapshot. A player not snapshotted near the period's reset (e.g. one returning after a long absence) is diffed against an older, possibly partial snapshot, which can surface lifetime totals as `{ \"old\": null, \"new\": <total> }`.",
            params(PlayerQuery),
            responses((status = 200, body = SessionDeltaResponse), (status = 404, body = crate::error::ErrorResponse)),
            tag = "Player",
            security(("api_key" = []))
        )]
        pub async fn $name(
            State(state): State<AppState>,
            Query(query): Query<PlayerQuery>,
        ) -> Result<Json<SessionDeltaResponse>, ApiError> {
            delta_response(&state, &query.player, Period::$period.last_reset(Utc::now())).await
        }
    };
}

period_handler!(session_daily, Daily, "/v3/player/sessions/daily");
period_handler!(session_weekly, Weekly, "/v3/player/sessions/weekly");
period_handler!(session_monthly, Monthly, "/v3/player/sessions/monthly");
period_handler!(session_yearly, Yearly, "/v3/player/sessions/yearly");

#[utoipa::path(
    get,
    path = "/v3/player/sessions/custom",
    description = "Returns the change in a player's stats since a starting point that you specify: the latest snapshot diffed against the most recent snapshot at or before that point. Provide exactly one of `duration` (for example `48h`, `10d`, or `2w`), `from` (a Unix millisecond timestamp or RFC 3339 string), or `marker` (the name of a saved marker). The `marker` form requires account ownership or the `All Sessions` permission. The `delta` is a recursive diff — unchanged fields are omitted, a changed numeric stat is the bare difference (new minus old, e.g. `50` or `-3`), and a field that appeared, disappeared, or changed non-numerically is `{ \"old\": <previous or null>, \"new\": <current or null> }`, with `old` null for a stat absent from the baseline snapshot.",
    params(CustomSessionQuery),
    responses(
        (status = 200, body = SessionDeltaResponse),
        (status = 400, body = crate::error::ErrorResponse),
        (status = 403, body = crate::error::ErrorResponse),
        (status = 404, body = crate::error::ErrorResponse),
    ),
    tag = "Player",
    security(("api_key" = []))
)]
pub async fn session_custom(
    State(state): State<AppState>,
    Extension(member): Extension<AuthenticatedMember>,
    dev_auth: Option<Extension<DeveloperKeyAuth>>,
    Query(query): Query<CustomSessionQuery>,
) -> Result<Json<SessionDeltaResponse>, ApiError> {
    let dev = dev_auth.as_ref().map(|Extension(d)| d);
    let now = Utc::now();
    let (uuid, _) = player::resolve_identifier(&state, &query.player).await?;

    let from = match (&query.duration, &query.from, &query.marker) {
        (Some(d), None, None) => {
            now - parse_duration(d).ok_or_else(|| {
                ApiError::BadRequest("'duration' must be like 48h, 10d, or 2w".into())
            })?
        }

        (None, Some(ts), None) => parse_timestamp(ts)?,

        (None, None, Some(name)) => {
            require_owner(&state, &uuid, member.0.discord_id, dev).await?;
            SessionRepository::new(state.db.pool())
                .get(&uuid, member.0.discord_id, name)
                .await?
                .ok_or_else(|| ApiError::NotFound(format!("marker '{name}' not found")))?
                .snapshot_timestamp
        }

        _ => {
            return Err(ApiError::BadRequest(
                "specify exactly one of 'duration', 'from', or 'marker'".into(),
            ));
        }
    };

    delta_response(&state, &query.player, from).await
}

async fn delta_response(
    state: &AppState,
    player: &str,
    from: DateTime<Utc>,
) -> Result<Json<SessionDeltaResponse>, ApiError> {
    let (uuid, _) = player::resolve_identifier(state, player).await?;
    let cache = CacheRepository::new(state.db.pool());

    let snapshot = cache
        .get_snapshot_at(&uuid, from)
        .await?
        .ok_or_else(|| ApiError::NotFound("no snapshot data for this player".into()))?;

    let current = cache
        .get_latest_snapshot(&uuid)
        .await?
        .ok_or_else(|| ApiError::NotFound("no current data".into()))?;

    let delta = session_delta(&snapshot, &current).unwrap_or(Value::Object(Map::new()));

    Ok(Json(SessionDeltaResponse {
        uuid,
        from: from.timestamp_millis(),
        from_readable: from.format("%b %d, %Y %H:%M UTC").to_string(),
        delta,
    }))
}

pub(crate) fn parse_duration(s: &str) -> Option<Duration> {
    let (digits, unit) = s.split_at(s.len().checked_sub(1)?);
    let n: i64 = digits.parse().ok()?;
    if n <= 0 {
        return None;
    }
    match unit {
        "h" => Some(Duration::hours(n)),
        "d" => Some(Duration::days(n)),
        "w" => Some(Duration::weeks(n)),
        _ => None,
    }
}

#[utoipa::path(
    get,
    path = "/v3/player/sessions/markers",
    description = "Lists the session markers saved for a player. Requires account ownership or the `All Sessions` permission.",
    params(PlayerQuery),
    responses(
        (status = 200, body = MarkerListResponse),
        (status = 403, body = crate::error::ErrorResponse),
    ),
    tag = "Player",
    security(("api_key" = []))
)]
pub async fn list_markers(
    State(state): State<AppState>,
    Extension(member): Extension<AuthenticatedMember>,
    dev_auth: Option<Extension<DeveloperKeyAuth>>,
    Query(query): Query<PlayerQuery>,
) -> Result<Json<MarkerListResponse>, ApiError> {
    let (uuid, _) = player::resolve_identifier(&state, &query.player).await?;
    require_owner(
        &state,
        &uuid,
        member.0.discord_id,
        dev_auth.as_ref().map(|Extension(d)| d),
    )
    .await?;

    let markers = SessionRepository::new(state.db.pool())
        .list(&uuid, member.0.discord_id)
        .await?;

    Ok(Json(MarkerListResponse {
        uuid,
        markers: markers.iter().map(to_marker_response).collect(),
    }))
}

#[utoipa::path(
    post,
    path = "/v3/player/sessions/markers",
    description = "Saves the current snapshot as a named marker. When the name is omitted, it defaults to today's date. Requires account ownership or the `All Sessions` permission.",
    params(PlayerQuery),
    request_body = CreateMarkerRequest,
    responses(
        (status = 200, body = MarkerResponse),
        (status = 403, body = crate::error::ErrorResponse),
    ),
    tag = "Player",
    security(("api_key" = []))
)]
pub async fn create_marker(
    State(state): State<AppState>,
    Extension(member): Extension<AuthenticatedMember>,
    dev_auth: Option<Extension<DeveloperKeyAuth>>,
    Query(query): Query<PlayerQuery>,
    Json(body): Json<CreateMarkerRequest>,
) -> Result<Json<MarkerResponse>, ApiError> {
    let (uuid, _) = player::resolve_identifier(&state, &query.player).await?;
    require_owner(
        &state,
        &uuid,
        member.0.discord_id,
        dev_auth.as_ref().map(|Extension(d)| d),
    )
    .await?;

    let name = body
        .name
        .unwrap_or_else(|| Utc::now().format("%b %d, %Y").to_string());
    validate_marker_name(&name)?;

    let marker = SessionRepository::new(state.db.pool())
        .create(&uuid, member.0.discord_id, &name, Utc::now())
        .await?;

    Ok(Json(to_marker_response(&marker)))
}

#[utoipa::path(
    patch,
    path = "/v3/player/sessions/markers/{name}",
    description = "Renames a saved marker. Requires account ownership or the `All Sessions` permission.",
    params(("name" = String, Path, description = "Current marker name"), PlayerQuery),
    request_body = RenameMarkerRequest,
    responses(
        (status = 200, body = SuccessResponse),
        (status = 403, body = crate::error::ErrorResponse),
        (status = 404, body = crate::error::ErrorResponse),
    ),
    tag = "Player",
    security(("api_key" = []))
)]
pub async fn rename_marker(
    State(state): State<AppState>,
    Extension(member): Extension<AuthenticatedMember>,
    dev_auth: Option<Extension<DeveloperKeyAuth>>,
    Path(name): Path<String>,
    Query(query): Query<PlayerQuery>,
    Json(body): Json<RenameMarkerRequest>,
) -> Result<Json<SuccessResponse>, ApiError> {
    let (uuid, _) = player::resolve_identifier(&state, &query.player).await?;
    require_owner(
        &state,
        &uuid,
        member.0.discord_id,
        dev_auth.as_ref().map(|Extension(d)| d),
    )
    .await?;
    validate_marker_name(&body.new_name)?;

    let ok = SessionRepository::new(state.db.pool())
        .rename(&uuid, member.0.discord_id, &name, &body.new_name)
        .await?;

    if !ok {
        return Err(ApiError::NotFound(format!("marker '{name}' not found")));
    }
    Ok(Json(SuccessResponse { success: true }))
}

#[utoipa::path(
    delete,
    path = "/v3/player/sessions/markers/{name}",
    description = "Deletes a saved marker. Requires account ownership or the `All Sessions` permission.",
    params(("name" = String, Path, description = "Marker name"), PlayerQuery),
    responses(
        (status = 200, body = SuccessResponse),
        (status = 403, body = crate::error::ErrorResponse),
        (status = 404, body = crate::error::ErrorResponse),
    ),
    tag = "Player",
    security(("api_key" = []))
)]
pub async fn delete_marker(
    State(state): State<AppState>,
    Extension(member): Extension<AuthenticatedMember>,
    dev_auth: Option<Extension<DeveloperKeyAuth>>,
    Path(name): Path<String>,
    Query(query): Query<PlayerQuery>,
) -> Result<Json<SuccessResponse>, ApiError> {
    let (uuid, _) = player::resolve_identifier(&state, &query.player).await?;
    require_owner(
        &state,
        &uuid,
        member.0.discord_id,
        dev_auth.as_ref().map(|Extension(d)| d),
    )
    .await?;

    let ok = SessionRepository::new(state.db.pool())
        .delete(&uuid, member.0.discord_id, &name)
        .await?;

    if !ok {
        return Err(ApiError::NotFound(format!("marker '{name}' not found")));
    }
    Ok(Json(SuccessResponse { success: true }))
}

#[utoipa::path(
    get,
    path = "/v3/player/sessions/snapshots",
    description = "Lists a player's snapshot timestamps, or returns a single snapshot in full when `at` is provided. Use `before` and `after` to bound the list; each accepts a Unix millisecond timestamp or an RFC 3339 string. Requires account ownership or the `All Sessions` permission.",
    params(SnapshotQuery),
    responses(
        (status = 200, body = SnapshotListResponse),
        (status = 403, body = crate::error::ErrorResponse),
        (status = 404, body = crate::error::ErrorResponse),
    ),
    tag = "Player",
    security(("api_key" = []))
)]
pub async fn list_snapshots(
    State(state): State<AppState>,
    Extension(member): Extension<AuthenticatedMember>,
    dev_auth: Option<Extension<DeveloperKeyAuth>>,
    Query(query): Query<SnapshotQuery>,
) -> Result<Json<Value>, ApiError> {
    let (uuid, _) = player::resolve_identifier(&state, &query.player).await?;
    require_owner(
        &state,
        &uuid,
        member.0.discord_id,
        dev_auth.as_ref().map(|Extension(d)| d),
    )
    .await?;
    let cache = CacheRepository::new(state.db.pool());

    if let Some(ref at) = query.at {
        let ts = parse_timestamp(at)?;
        let data = cache
            .get_snapshot_at(&uuid, ts)
            .await?
            .ok_or_else(|| ApiError::NotFound("no snapshot data for this player".into()))?;
        return Ok(Json(serde_json::json!({
            "uuid": uuid,
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

    let rows = cache.list_snapshot_timestamps(&uuid, before, after).await?;

    let snapshots: Vec<Value> = rows
        .into_iter()
        .map(|ts| {
            serde_json::json!({
                "timestamp": ts.timestamp_millis(),
                "readable": ts.format("%b %d, %Y %H:%M UTC").to_string(),
            })
        })
        .collect();

    Ok(Json(
        serde_json::json!({ "uuid": uuid, "snapshots": snapshots }),
    ))
}

async fn require_owner(
    state: &AppState,
    uuid: &str,
    discord_id: i64,
    dev_auth: Option<&DeveloperKeyAuth>,
) -> Result<(), ApiError> {
    if dev_auth.is_some_and(|d| d.has(permissions::ALL_SESSIONS)) {
        return Ok(());
    }
    if !AccountRepository::new(state.db.pool())
        .is_owned_by(uuid, discord_id)
        .await?
    {
        return Err(ApiError::Forbidden("you do not own this account".into()));
    }
    Ok(())
}

pub(crate) fn parse_timestamp(s: &str) -> Result<DateTime<Utc>, ApiError> {
    if let Ok(millis) = s.parse::<i64>() {
        return DateTime::from_timestamp_millis(millis)
            .ok_or_else(|| ApiError::BadRequest("invalid timestamp".into()));
    }
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|_| ApiError::BadRequest("timestamp must be unix millis or RFC 3339".into()))
}

fn validate_marker_name(name: &str) -> Result<(), ApiError> {
    if name.is_empty() || name.len() > 32 {
        return Err(ApiError::BadRequest(
            "marker name must be 1-32 characters".into(),
        ));
    }
    Ok(())
}

fn to_marker_response(m: &SessionMarker) -> MarkerResponse {
    MarkerResponse {
        id: m.id,
        name: m.name.clone(),
        snapshot_timestamp: m.snapshot_timestamp.timestamp_millis(),
        snapshot_readable: m
            .snapshot_timestamp
            .format("%b %d, %Y %H:%M UTC")
            .to_string(),
        created_at: m.created_at.timestamp_millis(),
        created_readable: m.created_at.format("%b %d, %Y %H:%M UTC").to_string(),
    }
}
