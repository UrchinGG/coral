use axum::{Extension, Json, extract::{Path, State}};

use database::PluginRegistryRepository;

use crate::{error::ApiError, state::AppState};

use super::super::session_auth::AuthenticatedStarfishUser;
use super::dto::{
    InstallResponse, InstalledEntryDto, InstalledResponse, RateRequest, ReleaseInfoDto,
};


pub async fn install_plugin(
    State(state): State<AppState>,
    Extension(caller): Extension<AuthenticatedStarfishUser>,
    Path(slug): Path<String>,
) -> Result<Json<InstallResponse>, ApiError> {
    let repo = PluginRegistryRepository::new(state.db.pool());

    let plugin = repo.get_plugin_by_slug(&slug).await?
        .ok_or_else(|| ApiError::NotFound(format!("plugin {slug} not found")))?;

    if plugin.disabled {
        return Err(ApiError::Forbidden(
            plugin.disabled_reason.unwrap_or_else(|| "plugin_disabled".into()),
        ));
    }

    let release = repo.get_latest_release(plugin.id).await?
        .ok_or_else(|| ApiError::NotFound(format!("{slug} has no available release")))?;

    repo.upsert_install(caller.user.id, plugin.id, release.id).await?;

    Ok(Json(InstallResponse {
        slug: plugin.slug.clone(),
        version: release.version.clone(),
        asset_sha256: hex::encode(&release.asset_sha256),
        asset_size: release.asset_size,
        manifest: release.manifest_json,
        body_url: format!("/api/v1/starfish/plugins/{}/body", plugin.slug),
    }))
}


pub async fn uninstall_plugin(
    State(state): State<AppState>,
    Extension(caller): Extension<AuthenticatedStarfishUser>,
    Path(slug): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let repo = PluginRegistryRepository::new(state.db.pool());
    let plugin = repo.get_plugin_by_slug(&slug).await?
        .ok_or_else(|| ApiError::NotFound(format!("plugin {slug} not found")))?;
    repo.delete_install(caller.user.id, plugin.id).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}


pub async fn rate_plugin(
    State(state): State<AppState>,
    Extension(caller): Extension<AuthenticatedStarfishUser>,
    Path(slug): Path<String>,
    Json(req): Json<RateRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !(1..=5).contains(&req.stars) {
        return Err(ApiError::BadRequest("stars must be 1-5".into()));
    }
    if let Some(review) = &req.review {
        if review.len() > 2000 {
            return Err(ApiError::BadRequest("review exceeds 2000 chars".into()));
        }
    }

    let repo = PluginRegistryRepository::new(state.db.pool());
    let plugin = repo.get_plugin_by_slug(&slug).await?
        .ok_or_else(|| ApiError::NotFound(format!("plugin {slug} not found")))?;

    if repo.get_install(caller.user.id, plugin.id).await?.is_none() {
        return Err(ApiError::Conflict("must install before rating".into()));
    }

    repo.upsert_rating(caller.user.id, plugin.id, req.stars, req.review.as_deref()).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}


pub async fn list_installed(
    State(state): State<AppState>,
    Extension(caller): Extension<AuthenticatedStarfishUser>,
) -> Result<Json<InstalledResponse>, ApiError> {
    let repo = PluginRegistryRepository::new(state.db.pool());
    let rows = repo.list_user_installs(caller.user.id).await?;

    let installs = rows.into_iter().map(|r| InstalledEntryDto {
        update_available: r.installed_version != r.latest_version,
        latest_release: ReleaseInfoDto {
            version: r.latest_version.clone(),
            git_sha: r.latest_git_sha.clone(),
            changelog: r.latest_changelog,
            asset_sha256: hex::encode(&r.latest_asset_sha256),
            asset_size: r.latest_asset_size,
            yanked: false,
            yanked_reason: None,
            created_at: r.latest_created_at,
        },
        slug: r.slug,
        installed_version: r.installed_version,
        latest_version: r.latest_version,
        disabled: r.disabled,
    }).collect();

    Ok(Json(InstalledResponse { installs }))
}
