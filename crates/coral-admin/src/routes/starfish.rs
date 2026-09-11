use axum::http::StatusCode;
use axum::{Extension, Json, Router, extract::*, routing::get, routing::post};
use chrono::{DateTime, Utc};
use database::{PluginRegistryRepository, StarfishRepository};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::FromRow;

use crate::auth::AdminActor;
use crate::identity;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list))
        .route("/{id}", get(detail))
        .route("/{id}/license", post(set_license_status))
        .route("/{id}/sessions/revoke", post(revoke_sessions))
}

#[derive(Deserialize)]
struct ListParams {
    search: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Serialize, FromRow)]
struct UserRow {
    id: i64,
    #[serde(serialize_with = "crate::serde_id::discord_id")]
    discord_id: i64,
    license_status: String,
    github_username: Option<String>,
    hwid_count: i64,
    has_active_session: bool,
    last_heartbeat_at: Option<DateTime<Utc>>,
    plugins_installed: i64,
    plugins_owned: i64,
    updated_at: DateTime<Utc>,
    #[sqlx(default)]
    discord_username: Option<String>,
    #[sqlx(default)]
    member_id: Option<i64>,
}

#[derive(Serialize)]
struct ListResponse {
    total: i64,
    users: Vec<UserRow>,
}

async fn list(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Json<ListResponse> {
    let limit = params.limit.unwrap_or(50).clamp(1, 200);
    let offset = params.offset.unwrap_or(0).max(0);
    let pool = state.db.pool();

    let pattern = params
        .search
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| format!("%{s}%"));

    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM starfish_users u
         LEFT JOIN discord_username_cache c ON c.discord_id = u.discord_id
         WHERE u.license_status <> 'inactive'
           AND ($1::text IS NULL OR u.discord_id::text ILIKE $1 OR c.username ILIKE $1)",
    )
    .bind(&pattern)
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    let mut users: Vec<UserRow> = sqlx::query_as(
        "SELECT
            u.id, u.discord_id, u.license_status, u.github_username, u.updated_at,
            (SELECT COUNT(*) FROM starfish_hwids h WHERE h.user_id = u.id) AS hwid_count,
            EXISTS(SELECT 1 FROM starfish_sessions s WHERE s.user_id = u.id AND s.expires_at > NOW()) AS has_active_session,
            (SELECT MAX(last_heartbeat_at) FROM starfish_sessions s WHERE s.user_id = u.id) AS last_heartbeat_at,
            COALESCE((SELECT COUNT(*) FROM plugin_installs pi WHERE pi.user_id = u.id), 0) AS plugins_installed,
            COALESCE((SELECT COUNT(*) FROM plugins p WHERE p.owner_user_id = u.id), 0) AS plugins_owned
         FROM starfish_users u
         LEFT JOIN discord_username_cache c ON c.discord_id = u.discord_id
         WHERE u.license_status <> 'inactive'
           AND ($1::text IS NULL OR u.discord_id::text ILIKE $1 OR c.username ILIKE $1)
         ORDER BY u.updated_at DESC
         LIMIT $2 OFFSET $3",
    )
    .bind(&pattern)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let discord_ids: Vec<i64> = users.iter().map(|u| u.discord_id).collect();
    let (names, member_ids) = tokio::join!(
        identity::resolve_discord_usernames(&state, &discord_ids),
        identity::member_ids_by_discord_id(&state, &discord_ids),
    );
    for u in &mut users {
        u.discord_username = names.get(&u.discord_id).cloned();
        u.member_id = member_ids.get(&u.discord_id).copied();
    }

    Json(ListResponse { total, users })
}

#[derive(Serialize)]
struct HwidView {
    id: i64,
    hwid_hash: String,
    is_active: bool,
    registered_at: DateTime<Utc>,
    has_components: bool,
}

#[derive(Serialize)]
struct SessionView {
    id: i64,
    hwid_id: i64,
    issued_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    last_heartbeat_at: DateTime<Utc>,
}

#[derive(Serialize)]
struct InstalledPluginView {
    slug: String,
    installed_version: String,
    latest_version: String,
    disabled: bool,
}

#[derive(Serialize)]
struct OwnedPluginView {
    slug: String,
    display_name: String,
    official: bool,
    unlisted: bool,
    disabled: bool,
    latest_version: Option<String>,
}

#[derive(Serialize)]
struct Detail {
    id: i64,
    #[serde(serialize_with = "crate::serde_id::discord_id")]
    discord_id: i64,
    discord_username: Option<String>,
    member_id: Option<i64>,
    license_status: String,
    github_username: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    hwids: Vec<HwidView>,
    sessions: Vec<SessionView>,
    installed_plugins: Vec<InstalledPluginView>,
    owned_plugins: Vec<OwnedPluginView>,
}

async fn detail(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Detail>, StatusCode> {
    let pool = state.db.pool();
    let starfish = StarfishRepository::new(pool);
    let user = starfish
        .get_user_by_id(id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let discord_ids = [user.discord_id];
    let (names, member_ids) = tokio::join!(
        identity::resolve_discord_usernames(&state, &discord_ids),
        identity::member_ids_by_discord_id(&state, &discord_ids),
    );
    let discord_username = names.get(&user.discord_id).cloned();
    let member_id = member_ids.get(&user.discord_id).copied();

    let hwids_raw = starfish.get_user_hwids(id).await.unwrap_or_default();
    let mut hwids = Vec::with_capacity(hwids_raw.len());
    for h in hwids_raw {
        let has_components = starfish
            .get_hwid_components(h.id)
            .await
            .ok()
            .flatten()
            .is_some();
        hwids.push(HwidView {
            id: h.id,
            hwid_hash: h.hwid_hash,
            is_active: h.is_active,
            registered_at: h.registered_at,
            has_components,
        });
    }

    let sessions = starfish
        .get_user_sessions(id)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|s| SessionView {
            id: s.id,
            hwid_id: s.hwid_id,
            issued_at: s.issued_at,
            expires_at: s.expires_at,
            last_heartbeat_at: s.last_heartbeat_at,
        })
        .collect();

    let plugins = PluginRegistryRepository::new(pool);
    let installed_plugins = plugins
        .list_user_installs(id)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|i| InstalledPluginView {
            slug: i.slug,
            installed_version: i.installed_version,
            latest_version: i.latest_version,
            disabled: i.disabled,
        })
        .collect();
    let owned_plugins = plugins
        .list_my_plugins_with_latest(id)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|p| OwnedPluginView {
            slug: p.slug,
            display_name: p.display_name,
            official: p.official,
            unlisted: p.unlisted,
            disabled: p.disabled,
            latest_version: p.latest_version,
        })
        .collect();

    Ok(Json(Detail {
        id: user.id,
        discord_id: user.discord_id,
        discord_username,
        member_id,
        license_status: user.license_status,
        github_username: user.github_username,
        created_at: user.created_at,
        updated_at: user.updated_at,
        hwids,
        sessions,
        installed_plugins,
        owned_plugins,
    }))
}

#[derive(Serialize)]
struct OkResponse {
    ok: bool,
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
    let starfish = StarfishRepository::new(state.db.pool());
    let user = starfish
        .get_user_by_id(id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    starfish
        .set_license_status(user.discord_id, &req.status)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    audit(
        &state,
        actor,
        "set_license_status",
        user.discord_id,
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
    let starfish = StarfishRepository::new(state.db.pool());
    let user = starfish
        .get_user_by_id(id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    starfish
        .delete_user_sessions(id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    starfish
        .delete_user_refresh_tokens(id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    audit(
        &state,
        actor,
        "revoke_starfish_sessions",
        user.discord_id,
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
