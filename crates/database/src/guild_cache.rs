use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use futures_util::TryStreamExt;
use serde_json::{Map, Value};
use sqlx::{FromRow, PgPool};

use crate::cache::{calculate_delta, deep_merge_mut};

const RECONSTRUCTION_THRESHOLD: Duration = Duration::from_millis(2);

#[derive(FromRow)]
struct GuildSnapshotRow {
    is_baseline: bool,
    data: Value,
    timestamp: DateTime<Utc>,
}

pub struct GuildCacheRepository<'a> {
    pool: &'a PgPool,
}

impl<'a> GuildCacheRepository<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn store_snapshot(&self, guild_id: &str, raw: &Value) -> Result<bool, sqlx::Error> {
        let normalized = keyed_members(raw);

        let Some(baseline) = self.latest_baseline(guild_id).await? else {
            self.insert(guild_id, &normalized, true).await?;
            return Ok(true);
        };

        let (current, reconstruct_time) = self
            .reconstruct(guild_id, baseline.timestamp, baseline.data, Utc::now())
            .await?;
        match calculate_delta(&current, &normalized) {
            None => Ok(false),
            Some(delta) => {
                if reconstruct_time > RECONSTRUCTION_THRESHOLD {
                    self.insert(guild_id, &normalized, true).await?;
                } else {
                    self.insert(guild_id, &delta, false).await?;
                }
                Ok(true)
            }
        }
    }

    pub async fn get_current(&self, guild_id: &str) -> Result<Option<Value>, sqlx::Error> {
        self.get_at(guild_id, Utc::now()).await
    }

    pub async fn get_at(
        &self,
        guild_id: &str,
        at: DateTime<Utc>,
    ) -> Result<Option<Value>, sqlx::Error> {
        let baseline: Option<GuildSnapshotRow> = sqlx::query_as(
            "SELECT is_baseline, data, timestamp FROM guild_snapshots
             WHERE guild_id = $1 AND is_baseline = true AND timestamp <= $2
             ORDER BY timestamp DESC LIMIT 1",
        )
        .bind(guild_id)
        .bind(at)
        .fetch_optional(self.pool)
        .await?;

        let Some(baseline) = baseline else {
            return Ok(None);
        };
        let (reconstructed, _) = self
            .reconstruct(guild_id, baseline.timestamp, baseline.data, at)
            .await?;
        Ok(Some(array_members(&reconstructed)))
    }

    async fn latest_baseline(
        &self,
        guild_id: &str,
    ) -> Result<Option<GuildSnapshotRow>, sqlx::Error> {
        sqlx::query_as(
            "SELECT is_baseline, data, timestamp FROM guild_snapshots
             WHERE guild_id = $1 AND is_baseline = true
             ORDER BY timestamp DESC LIMIT 1",
        )
        .bind(guild_id)
        .fetch_optional(self.pool)
        .await
    }

    async fn reconstruct(
        &self,
        guild_id: &str,
        baseline_ts: DateTime<Utc>,
        baseline_data: Value,
        until: DateTime<Utc>,
    ) -> Result<(Value, Duration), sqlx::Error> {
        let mut stream = sqlx::query_as::<_, (Value,)>(
            "SELECT data FROM guild_snapshots
             WHERE guild_id = $1 AND is_baseline = false AND timestamp > $2 AND timestamp <= $3
             ORDER BY timestamp ASC",
        )
        .bind(guild_id)
        .bind(baseline_ts)
        .bind(until)
        .fetch(self.pool);

        let mut current = baseline_data;
        let mut merge_time = Duration::ZERO;
        while let Some((delta,)) = stream.try_next().await? {
            let start = Instant::now();
            deep_merge_mut(&mut current, &delta);
            merge_time += start.elapsed();
        }
        Ok((current, merge_time))
    }

    async fn insert(
        &self,
        guild_id: &str,
        data: &Value,
        is_baseline: bool,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO guild_snapshots (guild_id, is_baseline, data) VALUES ($1, $2, $3)",
        )
        .bind(guild_id)
        .bind(is_baseline)
        .bind(data)
        .execute(self.pool)
        .await?;
        Ok(())
    }
}

fn keyed_members(raw: &Value) -> Value {
    let mut obj = raw.as_object().cloned().unwrap_or_default();
    if let Some(Value::Array(members)) = obj.get("members") {
        let keyed: Map<String, Value> = members
            .iter()
            .filter_map(|m| Some((m.get("uuid")?.as_str()?.to_string(), m.clone())))
            .collect();
        obj.insert("members".into(), Value::Object(keyed));
    }
    Value::Object(obj)
}

fn array_members(normalized: &Value) -> Value {
    let mut obj = normalized.as_object().cloned().unwrap_or_default();
    if let Some(Value::Object(members)) = obj.get("members") {
        obj.insert(
            "members".into(),
            Value::Array(members.values().cloned().collect()),
        );
    }
    Value::Object(obj)
}
