use anyhow::Result;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;
use tracing::warn;

pub struct MemberRow {
    pub discord_id: i64,
    pub uuid: Option<String>,
    pub join_date: Option<String>,
    pub request_count: i64,
    pub tagging_disabled: bool,
    pub config: Value,
    pub minecraft_accounts: Vec<String>,
}

pub struct TagRow {
    pub tag_type: String,
    pub reason: String,
    pub added_by: i64,
    pub added_on: Option<String>,
    pub hide_username: bool,
}

pub struct BlacklistRow {
    pub uuid: String,
    pub is_locked: bool,
    pub lock_reason: Option<String>,
    pub locked_by: Option<i64>,
    pub locked_at: Option<String>,
    pub tags: Vec<TagRow>,
}

pub struct Sink {
    pool: PgPool,
}

impl Sink {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn wipe_blacklist(&self) -> Result<()> {
        sqlx::query("DELETE FROM player_events")
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn insert_members(&self, rows: &[MemberRow]) -> usize {
        let mut errors = 0;
        for row in rows {
            if let Err(e) = self.insert_member(row).await {
                warn!("Failed to migrate member {}: {e}", row.discord_id);
                errors += 1;
            }
        }
        errors
    }

    pub async fn insert_blacklist(&self, rows: &[BlacklistRow]) -> usize {
        let mut errors = 0;
        for row in rows {
            if let Err(e) = self.insert_blacklist_player(row).await {
                warn!("Failed to migrate player {}: {e}", row.uuid);
                errors += 1;
            }
        }
        errors
    }

    async fn insert_member(&self, m: &MemberRow) -> Result<(), sqlx::Error> {
        let join_date = parse_ts(m.join_date.as_deref()).unwrap_or_else(Utc::now);
        let api_key = uuid::Uuid::new_v4().to_string();

        let (member_id,): (i64,) = sqlx::query_as(
            r#"
            INSERT INTO members (discord_id, uuid, api_key, join_date, request_count, tagging_disabled, config)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (discord_id) DO UPDATE SET
                uuid = EXCLUDED.uuid,
                request_count = EXCLUDED.request_count,
                tagging_disabled = EXCLUDED.tagging_disabled,
                config = EXCLUDED.config
            RETURNING id
            "#,
        )
        .bind(m.discord_id).bind(&m.uuid).bind(&api_key)
        .bind(join_date).bind(m.request_count)
        .bind(m.tagging_disabled).bind(&m.config)
        .fetch_one(&self.pool).await?;

        for uuid in &m.minecraft_accounts {
            let _ = sqlx::query(
                "INSERT INTO minecraft_accounts (member_id, uuid) VALUES ($1, $2) ON CONFLICT DO NOTHING",
            )
            .bind(member_id)
            .bind(uuid)
            .execute(&self.pool)
            .await;
        }

        Ok(())
    }

    async fn insert_blacklist_player(&self, p: &BlacklistRow) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM player_events WHERE uuid = $1")
            .bind(&p.uuid)
            .execute(&self.pool)
            .await?;

        if p.is_locked {
            let locked_at = parse_ts(p.locked_at.as_deref()).unwrap_or_else(Utc::now);
            sqlx::query(
                "INSERT INTO player_events (uuid, kind, reason, author, ts) VALUES ($1, 'lock', $2, $3, $4)",
            )
            .bind(&p.uuid)
            .bind(&p.lock_reason)
            .bind(p.locked_by)
            .bind(locked_at)
            .execute(&self.pool)
            .await?;
        }

        for tag in &p.tags {
            let added_on = parse_ts(tag.added_on.as_deref()).unwrap_or_else(Utc::now);
            let _ = sqlx::query(
                "INSERT INTO player_events (uuid, kind, tag_type, reason, hide_username, author, ts) VALUES ($1, 'tag_set', $2, $3, $4, $5, $6)",
            )
            .bind(&p.uuid)
            .bind(&tag.tag_type)
            .bind(&tag.reason)
            .bind(tag.hide_username)
            .bind(tag.added_by)
            .bind(added_on)
            .execute(&self.pool)
            .await;
        }

        Ok(())
    }
}

fn parse_ts(s: Option<&str>) -> Option<DateTime<Utc>> {
    let s = s?;
    match DateTime::parse_from_rfc3339(s) {
        Ok(dt) => Some(dt.with_timezone(&Utc)),
        Err(e) => {
            warn!("Bad timestamp '{s}': {e}");
            None
        }
    }
}
