use std::collections::HashMap;

use axum::http::StatusCode;
use axum::{Extension, Json, Router, extract::*, routing::get, routing::post};
use chrono::{DateTime, Utc};
use coral_redis::{RateLimiter, SESSION_RATE_LIMIT, SESSION_UUID_BUDGET};
use database::{
    DeveloperKey, DeveloperKeyRepository, Member, MemberRepository, StarfishRepository, standing,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{FromRow, Postgres, QueryBuilder};

use crate::auth::AdminActor;
use crate::identity;
use crate::state::AppState;

async fn compute_budget_utilization(state: &AppState, discord_ids: &[i64]) -> HashMap<i64, f64> {
    let Some(redis) = &state.redis else {
        return HashMap::new();
    };
    if discord_ids.is_empty() {
        return HashMap::new();
    }
    let limiter = RateLimiter::new(redis.clone());
    let names: Vec<String> = discord_ids
        .iter()
        .flat_map(|id| [format!("sf:{id}"), format!("sfuuids:{id}")])
        .collect();
    let Ok(counts) = limiter.usage_many(&names).await else {
        return HashMap::new();
    };
    discord_ids
        .iter()
        .enumerate()
        .map(|(i, id)| {
            let session_pct =
                counts.get(i * 2).copied().unwrap_or(0) as f64 / SESSION_RATE_LIMIT as f64;
            let uuid_pct =
                counts.get(i * 2 + 1).copied().unwrap_or(0) as f64 / SESSION_UUID_BUDGET as f64;
            (*id, session_pct.max(uuid_pct))
        })
        .collect()
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list))
        .route("/{id}", get(detail))
        .route("/{id}/lock", post(lock))
        .route("/{id}/unlock", post(unlock))
        .route("/{id}/access-level", post(set_access_level))
        .route("/{id}/tagging-disabled", post(set_tagging_disabled))
        .route("/{id}/strikes", post(add_strike))
        .route(
            "/{id}/strikes/{index}",
            axum::routing::delete(remove_strike),
        )
        .route("/{id}/api-key/regenerate", post(regenerate_api_key))
        .route("/{id}/ratelimit/reset", post(reset_rate_limit))
        .route("/{id}/dev-key", post(create_dev_key).delete(delete_dev_key))
        .route("/{id}/dev-key/lock", post(set_dev_key_locked))
        .route("/{id}/dev-key/rate-limit", post(set_dev_key_rate_limit))
        .route("/{id}/dev-key/permissions", post(set_dev_key_permissions))
        .route("/{id}/starfish/license", post(set_license_status))
        .route("/{id}/starfish/sessions/revoke", post(revoke_sessions))
}

#[derive(Deserialize)]
struct ListParams {
    limit: Option<i64>,
    offset: Option<i64>,
    search: Option<String>,
    sort: Option<String>,
    dir: Option<String>,
    rank: Option<i16>,
    locked: Option<bool>,
    haskey: Option<bool>,
}

#[derive(Default)]
struct SearchExtra {
    ign_uuid: Option<String>,
    discord_ids: Vec<i64>,
}

async fn resolve_search_extra(state: &AppState, s: &str) -> SearchExtra {
    let ign_uuid = if identity::looks_like_ign(s) {
        identity::resolve_minecraft_uuid(state, s).await
    } else {
        None
    };
    let discord_ids: Vec<i64> = sqlx::query_scalar(
        "SELECT discord_id FROM discord_username_cache WHERE username ILIKE $1 LIMIT 25",
    )
    .bind(format!("%{s}%"))
    .fetch_all(state.db.pool())
    .await
    .unwrap_or_default();
    SearchExtra {
        ign_uuid,
        discord_ids,
    }
}

fn apply_filters(qb: &mut QueryBuilder<'_, Postgres>, p: &ListParams, extra: &SearchExtra) {
    let mut sep = " WHERE ";
    if let Some(s) = p.search.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        let pattern = format!("%{s}%");
        qb.push(sep)
            .push("(discord_id::text LIKE ")
            .push_bind(pattern.clone())
            .push(" OR uuid LIKE ")
            .push_bind(pattern);
        if let Some(uuid) = &extra.ign_uuid {
            qb.push(" OR uuid = ").push_bind(uuid.clone());
        }
        if !extra.discord_ids.is_empty() {
            qb.push(" OR discord_id = ANY(")
                .push_bind(extra.discord_ids.clone())
                .push(")");
        }
        qb.push(")");
        sep = " AND ";
    }
    if let Some(rank) = p.rank {
        qb.push(sep).push("access_level >= ").push_bind(rank);
        sep = " AND ";
    }
    if p.locked.unwrap_or(false) {
        qb.push(sep).push("key_locked = true");
        sep = " AND ";
    }
    if p.haskey.unwrap_or(false) {
        qb.push(sep).push("api_key IS NOT NULL");
        sep = " AND ";
    }
    let _ = sep;
}

#[derive(Serialize)]
struct ListResponse {
    total: i64,
    members: Vec<Summary>,
}

#[derive(Serialize, FromRow)]
struct Summary {
    id: i64,
    #[serde(serialize_with = "crate::serde_id::discord_id")]
    discord_id: i64,
    uuid: Option<String>,
    join_date: DateTime<Utc>,
    request_count: i64,
    access_level: i16,
    key_locked: bool,
    tagging_disabled: bool,
    has_api_key: bool,
    strike_count: i64,
    has_dev_key: bool,
    last_seen_ip: Option<String>,
    #[sqlx(default)]
    is_owner: bool,
    #[sqlx(default)]
    discord_username: Option<String>,
    #[sqlx(default)]
    minecraft_username: Option<String>,
    #[sqlx(default)]
    budget_utilization: Option<f64>,
}

async fn list(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Json<ListResponse> {
    let limit = params.limit.unwrap_or(50).clamp(1, 200);
    let offset = params.offset.unwrap_or(0).max(0);
    let pool = state.db.pool();

    let extra = match params
        .search
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(s) => resolve_search_extra(&state, s).await,
        None => SearchExtra::default(),
    };

    let order = match params.sort.as_deref() {
        Some("requests") => "request_count",
        Some("joined") => "join_date",
        Some("access") => "access_level",
        _ => "id",
    };
    let dir = if params.dir.as_deref() == Some("asc") {
        "ASC"
    } else {
        "DESC"
    };

    let mut count = QueryBuilder::<Postgres>::new("SELECT COUNT(*) FROM members");
    apply_filters(&mut count, &params, &extra);
    let total = count
        .build_query_scalar()
        .fetch_one(pool)
        .await
        .unwrap_or(0);

    let mut q = QueryBuilder::<Postgres>::new(
        "SELECT id, discord_id, uuid, join_date, request_count, access_level, key_locked,
                tagging_disabled, api_key IS NOT NULL as has_api_key,
                COALESCE(jsonb_array_length(strikes), 0) AS strike_count,
                EXISTS(SELECT 1 FROM developer_keys dk WHERE dk.member_id = members.id) AS has_dev_key,
                (SELECT ip_address::text FROM api_key_ips aki
                 WHERE aki.member_id = members.id ORDER BY last_seen DESC LIMIT 1) AS last_seen_ip
         FROM members",
    );
    apply_filters(&mut q, &params, &extra);
    q.push(format!(" ORDER BY {order} {dir} LIMIT "))
        .push_bind(limit)
        .push(" OFFSET ")
        .push_bind(offset);
    let mut members = q
        .build_query_as::<Summary>()
        .fetch_all(pool)
        .await
        .unwrap_or_default();
    for m in &mut members {
        m.is_owner = state.owner_ids.contains(&m.discord_id);
    }

    let discord_ids: Vec<i64> = members.iter().map(|m| m.discord_id).collect();
    let uuids: Vec<String> = members.iter().filter_map(|m| m.uuid.clone()).collect();
    let (names, utilization) = tokio::join!(
        identity::resolve(&state, &discord_ids, &uuids),
        compute_budget_utilization(&state, &discord_ids),
    );
    for m in &mut members {
        m.discord_username = names.discord.get(&m.discord_id).cloned();
        m.minecraft_username = m
            .uuid
            .as_ref()
            .and_then(|u| names.minecraft.get(u).cloned());
        m.budget_utilization = utilization.get(&m.discord_id).copied();
    }

    Json(ListResponse { total, members })
}

#[derive(Serialize, FromRow)]
struct IpRecord {
    ip_address: String,
    first_seen: DateTime<Utc>,
    last_seen: DateTime<Utc>,
}

#[derive(Serialize, FromRow)]
struct AltAccount {
    uuid: String,
    added_at: DateTime<Utc>,
    #[sqlx(default)]
    minecraft_username: Option<String>,
}

#[derive(Serialize)]
struct StandingView {
    can_vote: bool,
    vote_reason: String,
    can_tag: bool,
    tag_reason: String,
    effective_level: i16,
    strike_count: usize,
    accepted_tags: i64,
    rejected_tags: i64,
    accurate_verdicts: i64,
    incorrect_verdicts: i64,
    bonus_verdicts: i64,
}

#[derive(Serialize, FromRow, Clone)]
struct AuthoredTag {
    id: i64,
    uuid: String,
    kind: String,
    tag_type: Option<String>,
    reason: Option<String>,
    ts: DateTime<Utc>,
    #[sqlx(default)]
    minecraft_username: Option<String>,
}

#[derive(Serialize)]
struct DevKeyView {
    label: String,
    permissions: i64,
    rate_limit: i32,
    request_count: i64,
    locked: bool,
    api_key: Option<String>,
}

impl From<DeveloperKey> for DevKeyView {
    fn from(k: DeveloperKey) -> Self {
        Self {
            label: k.label,
            permissions: k.permissions,
            rate_limit: k.rate_limit,
            request_count: k.request_count,
            locked: k.locked,
            api_key: None,
        }
    }
}

#[derive(Serialize)]
struct StarfishView {
    license_status: String,
    has_active_session: bool,
}

#[derive(Serialize)]
struct Detail {
    id: i64,
    #[serde(serialize_with = "crate::serde_id::discord_id")]
    discord_id: i64,
    discord_username: Option<String>,
    uuid: Option<String>,
    minecraft_username: Option<String>,
    api_key_preview: Option<String>,
    join_date: DateTime<Utc>,
    request_count: i64,
    access_level: i16,
    key_locked: bool,
    tagging_disabled: bool,
    is_owner: bool,
    standing: StandingView,
    strikes: serde_json::Value,
    config: serde_json::Value,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    ips: Vec<IpRecord>,
    alt_accounts: Vec<AltAccount>,
    dev_key: Option<DevKeyView>,
    starfish: Option<StarfishView>,
    authored_tags: Vec<AuthoredTag>,
}

async fn detail(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Detail>, StatusCode> {
    let member = member_or_404(&state, id).await?;
    let pool = state.db.pool();

    let (created_at, updated_at, api_key_preview): (DateTime<Utc>, DateTime<Utc>, Option<String>) =
        sqlx::query_as("SELECT created_at, updated_at, LEFT(api_key, 8) FROM members WHERE id = $1")
            .bind(id)
            .fetch_one(pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let ips = sqlx::query_as::<_, IpRecord>(
        "SELECT ip_address::text, first_seen, last_seen
         FROM api_key_ips WHERE member_id = $1 ORDER BY last_seen DESC",
    )
    .bind(id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let mut alt_accounts = sqlx::query_as::<_, AltAccount>(
        "SELECT uuid, added_at FROM minecraft_accounts WHERE member_id = $1 ORDER BY added_at DESC",
    )
    .bind(id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let dev_key = DeveloperKeyRepository::new(pool)
        .get_by_member_id(id)
        .await
        .ok()
        .flatten()
        .map(DevKeyView::from);

    let starfish_repo = StarfishRepository::new(pool);
    let starfish = match starfish_repo
        .get_user_by_discord_id(member.discord_id)
        .await
    {
        Ok(Some(user)) => {
            let has_active_session: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM starfish_sessions WHERE user_id = $1 AND expires_at > NOW())",
            )
            .bind(user.id)
            .fetch_one(pool)
            .await
            .unwrap_or(false);
            Some(StarfishView {
                license_status: user.license_status,
                has_active_session,
            })
        }
        _ => None,
    };

    let mut authored_tags = sqlx::query_as::<_, AuthoredTag>(
        "SELECT id, uuid, kind, tag_type, reason, ts FROM player_events
         WHERE author = $1 AND kind IN ('tag_set', 'tag_clear')
         ORDER BY ts DESC LIMIT 50",
    )
    .bind(member.discord_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let mut uuids: Vec<String> = alt_accounts.iter().map(|a| a.uuid.clone()).collect();
    uuids.extend(authored_tags.iter().map(|t| t.uuid.clone()));
    if let Some(uuid) = &member.uuid {
        uuids.push(uuid.clone());
    }
    let names = identity::resolve(&state, &[member.discord_id], &uuids).await;
    for alt in &mut alt_accounts {
        alt.minecraft_username = names.minecraft.get(&alt.uuid).cloned();
    }
    for tag in &mut authored_tags {
        tag.minecraft_username = names.minecraft.get(&tag.uuid).cloned();
    }

    let explanation = standing::explain(&member);
    let standing = StandingView {
        can_vote: explanation.can_vote,
        vote_reason: explanation.vote_reason,
        can_tag: explanation.can_tag,
        tag_reason: explanation.tag_reason,
        effective_level: standing::effective_level(&member),
        strike_count: standing::strike_count(&member),
        accepted_tags: member.accepted_tags,
        rejected_tags: member.rejected_tags,
        accurate_verdicts: member.accurate_verdicts,
        incorrect_verdicts: member.incorrect_verdicts,
        bonus_verdicts: member.bonus_verdicts,
    };

    Ok(Json(Detail {
        id: member.id,
        discord_id: member.discord_id,
        discord_username: names.discord.get(&member.discord_id).cloned(),
        minecraft_username: member
            .uuid
            .as_ref()
            .and_then(|u| names.minecraft.get(u).cloned()),
        uuid: member.uuid.clone(),
        api_key_preview,
        join_date: member.join_date,
        request_count: member.request_count,
        access_level: member.access_level,
        key_locked: member.key_locked,
        tagging_disabled: member.tagging_disabled,
        is_owner: state.owner_ids.contains(&member.discord_id),
        standing,
        strikes: normalize_strikes(&member.strikes),
        config: member.config.clone(),
        created_at,
        updated_at,
        ips,
        alt_accounts,
        dev_key,
        starfish,
        authored_tags,
    }))
}

async fn member_or_404(state: &AppState, id: i64) -> Result<Member, StatusCode> {
    MemberRepository::new(state.db.pool())
        .get_by_id(id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)
}

fn normalize_strikes(strikes: &serde_json::Value) -> serde_json::Value {
    let Some(array) = strikes.as_array() else {
        return strikes.clone();
    };
    serde_json::Value::Array(
        array
            .iter()
            .map(|strike| {
                let mut strike = strike.clone();
                if let Some(obj) = strike.as_object_mut() {
                    if let Some(id) = obj.get("struck_by").and_then(|v| v.as_i64()) {
                        obj.insert(
                            "struck_by".to_string(),
                            serde_json::Value::String(id.to_string()),
                        );
                    }
                }
                strike
            })
            .collect(),
    )
}

#[derive(Serialize)]
struct OkResponse {
    ok: bool,
}

async fn lock(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path(id): Path<i64>,
) -> Result<Json<OkResponse>, StatusCode> {
    let member = member_or_404(&state, id).await?;
    MemberRepository::new(state.db.pool())
        .lock_key(member.discord_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    audit(&state, actor, "lock_member", member.discord_id, json!({})).await;
    Ok(Json(OkResponse { ok: true }))
}

async fn unlock(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path(id): Path<i64>,
) -> Result<Json<OkResponse>, StatusCode> {
    let member = member_or_404(&state, id).await?;
    MemberRepository::new(state.db.pool())
        .unlock_key(member.discord_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    audit(&state, actor, "unlock_member", member.discord_id, json!({})).await;
    Ok(Json(OkResponse { ok: true }))
}

#[derive(Deserialize)]
struct AccessLevelRequest {
    level: i16,
}

async fn set_access_level(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path(id): Path<i64>,
    Json(req): Json<AccessLevelRequest>,
) -> Result<Json<OkResponse>, StatusCode> {
    let member = member_or_404(&state, id).await?;
    MemberRepository::new(state.db.pool())
        .set_access_level(member.discord_id, req.level)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    audit(
        &state,
        actor,
        "set_access_level",
        member.discord_id,
        json!({"level": req.level}),
    )
    .await;
    Ok(Json(OkResponse { ok: true }))
}

#[derive(Deserialize)]
struct TaggingDisabledRequest {
    disabled: bool,
}

async fn set_tagging_disabled(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path(id): Path<i64>,
    Json(req): Json<TaggingDisabledRequest>,
) -> Result<Json<OkResponse>, StatusCode> {
    let member = member_or_404(&state, id).await?;
    MemberRepository::new(state.db.pool())
        .set_tagging_disabled(member.discord_id, req.disabled)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    audit(
        &state,
        actor,
        "set_tagging_disabled",
        member.discord_id,
        json!({"disabled": req.disabled}),
    )
    .await;
    Ok(Json(OkResponse { ok: true }))
}

#[derive(Deserialize)]
struct StrikeRequest {
    reason: String,
}

async fn add_strike(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path(id): Path<i64>,
    Json(req): Json<StrikeRequest>,
) -> Result<Json<OkResponse>, StatusCode> {
    let member = member_or_404(&state, id).await?;
    MemberRepository::new(state.db.pool())
        .add_strike(member.discord_id, &req.reason, actor.discord_id as u64)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    audit(
        &state,
        actor,
        "add_strike",
        member.discord_id,
        json!({"reason": req.reason}),
    )
    .await;
    Ok(Json(OkResponse { ok: true }))
}

async fn remove_strike(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path((id, index)): Path<(i64, i64)>,
) -> Result<Json<OkResponse>, StatusCode> {
    let member = member_or_404(&state, id).await?;
    MemberRepository::new(state.db.pool())
        .remove_strike(member.discord_id, index.max(0) as usize)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    audit(
        &state,
        actor,
        "remove_strike",
        member.discord_id,
        json!({"index": index}),
    )
    .await;
    Ok(Json(OkResponse { ok: true }))
}

#[derive(Serialize)]
struct NewApiKey {
    api_key: String,
}

async fn regenerate_api_key(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path(id): Path<i64>,
) -> Result<Json<NewApiKey>, StatusCode> {
    let member = member_or_404(&state, id).await?;
    let new_key = uuid::Uuid::new_v4().to_string();
    MemberRepository::new(state.db.pool())
        .set_api_key(member.discord_id, &new_key)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    audit(
        &state,
        actor,
        "regenerate_api_key",
        member.discord_id,
        json!({}),
    )
    .await;
    Ok(Json(NewApiKey { api_key: new_key }))
}

async fn reset_rate_limit(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path(id): Path<i64>,
) -> Result<Json<OkResponse>, StatusCode> {
    let member = member_or_404(&state, id).await?;
    if let Some(redis) = &state.redis {
        let limiter = RateLimiter::new(redis.clone());
        limiter
            .reset(&format!("sf:{}", member.discord_id))
            .await
            .ok();
        limiter
            .reset(&format!("sfuuids:{}", member.discord_id))
            .await
            .ok();
    }
    audit(
        &state,
        actor,
        "reset_rate_limit",
        member.discord_id,
        json!({}),
    )
    .await;
    Ok(Json(OkResponse { ok: true }))
}

#[derive(Deserialize)]
struct CreateDevKeyRequest {
    label: String,
    permissions: i64,
    rate_limit: i32,
}

async fn create_dev_key(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path(id): Path<i64>,
    Json(req): Json<CreateDevKeyRequest>,
) -> Result<Json<DevKeyView>, StatusCode> {
    let member = member_or_404(&state, id).await?;
    let key = uuid::Uuid::new_v4().to_string();
    let created = DeveloperKeyRepository::new(state.db.pool())
        .create(member.id, &key, &req.label, req.permissions, req.rate_limit)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    audit(
        &state,
        actor,
        "create_dev_key",
        member.discord_id,
        json!({"label": req.label, "permissions": req.permissions, "rate_limit": req.rate_limit}),
    )
    .await;
    let mut view = DevKeyView::from(created);
    view.api_key = Some(key);
    Ok(Json(view))
}

async fn delete_dev_key(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path(id): Path<i64>,
) -> Result<Json<OkResponse>, StatusCode> {
    let member = member_or_404(&state, id).await?;
    DeveloperKeyRepository::new(state.db.pool())
        .delete(member.id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    audit(
        &state,
        actor,
        "delete_dev_key",
        member.discord_id,
        json!({}),
    )
    .await;
    Ok(Json(OkResponse { ok: true }))
}

#[derive(Deserialize)]
struct DevKeyLockRequest {
    locked: bool,
}

async fn set_dev_key_locked(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path(id): Path<i64>,
    Json(req): Json<DevKeyLockRequest>,
) -> Result<Json<OkResponse>, StatusCode> {
    let member = member_or_404(&state, id).await?;
    DeveloperKeyRepository::new(state.db.pool())
        .set_locked(member.id, req.locked)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    audit(
        &state,
        actor,
        "set_dev_key_locked",
        member.discord_id,
        json!({"locked": req.locked}),
    )
    .await;
    Ok(Json(OkResponse { ok: true }))
}

#[derive(Deserialize)]
struct DevKeyRateLimitRequest {
    rate_limit: i32,
}

async fn set_dev_key_rate_limit(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path(id): Path<i64>,
    Json(req): Json<DevKeyRateLimitRequest>,
) -> Result<Json<OkResponse>, StatusCode> {
    let member = member_or_404(&state, id).await?;
    DeveloperKeyRepository::new(state.db.pool())
        .set_rate_limit(member.id, req.rate_limit)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    audit(
        &state,
        actor,
        "set_dev_key_rate_limit",
        member.discord_id,
        json!({"rate_limit": req.rate_limit}),
    )
    .await;
    Ok(Json(OkResponse { ok: true }))
}

#[derive(Deserialize)]
struct DevKeyPermissionsRequest {
    permissions: i64,
}

async fn set_dev_key_permissions(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path(id): Path<i64>,
    Json(req): Json<DevKeyPermissionsRequest>,
) -> Result<Json<OkResponse>, StatusCode> {
    let member = member_or_404(&state, id).await?;
    DeveloperKeyRepository::new(state.db.pool())
        .set_permissions(member.id, req.permissions)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    audit(
        &state,
        actor,
        "set_dev_key_permissions",
        member.discord_id,
        json!({"permissions": req.permissions}),
    )
    .await;
    Ok(Json(OkResponse { ok: true }))
}

#[derive(Deserialize)]
struct LicenseStatusRequest {
    status: String,
}

async fn set_license_status(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path(id): Path<i64>,
    Json(req): Json<LicenseStatusRequest>,
) -> Result<Json<OkResponse>, StatusCode> {
    let member = member_or_404(&state, id).await?;
    StarfishRepository::new(state.db.pool())
        .set_license_status(member.discord_id, &req.status)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    audit(
        &state,
        actor,
        "set_license_status",
        member.discord_id,
        json!({"status": req.status}),
    )
    .await;
    Ok(Json(OkResponse { ok: true }))
}

async fn revoke_sessions(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path(id): Path<i64>,
) -> Result<Json<OkResponse>, StatusCode> {
    let member = member_or_404(&state, id).await?;
    let starfish = StarfishRepository::new(state.db.pool());
    let user = starfish
        .get_user_by_discord_id(member.discord_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    starfish
        .delete_user_sessions(user.id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    starfish
        .delete_user_refresh_tokens(user.id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    audit(
        &state,
        actor,
        "revoke_starfish_sessions",
        member.discord_id,
        json!({}),
    )
    .await;
    Ok(Json(OkResponse { ok: true }))
}

async fn audit(
    state: &AppState,
    actor: AdminActor,
    action: &str,
    target_discord_id: i64,
    details: serde_json::Value,
) {
    crate::audit::log(
        state,
        actor.discord_id,
        action,
        &target_discord_id.to_string(),
        details,
    )
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_strikes_stringifies_struck_by() {
        let strikes = json!([
            {"reason": "abuse", "struck_by": 1_547_000_000_000_000_000i64, "timestamp": "2026-01-01T00:00:00Z"},
        ]);
        let normalized = normalize_strikes(&strikes);
        assert_eq!(normalized[0]["struck_by"], json!("1547000000000000000"));
        assert_eq!(normalized[0]["reason"], json!("abuse"));
    }

    #[test]
    fn normalize_strikes_handles_empty_array() {
        let strikes = json!([]);
        assert_eq!(normalize_strikes(&strikes), json!([]));
    }

    #[test]
    fn normalize_strikes_passes_through_non_array() {
        let value = json!({"unexpected": true});
        assert_eq!(normalize_strikes(&value), value);
    }
}
