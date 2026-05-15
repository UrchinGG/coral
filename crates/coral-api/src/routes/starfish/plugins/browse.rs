use axum::{Extension, Json, body::Body, extract::{Path, Query, State}, http::{StatusCode, header}, response::Response};
use chrono::Utc;

use database::{PluginRegistryRepository, PluginSortMode};

use crate::{error::ApiError, state::AppState};

use super::super::session_auth::AuthenticatedStarfishUser;
use super::dto::{
    BodyQuery, DisabledEntryDto, DisabledQuery, DisabledResponse,
    PluginDetailDto, PluginListQuery, PluginListResponse, PluginSummaryDto, ReleaseInfoDto,
};


const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 200;


pub async fn list_plugins(
    State(state): State<AppState>,
    Query(q): Query<PluginListQuery>,
) -> Result<Json<PluginListResponse>, ApiError> {
    let sort = parse_sort(q.sort.as_deref());
    let limit = q.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let offset = q.offset.unwrap_or(0).max(0);

    let repo = PluginRegistryRepository::new(state.db.pool());
    let (total, summaries) = repo.list_plugins(
        sort, q.tag.as_deref(), q.q.as_deref(), limit, offset,
    ).await?;

    let plugins = summaries.into_iter().map(|s| PluginSummaryDto {
        slug: s.slug,
        display_name: s.display_name,
        description: s.description,
        author: s.author,
        official: s.official,
        tags: s.tags,
        latest_version: s.latest_version,
        updated_at: s.updated_at,
        installs_30d: s.installs_30d,
        installs_total: s.installs_total,
        rating_mean: s.rating_mean,
        rating_count: s.rating_count,
        rating_bayesian: s.rating_bayesian,
    }).collect();

    Ok(Json(PluginListResponse { total, plugins }))
}


pub async fn get_plugin(
    State(state): State<AppState>,
    Extension(caller): Extension<AuthenticatedStarfishUser>,
    Path(slug): Path<String>,
) -> Result<Json<PluginDetailDto>, ApiError> {
    let repo = PluginRegistryRepository::new(state.db.pool());

    let plugin = repo.get_plugin_by_slug(&slug).await?
        .ok_or_else(|| ApiError::NotFound(format!("plugin {slug} not found")))?;

    let releases = repo.list_releases(plugin.id).await?;
    let latest = releases.iter().find(|r| !r.yanked)
        .ok_or_else(|| ApiError::NotFound("plugin has no available release".into()))?
        .clone();

    let (rating_mean, rating_count, rating_bayesian) = repo.plugin_rating_stats(plugin.id).await?;
    let (installs_30d, installs_total) = repo.plugin_install_counts(plugin.id).await?;

    let user_rating = repo.get_user_rating(caller.user.id, plugin.id).await?.map(|r| r.stars);
    let install = repo.get_install(caller.user.id, plugin.id).await?;
    let installed_version = match &install {
        Some(i) => repo.list_releases(plugin.id).await?.iter()
            .find(|r| r.id == i.release_id).map(|r| r.version.clone()),
        None => None,
    };

    let readme = match &plugin.page_override {
        Some(page) if !page.trim().is_empty() => Some(page.clone()),
        _ => repo.get_release_readme(latest.id).await?,
    };

    let author = fetch_author(&state, plugin.owner_user_id).await;
    let repo_url = format!("https://github.com/{}", plugin.repo);

    let latest_release = release_to_dto(&latest);
    let releases = releases.iter().map(release_to_dto).collect();

    Ok(Json(PluginDetailDto {
        slug: plugin.slug,
        display_name: plugin.display_name,
        description: plugin.description,
        author,
        official: plugin.official,
        unlisted: plugin.unlisted,
        tags: plugin.tags,
        license: plugin.license,
        homepage: plugin.homepage,
        repo_url,
        latest_release,
        releases,
        readme,
        installs_30d,
        installs_total,
        rating_mean,
        rating_count,
        rating_bayesian,
        user_rating,
        is_installed: install.is_some(),
        installed_version,
    }))
}


pub async fn download_body(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Query(q): Query<BodyQuery>,
) -> Result<Response, ApiError> {
    let repo = PluginRegistryRepository::new(state.db.pool());

    let plugin = repo.get_plugin_by_slug(&slug).await?
        .ok_or_else(|| ApiError::NotFound(format!("plugin {slug} not found")))?;

    let release = match q.version {
        Some(ref v) => repo.get_release_by_version(plugin.id, v).await?
            .ok_or_else(|| ApiError::NotFound(format!("{slug}@{v} not found")))?,
        None => repo.get_latest_release(plugin.id).await?
            .ok_or_else(|| ApiError::NotFound(format!("{slug} has no available release")))?,
    };

    let body_row = repo.get_release_body(release.id).await?
        .ok_or_else(|| ApiError::Internal("release row missing".into()))?;

    let filename = format!("{slug}-{}.zip", release.version);

    match body_row.body_cache {
        Some(bytes) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/zip")
            .header(header::CONTENT_DISPOSITION, format!("attachment; filename=\"{filename}\""))
            .header(header::CONTENT_LENGTH, bytes.len())
            .header("X-Content-SHA256", hex::encode(&body_row.asset_sha256))
            .body(Body::from(bytes))
            .map_err(|e| ApiError::Internal(format!("response build: {e}"))),

        None => Response::builder()
            .status(StatusCode::FOUND)
            .header(header::LOCATION, body_row.asset_url)
            .header("X-Content-SHA256", hex::encode(&body_row.asset_sha256))
            .body(Body::empty())
            .map_err(|e| ApiError::Internal(format!("response build: {e}"))),
    }
}


pub async fn list_disabled(
    State(state): State<AppState>,
    Query(q): Query<DisabledQuery>,
) -> Result<Json<DisabledResponse>, ApiError> {
    let repo = PluginRegistryRepository::new(state.db.pool());
    let since = q.since.unwrap_or_else(|| chrono::DateTime::<Utc>::UNIX_EPOCH);
    let entries = repo.list_disabled_since(since).await?;

    Ok(Json(DisabledResponse {
        as_of: Utc::now(),
        disabled: entries.into_iter().map(|e| DisabledEntryDto {
            slug: e.slug,
            reason: e.reason.unwrap_or_else(|| "no reason provided".into()),
            disabled_at: e.disabled_at,
        }).collect(),
    }))
}


pub(super) fn release_to_dto(r: &database::PluginRelease) -> ReleaseInfoDto {
    ReleaseInfoDto {
        version: r.version.clone(),
        git_sha: r.git_sha.clone(),
        changelog: r.changelog.clone(),
        asset_sha256: hex::encode(&r.asset_sha256),
        asset_size: r.asset_size,
        yanked: r.yanked,
        yanked_reason: r.yanked_reason.clone(),
        created_at: r.created_at,
    }
}


fn parse_sort(s: Option<&str>) -> PluginSortMode {
    match s {
        Some("popular") => PluginSortMode::Popular,
        Some("rating")  => PluginSortMode::Rating,
        Some("recent")  => PluginSortMode::Recent,
        Some("new")     => PluginSortMode::New,
        _ => PluginSortMode::default(),
    }
}


async fn fetch_author(state: &AppState, owner_user_id: i64) -> String {
    database::StarfishRepository::new(state.db.pool())
        .get_user_by_id(owner_user_id).await
        .ok().flatten()
        .and_then(|u| u.github_username)
        .unwrap_or_else(|| "unknown".into())
}
