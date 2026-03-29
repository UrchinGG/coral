use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{FromRow, PgPool};


#[derive(Debug, Clone, FromRow)]
pub struct Member {
    pub id: i64,
    pub discord_id: i64,
    pub uuid: Option<String>,
    pub api_key: Option<String>,
    pub join_date: DateTime<Utc>,
    pub request_count: i64,
    pub access_level: i16,
    pub key_locked: bool,
    pub tagging_disabled: bool,
    pub accepted_tags: i64,
    pub rejected_tags: i64,
    pub accurate_verdicts: i64,
    pub config: Value,
}


pub struct MemberRepository<'a> {
    pool: &'a PgPool,
}


impl<'a> MemberRepository<'a> {
    pub fn new(pool: &'a PgPool) -> Self { Self { pool } }

    pub async fn get_by_id(&self, id: i64) -> Result<Option<Member>, sqlx::Error> {
        sqlx::query_as(
            "SELECT id, discord_id, uuid, api_key, join_date, request_count,
                    access_level, key_locked, tagging_disabled, accepted_tags, rejected_tags, accurate_verdicts, config
             FROM members WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(self.pool)
        .await
    }

    pub async fn get_by_discord_id(&self, discord_id: i64) -> Result<Option<Member>, sqlx::Error> {
        sqlx::query_as(
            "SELECT id, discord_id, uuid, api_key, join_date, request_count,
                    access_level, key_locked, tagging_disabled, accepted_tags, rejected_tags, accurate_verdicts, config
             FROM members WHERE discord_id = $1",
        )
        .bind(discord_id)
        .fetch_optional(self.pool)
        .await
    }

    pub async fn get_linked_by_discord_ids(&self, discord_ids: &[i64]) -> Result<Vec<Member>, sqlx::Error> {
        sqlx::query_as(
            "SELECT id, discord_id, uuid, api_key, join_date, request_count,
                    access_level, key_locked, tagging_disabled, accepted_tags, rejected_tags, accurate_verdicts, config
             FROM members WHERE discord_id = ANY($1) AND uuid IS NOT NULL",
        )
        .bind(discord_ids)
        .fetch_all(self.pool)
        .await
    }

    pub async fn get_by_api_key(&self, api_key: &str) -> Result<Option<Member>, sqlx::Error> {
        sqlx::query_as(
            "SELECT id, discord_id, uuid, api_key, join_date, request_count,
                    access_level, key_locked, tagging_disabled, accepted_tags, rejected_tags, accurate_verdicts, config
             FROM members WHERE api_key = $1",
        )
        .bind(api_key)
        .fetch_optional(self.pool)
        .await
    }

    pub async fn create(&self, discord_id: i64) -> Result<Member, sqlx::Error> {
        sqlx::query_as(
            "INSERT INTO members (discord_id) VALUES ($1)
             ON CONFLICT (discord_id) DO UPDATE SET discord_id = EXCLUDED.discord_id
             RETURNING id, discord_id, uuid, api_key, join_date, request_count,
                       access_level, key_locked, tagging_disabled, accepted_tags, rejected_tags, accurate_verdicts, config",
        )
        .bind(discord_id)
        .fetch_one(self.pool)
        .await
    }

    pub async fn set_uuid(&self, discord_id: i64, uuid: &str) -> Result<bool, sqlx::Error> {
        sqlx::query("UPDATE members SET uuid = $2 WHERE discord_id = $1")
            .bind(discord_id)
            .bind(uuid)
            .execute(self.pool)
            .await
            .map(|r| r.rows_affected() > 0)
    }

    pub async fn clear_uuid(&self, discord_id: i64) -> Result<bool, sqlx::Error> {
        sqlx::query("UPDATE members SET uuid = NULL WHERE discord_id = $1")
            .bind(discord_id)
            .execute(self.pool)
            .await
            .map(|r| r.rows_affected() > 0)
    }

    pub async fn set_api_key(&self, discord_id: i64, api_key: &str) -> Result<bool, sqlx::Error> {
        sqlx::query("UPDATE members SET api_key = $2 WHERE discord_id = $1")
            .bind(discord_id)
            .bind(api_key)
            .execute(self.pool)
            .await
            .map(|r| r.rows_affected() > 0)
    }

    pub async fn lock_key(&self, discord_id: i64) -> Result<bool, sqlx::Error> {
        sqlx::query("UPDATE members SET key_locked = true WHERE discord_id = $1")
            .bind(discord_id)
            .execute(self.pool)
            .await
            .map(|r| r.rows_affected() > 0)
    }

    pub async fn revoke_api_key(&self, discord_id: i64) -> Result<bool, sqlx::Error> {
        sqlx::query("UPDATE members SET api_key = NULL, key_locked = true WHERE discord_id = $1")
            .bind(discord_id)
            .execute(self.pool)
            .await
            .map(|r| r.rows_affected() > 0)
    }

    pub async fn unlock_key(&self, discord_id: i64) -> Result<bool, sqlx::Error> {
        sqlx::query("UPDATE members SET key_locked = false WHERE discord_id = $1")
            .bind(discord_id)
            .execute(self.pool)
            .await
            .map(|r| r.rows_affected() > 0)
    }

    pub async fn set_access_level(&self, discord_id: i64, level: i16) -> Result<bool, sqlx::Error> {
        sqlx::query("UPDATE members SET access_level = $2 WHERE discord_id = $1")
            .bind(discord_id)
            .bind(level)
            .execute(self.pool)
            .await
            .map(|r| r.rows_affected() > 0)
    }

    pub async fn increment_request_count(&self, api_key: &str) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE members SET request_count = request_count + 1 WHERE api_key = $1")
            .bind(api_key)
            .execute(self.pool)
            .await?;
        Ok(())
    }

    pub async fn update_config(&self, discord_id: i64, config: &Value) -> Result<bool, sqlx::Error> {
        sqlx::query("UPDATE members SET config = $2 WHERE discord_id = $1")
            .bind(discord_id)
            .bind(config)
            .execute(self.pool)
            .await
            .map(|r| r.rows_affected() > 0)
    }

    pub async fn set_tagging_disabled(&self, discord_id: i64, disabled: bool) -> Result<bool, sqlx::Error> {
        sqlx::query("UPDATE members SET tagging_disabled = $2 WHERE discord_id = $1")
            .bind(discord_id)
            .bind(disabled)
            .execute(self.pool)
            .await
            .map(|r| r.rows_affected() > 0)
    }

    pub async fn increment_accepted_tags(&self, discord_id: i64) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE members SET accepted_tags = accepted_tags + 1 WHERE discord_id = $1")
            .bind(discord_id)
            .execute(self.pool)
            .await?;
        Ok(())
    }

    pub async fn increment_rejected_tags(&self, discord_id: i64) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE members SET rejected_tags = rejected_tags + 1 WHERE discord_id = $1")
            .bind(discord_id)
            .execute(self.pool)
            .await?;
        Ok(())
    }

    pub async fn increment_accurate_verdicts(&self, discord_ids: &[i64]) -> Result<(), sqlx::Error> {
        if discord_ids.is_empty() { return Ok(()) }
        sqlx::query("UPDATE members SET accurate_verdicts = accurate_verdicts + 1 WHERE discord_id = ANY($1)")
            .bind(discord_ids)
            .execute(self.pool)
            .await?;
        Ok(())
    }

    pub async fn add_strike(&self, discord_id: i64, reason: &str, struck_by: u64) -> Result<bool, sqlx::Error> {
        let strike = serde_json::json!({
            "reason": reason,
            "struck_by": struck_by,
            "timestamp": Utc::now().to_rfc3339(),
        });
        sqlx::query(
            "UPDATE members SET config = jsonb_set(
                config, '{strikes}', COALESCE(config->'strikes', '[]'::jsonb) || $2::jsonb
            ) WHERE discord_id = $1",
        )
        .bind(discord_id)
        .bind(strike)
        .execute(self.pool)
        .await
        .map(|r| r.rows_affected() > 0)
    }

    pub async fn remove_strike(&self, discord_id: i64, index: usize) -> Result<bool, sqlx::Error> {
        sqlx::query(
            "UPDATE members SET config = jsonb_set(
                config, '{strikes}', (config->'strikes') - $2::int
            ) WHERE discord_id = $1",
        )
        .bind(discord_id)
        .bind(index as i32)
        .execute(self.pool)
        .await
        .map(|r| r.rows_affected() > 0)
    }

    pub async fn count(&self) -> Result<i64, sqlx::Error> {
        let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM members")
            .fetch_one(self.pool).await?;
        Ok(count)
    }

    pub async fn total_requests(&self) -> Result<i64, sqlx::Error> {
        let (total,): (i64,) = sqlx::query_as("SELECT COALESCE(SUM(request_count), 0) FROM members")
            .fetch_one(self.pool).await?;
        Ok(total)
    }

    pub async fn record_ip(&self, member_id: i64, ip: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO api_key_ips (member_id, ip_address) VALUES ($1, $2::inet)
             ON CONFLICT (member_id, ip_address) DO UPDATE SET last_seen = NOW()",
        )
        .bind(member_id)
        .bind(ip)
        .execute(self.pool)
        .await?;
        Ok(())
    }
}
