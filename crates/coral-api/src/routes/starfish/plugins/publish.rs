use axum::{Extension, Json, extract::State};
use chrono::Utc;

use database::{NewPlugin, NewRelease, PluginRegistryRepository};

use crate::{error::ApiError, state::AppState};

use super::super::session_auth::AuthenticatedStarfishUser;
use super::dto::{PublishRequest, PublishResponse};
use super::github_api;
use super::manifest::{self, ExtractedPlugin};


const ASSET_NAME: &str = "plugin.zip";


pub async fn publish_plugin(
    State(state): State<AppState>,
    Extension(caller): Extension<AuthenticatedStarfishUser>,
    Json(req): Json<PublishRequest>,
) -> Result<Json<PublishResponse>, ApiError> {
    super::super::rate_limit(&state, &format!("sf:publish:{}", caller.user.id), 10).await?;

    let github_user_id = caller.user.github_user_id
        .ok_or_else(|| ApiError::BadRequest("link_github_first".into()))?;
    let github_username = caller.user.github_username.clone()
        .ok_or_else(|| ApiError::BadRequest("link_github_first".into()))?;

    let (token_user_id, _) = github_api::get_user(&req.github_access_token).await?;
    if token_user_id != github_user_id {
        return Err(ApiError::Forbidden("github_token_user_mismatch".into()));
    }

    let expected_tag = format!("v{}", req.version);
    if req.release_tag != expected_tag {
        return Err(ApiError::BadRequest(format!(
            "release_tag must be {expected_tag}"
        )));
    }

    let repo = github_api::get_repo(&req.github_access_token, &req.repo).await?;
    let permission = github_api::get_permission(&req.github_access_token, &req.repo, &github_username).await?;
    if !matches!(permission.as_str(), "admin" | "maintain" | "write") {
        return Err(ApiError::Forbidden("insufficient_repo_permission".into()));
    }

    let release = github_api::get_release_by_tag(&req.github_access_token, &req.repo, &req.release_tag).await?;
    let asset = release.assets.iter().find(|a| a.name == ASSET_NAME)
        .ok_or_else(|| ApiError::BadRequest(format!("release has no '{ASSET_NAME}' asset")))?;

    let zip_bytes = github_api::download_asset(&req.github_access_token, asset).await?;
    let asset_sha = manifest::sha256_bytes(&zip_bytes);
    verify_github_digest(asset, &asset_sha)?;

    let ExtractedPlugin { manifest, manifest_json, readme } =
        manifest::extract_and_validate(&zip_bytes, &req.version)?;

    let plugins_repo = PluginRegistryRepository::new(state.db.pool());

    let existing_by_slug = plugins_repo.get_plugin_by_slug(&manifest.name).await?;
    let existing_by_repo_id = plugins_repo.get_plugin_by_github_repo_id(repo.id).await?;

    let plugin = match (existing_by_slug, existing_by_repo_id) {
        (Some(by_slug), _) if by_slug.owner_user_id != caller.user.id =>
            return Err(ApiError::Forbidden("plugin_slug_owned_by_another_user".into())),

        (Some(by_slug), Some(by_repo)) if by_slug.id != by_repo.id =>
            return Err(ApiError::Conflict("slug_and_repo_id_belong_to_different_plugins".into())),

        (Some(by_slug), _) => {
            plugins_repo.update_plugin_metadata(
                by_slug.id,
                &repo.full_name,
                &manifest.display_name,
                &manifest.description,
                &manifest.tags,
                &manifest.license,
                manifest.homepage.as_deref(),
            ).await?
        }

        (None, Some(by_repo)) => {
            return Err(ApiError::Conflict(format!(
                "this GitHub repo is already published as '{}'", by_repo.slug
            )));
        }

        (None, None) => {
            plugins_repo.create_plugin(NewPlugin {
                slug: &manifest.name,
                owner_user_id: caller.user.id,
                repo: &repo.full_name,
                github_repo_id: repo.id,
                display_name: &manifest.display_name,
                description: &manifest.description,
                tags: &manifest.tags,
                license: &manifest.license,
                homepage: manifest.homepage.as_deref(),
            }).await?
        }
    };

    if plugins_repo.get_release_by_version(plugin.id, &req.version).await?.is_some() {
        return Err(ApiError::Conflict(format!(
            "version {} already published", req.version
        )));
    }

    let release_row = plugins_repo.create_release(NewRelease {
        plugin_id: plugin.id,
        version: &req.version,
        git_sha: &release.target_commitish,
        asset_url: &asset.browser_download_url,
        asset_sha256: &asset_sha,
        asset_size: asset.size,
        body_cache: &zip_bytes,
        readme_cache: readme.as_deref(),
        manifest_json: &manifest_json,
        changelog: release.body.as_deref(),
    }).await?;

    plugins_repo.clear_old_body_caches(plugin.id, release_row.id).await?;

    Ok(Json(PublishResponse {
        slug: plugin.slug,
        version: release_row.version,
        asset_sha256: hex::encode(asset_sha),
        published_at: Utc::now(),
    }))
}


fn verify_github_digest(asset: &github_api::Asset, computed: &[u8; 32]) -> Result<(), ApiError> {
    let Some(digest) = asset.digest.as_deref() else { return Ok(()); };

    let claimed_hex = digest.strip_prefix("sha256:")
        .ok_or_else(|| ApiError::BadRequest(format!("unsupported digest format: {digest}")))?;
    let claimed = hex::decode(claimed_hex)
        .map_err(|_| ApiError::BadRequest("invalid digest hex".into()))?;

    if claimed.as_slice() != computed {
        return Err(ApiError::BadRequest("asset_sha256_mismatch".into()));
    }
    Ok(())
}
