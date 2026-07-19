use axum::{
    Extension, Json,
    extract::{Path, State},
};
use serde::{Deserialize, Serialize};

use database::PluginRegistryRepository;

use crate::{error::ApiError, state::AppState};

use super::super::{is_owner, require_owner, session_auth::AuthenticatedStarfishUser};

const MAX_DISPLAY_NAME_CHARS: usize = 64;

#[derive(Deserialize)]
pub struct PatchPluginRequest {
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub homepage: Option<String>,
}

#[derive(Deserialize)]
pub struct SetOfficialRequest {
    pub official: bool,
}

#[derive(Deserialize)]
pub struct SetDisabledRequest {
    pub disabled: bool,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Deserialize)]
pub struct YankRequest {
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Serialize)]
pub struct OkResponse {
    pub ok: bool,
}

pub async fn list_mine(
    State(state): State<AppState>,
    Extension(caller): Extension<AuthenticatedStarfishUser>,
) -> Result<Json<Vec<super::dto::OwnedPluginDto>>, ApiError> {
    let repo = PluginRegistryRepository::new(state.db.pool());
    let plugins = repo.list_my_plugins_with_latest(caller.user.id).await?;
    Ok(Json(plugins.into_iter().map(Into::into).collect()))
}

pub async fn patch_plugin(
    State(state): State<AppState>,
    Extension(caller): Extension<AuthenticatedStarfishUser>,
    Path(slug): Path<String>,
    Json(patch): Json<PatchPluginRequest>,
) -> Result<Json<database::Plugin>, ApiError> {
    let repo = PluginRegistryRepository::new(state.db.pool());
    let plugin = ensure_plugin_owner(&repo, &slug, caller.user.id).await?;

    let display_name = patch
        .display_name
        .unwrap_or_else(|| plugin.display_name.clone());
    let description = patch
        .description
        .unwrap_or_else(|| plugin.description.clone());
    let homepage = patch.homepage.or(plugin.homepage.clone());

    if display_name.trim().is_empty() || display_name.len() > MAX_DISPLAY_NAME_CHARS {
        return Err(ApiError::BadRequest(format!(
            "display_name must be 1-{MAX_DISPLAY_NAME_CHARS} chars"
        )));
    }
    if description.trim().is_empty() || description.len() > super::manifest::MAX_DESCRIPTION_CHARS {
        return Err(ApiError::BadRequest(format!(
            "description must be 1-{} chars",
            super::manifest::MAX_DESCRIPTION_CHARS
        )));
    }

    let updated = repo
        .update_plugin_metadata(
            plugin.id,
            &plugin.repo,
            &display_name,
            &description,
            homepage.as_deref(),
        )
        .await?;

    Ok(Json(updated))
}

pub async fn set_official(
    State(state): State<AppState>,
    Extension(caller): Extension<AuthenticatedStarfishUser>,
    Path(slug): Path<String>,
    Json(req): Json<SetOfficialRequest>,
) -> Result<Json<OkResponse>, ApiError> {
    require_owner(&caller)?;
    let repo = PluginRegistryRepository::new(state.db.pool());
    let plugin = repo
        .get_plugin_by_slug(&slug)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("plugin {slug} not found")))?;
    repo.set_official(plugin.id, req.official).await?;
    Ok(Json(OkResponse { ok: true }))
}

pub async fn set_disabled(
    State(state): State<AppState>,
    Extension(caller): Extension<AuthenticatedStarfishUser>,
    Path(slug): Path<String>,
    Json(req): Json<SetDisabledRequest>,
) -> Result<Json<OkResponse>, ApiError> {
    require_owner(&caller)?;
    let repo = PluginRegistryRepository::new(state.db.pool());
    let plugin = repo
        .get_plugin_by_slug(&slug)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("plugin {slug} not found")))?;
    repo.set_disabled(plugin.id, req.disabled, req.reason.as_deref())
        .await?;
    Ok(Json(OkResponse { ok: true }))
}

pub async fn yank_release(
    State(state): State<AppState>,
    Extension(caller): Extension<AuthenticatedStarfishUser>,
    Path((slug, version)): Path<(String, String)>,
    Json(req): Json<YankRequest>,
) -> Result<Json<OkResponse>, ApiError> {
    let repo = PluginRegistryRepository::new(state.db.pool());
    let plugin = ensure_plugin_owner_or_admin(&repo, &slug, &caller).await?;
    let reason = req
        .reason
        .unwrap_or_else(|| format!("yanked by {}", caller.user.id));
    let ok = repo.yank_release(plugin.id, &version, &reason).await?;
    if !ok {
        return Err(ApiError::NotFound(format!("release {version} not found")));
    }
    Ok(Json(OkResponse { ok: true }))
}

pub async fn unyank_release(
    State(state): State<AppState>,
    Extension(caller): Extension<AuthenticatedStarfishUser>,
    Path((slug, version)): Path<(String, String)>,
) -> Result<Json<OkResponse>, ApiError> {
    let repo = PluginRegistryRepository::new(state.db.pool());
    let plugin = ensure_plugin_owner_or_admin(&repo, &slug, &caller).await?;
    let ok = repo.unyank_release(plugin.id, &version).await?;
    if !ok {
        return Err(ApiError::NotFound(format!("release {version} not found")));
    }
    Ok(Json(OkResponse { ok: true }))
}

pub async fn delete_plugin(
    State(state): State<AppState>,
    Extension(caller): Extension<AuthenticatedStarfishUser>,
    Path(slug): Path<String>,
) -> Result<Json<OkResponse>, ApiError> {
    let repo = PluginRegistryRepository::new(state.db.pool());
    let plugin = ensure_plugin_owner_or_admin(&repo, &slug, &caller).await?;
    repo.delete_plugin(plugin.id).await?;
    Ok(Json(OkResponse { ok: true }))
}

pub async fn delete_release(
    State(state): State<AppState>,
    Extension(caller): Extension<AuthenticatedStarfishUser>,
    Path((slug, version)): Path<(String, String)>,
) -> Result<Json<OkResponse>, ApiError> {
    let repo = PluginRegistryRepository::new(state.db.pool());
    let plugin = ensure_plugin_owner_or_admin(&repo, &slug, &caller).await?;

    if !is_owner(caller.user.discord_id) {
        let installs = repo.release_install_count(plugin.id, &version).await?;
        if installs > 0 {
            return Err(ApiError::Conflict(format!(
                "release v{version} has {installs} active install(s) — unlist it instead"
            )));
        }
    }

    let ok = repo.delete_release(plugin.id, &version).await?;
    if !ok {
        return Err(ApiError::NotFound(format!("release {version} not found")));
    }
    Ok(Json(OkResponse { ok: true }))
}

async fn ensure_plugin_owner(
    repo: &PluginRegistryRepository<'_>,
    slug: &str,
    user_id: i64,
) -> Result<database::Plugin, ApiError> {
    let plugin = repo
        .get_plugin_by_slug(slug)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("plugin {slug} not found")))?;
    if plugin.owner_user_id != user_id {
        return Err(ApiError::Forbidden("not_plugin_owner".into()));
    }
    Ok(plugin)
}

async fn ensure_plugin_owner_or_admin(
    repo: &PluginRegistryRepository<'_>,
    slug: &str,
    caller: &AuthenticatedStarfishUser,
) -> Result<database::Plugin, ApiError> {
    let plugin = repo
        .get_plugin_by_slug(slug)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("plugin {slug} not found")))?;
    if plugin.owner_user_id != caller.user.id && !is_owner(caller.user.discord_id) {
        return Err(ApiError::Forbidden("not_plugin_owner".into()));
    }
    Ok(plugin)
}
