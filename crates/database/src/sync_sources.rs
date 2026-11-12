use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{FromRow, PgPool};

const SOURCE_COLUMNS: &str =
    "id, name, base_url, api_key, has_feed, max_age_secs, last_seq, created_at";

/// An external data source the sync reads, keyed by Discord ID.
#[derive(Debug, Clone, FromRow)]
pub struct SyncSource {
    pub id: i64,
    pub name: String,
    pub base_url: String,
    pub api_key: Option<String>,
    pub has_feed: bool,
    pub max_age_secs: i32,
    pub last_seq: Option<i64>,
    pub created_at: DateTime<Utc>,
}

/// What a source last said about a member. `value` is `None` when the source
/// answered that it does not know them.
#[derive(Debug, Clone, FromRow)]
pub struct CachedSourceValue {
    pub value: Option<Value>,
    pub fetched_at: DateTime<Utc>,
    pub stale: bool,
}

pub struct NewSyncSource<'a> {
    pub name: &'a str,
    pub base_url: &'a str,
    pub api_key: Option<&'a str>,
    pub has_feed: bool,
    pub max_age_secs: i32,
    pub created_by: i64,
}

pub struct SyncSourceRepository<'a> {
    pool: &'a PgPool,
}

impl<'a> SyncSourceRepository<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn list(&self) -> Result<Vec<SyncSource>, sqlx::Error> {
        sqlx::query_as(&format!(
            "SELECT {SOURCE_COLUMNS} FROM sync_sources ORDER BY name"
        ))
        .fetch_all(self.pool)
        .await
    }

    pub async fn get(&self, id: i64) -> Result<Option<SyncSource>, sqlx::Error> {
        sqlx::query_as(&format!(
            "SELECT {SOURCE_COLUMNS} FROM sync_sources WHERE id = $1"
        ))
        .bind(id)
        .fetch_optional(self.pool)
        .await
    }

    /// `None` when a source with that name already exists.
    pub async fn add(&self, source: &NewSyncSource<'_>) -> Result<Option<SyncSource>, sqlx::Error> {
        sqlx::query_as(&format!(
            "INSERT INTO sync_sources (name, base_url, api_key, has_feed, max_age_secs, created_by)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (name) DO NOTHING
             RETURNING {SOURCE_COLUMNS}"
        ))
        .bind(source.name)
        .bind(source.base_url)
        .bind(source.api_key)
        .bind(source.has_feed)
        .bind(source.max_age_secs)
        .bind(source.created_by)
        .fetch_optional(self.pool)
        .await
    }

    pub async fn remove(&self, id: i64) -> Result<bool, sqlx::Error> {
        Ok(sqlx::query("DELETE FROM sync_sources WHERE id = $1")
            .bind(id)
            .execute(self.pool)
            .await?
            .rows_affected()
            > 0)
    }

    pub async fn set_last_seq(&self, id: i64, seq: i64) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE sync_sources SET last_seq = $2 WHERE id = $1")
            .bind(id)
            .bind(seq)
            .execute(self.pool)
            .await?;
        Ok(())
    }

    pub async fn for_guild(&self, guild_id: i64) -> Result<Vec<SyncSource>, sqlx::Error> {
        sqlx::query_as(&format!(
            "SELECT {} FROM sync_sources s
             JOIN guild_sync_sources g ON g.source_id = s.id
             WHERE g.guild_id = $1
             ORDER BY s.name",
            prefixed("s.")
        ))
        .bind(guild_id)
        .fetch_all(self.pool)
        .await
    }

    /// Replaces the server's chosen sources. The server must have a sync config.
    pub async fn set_for_guild(
        &self,
        guild_id: i64,
        source_ids: &[i64],
    ) -> Result<(), sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM guild_sync_sources WHERE guild_id = $1")
            .bind(guild_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "INSERT INTO guild_sync_sources (guild_id, source_id)
             SELECT $1, unnest($2::bigint[])",
        )
        .bind(guild_id)
        .bind(source_ids)
        .execute(&mut *tx)
        .await?;
        tx.commit().await
    }

    pub async fn guilds_using(&self, source_id: i64) -> Result<Vec<i64>, sqlx::Error> {
        sqlx::query_scalar("SELECT guild_id FROM guild_sync_sources WHERE source_id = $1")
            .bind(source_id)
            .fetch_all(self.pool)
            .await
    }

    pub async fn cached(
        &self,
        source_id: i64,
        discord_id: i64,
    ) -> Result<Option<CachedSourceValue>, sqlx::Error> {
        sqlx::query_as(
            "SELECT value, fetched_at, stale FROM sync_source_values
             WHERE source_id = $1 AND discord_id = $2",
        )
        .bind(source_id)
        .bind(discord_id)
        .fetch_optional(self.pool)
        .await
    }

    pub async fn store(
        &self,
        source_id: i64,
        discord_id: i64,
        value: Option<&Value>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO sync_source_values (source_id, discord_id, value, fetched_at, stale)
             VALUES ($1, $2, $3, now(), false)
             ON CONFLICT (source_id, discord_id)
             DO UPDATE SET value = EXCLUDED.value, fetched_at = now(), stale = false",
        )
        .bind(source_id)
        .bind(discord_id)
        .bind(value)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    pub async fn mark_stale(&self, source_id: i64, discord_ids: &[i64]) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE sync_source_values SET stale = true
             WHERE source_id = $1 AND discord_id = ANY($2)",
        )
        .bind(source_id)
        .bind(discord_ids)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    pub async fn mark_all_stale(&self, source_id: i64) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE sync_source_values SET stale = true WHERE source_id = $1")
            .bind(source_id)
            .execute(self.pool)
            .await?;
        Ok(())
    }
}

fn prefixed(prefix: &str) -> String {
    SOURCE_COLUMNS
        .split(", ")
        .map(|column| format!("{prefix}{column}"))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::GuildConfigRepository;

    async fn test_pool() -> Option<PgPool> {
        dotenvy::dotenv().ok();
        sqlx::postgres::PgPoolOptions::new()
            .max_connections(2)
            .connect(&std::env::var("DATABASE_URL").ok()?)
            .await
            .ok()
    }

    fn unique(tag: &str) -> String {
        format!("{tag}_{}", Utc::now().timestamp_nanos_opt().unwrap())
    }

    #[tokio::test]
    async fn a_source_is_chosen_per_server_and_caches_values() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let sources = SyncSourceRepository::new(&pool);
        let name = unique("demo");
        let source = sources
            .add(&NewSyncSource {
                name: &name,
                base_url: "http://demo",
                api_key: None,
                has_feed: true,
                max_age_secs: 3600,
                created_by: 1,
            })
            .await
            .unwrap()
            .unwrap();
        let duplicate = NewSyncSource {
            name: &name,
            base_url: "http://other",
            api_key: None,
            has_feed: false,
            max_age_secs: 60,
            created_by: 1,
        };
        assert!(sources.add(&duplicate).await.unwrap().is_none());

        let guild = Utc::now().timestamp_nanos_opt().unwrap();
        GuildConfigRepository::new(&pool)
            .upsert(guild, 1)
            .await
            .unwrap();
        sources.set_for_guild(guild, &[source.id]).await.unwrap();
        assert_eq!(sources.for_guild(guild).await.unwrap()[0].name, name);
        assert_eq!(sources.guilds_using(source.id).await.unwrap(), vec![guild]);

        sources
            .store(source.id, 7, Some(&json!({"elo": 1200})))
            .await
            .unwrap();
        sources.store(source.id, 8, None).await.unwrap();
        sources.mark_stale(source.id, &[7]).await.unwrap();
        let seven = sources.cached(source.id, 7).await.unwrap().unwrap();
        assert_eq!(seven.value, Some(json!({"elo": 1200})));
        assert!(seven.stale);
        assert_eq!(
            sources.cached(source.id, 8).await.unwrap().unwrap().value,
            None
        );

        sources.mark_all_stale(source.id).await.unwrap();
        assert!(sources.cached(source.id, 8).await.unwrap().unwrap().stale);

        assert!(sources.remove(source.id).await.unwrap());
        assert!(sources.cached(source.id, 7).await.unwrap().is_none());
        assert!(sources.for_guild(guild).await.unwrap().is_empty());
    }
}
