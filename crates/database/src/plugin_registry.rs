use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};


#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Plugin {
    pub id: i64,
    pub slug: String,
    pub owner_user_id: i64,
    pub repo: String,
    pub github_repo_id: i64,
    pub display_name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub license: String,
    pub homepage: Option<String>,
    pub page_override: Option<String>,
    pub unlisted: bool,
    pub unlisted_at: Option<DateTime<Utc>>,
    pub official: bool,
    pub disabled: bool,
    pub disabled_reason: Option<String>,
    pub disabled_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}


#[derive(Debug, Clone, FromRow, Serialize)]
pub struct PluginRelease {
    pub id: i64,
    pub plugin_id: i64,
    pub version: String,
    pub git_sha: String,
    pub asset_url: String,
    pub asset_sha256: Vec<u8>,
    pub asset_size: i32,
    pub manifest_json: serde_json::Value,
    pub changelog: Option<String>,
    pub yanked: bool,
    pub yanked_at: Option<DateTime<Utc>>,
    pub yanked_reason: Option<String>,
    pub created_at: DateTime<Utc>,
}


#[derive(Debug, Clone, FromRow)]
pub struct ReleaseBody {
    pub body_cache: Option<Vec<u8>>,
    pub asset_url: String,
    pub asset_sha256: Vec<u8>,
}


#[derive(Debug, Clone, FromRow)]
pub struct PluginInstall {
    pub user_id: i64,
    pub plugin_id: i64,
    pub release_id: i64,
    pub installed_at: DateTime<Utc>,
    pub last_updated_at: DateTime<Utc>,
}


#[derive(Debug, Clone, FromRow, Serialize)]
pub struct PluginRating {
    pub user_id: i64,
    pub plugin_id: i64,
    pub stars: i16,
    pub review: Option<String>,
    pub updated_at: DateTime<Utc>,
}


#[derive(Debug, Clone, FromRow)]
pub struct PluginSortConfig {
    pub id: i32,
    pub velocity_weight: f32,
    pub rating_weight: f32,
    pub recency_weight: f32,
    pub rating_prior_confidence: f32,
    pub rating_prior_mean: f32,
    pub updated_at: DateTime<Utc>,
}


#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PluginSortMode {
    Popular,
    Rating,
    Recent,
    New,
}


impl Default for PluginSortMode {
    fn default() -> Self { Self::Popular }
}


#[derive(Debug, Clone, FromRow, Serialize)]
pub struct PluginSummary {
    pub slug: String,
    pub display_name: String,
    pub description: String,
    pub author: String,
    pub official: bool,
    pub tags: Vec<String>,
    pub latest_version: String,
    pub updated_at: DateTime<Utc>,
    pub installs_30d: i64,
    pub installs_total: i64,
    pub rating_mean: Option<f32>,
    pub rating_count: i64,
    pub rating_bayesian: f32,
}


#[derive(Debug, Clone, FromRow, Serialize)]
pub struct DisabledEntry {
    pub slug: String,
    pub reason: Option<String>,
    pub disabled_at: DateTime<Utc>,
}


#[derive(Debug, Clone, FromRow)]
pub struct InstalledWithLatest {
    pub slug: String,
    pub installed_version: String,
    pub latest_version: String,
    pub latest_git_sha: String,
    pub disabled: bool,
    pub latest_release_id: i64,
    pub latest_changelog: Option<String>,
    pub latest_asset_sha256: Vec<u8>,
    pub latest_asset_size: i32,
    pub latest_created_at: DateTime<Utc>,
}


pub struct PluginRegistryRepository<'a> {
    pool: &'a PgPool,
}


pub struct NewPlugin<'a> {
    pub slug: &'a str,
    pub owner_user_id: i64,
    pub repo: &'a str,
    pub github_repo_id: i64,
    pub display_name: &'a str,
    pub description: &'a str,
    pub tags: &'a [String],
    pub license: &'a str,
    pub homepage: Option<&'a str>,
}


pub struct NewRelease<'a> {
    pub plugin_id: i64,
    pub version: &'a str,
    pub git_sha: &'a str,
    pub asset_url: &'a str,
    pub asset_sha256: &'a [u8],
    pub asset_size: i32,
    pub body_cache: &'a [u8],
    pub readme_cache: Option<&'a str>,
    pub manifest_json: &'a serde_json::Value,
    pub changelog: Option<&'a str>,
}


impl<'a> PluginRegistryRepository<'a> {
    pub fn new(pool: &'a PgPool) -> Self { Self { pool } }


    pub async fn get_plugin_by_slug(&self, slug: &str) -> Result<Option<Plugin>, sqlx::Error> {
        sqlx::query_as("SELECT * FROM plugins WHERE slug = $1")
            .bind(slug)
            .fetch_optional(self.pool)
            .await
    }

    pub async fn get_plugin_by_github_repo_id(&self, github_repo_id: i64) -> Result<Option<Plugin>, sqlx::Error> {
        sqlx::query_as("SELECT * FROM plugins WHERE github_repo_id = $1")
            .bind(github_repo_id)
            .fetch_optional(self.pool)
            .await
    }

    pub async fn create_plugin(&self, p: NewPlugin<'_>) -> Result<Plugin, sqlx::Error> {
        sqlx::query_as(
            "INSERT INTO plugins (slug, owner_user_id, repo, github_repo_id, display_name, description, tags, license, homepage)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
             RETURNING *",
        )
        .bind(p.slug)
        .bind(p.owner_user_id)
        .bind(p.repo)
        .bind(p.github_repo_id)
        .bind(p.display_name)
        .bind(p.description)
        .bind(p.tags)
        .bind(p.license)
        .bind(p.homepage)
        .fetch_one(self.pool)
        .await
    }

    pub async fn update_plugin_metadata(
        &self,
        plugin_id: i64,
        repo: &str,
        display_name: &str,
        description: &str,
        tags: &[String],
        license: &str,
        homepage: Option<&str>,
    ) -> Result<Plugin, sqlx::Error> {
        sqlx::query_as(
            "UPDATE plugins SET
                repo = $2, display_name = $3, description = $4, tags = $5,
                license = $6, homepage = $7, updated_at = NOW()
             WHERE id = $1 RETURNING *",
        )
        .bind(plugin_id)
        .bind(repo)
        .bind(display_name)
        .bind(description)
        .bind(tags)
        .bind(license)
        .bind(homepage)
        .fetch_one(self.pool)
        .await
    }

    pub async fn set_page_override(&self, plugin_id: i64, page: Option<&str>) -> Result<Plugin, sqlx::Error> {
        sqlx::query_as(
            "UPDATE plugins SET page_override = $2, updated_at = NOW()
             WHERE id = $1 RETURNING *",
        )
        .bind(plugin_id)
        .bind(page)
        .fetch_one(self.pool)
        .await
    }

    pub async fn set_unlisted(&self, plugin_id: i64, unlisted: bool) -> Result<Plugin, sqlx::Error> {
        sqlx::query_as(
            "UPDATE plugins SET
                unlisted = $2,
                unlisted_at = CASE WHEN $2 THEN NOW() ELSE NULL END,
                updated_at = NOW()
             WHERE id = $1 RETURNING *",
        )
        .bind(plugin_id)
        .bind(unlisted)
        .fetch_one(self.pool)
        .await
    }

    pub async fn unyank_release(&self, plugin_id: i64, version: &str) -> Result<bool, sqlx::Error> {
        sqlx::query(
            "UPDATE plugin_releases SET yanked = false, yanked_at = NULL, yanked_reason = NULL
             WHERE plugin_id = $1 AND version = $2",
        )
        .bind(plugin_id)
        .bind(version)
        .execute(self.pool)
        .await
        .map(|r| r.rows_affected() > 0)
    }

    pub async fn delete_plugin(&self, plugin_id: i64) -> Result<bool, sqlx::Error> {
        sqlx::query("DELETE FROM plugins WHERE id = $1")
            .bind(plugin_id)
            .execute(self.pool)
            .await
            .map(|r| r.rows_affected() > 0)
    }

    pub async fn release_install_count(&self, plugin_id: i64, version: &str) -> Result<i64, sqlx::Error> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM plugin_installs i
               JOIN plugin_releases r ON r.id = i.release_id
              WHERE r.plugin_id = $1 AND r.version = $2",
        )
        .bind(plugin_id)
        .bind(version)
        .fetch_one(self.pool)
        .await?;
        Ok(row.0)
    }

    pub async fn delete_release(&self, plugin_id: i64, version: &str) -> Result<bool, sqlx::Error> {
        sqlx::query("DELETE FROM plugin_releases WHERE plugin_id = $1 AND version = $2")
            .bind(plugin_id)
            .bind(version)
            .execute(self.pool)
            .await
            .map(|r| r.rows_affected() > 0)
    }

    pub async fn list_my_plugins(&self, owner_user_id: i64) -> Result<Vec<Plugin>, sqlx::Error> {
        sqlx::query_as(
            "SELECT * FROM plugins WHERE owner_user_id = $1 ORDER BY created_at DESC",
        )
        .bind(owner_user_id)
        .fetch_all(self.pool)
        .await
    }

    pub async fn set_plugin_disabled(&self, slug: &str, reason: &str) -> Result<bool, sqlx::Error> {
        sqlx::query(
            "UPDATE plugins SET disabled = true, disabled_reason = $2, disabled_at = NOW(), updated_at = NOW()
             WHERE slug = $1",
        )
        .bind(slug)
        .bind(reason)
        .execute(self.pool)
        .await
        .map(|r| r.rows_affected() > 0)
    }

    pub async fn list_disabled_since(&self, since: DateTime<Utc>) -> Result<Vec<DisabledEntry>, sqlx::Error> {
        sqlx::query_as(
            "SELECT slug, disabled_reason AS reason, disabled_at
             FROM plugins
             WHERE disabled = true AND disabled_at IS NOT NULL AND disabled_at > $1
             ORDER BY disabled_at",
        )
        .bind(since)
        .fetch_all(self.pool)
        .await
    }


    pub async fn create_release(&self, r: NewRelease<'_>) -> Result<PluginRelease, sqlx::Error> {
        sqlx::query_as(
            "INSERT INTO plugin_releases
                (plugin_id, version, git_sha, asset_url, asset_sha256, asset_size,
                 body_cache, readme_cache, manifest_json, changelog)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
             RETURNING id, plugin_id, version, git_sha, asset_url, asset_sha256, asset_size,
                       manifest_json, changelog, yanked, yanked_at, yanked_reason, created_at",
        )
        .bind(r.plugin_id)
        .bind(r.version)
        .bind(r.git_sha)
        .bind(r.asset_url)
        .bind(r.asset_sha256)
        .bind(r.asset_size)
        .bind(r.body_cache)
        .bind(r.readme_cache)
        .bind(r.manifest_json)
        .bind(r.changelog)
        .fetch_one(self.pool)
        .await
    }

    pub async fn get_release_by_version(&self, plugin_id: i64, version: &str) -> Result<Option<PluginRelease>, sqlx::Error> {
        sqlx::query_as(
            "SELECT id, plugin_id, version, git_sha, asset_url, asset_sha256, asset_size,
                    manifest_json, changelog, yanked, yanked_at, yanked_reason, created_at
             FROM plugin_releases WHERE plugin_id = $1 AND version = $2",
        )
        .bind(plugin_id)
        .bind(version)
        .fetch_optional(self.pool)
        .await
    }

    pub async fn get_latest_release(&self, plugin_id: i64) -> Result<Option<PluginRelease>, sqlx::Error> {
        sqlx::query_as(
            "SELECT id, plugin_id, version, git_sha, asset_url, asset_sha256, asset_size,
                    manifest_json, changelog, yanked, yanked_at, yanked_reason, created_at
             FROM plugin_releases
             WHERE plugin_id = $1 AND NOT yanked
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(plugin_id)
        .fetch_optional(self.pool)
        .await
    }

    pub async fn list_releases(&self, plugin_id: i64) -> Result<Vec<PluginRelease>, sqlx::Error> {
        sqlx::query_as(
            "SELECT id, plugin_id, version, git_sha, asset_url, asset_sha256, asset_size,
                    manifest_json, changelog, yanked, yanked_at, yanked_reason, created_at
             FROM plugin_releases
             WHERE plugin_id = $1
             ORDER BY created_at DESC",
        )
        .bind(plugin_id)
        .fetch_all(self.pool)
        .await
    }

    pub async fn get_release_body(&self, release_id: i64) -> Result<Option<ReleaseBody>, sqlx::Error> {
        sqlx::query_as(
            "SELECT body_cache, asset_url, asset_sha256 FROM plugin_releases WHERE id = $1",
        )
        .bind(release_id)
        .fetch_optional(self.pool)
        .await
    }

    pub async fn clear_old_body_caches(&self, plugin_id: i64, keep_release_id: i64) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE plugin_releases SET body_cache = NULL
             WHERE plugin_id = $1 AND id != $2 AND body_cache IS NOT NULL",
        )
        .bind(plugin_id)
        .bind(keep_release_id)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_release_readme(&self, release_id: i64) -> Result<Option<String>, sqlx::Error> {
        let row: Option<(Option<String>,)> = sqlx::query_as(
            "SELECT readme_cache FROM plugin_releases WHERE id = $1",
        )
        .bind(release_id)
        .fetch_optional(self.pool)
        .await?;
        Ok(row.and_then(|(r,)| r))
    }

    pub async fn yank_release(&self, plugin_id: i64, version: &str, reason: &str) -> Result<bool, sqlx::Error> {
        sqlx::query(
            "UPDATE plugin_releases SET yanked = true, yanked_at = NOW(), yanked_reason = $3
             WHERE plugin_id = $1 AND version = $2",
        )
        .bind(plugin_id)
        .bind(version)
        .bind(reason)
        .execute(self.pool)
        .await
        .map(|r| r.rows_affected() > 0)
    }


    pub async fn upsert_install(&self, user_id: i64, plugin_id: i64, release_id: i64) -> Result<PluginInstall, sqlx::Error> {
        sqlx::query_as(
            "INSERT INTO plugin_installs (user_id, plugin_id, release_id)
             VALUES ($1, $2, $3)
             ON CONFLICT (user_id, plugin_id) DO UPDATE SET
                release_id = EXCLUDED.release_id,
                last_updated_at = NOW()
             RETURNING *",
        )
        .bind(user_id)
        .bind(plugin_id)
        .bind(release_id)
        .fetch_one(self.pool)
        .await
    }

    pub async fn get_install(&self, user_id: i64, plugin_id: i64) -> Result<Option<PluginInstall>, sqlx::Error> {
        sqlx::query_as("SELECT * FROM plugin_installs WHERE user_id = $1 AND plugin_id = $2")
            .bind(user_id)
            .bind(plugin_id)
            .fetch_optional(self.pool)
            .await
    }

    pub async fn delete_install(&self, user_id: i64, plugin_id: i64) -> Result<bool, sqlx::Error> {
        sqlx::query("DELETE FROM plugin_installs WHERE user_id = $1 AND plugin_id = $2")
            .bind(user_id)
            .bind(plugin_id)
            .execute(self.pool)
            .await
            .map(|r| r.rows_affected() > 0)
    }

    pub async fn list_user_installs(&self, user_id: i64) -> Result<Vec<InstalledWithLatest>, sqlx::Error> {
        sqlx::query_as(
            "SELECT
                p.slug,
                ir.version AS installed_version,
                COALESCE(lr.version, ir.version) AS latest_version,
                COALESCE(lr.git_sha, ir.git_sha) AS latest_git_sha,
                p.disabled,
                COALESCE(lr.id, ir.id) AS latest_release_id,
                lr.changelog AS latest_changelog,
                COALESCE(lr.asset_sha256, ir.asset_sha256) AS latest_asset_sha256,
                COALESCE(lr.asset_size, ir.asset_size) AS latest_asset_size,
                COALESCE(lr.created_at, ir.created_at) AS latest_created_at
             FROM plugin_installs pi
             JOIN plugins p ON p.id = pi.plugin_id
             JOIN plugin_releases ir ON ir.id = pi.release_id
             LEFT JOIN LATERAL (
                SELECT * FROM plugin_releases
                WHERE plugin_id = pi.plugin_id AND NOT yanked
                ORDER BY created_at DESC LIMIT 1
             ) lr ON true
             WHERE pi.user_id = $1
             ORDER BY p.slug",
        )
        .bind(user_id)
        .fetch_all(self.pool)
        .await
    }


    pub async fn upsert_rating(&self, user_id: i64, plugin_id: i64, stars: i16, review: Option<&str>) -> Result<PluginRating, sqlx::Error> {
        sqlx::query_as(
            "INSERT INTO plugin_ratings (user_id, plugin_id, stars, review)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT (user_id, plugin_id) DO UPDATE SET
                stars = EXCLUDED.stars,
                review = EXCLUDED.review,
                updated_at = NOW()
             RETURNING *",
        )
        .bind(user_id)
        .bind(plugin_id)
        .bind(stars)
        .bind(review)
        .fetch_one(self.pool)
        .await
    }

    pub async fn get_user_rating(&self, user_id: i64, plugin_id: i64) -> Result<Option<PluginRating>, sqlx::Error> {
        sqlx::query_as("SELECT * FROM plugin_ratings WHERE user_id = $1 AND plugin_id = $2")
            .bind(user_id)
            .bind(plugin_id)
            .fetch_optional(self.pool)
            .await
    }

    pub async fn list_plugin_ratings(&self, plugin_id: i64, limit: i64) -> Result<Vec<PluginRating>, sqlx::Error> {
        sqlx::query_as(
            "SELECT * FROM plugin_ratings WHERE plugin_id = $1 AND review IS NOT NULL
             ORDER BY updated_at DESC LIMIT $2",
        )
        .bind(plugin_id)
        .bind(limit)
        .fetch_all(self.pool)
        .await
    }


    pub async fn get_sort_config(&self) -> Result<PluginSortConfig, sqlx::Error> {
        sqlx::query_as("SELECT * FROM plugin_sort_config WHERE id = 1")
            .fetch_one(self.pool)
            .await
    }


    pub async fn list_plugins(
        &self,
        sort: PluginSortMode,
        tag: Option<&str>,
        query: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<(i64, Vec<PluginSummary>), sqlx::Error> {
        let (total,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*)::bigint FROM plugins p
             WHERE NOT p.disabled AND NOT p.unlisted
               AND ($1::text IS NULL OR $1 = ANY(p.tags))
               AND ($2::text IS NULL OR p.slug ILIKE '%' || $2 || '%' OR p.display_name ILIKE '%' || $2 || '%' OR p.description ILIKE '%' || $2 || '%')",
        )
        .bind(tag)
        .bind(query)
        .fetch_one(self.pool)
        .await?;

        let order_sql = match sort {
            PluginSortMode::Popular => "score DESC",
            PluginSortMode::Rating  => "rating_bayesian DESC, rating_count DESC",
            PluginSortMode::Recent  => "last_released_at DESC NULLS LAST",
            PluginSortMode::New     => "created_at DESC",
        };

        let sql = format!(
            r#"
            WITH stats AS (
                SELECT
                    p.id, p.slug, p.display_name, p.description, p.tags, p.official,
                    p.updated_at, p.created_at,
                    COALESCE((
                        SELECT u.github_username FROM starfish_users u WHERE u.id = p.owner_user_id
                    ), 'unknown') AS author,
                    COALESCE((
                        SELECT COUNT(*) FROM plugin_installs pi
                        WHERE pi.plugin_id = p.id AND pi.installed_at > NOW() - INTERVAL '30 days'
                    ), 0)::bigint AS installs_30d,
                    COALESCE((
                        SELECT COUNT(*) FROM plugin_installs WHERE plugin_id = p.id
                    ), 0)::bigint AS installs_total,
                    (SELECT AVG(stars)::real FROM plugin_ratings WHERE plugin_id = p.id) AS rating_mean,
                    COALESCE((
                        SELECT COUNT(*) FROM plugin_ratings WHERE plugin_id = p.id
                    ), 0)::bigint AS rating_count,
                    (SELECT version FROM plugin_releases
                        WHERE plugin_id = p.id AND NOT yanked
                        ORDER BY created_at DESC LIMIT 1) AS latest_version,
                    (SELECT MAX(created_at) FROM plugin_releases
                        WHERE plugin_id = p.id AND NOT yanked) AS last_released_at
                FROM plugins p
                WHERE NOT p.disabled AND NOT p.unlisted
                  AND ($1::text IS NULL OR $1 = ANY(p.tags))
                  AND ($2::text IS NULL OR p.slug ILIKE '%' || $2 || '%' OR p.display_name ILIKE '%' || $2 || '%' OR p.description ILIKE '%' || $2 || '%')
                  AND EXISTS (SELECT 1 FROM plugin_releases WHERE plugin_id = p.id AND NOT yanked)
            ),
            bayes AS (
                SELECT s.*,
                    ((c.rating_prior_confidence * c.rating_prior_mean) + COALESCE(s.rating_mean, 0) * s.rating_count)
                        / (c.rating_prior_confidence + s.rating_count) AS rating_bayesian
                FROM stats s, plugin_sort_config c WHERE c.id = 1
            ),
            ranked AS (
                SELECT b.*,
                    PERCENT_RANK() OVER (ORDER BY installs_30d)                    AS velocity_pct,
                    PERCENT_RANK() OVER (ORDER BY rating_bayesian)                 AS rating_pct,
                    PERCENT_RANK() OVER (ORDER BY last_released_at NULLS FIRST)    AS recency_pct
                FROM bayes b
            )
            SELECT
                r.slug, r.display_name, r.description, r.author, r.official, r.tags,
                r.latest_version, r.updated_at,
                r.installs_30d, r.installs_total,
                r.rating_mean, r.rating_count, r.rating_bayesian::real AS rating_bayesian,
                (r.velocity_pct * c.velocity_weight + r.rating_pct * c.rating_weight + r.recency_pct * c.recency_weight) AS score
            FROM ranked r, plugin_sort_config c WHERE c.id = 1
            ORDER BY {order_sql}
            LIMIT $3 OFFSET $4
            "#,
        );

        let plugins: Vec<PluginSummary> = sqlx::query_as(&sql)
            .bind(tag)
            .bind(query)
            .bind(limit)
            .bind(offset)
            .fetch_all(self.pool)
            .await?;

        Ok((total, plugins))
    }


    pub async fn plugin_rating_stats(&self, plugin_id: i64) -> Result<(Option<f32>, i64, f32), sqlx::Error> {
        let row: (Option<f32>, i64, f32) = sqlx::query_as(
            "SELECT
                (SELECT AVG(stars)::real FROM plugin_ratings WHERE plugin_id = $1),
                (SELECT COUNT(*)::bigint FROM plugin_ratings WHERE plugin_id = $1),
                (
                    SELECT ((c.rating_prior_confidence * c.rating_prior_mean) + COALESCE((SELECT SUM(stars)::real FROM plugin_ratings WHERE plugin_id = $1), 0))
                        / (c.rating_prior_confidence + COALESCE((SELECT COUNT(*)::real FROM plugin_ratings WHERE plugin_id = $1), 0))
                    FROM plugin_sort_config c WHERE c.id = 1
                )::real
            ",
        )
        .bind(plugin_id)
        .fetch_one(self.pool)
        .await?;
        Ok(row)
    }

    pub async fn plugin_install_counts(&self, plugin_id: i64) -> Result<(i64, i64), sqlx::Error> {
        let row: (i64, i64) = sqlx::query_as(
            "SELECT
                (SELECT COUNT(*)::bigint FROM plugin_installs WHERE plugin_id = $1 AND installed_at > NOW() - INTERVAL '30 days'),
                (SELECT COUNT(*)::bigint FROM plugin_installs WHERE plugin_id = $1)",
        )
        .bind(plugin_id)
        .fetch_one(self.pool)
        .await?;
        Ok(row)
    }
}
