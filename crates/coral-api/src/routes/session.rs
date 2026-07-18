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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub displayname: Option<String>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub displayname: Option<String>,
    pub markers: Vec<MarkerResponse>,
}

#[derive(Serialize, ToSchema)]
pub struct SnapshotListResponse {
    pub uuid: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub displayname: Option<String>,
    pub snapshots: Vec<i64>,
}

#[derive(Serialize, ToSchema)]
pub struct SnapshotDataResponse {
    pub uuid: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub displayname: Option<String>,
    pub timestamp: i64,
    #[schema(value_type = Value)]
    pub data: Value,
}

macro_rules! period_handler {
    ($name:ident, $period:ident, $path:literal) => {
        #[utoipa::path(
            get, path = $path,
            description = "Returns the change in a player's stats since the start of the period: the latest snapshot diffed against the most recent snapshot at or before the period's reset. The `delta` is a recursive diff: unchanged fields are omitted, a changed numeric stat is the bare difference (new minus old, e.g. `50` or `-3`), and a field that appeared, disappeared, or changed non-numerically is `{ \"old\": <previous or null>, \"new\": <current or null> }`, with `old` null for a stat absent from the baseline snapshot. A player not snapshotted near the period's reset (e.g. one returning after a long absence) is diffed against an older, possibly partial snapshot, which can surface lifetime totals as `{ \"old\": null, \"new\": <total> }`.",
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
    description = "Returns the change in a player's stats since a starting point that you specify: the latest snapshot diffed against the most recent snapshot at or before that point. Provide exactly one of `duration` (for example `48h`, `10d`, or `2w`), `from` (a Unix millisecond timestamp or RFC 3339 string), or `marker` (the name of a saved marker). The `from` and `marker` forms, and `duration` values outside 1h-24h, 1d-7d, or 1w and up, require account ownership or the `All Sessions` permission. The `delta` is a recursive diff: unchanged fields are omitted, a changed numeric stat is the bare difference (new minus old, e.g. `50` or `-3`), and a field that appeared, disappeared, or changed non-numerically is `{ \"old\": <previous or null>, \"new\": <current or null> }`, with `old` null for a stat absent from the baseline snapshot.",
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
    let owned = is_owner(&state, &uuid, member.0.discord_id, dev).await?;

    let from = match (&query.duration, &query.from, &query.marker) {
        (Some(d), None, None) => {
            let duration = parse_duration(d).ok_or_else(|| {
                ApiError::BadRequest("'duration' must be like 48h, 10d, or 2w".into())
            })?;
            if !owned && !is_unowned_duration_allowed(d) {
                return Err(ApiError::Forbidden(
                    "you do not own this account; 'duration' is limited to 1h-24h, 1d-7d, or 1w and up".into(),
                ));
            }
            now - duration
        }

        (None, Some(ts), None) => {
            if !owned {
                return Err(ApiError::Forbidden(
                    "you do not own this account; use 'duration' instead of 'from'".into(),
                ));
            }
            parse_timestamp(ts)?
        }

        (None, None, Some(name)) => {
            if !owned {
                return Err(ApiError::Forbidden("you do not own this account".into()));
            }
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

    let displayname = player::format_display_name(&current);
    let delta = session_delta(&snapshot, &current).unwrap_or(Value::Object(Map::new()));

    Ok(Json(SessionDeltaResponse {
        uuid,
        displayname,
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

    let displayname = player::cached_display_name(&state, &uuid).await;
    Ok(Json(MarkerListResponse {
        uuid,
        displayname,
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
            "displayname": player::format_display_name(&data),
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

    let displayname = player::cached_display_name(&state, &uuid).await;
    Ok(Json(
        serde_json::json!({ "uuid": uuid, "displayname": displayname, "snapshots": snapshots }),
    ))
}

async fn is_owner(
    state: &AppState,
    uuid: &str,
    discord_id: i64,
    dev_auth: Option<&DeveloperKeyAuth>,
) -> Result<bool, ApiError> {
    if dev_auth.is_some_and(|d| d.has(permissions::ALL_SESSIONS)) {
        return Ok(true);
    }
    AccountRepository::new(state.db.pool())
        .is_owned_by(uuid, discord_id)
        .await
        .map_err(Into::into)
}

async fn require_owner(
    state: &AppState,
    uuid: &str,
    discord_id: i64,
    dev_auth: Option<&DeveloperKeyAuth>,
) -> Result<(), ApiError> {
    if is_owner(state, uuid, discord_id, dev_auth).await? {
        Ok(())
    } else {
        Err(ApiError::Forbidden("you do not own this account".into()))
    }
}

fn is_unowned_duration_allowed(s: &str) -> bool {
    let Some((digits, unit)) = s.split_at_checked(s.len().saturating_sub(1)) else {
        return false;
    };
    let Ok(n) = digits.parse::<i64>() else {
        return false;
    };
    match unit {
        "h" => (1..=24).contains(&n),
        "d" => (1..=7).contains(&n),
        "w" => n >= 1,
        _ => false,
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_hours_up_to_a_day() {
        assert!(is_unowned_duration_allowed("1h"));
        assert!(is_unowned_duration_allowed("24h"));
        assert!(!is_unowned_duration_allowed("25h"));
        assert!(!is_unowned_duration_allowed("0h"));
    }

    #[test]
    fn allows_days_up_to_a_week() {
        assert!(is_unowned_duration_allowed("1d"));
        assert!(is_unowned_duration_allowed("7d"));
        assert!(!is_unowned_duration_allowed("8d"));
        assert!(!is_unowned_duration_allowed("10d"));
    }

    #[test]
    fn allows_any_number_of_weeks() {
        assert!(is_unowned_duration_allowed("1w"));
        assert!(is_unowned_duration_allowed("2w"));
        assert!(is_unowned_duration_allowed("52w"));
        assert!(!is_unowned_duration_allowed("0w"));
    }

    #[test]
    fn rejects_malformed_durations() {
        assert!(!is_unowned_duration_allowed(""));
        assert!(!is_unowned_duration_allowed("h"));
        assert!(!is_unowned_duration_allowed("1m"));
        assert!(!is_unowned_duration_allowed("-1h"));
        assert!(!is_unowned_duration_allowed("1.5d"));
    }
}
