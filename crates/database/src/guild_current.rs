use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sqlx::PgPool;

use hypixel::Guild;

pub struct GuildCurrentRepository<'a> {
    pool: &'a PgPool,
}

impl<'a> GuildCurrentRepository<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn upsert(&self, raw: &Value) -> Result<(), sqlx::Error> {
        let Some(guild) = Guild::from_value(raw) else {
            return Ok(());
        };
        sqlx::query(
            "INSERT INTO guild_current
                (guild_id, name, tag, tag_color, level, experience, member_count, created, raw, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, NOW())
             ON CONFLICT (guild_id) DO UPDATE SET
                name = EXCLUDED.name, tag = EXCLUDED.tag, tag_color = EXCLUDED.tag_color,
                level = EXCLUDED.level, experience = EXCLUDED.experience,
                member_count = EXCLUDED.member_count, created = EXCLUDED.created,
                raw = EXCLUDED.raw, updated_at = NOW()",
        )
        .bind(&guild.id)
        .bind(&guild.name)
        .bind(&guild.tag)
        .bind(&guild.tag_color)
        .bind(guild.level as i32)
        .bind(guild.experience as i64)
        .bind(guild.member_count() as i32)
        .bind(guild.created)
        .bind(raw)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    pub async fn get(&self, guild_id: &str) -> Result<Option<(Value, DateTime<Utc>)>, sqlx::Error> {
        sqlx::query_as("SELECT raw, updated_at FROM guild_current WHERE guild_id = $1")
            .bind(guild_id)
            .fetch_optional(self.pool)
            .await
    }

    pub async fn get_by_name(
        &self,
        name: &str,
    ) -> Result<Option<(Value, DateTime<Utc>)>, sqlx::Error> {
        sqlx::query_as(
            "SELECT raw, updated_at FROM guild_current WHERE LOWER(name) = LOWER($1) LIMIT 1",
        )
        .bind(name)
        .fetch_optional(self.pool)
        .await
    }

    pub async fn get_by_member(
        &self,
        uuid: &str,
    ) -> Result<Option<(Value, DateTime<Utc>)>, sqlx::Error> {
        sqlx::query_as("SELECT raw, updated_at FROM guild_current WHERE raw @> $1 LIMIT 1")
            .bind(json!({ "members": [{ "uuid": uuid }] }))
            .fetch_optional(self.pool)
            .await
    }

    pub async fn count_guilds(&self) -> Result<i64, sqlx::Error> {
        let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM guild_current")
            .fetch_one(self.pool)
            .await?;
        Ok(count)
    }

    pub async fn stale_guilds_with_member(
        &self,
        uuid: &str,
        cutoff: DateTime<Utc>,
    ) -> Result<Vec<String>, sqlx::Error> {
        sqlx::query_scalar::<_, String>(
            "SELECT guild_id FROM guild_current WHERE raw @> $1 AND updated_at < $2",
        )
        .bind(json!({ "members": [{ "uuid": uuid }] }))
        .bind(cutoff)
        .fetch_all(self.pool)
        .await
    }
}
