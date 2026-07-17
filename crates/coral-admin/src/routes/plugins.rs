use axum::http::StatusCode;
use axum::{Extension, Json, Router, extract::*, routing::get, routing::post};
use chrono::{DateTime, Utc};
use database::{
    Plugin, PluginRegistryRepository, PluginRelease, PluginSortMode, PluginSummary,
    StarfishRepository,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::auth::AdminActor;
use crate::identity;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list))
        .route("/{slug}", get(detail))
        .route("/{slug}/official", post(set_official))
        .route("/{slug}/unlisted", post(set_unlisted))
        .route("/{slug}/disabled", post(set_disabled))
        .route("/{slug}", axum::routing::delete(delete_plugin))
        .route("/{slug}/releases/{version}/yank", post(yank_release))
        .route("/{slug}/releases/{version}/unyank", post(unyank_release))
        .route(
            "/{slug}/releases/{version}",
            axum::routing::delete(delete_release),
        )
        .route(
            "/{slug}/reviews/{user_id}",
            axum::routing::delete(delete_review),
        )
}

#[derive(Deserialize)]
struct ListParams {
    search: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Serialize)]
struct PluginRow {
    #[serde(flatten)]
    summary: PluginSummary,
    owner_discord_username: Option<String>,
    owner_member_id: Option<i64>,
}

#[derive(Serialize)]
struct ListResponse {
    total: i64,
    plugins: Vec<PluginRow>,
}

async fn list(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Json<ListResponse> {
    let limit = params.limit.unwrap_or(50).clamp(1, 200);
    let offset = params.offset.unwrap_or(0).max(0);
    let repo = PluginRegistryRepository::new(state.db.pool());

    let search = params
        .search
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let (total, summaries) = repo
        .list_plugins(PluginSortMode::New, None, search, None, true, limit, offset)
        .await
        .unwrap_or((0, vec![]));

    let discord_ids: Vec<i64> = summaries.iter().map(|p| p.owner_discord_id).collect();
    let (names, member_ids) = tokio::join!(
        identity::resolve_discord_usernames(&state, &discord_ids),
        identity::member_ids_by_discord_id(&state, &discord_ids),
    );

    let plugins = summaries
        .into_iter()
        .map(|summary| {
            let owner_discord_username = names.get(&summary.owner_discord_id).cloned();
            let owner_member_id = member_ids.get(&summary.owner_discord_id).copied();
            PluginRow {
                summary,
                owner_discord_username,
                owner_member_id,
            }
        })
        .collect();

    Json(ListResponse { total, plugins })
}

#[derive(Serialize)]
struct ReleaseView {
    id: i64,
    version: String,
    git_sha: String,
    asset_url: String,
    asset_sha256: String,
    content_sha256: Option<String>,
    asset_size: i32,
    changelog: Option<String>,
    yanked: bool,
    yanked_at: Option<DateTime<Utc>>,
    yanked_reason: Option<String>,
    created_at: DateTime<Utc>,
}

impl From<PluginRelease> for ReleaseView {
    fn from(r: PluginRelease) -> Self {
        Self {
            id: r.id,
            version: r.version,
            git_sha: r.git_sha,
            asset_url: r.asset_url,
            asset_sha256: hex::encode(&r.asset_sha256),
            content_sha256: r.content_sha256.map(hex::encode),
            asset_size: r.asset_size,
            changelog: r.changelog,
            yanked: r.yanked,
            yanked_at: r.yanked_at,
            yanked_reason: r.yanked_reason,
            created_at: r.created_at,
        }
    }
}

#[derive(Serialize)]
struct ReviewView {
    user_id: i64,
    #[serde(serialize_with = "crate::serde_id::discord_id_opt")]
    discord_id: Option<i64>,
    discord_username: Option<String>,
    stars: i16,
    review: Option<String>,
    updated_at: DateTime<Utc>,
}

#[derive(Serialize)]
struct DetailResponse {
    plugin: Plugin,
    #[serde(serialize_with = "crate::serde_id::discord_id_opt")]
    owner_discord_id: Option<i64>,
    owner_discord_username: Option<String>,
    owner_member_id: Option<i64>,
    releases: Vec<ReleaseView>,
    installs_30d: i64,
    installs_total: i64,
    reviews: Vec<ReviewView>,
}

async fn detail(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<DetailResponse>, StatusCode> {
    let repo = PluginRegistryRepository::new(state.db.pool());
    let plugin = repo
        .get_plugin_by_slug(&slug)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let owner = StarfishRepository::new(state.db.pool())
        .get_user_by_id(plugin.owner_user_id)
        .await
        .ok()
        .flatten();
    let owner_discord_id = owner.as_ref().map(|u| u.discord_id);
    let owner_discord_username = match owner_discord_id {
        Some(id) => identity::resolve_discord_usernames(&state, &[id])
            .await
            .get(&id)
            .cloned(),
        None => None,
    };
    let owner_member_id = match owner_discord_id {
        Some(id) => identity::member_ids_by_discord_id(&state, &[id])
            .await
            .get(&id)
            .copied(),
        None => None,
    };

    let releases = repo
        .list_releases(plugin.id)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(ReleaseView::from)
        .collect();
    let (installs_30d, installs_total) = repo
        .plugin_install_counts(plugin.id)
        .await
        .unwrap_or((0, 0));
    let ratings = repo
        .list_plugin_ratings(plugin.id, 50)
        .await
        .unwrap_or_default();

    let reviewer_ids: Vec<i64> = ratings.iter().map(|r| r.user_id).collect();
    let reviewer_discord_ids: std::collections::HashMap<i64, i64> =
        sqlx::query_as::<_, (i64, i64)>(
            "SELECT id, discord_id FROM starfish_users WHERE id = ANY($1)",
        )
        .bind(&reviewer_ids)
        .fetch_all(state.db.pool())
        .await
        .unwrap_or_default()
        .into_iter()
        .collect();
    let discord_ids: Vec<i64> = reviewer_discord_ids.values().copied().collect();
    let names = identity::resolve_discord_usernames(&state, &discord_ids).await;

    let reviews = ratings
        .into_iter()
        .map(|r| {
            let discord_id = reviewer_discord_ids.get(&r.user_id).copied();
            ReviewView {
                user_id: r.user_id,
                discord_username: discord_id.and_then(|id| names.get(&id).cloned()),
                discord_id,
                stars: r.stars,
                review: r.review,
                updated_at: r.updated_at,
            }
        })
        .collect();

    Ok(Json(DetailResponse {
        plugin,
        owner_discord_id,
        owner_discord_username,
        owner_member_id,
        releases,
        installs_30d,
        installs_total,
        reviews,
    }))
}

#[derive(Serialize)]
struct OkResponse {
    ok: bool,
}

async fn plugin_or_404(state: &AppState, slug: &str) -> Result<Plugin, StatusCode> {
    PluginRegistryRepository::new(state.db.pool())
        .get_plugin_by_slug(slug)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)
}

#[derive(Deserialize)]
struct SetOfficialRequest {
    official: bool,
}

async fn set_official(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path(slug): Path<String>,
    Json(req): Json<SetOfficialRequest>,
) -> Result<Json<OkResponse>, StatusCode> {
    let plugin = plugin_or_404(&state, &slug).await?;
    PluginRegistryRepository::new(state.db.pool())
        .set_official(plugin.id, req.official)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    audit(
        &state,
        actor,
        "set_plugin_official",
        &slug,
        json!({"official": req.official}),
    )
    .await;
    Ok(Json(OkResponse { ok: true }))
}

#[derive(Deserialize)]
struct SetUnlistedRequest {
    unlisted: bool,
}

async fn set_unlisted(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path(slug): Path<String>,
    Json(req): Json<SetUnlistedRequest>,
) -> Result<Json<OkResponse>, StatusCode> {
    let plugin = plugin_or_404(&state, &slug).await?;
    PluginRegistryRepository::new(state.db.pool())
        .set_unlisted(plugin.id, req.unlisted)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    audit(
        &state,
        actor,
        "set_plugin_unlisted",
        &slug,
        json!({"unlisted": req.unlisted}),
    )
    .await;
    Ok(Json(OkResponse { ok: true }))
}

#[derive(Deserialize)]
struct SetDisabledRequest {
    disabled: bool,
    #[serde(default)]
    reason: Option<String>,
}

async fn set_disabled(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path(slug): Path<String>,
    Json(req): Json<SetDisabledRequest>,
) -> Result<Json<OkResponse>, StatusCode> {
    let plugin = plugin_or_404(&state, &slug).await?;
    PluginRegistryRepository::new(state.db.pool())
        .set_disabled(plugin.id, req.disabled, req.reason.as_deref())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    audit(
        &state,
        actor,
        "set_plugin_disabled",
        &slug,
        json!({"disabled": req.disabled, "reason": req.reason}),
    )
    .await;
    Ok(Json(OkResponse { ok: true }))
}

async fn delete_plugin(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path(slug): Path<String>,
) -> Result<Json<OkResponse>, StatusCode> {
    let plugin = plugin_or_404(&state, &slug).await?;
    PluginRegistryRepository::new(state.db.pool())
        .delete_plugin(plugin.id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    audit(&state, actor, "delete_plugin", &slug, json!({})).await;
    Ok(Json(OkResponse { ok: true }))
}

#[derive(Deserialize)]
struct YankRequest {
    #[serde(default)]
    reason: Option<String>,
}

async fn yank_release(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path((slug, version)): Path<(String, String)>,
    Json(req): Json<YankRequest>,
) -> Result<Json<OkResponse>, StatusCode> {
    let plugin = plugin_or_404(&state, &slug).await?;
    let reason = req
        .reason
        .unwrap_or_else(|| format!("yanked by admin {}", actor.discord_id));
    let ok = PluginRegistryRepository::new(state.db.pool())
        .yank_release(plugin.id, &version, &reason)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !ok {
        return Err(StatusCode::NOT_FOUND);
    }
    audit(
        &state,
        actor,
        "yank_release",
        &slug,
        json!({"version": version, "reason": reason}),
    )
    .await;
    Ok(Json(OkResponse { ok: true }))
}

async fn unyank_release(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path((slug, version)): Path<(String, String)>,
) -> Result<Json<OkResponse>, StatusCode> {
    let plugin = plugin_or_404(&state, &slug).await?;
    let ok = PluginRegistryRepository::new(state.db.pool())
        .unyank_release(plugin.id, &version)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !ok {
        return Err(StatusCode::NOT_FOUND);
    }
    audit(
        &state,
        actor,
        "unyank_release",
        &slug,
        json!({"version": version}),
    )
    .await;
    Ok(Json(OkResponse { ok: true }))
}

async fn delete_release(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path((slug, version)): Path<(String, String)>,
) -> Result<Json<OkResponse>, StatusCode> {
    let plugin = plugin_or_404(&state, &slug).await?;
    let ok = PluginRegistryRepository::new(state.db.pool())
        .delete_release(plugin.id, &version)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !ok {
        return Err(StatusCode::NOT_FOUND);
    }
    audit(
        &state,
        actor,
        "delete_release",
        &slug,
        json!({"version": version}),
    )
    .await;
    Ok(Json(OkResponse { ok: true }))
}

async fn delete_review(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path((slug, user_id)): Path<(String, i64)>,
) -> Result<Json<OkResponse>, StatusCode> {
    let plugin = plugin_or_404(&state, &slug).await?;
    let ok = PluginRegistryRepository::new(state.db.pool())
        .delete_rating(user_id, plugin.id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !ok {
        return Err(StatusCode::NOT_FOUND);
    }
    audit(
        &state,
        actor,
        "delete_plugin_review",
        &slug,
        json!({"user_id": user_id}),
    )
    .await;
    Ok(Json(OkResponse { ok: true }))
}

async fn audit(
    state: &AppState,
    actor: AdminActor,
    action: &str,
    target: &str,
    details: serde_json::Value,
) {
    crate::audit::log(state, actor.discord_id, action, target, details).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_release(content_sha256: Option<Vec<u8>>) -> PluginRelease {
        PluginRelease {
            id: 1,
            plugin_id: 2,
            version: "1.0.0".into(),
            git_sha: "abc123".into(),
            asset_url: "https://example.com/asset.zip".into(),
            asset_sha256: vec![0xde, 0xad, 0xbe, 0xef],
            content_sha256,
            asset_size: 1024,
            manifest_json: serde_json::json!({}),
            changelog: None,
            yanked: false,
            yanked_at: None,
            yanked_reason: None,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn release_view_hex_encodes_hashes() {
        let view = ReleaseView::from(sample_release(Some(vec![0xca, 0xfe])));
        assert_eq!(view.asset_sha256, "deadbeef");
        assert_eq!(view.content_sha256.as_deref(), Some("cafe"));
    }

    #[test]
    fn release_view_handles_missing_content_hash() {
        let view = ReleaseView::from(sample_release(None));
        assert_eq!(view.content_sha256, None);
    }
}
