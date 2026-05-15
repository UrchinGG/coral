use axum::{Extension, Json, extract::{Path, State}};
use serde::{Deserialize, Serialize};

use database::PluginRegistryRepository;

use crate::{error::ApiError, state::AppState};

use super::super::session_auth::AuthenticatedStarfishUser;


const MAX_PAGE_OVERRIDE_BYTES: usize = 16 * 1024;


#[derive(Deserialize)]
pub struct PatchPluginRequest {
    pub description: Option<String>,
    pub homepage: Option<String>,
    pub tags: Option<Vec<String>>,
    pub page_override: Option<String>,
}


#[derive(Deserialize)]
pub struct UnlistRequest {
    pub unlisted: bool,
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
) -> Result<Json<Vec<database::Plugin>>, ApiError> {
    let repo = PluginRegistryRepository::new(state.db.pool());
    let plugins = repo.list_my_plugins(caller.user.id).await?;
    Ok(Json(plugins))
}


pub async fn patch_plugin(
    State(state): State<AppState>,
    Extension(caller): Extension<AuthenticatedStarfishUser>,
    Path(slug): Path<String>,
    Json(patch): Json<PatchPluginRequest>,
) -> Result<Json<database::Plugin>, ApiError> {
    let repo = PluginRegistryRepository::new(state.db.pool());
    let plugin = ensure_owner(&repo, &slug, caller.user.id).await?;

    if let Some(page) = patch.page_override.as_deref() {
        if page.len() > MAX_PAGE_OVERRIDE_BYTES {
            return Err(ApiError::BadRequest(format!(
                "page_override exceeds {MAX_PAGE_OVERRIDE_BYTES} bytes"
            )));
        }
        repo.set_page_override(plugin.id, Some(page).filter(|p| !p.is_empty())).await?;
    }

    let description = patch.description.unwrap_or_else(|| plugin.description.clone());
    let homepage = patch.homepage.or(plugin.homepage.clone());
    let tags = patch.tags.unwrap_or_else(|| plugin.tags.clone());
    validate_tags(&tags)?;

    let updated = repo.update_plugin_metadata(
        plugin.id,
        &plugin.repo,
        &plugin.display_name,
        &description,
        &tags,
        &plugin.license,
        homepage.as_deref(),
    ).await?;

    Ok(Json(updated))
}


pub async fn set_unlisted(
    State(state): State<AppState>,
    Extension(caller): Extension<AuthenticatedStarfishUser>,
    Path(slug): Path<String>,
    Json(req): Json<UnlistRequest>,
) -> Result<Json<OkResponse>, ApiError> {
    let repo = PluginRegistryRepository::new(state.db.pool());
    let plugin = ensure_owner(&repo, &slug, caller.user.id).await?;
    repo.set_unlisted(plugin.id, req.unlisted).await?;
    Ok(Json(OkResponse { ok: true }))
}


pub async fn yank_release(
    State(state): State<AppState>,
    Extension(caller): Extension<AuthenticatedStarfishUser>,
    Path((slug, version)): Path<(String, String)>,
    Json(req): Json<YankRequest>,
) -> Result<Json<OkResponse>, ApiError> {
    let repo = PluginRegistryRepository::new(state.db.pool());
    let plugin = ensure_owner(&repo, &slug, caller.user.id).await?;
    let reason = req.reason.unwrap_or_else(|| format!("yanked by {}", caller.user.id));
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
    let plugin = ensure_owner(&repo, &slug, caller.user.id).await?;
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
    let plugin = ensure_owner(&repo, &slug, caller.user.id).await?;
    repo.delete_plugin(plugin.id).await?;
    Ok(Json(OkResponse { ok: true }))
}


pub async fn delete_release(
    State(state): State<AppState>,
    Extension(caller): Extension<AuthenticatedStarfishUser>,
    Path((slug, version)): Path<(String, String)>,
) -> Result<Json<OkResponse>, ApiError> {
    let repo = PluginRegistryRepository::new(state.db.pool());
    let plugin = ensure_owner(&repo, &slug, caller.user.id).await?;

    let installs = repo.release_install_count(plugin.id, &version).await?;
    if installs > 0 {
        return Err(ApiError::Conflict(format!(
            "release v{version} has {installs} active install(s) — unlist it instead"
        )));
    }

    let ok = repo.delete_release(plugin.id, &version).await?;
    if !ok {
        return Err(ApiError::NotFound(format!("release {version} not found")));
    }
    Ok(Json(OkResponse { ok: true }))
}


async fn ensure_owner(
    repo: &PluginRegistryRepository<'_>,
    slug: &str,
    user_id: i64,
) -> Result<database::Plugin, ApiError> {
    let plugin = repo.get_plugin_by_slug(slug).await?
        .ok_or_else(|| ApiError::NotFound(format!("plugin {slug} not found")))?;
    if plugin.owner_user_id != user_id {
        return Err(ApiError::Forbidden("not_plugin_owner".into()));
    }
    Ok(plugin)
}


fn validate_tags(tags: &[String]) -> Result<(), ApiError> {
    use super::manifest::{MAX_TAGS_PER_PLUGIN, TAG_ALLOWLIST};
    if tags.len() > MAX_TAGS_PER_PLUGIN {
        return Err(ApiError::BadRequest(format!("at most {MAX_TAGS_PER_PLUGIN} tags allowed")));
    }
    for tag in tags {
        if !TAG_ALLOWLIST.contains(&tag.as_str()) {
            return Err(ApiError::BadRequest(format!("tag '{tag}' is not in the allowlist")));
        }
    }
    Ok(())
}
