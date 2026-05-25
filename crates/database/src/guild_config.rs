use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool};

#[derive(Debug, Clone, FromRow)]
pub struct GuildConfig {
    pub id: i64,
    pub guild_id: i64,
    pub link_role_id: Option<i64>,
    pub unlinked_role_id: Option<i64>,
    pub nickname_template: Option<String>,
    pub link_channel_id: Option<i64>,
    pub link_message_id: Option<i64>,
    pub configured_by: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
pub struct GuildRoleRule {
    pub id: i64,
    pub guild_id: i64,
    pub role_id: i64,
    pub condition: String,
    pub priority: i32,
    pub created_at: DateTime<Utc>,
}

pub struct GuildConfigRepository<'a> {
    pool: &'a PgPool,
}

impl<'a> GuildConfigRepository<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn get(&self, guild_id: i64) -> Result<Option<GuildConfig>, sqlx::Error> {
        sqlx::query_as(
            "SELECT id, guild_id, link_role_id, unlinked_role_id, nickname_template,
                    link_channel_id, link_message_id, configured_by, created_at, updated_at
             FROM guild_config WHERE guild_id = $1",
        )
        .bind(guild_id)
        .fetch_optional(self.pool)
        .await
    }

    pub async fn get_all(&self) -> Result<Vec<GuildConfig>, sqlx::Error> {
        sqlx::query_as(
            "SELECT id, guild_id, link_role_id, unlinked_role_id, nickname_template,
                    link_channel_id, link_message_id, configured_by, created_at, updated_at
             FROM guild_config",
        )
        .fetch_all(self.pool)
        .await
    }

    pub async fn upsert(
        &self,
        guild_id: i64,
        configured_by: i64,
    ) -> Result<GuildConfig, sqlx::Error> {
        sqlx::query_as(
            "INSERT INTO guild_config (guild_id, configured_by) VALUES ($1, $2)
             ON CONFLICT (guild_id) DO UPDATE SET updated_at = NOW()
             RETURNING id, guild_id, link_role_id, unlinked_role_id, nickname_template,
                       link_channel_id, link_message_id, configured_by, created_at, updated_at",
        )
        .bind(guild_id)
        .bind(configured_by)
        .fetch_one(self.pool)
        .await
    }

    pub async fn set_link_role(
        &self,
        guild_id: i64,
        role_id: Option<i64>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE guild_config SET link_role_id = $2, updated_at = NOW() WHERE guild_id = $1",
        )
        .bind(guild_id)
        .bind(role_id)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    pub async fn set_unlinked_role(
        &self,
        guild_id: i64,
        role_id: Option<i64>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE guild_config SET unlinked_role_id = $2, updated_at = NOW() WHERE guild_id = $1",
        )
        .bind(guild_id)
        .bind(role_id)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    pub async fn set_nickname_template(
        &self,
        guild_id: i64,
        template: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE guild_config SET nickname_template = $2, updated_at = NOW() WHERE guild_id = $1")
            .bind(guild_id)
            .bind(template)
            .execute(self.pool)
            .await?;
        Ok(())
    }

    pub async fn set_link_channel(
        &self,
        guild_id: i64,
        channel_id: Option<i64>,
        message_id: Option<i64>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE guild_config SET link_channel_id = $2, link_message_id = $3, updated_at = NOW()
             WHERE guild_id = $1",
        )
        .bind(guild_id)
        .bind(channel_id)
        .bind(message_id)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete(&self, guild_id: i64) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM guild_config WHERE guild_id = $1")
            .bind(guild_id)
            .execute(self.pool)
            .await?;
        Ok(())
    }

    pub async fn get_role_rules(&self, guild_id: i64) -> Result<Vec<GuildRoleRule>, sqlx::Error> {
        sqlx::query_as(
            "SELECT id, guild_id, role_id, condition, priority, created_at
             FROM guild_role_rules WHERE guild_id = $1 ORDER BY priority, id",
        )
        .bind(guild_id)
        .fetch_all(self.pool)
        .await
    }

    pub async fn add_role_rule(
        &self,
        guild_id: i64,
        role_id: i64,
        condition: &str,
        priority: i32,
    ) -> Result<GuildRoleRule, sqlx::Error> {
        sqlx::query_as(
            "INSERT INTO guild_role_rules (guild_id, role_id, condition, priority)
             VALUES ($1, $2, $3, $4)
             RETURNING id, guild_id, role_id, condition, priority, created_at",
        )
        .bind(guild_id)
        .bind(role_id)
        .bind(condition)
        .bind(priority)
        .fetch_one(self.pool)
        .await
    }

    pub async fn update_role_rule_condition(
        &self,
        rule_id: i64,
        condition: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE guild_role_rules SET condition = $2 WHERE id = $1")
            .bind(rule_id)
            .bind(condition)
            .execute(self.pool)
            .await?;
        Ok(())
    }

    pub async fn remove_role_rule(&self, rule_id: i64) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM guild_role_rules WHERE id = $1")
            .bind(rule_id)
            .execute(self.pool)
            .await?;
        Ok(())
    }
}
