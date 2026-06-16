use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool};

#[derive(Debug, Clone, FromRow)]
pub struct PlayerEvent {
    pub id: i64,
    pub uuid: String,
    pub kind: String,
    pub tag_type: Option<String>,
    pub reason: Option<String>,
    pub hide_username: Option<bool>,
    pub expires_at: Option<DateTime<Utc>>,
    pub reviewed_by: Option<Vec<i64>>,
    pub author: Option<i64>,
    pub ts: DateTime<Utc>,
}

#[derive(Debug, Clone, Default)]
pub struct LockState {
    pub locked: bool,
    pub reason: Option<String>,
    pub locked_by: Option<i64>,
    pub locked_at: Option<DateTime<Utc>>,
}

#[derive(Debug)]
pub enum AddOutcome {
    Inserted(i64),
    Conflict(PlayerEvent),
}

#[derive(Debug)]
pub enum OverwriteOutcome {
    Inserted { old: PlayerEvent, new: PlayerEvent },
    OldNotActive,
    Conflict(PlayerEvent),
}

const COLS: &str =
    "id, uuid, kind, tag_type, reason, hide_username, expires_at, reviewed_by, author, ts";

pub struct BlacklistRepository<'a> {
    pool: &'a PgPool,
}

impl<'a> BlacklistRepository<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn add_event(
        &self,
        uuid: &str,
        tag_type: &str,
        reason: &str,
        hide_username: bool,
        expires_at: Option<DateTime<Utc>>,
        reviewed_by: Option<&[i64]>,
        author: Option<i64>,
        blocking_types: &[String],
    ) -> Result<AddOutcome, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        acquire_lock(&mut tx, uuid).await?;

        if !blocking_types.is_empty()
            && let Some(conflict) = active_conflict(&mut tx, uuid, blocking_types).await?
        {
            return Ok(AddOutcome::Conflict(conflict));
        }

        let (id,): (i64,) = sqlx::query_as(
            "INSERT INTO player_events
                (uuid, kind, tag_type, reason, hide_username, expires_at, reviewed_by, author)
             VALUES ($1, 'tag_set', $2, $3, $4, $5, $6, $7)
             RETURNING id",
        )
        .bind(uuid)
        .bind(tag_type)
        .bind(reason)
        .bind(hide_username)
        .bind(expires_at)
        .bind(reviewed_by)
        .bind(author)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(AddOutcome::Inserted(id))
    }

    pub async fn remove_event(
        &self,
        uuid: &str,
        tag_type: &str,
        author: Option<i64>,
    ) -> Result<bool, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        acquire_lock(&mut tx, uuid).await?;

        if latest_active(&mut tx, uuid, tag_type).await?.is_none() {
            return Ok(false);
        }
        sqlx::query(
            "INSERT INTO player_events (uuid, kind, tag_type, author)
             VALUES ($1, 'tag_clear', $2, $3)",
        )
        .bind(uuid)
        .bind(tag_type)
        .bind(author)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(true)
    }

    pub async fn overwrite_event(
        &self,
        uuid: &str,
        old_tag_type: &str,
        new_tag_type: &str,
        new_reason: &str,
        hide_username: bool,
        expires_at: Option<DateTime<Utc>>,
        reviewed_by: Option<&[i64]>,
        author: Option<i64>,
        blocking_types: &[String],
        expected_old_id: Option<i64>,
    ) -> Result<OverwriteOutcome, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        acquire_lock(&mut tx, uuid).await?;

        let Some(old) = latest_active(&mut tx, uuid, old_tag_type).await? else {
            return Ok(OverwriteOutcome::OldNotActive);
        };
        if let Some(expected) = expected_old_id
            && old.id != expected
        {
            return Ok(OverwriteOutcome::OldNotActive);
        }
        if !blocking_types.is_empty()
            && let Some(conflict) = active_conflict(&mut tx, uuid, blocking_types).await?
        {
            return Ok(OverwriteOutcome::Conflict(conflict));
        }

        let (_remove_id, ts): (i64, DateTime<Utc>) = sqlx::query_as(
            "INSERT INTO player_events (uuid, kind, tag_type, author)
             VALUES ($1, 'tag_clear', $2, $3)
             RETURNING id, ts",
        )
        .bind(uuid)
        .bind(old_tag_type)
        .bind(author)
        .fetch_one(&mut *tx)
        .await?;
        let new: PlayerEvent = sqlx::query_as(&format!(
            "INSERT INTO player_events
                (uuid, kind, tag_type, reason, hide_username, expires_at, reviewed_by, author, ts)
             VALUES ($1, 'tag_set', $2, $3, $4, $5, $6, $7, $8)
             RETURNING {COLS}",
        ))
        .bind(uuid)
        .bind(new_tag_type)
        .bind(new_reason)
        .bind(hide_username)
        .bind(expires_at)
        .bind(reviewed_by)
        .bind(author)
        .bind(ts)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(OverwriteOutcome::Inserted { old, new })
    }

    pub async fn lock_event(
        &self,
        uuid: &str,
        reason: Option<&str>,
        author: i64,
    ) -> Result<bool, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        acquire_lock(&mut tx, uuid).await?;

        if current_lock_kind(&mut tx, uuid).await?.as_deref() == Some("lock") {
            return Ok(false);
        }
        sqlx::query(
            "INSERT INTO player_events (uuid, kind, reason, author)
             VALUES ($1, 'lock', $2, $3)",
        )
        .bind(uuid)
        .bind(reason)
        .bind(author)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(true)
    }

    pub async fn unlock_event(&self, uuid: &str, author: i64) -> Result<bool, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        acquire_lock(&mut tx, uuid).await?;

        if current_lock_kind(&mut tx, uuid).await?.as_deref() != Some("lock") {
            return Ok(false);
        }
        sqlx::query(
            "INSERT INTO player_events (uuid, kind, author)
             VALUES ($1, 'unlock', $2)",
        )
        .bind(uuid)
        .bind(author)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(true)
    }

    pub async fn get_lock_state(&self, uuid: &str) -> Result<LockState, sqlx::Error> {
        let latest: Option<(String, Option<String>, Option<i64>, DateTime<Utc>)> = sqlx::query_as(
            "SELECT kind, reason, author, ts FROM player_events
             WHERE uuid = $1 AND kind IN ('lock', 'unlock')
             ORDER BY ts DESC, id DESC LIMIT 1",
        )
        .bind(uuid)
        .fetch_optional(self.pool)
        .await?;
        Ok(match latest {
            Some((kind, reason, author, ts)) if kind == "lock" => LockState {
                locked: true,
                reason,
                locked_by: author,
                locked_at: Some(ts),
            },
            _ => LockState::default(),
        })
    }

    pub async fn get_active_tags(&self, uuid: &str) -> Result<Vec<PlayerEvent>, sqlx::Error> {
        sqlx::query_as(&format!(
            "SELECT {COLS} FROM (
                 SELECT DISTINCT ON (tag_type) {COLS}
                 FROM player_events
                 WHERE uuid = $1 AND kind IN ('tag_set', 'tag_clear')
                 ORDER BY tag_type, ts DESC, id DESC
             ) latest
             WHERE kind = 'tag_set'
               AND (expires_at IS NULL OR expires_at > NOW())
             ORDER BY ts DESC, id DESC",
        ))
        .bind(uuid)
        .fetch_all(self.pool)
        .await
    }

    pub async fn get_active_tag(
        &self,
        uuid: &str,
        tag_type: &str,
    ) -> Result<Option<PlayerEvent>, sqlx::Error> {
        let latest: Option<PlayerEvent> = sqlx::query_as(&format!(
            "SELECT {COLS} FROM player_events
             WHERE uuid = $1 AND tag_type = $2 AND kind IN ('tag_set', 'tag_clear')
             ORDER BY ts DESC, id DESC LIMIT 1",
        ))
        .bind(uuid)
        .bind(tag_type)
        .fetch_optional(self.pool)
        .await?;
        Ok(latest.filter(|e| {
            e.kind == "tag_set" && e.expires_at.map(|exp| exp > Utc::now()).unwrap_or(true)
        }))
    }

    pub async fn get_tag_history(&self, uuid: &str) -> Result<Vec<PlayerEvent>, sqlx::Error> {
        sqlx::query_as(&format!(
            "SELECT {COLS} FROM player_events
             WHERE uuid = $1
             ORDER BY ts ASC",
        ))
        .bind(uuid)
        .fetch_all(self.pool)
        .await
    }

    pub async fn get_event_by_id(&self, id: i64) -> Result<Option<PlayerEvent>, sqlx::Error> {
        sqlx::query_as(&format!("SELECT {COLS} FROM player_events WHERE id = $1",))
            .bind(id)
            .fetch_optional(self.pool)
            .await
    }

    pub async fn get_active_expiring_tags(&self) -> Result<Vec<PlayerEvent>, sqlx::Error> {
        sqlx::query_as(&format!(
            "SELECT {COLS} FROM (
                 SELECT DISTINCT ON (uuid, tag_type) {COLS}
                 FROM player_events
                 WHERE kind IN ('tag_set', 'tag_clear')
                 ORDER BY uuid, tag_type, ts DESC, id DESC
             ) latest
             WHERE kind = 'tag_set'
               AND expires_at IS NOT NULL",
        ))
        .fetch_all(self.pool)
        .await
    }

    pub async fn count_active_tags(&self) -> Result<i64, sqlx::Error> {
        let (count,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM (
                 SELECT DISTINCT ON (uuid, tag_type) kind, expires_at
                 FROM player_events
                 WHERE kind IN ('tag_set', 'tag_clear')
                 ORDER BY uuid, tag_type, ts DESC, id DESC
             ) latest
             WHERE kind = 'tag_set'
               AND (expires_at IS NULL OR expires_at > NOW())",
        )
        .fetch_one(self.pool)
        .await?;
        Ok(count)
    }

    pub async fn count_active_tags_by_type(&self) -> Result<Vec<(String, i64)>, sqlx::Error> {
        sqlx::query_as(
            "SELECT tag_type, COUNT(*) as count FROM (
                 SELECT DISTINCT ON (uuid, tag_type) tag_type, kind, expires_at
                 FROM player_events
                 WHERE kind IN ('tag_set', 'tag_clear')
                 ORDER BY uuid, tag_type, ts DESC, id DESC
             ) latest
             WHERE kind = 'tag_set'
               AND (expires_at IS NULL OR expires_at > NOW())
             GROUP BY tag_type ORDER BY count DESC",
        )
        .fetch_all(self.pool)
        .await
    }

    pub async fn count_players(&self) -> Result<i64, sqlx::Error> {
        let (count,): (i64,) = sqlx::query_as("SELECT COUNT(DISTINCT uuid) FROM player_events")
            .fetch_one(self.pool)
            .await?;
        Ok(count)
    }

    pub async fn top_taggers(
        &self,
        since: Option<DateTime<Utc>>,
        limit: i64,
    ) -> Result<Vec<(i64, i64)>, sqlx::Error> {
        match since {
            Some(ts) => {
                sqlx::query_as(
                    "SELECT author, COUNT(*) FROM player_events
                 WHERE kind = 'tag_set' AND author IS NOT NULL AND author <> 0 AND ts >= $1
                 GROUP BY author ORDER BY COUNT(*) DESC LIMIT $2",
                )
                .bind(ts)
                .bind(limit)
                .fetch_all(self.pool)
                .await
            }
            None => {
                sqlx::query_as(
                    "SELECT author, COUNT(*) FROM player_events
                 WHERE kind = 'tag_set' AND author IS NOT NULL AND author <> 0
                 GROUP BY author ORDER BY COUNT(*) DESC LIMIT $1",
                )
                .bind(limit)
                .fetch_all(self.pool)
                .await
            }
        }
    }

    pub async fn count_events_by_author(&self, author: i64) -> Result<i64, sqlx::Error> {
        let (count,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM player_events
             WHERE author = $1 AND kind = 'tag_set'",
        )
        .bind(author)
        .fetch_one(self.pool)
        .await?;
        Ok(count)
    }

    pub async fn get_players_batch(
        &self,
        uuids: &[String],
    ) -> Result<Vec<(String, Vec<PlayerEvent>)>, sqlx::Error> {
        let tags: Vec<PlayerEvent> = sqlx::query_as(&format!(
            "SELECT {COLS} FROM (
                 SELECT DISTINCT ON (uuid, tag_type) {COLS}
                 FROM player_events
                 WHERE uuid = ANY($1) AND kind IN ('tag_set', 'tag_clear')
                 ORDER BY uuid, tag_type, ts DESC, id DESC
             ) latest
             WHERE kind = 'tag_set'
               AND (expires_at IS NULL OR expires_at > NOW())
             ORDER BY uuid, ts DESC, id DESC",
        ))
        .bind(uuids)
        .fetch_all(self.pool)
        .await?;

        Ok(uuids
            .iter()
            .map(|uuid| {
                let player_tags = tags.iter().filter(|t| &t.uuid == uuid).cloned().collect();
                (uuid.clone(), player_tags)
            })
            .collect())
    }
}

async fn acquire_lock(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    uuid: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
        .bind(uuid)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn latest_active(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    uuid: &str,
    tag_type: &str,
) -> Result<Option<PlayerEvent>, sqlx::Error> {
    let latest: Option<PlayerEvent> = sqlx::query_as(&format!(
        "SELECT {COLS} FROM player_events
         WHERE uuid = $1 AND tag_type = $2 AND kind IN ('tag_set', 'tag_clear')
         ORDER BY ts DESC, id DESC LIMIT 1",
    ))
    .bind(uuid)
    .bind(tag_type)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(latest.filter(|e| {
        e.kind == "tag_set" && e.expires_at.map(|exp| exp > Utc::now()).unwrap_or(true)
    }))
}

async fn active_conflict(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    uuid: &str,
    types: &[String],
) -> Result<Option<PlayerEvent>, sqlx::Error> {
    sqlx::query_as(&format!(
        "SELECT {COLS} FROM (
             SELECT DISTINCT ON (tag_type) {COLS}
             FROM player_events
             WHERE uuid = $1 AND tag_type = ANY($2) AND kind IN ('tag_set', 'tag_clear')
             ORDER BY tag_type, ts DESC, id DESC
         ) latest
         WHERE kind = 'tag_set'
           AND (expires_at IS NULL OR expires_at > NOW())
         LIMIT 1",
    ))
    .bind(uuid)
    .bind(types)
    .fetch_optional(&mut **tx)
    .await
}

async fn current_lock_kind(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    uuid: &str,
) -> Result<Option<String>, sqlx::Error> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT kind FROM player_events
         WHERE uuid = $1 AND kind IN ('lock', 'unlock')
         ORDER BY ts DESC, id DESC LIMIT 1",
    )
    .bind(uuid)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(row.map(|(k,)| k))
}
