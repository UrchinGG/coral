use axum::extract::rejection::{JsonRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::{Extension, Json};
use chrono::{DateTime, Utc};
use database::{Plugin, PluginRegistryRepository, PluginRelease};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{FromRow, Postgres, QueryBuilder};

use super::page::{Accounts, ContainsText, Mentions, RowCursor, RowPage, page_size, split_page};
use crate::auth::AdminActor;
use crate::error::{AdminError, Context};
use crate::refs::{EntityRef, Labels};
use crate::serde_ts;
use crate::state::AppState;

const INSTALL_WINDOW: &str = "30 days";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginsQuery {
    q: Option<String>,
    cursor: Option<RowCursor>,
    limit: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct PluginRow {
    plugin: EntityRef,
    description: String,
    owner: Option<EntityRef>,
    owner_github: Option<String>,
    official: bool,
    unlisted: bool,
    disabled: bool,
    latest_version: Option<String>,
    installs_30d: i64,
    installs_total: i64,
    rating_mean: Option<f32>,
    rating_count: i64,
    #[serde(serialize_with = "serde_ts::micros")]
    created_at: DateTime<Utc>,
    #[serde(serialize_with = "serde_ts::micros")]
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct PluginDetail {
    plugin: PluginView,
    owner: Option<EntityRef>,
    installs_30d: i64,
    installs_total: i64,
    releases: Vec<ReleaseView>,
    reviews: Vec<ReviewView>,
    labels: Labels,
}

#[derive(Debug, Serialize)]
struct PluginView {
    slug: String,
    display_name: String,
    description: String,
    repo: String,
    homepage: Option<String>,
    official: bool,
    unlisted: bool,
    #[serde(serialize_with = "serde_ts::micros_opt")]
    unlisted_at: Option<DateTime<Utc>>,
    disabled: bool,
    disabled_reason: Option<String>,
    #[serde(serialize_with = "serde_ts::micros_opt")]
    disabled_at: Option<DateTime<Utc>>,
    #[serde(serialize_with = "serde_ts::micros")]
    created_at: DateTime<Utc>,
    #[serde(serialize_with = "serde_ts::micros")]
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
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
    #[serde(serialize_with = "serde_ts::micros_opt")]
    yanked_at: Option<DateTime<Utc>>,
    yanked_reason: Option<String>,
    #[serde(serialize_with = "serde_ts::micros")]
    created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
struct ReviewView {
    user_id: i64,
    reviewer: Option<EntityRef>,
    stars: i16,
    review: Option<String>,
    #[serde(serialize_with = "serde_ts::micros")]
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OfficialBody {
    official: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnlistedBody {
    unlisted: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DisabledBody {
    disabled: bool,
    #[serde(default)]
    reason: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct YankBody {
    #[serde(default)]
    reason: Option<String>,
}

#[derive(FromRow)]
struct PluginRecord {
    id: i64,
    slug: String,
    display_name: String,
    description: String,
    official: bool,
    unlisted: bool,
    disabled: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    owner_discord_id: Option<i64>,
    owner_github: Option<String>,
    latest_version: Option<String>,
    installs_30d: i64,
    installs_total: i64,
    rating_mean: Option<f32>,
    rating_count: i64,
}

#[derive(FromRow)]
struct ReviewRecord {
    user_id: i64,
    discord_id: Option<i64>,
    stars: i16,
    review: Option<String>,
    updated_at: DateTime<Utc>,
}

pub async fn list(
    State(state): State<AppState>,
    query: Result<Query<PluginsQuery>, QueryRejection>,
) -> Result<Json<RowPage<PluginRow>>, AdminError> {
    let Query(query) = query?;
    let limit = page_size(query.limit)?;
    let records = plugin_records(&state, &query, limit).await?;
    let (records, next_cursor) = split_page(records, limit, |record| {
        RowCursor::at(record.created_at, record.id)
    });
    let mut mentions = Mentions::default();
    records
        .iter()
        .filter_map(|record| record.owner_discord_id)
        .for_each(|id| mentions.account(id));
    let accounts = Accounts::load(&state, &mentions.discord_ids()).await?;
    let labels = mentions.labels(&state).await?;
    Ok(Json(RowPage {
        items: records
            .into_iter()
            .map(|record| plugin_row(record, &accounts))
            .collect(),
        next_cursor,
        labels,
    }))
}

pub async fn detail(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<PluginDetail>, AdminError> {
    let repo = PluginRegistryRepository::new(state.db.pool());
    let plugin = plugin_by_slug(&repo, &slug).await?;
    let (owner, releases, installs, reviews) = tokio::try_join!(
        owner_of(&state, &plugin),
        async {
            repo.list_releases(plugin.id)
                .await
                .context("read the plugin releases")
        },
        async {
            repo.plugin_install_counts(plugin.id)
                .await
                .context("count the plugin installs")
        },
        reviews_of(&state, plugin.id),
    )?;
    let mut mentions = Mentions::default();
    owner.into_iter().for_each(|id| mentions.account(id));
    reviews
        .iter()
        .filter_map(|review| review.discord_id)
        .for_each(|id| mentions.account(id));
    let accounts = Accounts::load(&state, &mentions.discord_ids()).await?;
    let labels = mentions.labels(&state).await?;
    let (installs_30d, installs_total) = installs;
    Ok(Json(PluginDetail {
        owner: owner.map(|id| accounts.of(id)),
        plugin: plugin_view(plugin),
        installs_30d,
        installs_total,
        releases: releases.into_iter().map(release_view).collect(),
        reviews: reviews
            .into_iter()
            .map(|review| ReviewView {
                user_id: review.user_id,
                reviewer: review.discord_id.map(|id| accounts.of(id)),
                stars: review.stars,
                review: review.review,
                updated_at: review.updated_at,
            })
            .collect(),
        labels,
    }))
}

pub async fn set_official(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path(slug): Path<String>,
    body: Result<Json<OfficialBody>, JsonRejection>,
) -> Result<StatusCode, AdminError> {
    let Json(body) = body?;
    let repo = PluginRegistryRepository::new(state.db.pool());
    let plugin = plugin_by_slug(&repo, &slug).await?;
    repo.set_official(plugin.id, body.official)
        .await
        .context("store the official flag")?;
    audit(
        &state,
        &actor,
        "set_plugin_official",
        &slug,
        json!({ "official": body.official }),
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn set_unlisted(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path(slug): Path<String>,
    body: Result<Json<UnlistedBody>, JsonRejection>,
) -> Result<StatusCode, AdminError> {
    let Json(body) = body?;
    let repo = PluginRegistryRepository::new(state.db.pool());
    let plugin = plugin_by_slug(&repo, &slug).await?;
    repo.set_unlisted(plugin.id, body.unlisted)
        .await
        .context("store the unlisted flag")?;
    audit(
        &state,
        &actor,
        "set_plugin_unlisted",
        &slug,
        json!({ "unlisted": body.unlisted }),
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn set_disabled(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path(slug): Path<String>,
    body: Result<Json<DisabledBody>, JsonRejection>,
) -> Result<StatusCode, AdminError> {
    let Json(body) = body?;
    let reason = trimmed(body.reason.as_deref());
    if body.disabled && reason.is_none() {
        return Err(AdminError::BadRequest(
            "disabling a plugin needs a reason".into(),
        ));
    }
    let repo = PluginRegistryRepository::new(state.db.pool());
    let plugin = plugin_by_slug(&repo, &slug).await?;
    repo.set_disabled(plugin.id, body.disabled, reason)
        .await
        .context("store the disabled flag")?;
    let details = json!({ "disabled": body.disabled, "reason": reason });
    audit(&state, &actor, "set_plugin_disabled", &slug, details).await;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete_plugin(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path(slug): Path<String>,
) -> Result<StatusCode, AdminError> {
    let repo = PluginRegistryRepository::new(state.db.pool());
    let plugin = plugin_by_slug(&repo, &slug).await?;
    repo.delete_plugin(plugin.id)
        .await
        .context("delete the plugin")?;
    audit(&state, &actor, "delete_plugin", &slug, json!({})).await;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn yank_release(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path((slug, version)): Path<(String, String)>,
    body: Result<Json<YankBody>, JsonRejection>,
) -> Result<StatusCode, AdminError> {
    let Json(body) = body?;
    let reason = trimmed(body.reason.as_deref());
    let repo = PluginRegistryRepository::new(state.db.pool());
    let plugin = plugin_by_slug(&repo, &slug).await?;
    let yanked = repo
        .yank_release(plugin.id, &version, reason)
        .await
        .context("yank the release")?;
    require_release(yanked, &slug, &version)?;
    let details = json!({ "version": version, "reason": reason });
    audit(&state, &actor, "yank_release", &slug, details).await;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn unyank_release(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path((slug, version)): Path<(String, String)>,
) -> Result<StatusCode, AdminError> {
    let repo = PluginRegistryRepository::new(state.db.pool());
    let plugin = plugin_by_slug(&repo, &slug).await?;
    let restored = repo
        .unyank_release(plugin.id, &version)
        .await
        .context("restore the release")?;
    require_release(restored, &slug, &version)?;
    audit(
        &state,
        &actor,
        "unyank_release",
        &slug,
        json!({ "version": version }),
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete_release(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path((slug, version)): Path<(String, String)>,
) -> Result<StatusCode, AdminError> {
    let repo = PluginRegistryRepository::new(state.db.pool());
    let plugin = plugin_by_slug(&repo, &slug).await?;
    let deleted = repo
        .delete_release(plugin.id, &version)
        .await
        .context("delete the release")?;
    require_release(deleted, &slug, &version)?;
    audit(
        &state,
        &actor,
        "delete_release",
        &slug,
        json!({ "version": version }),
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete_review(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path((slug, user_id)): Path<(String, i64)>,
) -> Result<StatusCode, AdminError> {
    let repo = PluginRegistryRepository::new(state.db.pool());
    let plugin = plugin_by_slug(&repo, &slug).await?;
    let deleted = repo
        .delete_rating(user_id, plugin.id)
        .await
        .context("delete the review")?;
    if !deleted {
        return Err(AdminError::NotFound(format!(
            "Starfish user {user_id} has not reviewed {slug}"
        )));
    }
    audit(
        &state,
        &actor,
        "delete_plugin_review",
        &slug,
        json!({ "user_id": user_id }),
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

async fn plugin_records(
    state: &AppState,
    query: &PluginsQuery,
    limit: i64,
) -> Result<Vec<PluginRecord>, AdminError> {
    let mut sql = QueryBuilder::<Postgres>::new(format!(
        "SELECT p.id, p.slug, p.display_name, p.description, p.official, p.unlisted, p.disabled,
                p.created_at, p.updated_at,
                u.discord_id AS owner_discord_id, u.github_username AS owner_github,
                (SELECT r.version FROM plugin_releases r
                 WHERE r.plugin_id = p.id AND NOT r.yanked
                 ORDER BY r.created_at DESC LIMIT 1) AS latest_version,
                (SELECT count(*) FROM plugin_installs i
                 WHERE i.plugin_id = p.id AND i.installed_at > now() - interval '{INSTALL_WINDOW}') AS installs_30d,
                (SELECT count(*) FROM plugin_installs i WHERE i.plugin_id = p.id) AS installs_total,
                (SELECT avg(stars)::real FROM plugin_ratings pr WHERE pr.plugin_id = p.id) AS rating_mean,
                (SELECT count(*) FROM plugin_ratings pr WHERE pr.plugin_id = p.id) AS rating_count
         FROM plugins p
         LEFT JOIN starfish_users u ON u.id = p.owner_user_id
         WHERE TRUE"
    ));
    if let Some(text) = query.q.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
        let contains = ContainsText::new(text);
        sql.push(" AND (");
        contains.push_match(&mut sql, "p.slug");
        sql.push(" OR ");
        contains.push_match(&mut sql, "p.display_name");
        sql.push(" OR ");
        contains.push_match(&mut sql, "p.description");
        sql.push(")");
    }
    if let Some(cursor) = &query.cursor {
        sql.push(" AND (p.created_at, p.id) < (")
            .push_bind(cursor.ts()?)
            .push(", ")
            .push_bind(cursor.id()?)
            .push(")");
    }
    sql.push(" ORDER BY p.created_at DESC, p.id DESC LIMIT ")
        .push_bind(limit + 1);
    sql.build_query_as()
        .fetch_all(state.db.pool())
        .await
        .context("read plugins")
}

async fn plugin_by_slug(
    repo: &PluginRegistryRepository<'_>,
    slug: &str,
) -> Result<Plugin, AdminError> {
    repo.get_plugin_by_slug(slug)
        .await
        .context("read the plugin")?
        .ok_or_else(|| AdminError::NotFound(format!("no plugin is published as {slug}")))
}

async fn owner_of(state: &AppState, plugin: &Plugin) -> Result<Option<i64>, AdminError> {
    sqlx::query_scalar("SELECT discord_id FROM starfish_users WHERE id = $1")
        .bind(plugin.owner_user_id)
        .fetch_optional(state.db.pool())
        .await
        .context("read the plugin owner")
}

async fn reviews_of(state: &AppState, plugin_id: i64) -> Result<Vec<ReviewRecord>, AdminError> {
    sqlx::query_as(
        "SELECT r.user_id, u.discord_id, r.stars, r.review, r.updated_at
         FROM plugin_ratings r
         LEFT JOIN starfish_users u ON u.id = r.user_id
         WHERE r.plugin_id = $1
         ORDER BY r.updated_at DESC, r.user_id",
    )
    .bind(plugin_id)
    .fetch_all(state.db.pool())
    .await
    .context("read the plugin reviews")
}

fn plugin_row(record: PluginRecord, accounts: &Accounts) -> PluginRow {
    PluginRow {
        plugin: EntityRef::Plugin {
            slug: record.slug,
            name: Some(record.display_name),
        },
        description: record.description,
        owner: record.owner_discord_id.map(|id| accounts.of(id)),
        owner_github: record.owner_github,
        official: record.official,
        unlisted: record.unlisted,
        disabled: record.disabled,
        latest_version: record.latest_version,
        installs_30d: record.installs_30d,
        installs_total: record.installs_total,
        rating_mean: record.rating_mean,
        rating_count: record.rating_count,
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
}

fn plugin_view(plugin: Plugin) -> PluginView {
    PluginView {
        slug: plugin.slug,
        display_name: plugin.display_name,
        description: plugin.description,
        repo: plugin.repo,
        homepage: plugin.homepage,
        official: plugin.official,
        unlisted: plugin.unlisted,
        unlisted_at: plugin.unlisted_at,
        disabled: plugin.disabled,
        disabled_reason: plugin.disabled_reason,
        disabled_at: plugin.disabled_at,
        created_at: plugin.created_at,
        updated_at: plugin.updated_at,
    }
}

fn release_view(release: PluginRelease) -> ReleaseView {
    ReleaseView {
        id: release.id,
        version: release.version,
        git_sha: release.git_sha,
        asset_url: release.asset_url,
        asset_sha256: hex::encode(&release.asset_sha256),
        content_sha256: release.content_sha256.map(hex::encode),
        asset_size: release.asset_size,
        changelog: release.changelog,
        yanked: release.yanked,
        yanked_at: release.yanked_at,
        yanked_reason: release.yanked_reason,
        created_at: release.created_at,
    }
}

fn require_release(changed: bool, slug: &str, version: &str) -> Result<(), AdminError> {
    if !changed {
        return Err(AdminError::NotFound(format!(
            "{slug} has no release {version}"
        )));
    }
    Ok(())
}

fn trimmed(reason: Option<&str>) -> Option<&str> {
    reason.map(str::trim).filter(|reason| !reason.is_empty())
}

async fn audit(state: &AppState, actor: &AdminActor, action: &str, slug: &str, details: Value) {
    crate::audit::log(state, actor.discord_id, action, slug, details).await;
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
            manifest_json: json!({}),
            changelog: None,
            yanked: false,
            yanked_at: None,
            yanked_reason: None,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn releases_hex_encode_their_hashes() {
        let view = release_view(sample_release(Some(vec![0xca, 0xfe])));
        assert_eq!(view.asset_sha256, "deadbeef");
        assert_eq!(view.content_sha256.as_deref(), Some("cafe"));
        assert_eq!(release_view(sample_release(None)).content_sha256, None);
    }

    #[test]
    fn blank_reasons_count_as_none() {
        assert_eq!(trimmed(Some("  ")), None);
        assert_eq!(trimmed(Some(" spam ")), Some("spam"));
        assert_eq!(trimmed(None), None);
    }
}
