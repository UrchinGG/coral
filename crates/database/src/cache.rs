use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use futures_util::TryStreamExt;
use serde_json::{Map, Value};
use sqlx::{FromRow, PgPool};

const RECONSTRUCTION_THRESHOLD: Duration = Duration::from_millis(10);

pub enum SnapshotResult {
    Stored(i64),
    NoChange,
}

#[derive(Debug, FromRow)]
#[allow(dead_code)]
struct SnapshotRow {
    id: i64,
    is_baseline: bool,
    data: Value,
    timestamp: DateTime<Utc>,
}

pub struct CacheRepository<'a> {
    pool: &'a PgPool,
}

impl<'a> CacheRepository<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn count_snapshots(&self) -> Result<i64, sqlx::Error> {
        let (count,): (i64,) = sqlx::query_as(
            "SELECT reltuples::bigint FROM pg_class WHERE relname = 'player_snapshots'",
        )
        .fetch_one(self.pool)
        .await?;
        Ok(count)
    }

    pub async fn count_unique_players(&self) -> Result<i64, sqlx::Error> {
        let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM players")
            .fetch_one(self.pool)
            .await?;
        Ok(count)
    }

    pub async fn usernames(
        &self,
        uuids: &[String],
    ) -> Result<std::collections::HashMap<String, String>, sqlx::Error> {
        let rows: Vec<(String, Option<String>)> = sqlx::query_as(
            "SELECT DISTINCT ON (uuid) uuid, username FROM player_snapshots
             WHERE uuid = ANY($1) ORDER BY uuid, timestamp DESC",
        )
        .bind(uuids)
        .fetch_all(self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .filter_map(|(uuid, name)| name.map(|name| (uuid, name)))
            .collect())
    }

    async fn register_player(&self, uuid: &str) -> Result<(), sqlx::Error> {
        sqlx::query("INSERT INTO players (uuid) VALUES ($1) ON CONFLICT DO NOTHING")
            .bind(uuid)
            .execute(self.pool)
            .await?;
        Ok(())
    }

    pub async fn storage_bytes(&self) -> Result<i64, sqlx::Error> {
        let (size,): (i64,) =
            sqlx::query_as("SELECT pg_total_relation_size('player_snapshots')::bigint")
                .fetch_one(self.pool)
                .await?;
        Ok(size)
    }

    pub async fn store_snapshot(
        &self,
        uuid: &str,
        data: &Value,
        discord_id: Option<i64>,
        source: Option<&str>,
        username: Option<&str>,
    ) -> Result<SnapshotResult, sqlx::Error> {
        self.register_player(uuid).await?;
        let latest_baseline = self.get_latest_baseline(uuid).await?;

        let id = match latest_baseline {
            None => {
                self.insert_snapshot(uuid, data, discord_id, source, username, true)
                    .await?
            }
            Some(baseline) => {
                let current = self.reconstruct_current(uuid, &baseline).await?;
                match calculate_delta(&current, data) {
                    None => return Ok(SnapshotResult::NoChange),
                    Some(delta) => {
                        self.insert_snapshot(uuid, &delta, discord_id, source, username, false)
                            .await?
                    }
                }
            }
        };

        self.maybe_promote_to_baseline(uuid, data, username).await?;
        Ok(SnapshotResult::Stored(id))
    }

    pub async fn get_snapshot_at(
        &self,
        uuid: &str,
        timestamp: DateTime<Utc>,
    ) -> Result<Option<Value>, sqlx::Error> {
        let baseline: Option<SnapshotRow> = sqlx::query_as(
            "SELECT id, is_baseline, data, timestamp FROM player_snapshots
             WHERE uuid = $1 AND is_baseline = true AND timestamp <= $2
             ORDER BY timestamp DESC LIMIT 1",
        )
        .bind(uuid)
        .bind(timestamp)
        .fetch_optional(self.pool)
        .await?;

        if let Some(baseline) = baseline {
            let (value, _) = self
                .stream_reconstruct(uuid, baseline.timestamp, baseline.data, timestamp)
                .await?;
            return Ok(Some(value));
        }

        let first: Option<SnapshotRow> = sqlx::query_as(
            "SELECT id, is_baseline, data, timestamp FROM player_snapshots
             WHERE uuid = $1 AND is_baseline = true AND timestamp > $2
             ORDER BY timestamp ASC LIMIT 1",
        )
        .bind(uuid)
        .bind(timestamp)
        .fetch_optional(self.pool)
        .await?;

        Ok(first.map(|b| b.data))
    }

    pub async fn get_latest_snapshot(&self, uuid: &str) -> Result<Option<Value>, sqlx::Error> {
        self.get_snapshot_at(uuid, Utc::now()).await
    }

    pub async fn list_snapshot_timestamps(
        &self,
        uuid: &str,
        before: Option<DateTime<Utc>>,
        after: Option<DateTime<Utc>>,
    ) -> Result<Vec<DateTime<Utc>>, sqlx::Error> {
        sqlx::query_as::<_, (DateTime<Utc>,)>(
            "SELECT timestamp FROM player_snapshots
             WHERE uuid = $1
               AND ($2::timestamptz IS NULL OR timestamp < $2)
               AND ($3::timestamptz IS NULL OR timestamp > $3)
             ORDER BY timestamp ASC",
        )
        .bind(uuid)
        .bind(before)
        .bind(after)
        .fetch_all(self.pool)
        .await
        .map(|rows| rows.into_iter().map(|r| r.0).collect())
    }

    pub async fn get_latest_timestamp(
        &self,
        uuid: &str,
    ) -> Result<Option<DateTime<Utc>>, sqlx::Error> {
        sqlx::query_as::<_, (DateTime<Utc>,)>(
            "SELECT timestamp FROM player_snapshots WHERE uuid = $1
             ORDER BY timestamp DESC LIMIT 1",
        )
        .bind(uuid)
        .fetch_optional(self.pool)
        .await
        .map(|r| r.map(|r| r.0))
    }

    pub async fn get_latest_non_migration_timestamp(
        &self,
        uuid: &str,
    ) -> Result<Option<DateTime<Utc>>, sqlx::Error> {
        sqlx::query_as::<_, (DateTime<Utc>,)>(
            "SELECT timestamp FROM player_snapshots WHERE uuid = $1 AND source != 'migration'
             ORDER BY timestamp DESC LIMIT 1",
        )
        .bind(uuid)
        .fetch_optional(self.pool)
        .await
        .map(|r| r.map(|r| r.0))
    }

    pub async fn resolve_uuid(&self, username: &str) -> Result<Option<String>, sqlx::Error> {
        sqlx::query_as::<_, (String,)>(
            "SELECT uuid FROM player_snapshots WHERE LOWER(username) = $1
             ORDER BY timestamp DESC LIMIT 1",
        )
        .bind(username.to_lowercase())
        .fetch_optional(self.pool)
        .await
        .map(|r| r.map(|r| r.0))
    }

    pub async fn find_by_discord_username(
        &self,
        discord_username: &str,
    ) -> Result<Vec<(String, String)>, sqlx::Error> {
        sqlx::query_as(
            "SELECT DISTINCT ON (uuid) uuid, username FROM player_snapshots
             WHERE is_baseline = true AND username IS NOT NULL
               AND LOWER(data->'socialMedia'->'links'->>'DISCORD') = LOWER($1)
             ORDER BY uuid, timestamp DESC",
        )
        .bind(discord_username)
        .fetch_all(self.pool)
        .await
    }

    pub async fn get_username(&self, uuid: &str) -> Result<Option<String>, sqlx::Error> {
        sqlx::query_as::<_, (Option<String>,)>(
            "SELECT username FROM player_snapshots
             WHERE uuid = $1 AND username IS NOT NULL
             ORDER BY timestamp DESC LIMIT 1",
        )
        .bind(uuid)
        .fetch_optional(self.pool)
        .await
        .map(|r| r.and_then(|r| r.0))
    }

    pub async fn cache_username(&self, uuid: &str, username: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO player_snapshots (uuid, username, is_baseline, data) VALUES ($1, $2, false, '{}'::jsonb)",
        )
        .bind(uuid)
        .bind(username)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_all_snapshots_mapped<T>(
        &self,
        uuid: &str,
        map: impl Fn(&Value) -> Option<T>,
    ) -> Result<Vec<(DateTime<Utc>, T)>, sqlx::Error> {
        let mut stream = sqlx::query_as::<_, SnapshotRow>(
            "SELECT id, is_baseline, data, timestamp FROM player_snapshots
             WHERE uuid = $1 ORDER BY timestamp ASC",
        )
        .bind(uuid)
        .fetch(self.pool);

        let mut results = Vec::new();
        let mut current = Value::Object(Map::new());
        while let Some(row) = stream.try_next().await? {
            if row.is_baseline {
                current = row.data;
            } else {
                deep_merge_mut(&mut current, &row.data);
            }
            if let Some(mapped) = map(&current) {
                results.push((row.timestamp, mapped));
            }
        }
        Ok(results)
    }

    pub async fn get_snapshots_at_times(
        &self,
        uuid: &str,
        timestamps: &[DateTime<Utc>],
    ) -> Result<Vec<Option<(DateTime<Utc>, Value)>>, sqlx::Error> {
        if timestamps.is_empty() {
            return Ok(Vec::new());
        }

        let mut indexed: Vec<(usize, DateTime<Utc>)> =
            timestamps.iter().copied().enumerate().collect();
        indexed.sort_by_key(|(_, ts)| *ts);
        let earliest = indexed[0].1;
        let latest = indexed[indexed.len() - 1].1;

        let Some(baseline) = self.pick_baseline_for(uuid, earliest).await? else {
            return Ok(vec![None; timestamps.len()]);
        };

        let mut stream = sqlx::query_as::<_, SnapshotRow>(
            "SELECT id, is_baseline, data, timestamp FROM player_snapshots
             WHERE uuid = $1 AND timestamp > $2 AND timestamp <= $3
             ORDER BY timestamp ASC",
        )
        .bind(uuid)
        .bind(baseline.timestamp)
        .bind(latest)
        .fetch(self.pool);

        let mut current = baseline.data;
        let mut current_ts = baseline.timestamp;
        let mut results = vec![None; timestamps.len()];
        let mut pending: Option<SnapshotRow> = None;

        for &(orig_idx, target_ts) in &indexed {
            loop {
                let row = match pending.take() {
                    Some(r) => r,
                    None => match stream.try_next().await? {
                        Some(r) => r,
                        None => break,
                    },
                };
                if row.timestamp > target_ts {
                    pending = Some(row);
                    break;
                }
                current_ts = row.timestamp;
                if row.is_baseline {
                    current = row.data;
                } else {
                    deep_merge_mut(&mut current, &row.data);
                }
            }
            results[orig_idx] = Some((current_ts, current.clone()));
        }
        Ok(results)
    }

    async fn pick_baseline_for(
        &self,
        uuid: &str,
        target: DateTime<Utc>,
    ) -> Result<Option<SnapshotRow>, sqlx::Error> {
        let recent: Option<SnapshotRow> = sqlx::query_as(
            "SELECT id, is_baseline, data, timestamp FROM player_snapshots
             WHERE uuid = $1 AND is_baseline = true AND timestamp <= $2
             ORDER BY timestamp DESC LIMIT 1",
        )
        .bind(uuid)
        .bind(target)
        .fetch_optional(self.pool)
        .await?;

        if recent.is_some() {
            return Ok(recent);
        }

        sqlx::query_as(
            "SELECT id, is_baseline, data, timestamp FROM player_snapshots
             WHERE uuid = $1 AND is_baseline = true
             ORDER BY timestamp ASC LIMIT 1",
        )
        .bind(uuid)
        .fetch_optional(self.pool)
        .await
    }

    async fn insert_snapshot(
        &self,
        uuid: &str,
        data: &Value,
        discord_id: Option<i64>,
        source: Option<&str>,
        username: Option<&str>,
        is_baseline: bool,
    ) -> Result<i64, sqlx::Error> {
        let (id,): (i64,) = sqlx::query_as(
            "INSERT INTO player_snapshots (uuid, discord_id, source, username, is_baseline, data)
             VALUES ($1, $2, $3, $4, $5, $6)
             RETURNING id",
        )
        .bind(uuid)
        .bind(discord_id)
        .bind(source)
        .bind(username)
        .bind(is_baseline)
        .bind(data)
        .fetch_one(self.pool)
        .await?;
        Ok(id)
    }

    async fn maybe_promote_to_baseline(
        &self,
        uuid: &str,
        full_data: &Value,
        username: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        let Some(baseline) = self.get_latest_baseline(uuid).await? else {
            return Ok(());
        };
        let (_, merge_time) = self
            .stream_reconstruct(uuid, baseline.timestamp, baseline.data, Utc::now())
            .await?;

        if merge_time > RECONSTRUCTION_THRESHOLD {
            self.insert_snapshot(uuid, full_data, None, Some("promotion"), username, true)
                .await?;
        }
        Ok(())
    }

    async fn get_latest_baseline(&self, uuid: &str) -> Result<Option<SnapshotRow>, sqlx::Error> {
        sqlx::query_as(
            "SELECT id, is_baseline, data, timestamp FROM player_snapshots
             WHERE uuid = $1 AND is_baseline = true
             ORDER BY timestamp DESC LIMIT 1",
        )
        .bind(uuid)
        .fetch_optional(self.pool)
        .await
    }

    async fn reconstruct_current(
        &self,
        uuid: &str,
        baseline: &SnapshotRow,
    ) -> Result<Value, sqlx::Error> {
        let (value, _) = self
            .stream_reconstruct(uuid, baseline.timestamp, baseline.data.clone(), Utc::now())
            .await?;
        Ok(value)
    }

    async fn stream_reconstruct(
        &self,
        uuid: &str,
        baseline_ts: DateTime<Utc>,
        baseline_data: Value,
        until: DateTime<Utc>,
    ) -> Result<(Value, Duration), sqlx::Error> {
        let mut stream = sqlx::query_as::<_, (Value,)>(
            "SELECT data FROM player_snapshots
             WHERE uuid = $1 AND is_baseline = false AND timestamp > $2 AND timestamp <= $3
             ORDER BY timestamp ASC",
        )
        .bind(uuid)
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
}

pub fn calculate_delta(old: &Value, new: &Value) -> Option<Value> {
    match (old, new) {
        (Value::Object(old_map), Value::Object(new_map)) => {
            let delta = calculate_object_delta(old_map, new_map);
            if delta.is_empty() {
                None
            } else {
                Some(Value::Object(delta))
            }
        }
        _ if old == new => None,
        _ => Some(new.clone()),
    }
}

const DELTA_REMOVED: &str = "$removed";

fn calculate_object_delta(
    old: &Map<String, Value>,
    new: &Map<String, Value>,
) -> Map<String, Value> {
    let mut delta = Map::new();
    for (key, new_value) in new {
        match old.get(key) {
            Some(old_value) => {
                if let Some(field_delta) = calculate_delta(old_value, new_value) {
                    delta.insert(key.clone(), field_delta);
                }
            }
            None => {
                delta.insert(key.clone(), new_value.clone());
            }
        }
    }
    let removed: Vec<Value> = old
        .keys()
        .filter(|key| !new.contains_key(key.as_str()))
        .map(|key| Value::String(key.clone()))
        .collect();
    if !removed.is_empty() {
        delta.insert(DELTA_REMOVED.to_string(), Value::Array(removed));
    }
    delta
}

pub fn deep_merge_mut(base: &mut Value, delta: &Value) {
    match (base, delta) {
        (Value::Object(base_map), Value::Object(delta_map)) => {
            for (key, delta_value) in delta_map {
                if key == DELTA_REMOVED {
                    if let Value::Array(removed) = delta_value {
                        for entry in removed {
                            if let Value::String(removed_key) = entry {
                                base_map.remove(removed_key);
                            }
                        }
                    }
                    continue;
                }
                match base_map.get_mut(key) {
                    Some(base_value) => deep_merge_mut(base_value, delta_value),
                    None => {
                        base_map.insert(key.clone(), delta_value.clone());
                    }
                }
            }
        }
        (base, delta) => *base = delta.clone(),
    }
}

pub fn reconstruct(baseline: &Value, deltas: &[Value]) -> Value {
    let mut result = baseline.clone();
    for delta in deltas {
        deep_merge_mut(&mut result, delta);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_calculate_delta_no_change() {
        let old = json!({"a": 1, "b": 2});
        let new = json!({"a": 1, "b": 2});
        assert_eq!(calculate_delta(&old, &new), None);
    }

    #[test]
    fn test_calculate_delta_simple_change() {
        let old = json!({"a": 1, "b": 2});
        let new = json!({"a": 1, "b": 3});
        assert_eq!(calculate_delta(&old, &new), Some(json!({"b": 3})));
    }

    #[test]
    fn test_calculate_delta_nested() {
        let old = json!({"stats": {"kills": 100, "deaths": 50}});
        let new = json!({"stats": {"kills": 105, "deaths": 50}});
        assert_eq!(
            calculate_delta(&old, &new),
            Some(json!({"stats": {"kills": 105}}))
        );
    }

    #[test]
    fn test_deep_merge_mut() {
        let mut base = json!({"a": 1, "b": {"c": 2, "d": 3}});
        let delta = json!({"b": {"c": 5}});
        deep_merge_mut(&mut base, &delta);
        assert_eq!(base, json!({"a": 1, "b": {"c": 5, "d": 3}}));
    }

    #[test]
    fn test_reconstruct() {
        let baseline = json!({"kills": 100, "deaths": 50});
        let deltas = vec![json!({"kills": 105}), json!({"kills": 110, "deaths": 51})];
        let result = reconstruct(&baseline, &deltas);
        assert_eq!(result, json!({"kills": 110, "deaths": 51}));
    }

    #[test]
    fn test_delta_removal_roundtrip() {
        let old = json!({"a": 1, "b": 2, "c": 3});
        let new = json!({"a": 1, "c": 3});
        let delta = calculate_delta(&old, &new).unwrap();
        let mut reconstructed = old.clone();
        deep_merge_mut(&mut reconstructed, &delta);
        assert_eq!(reconstructed, new);
    }

    #[test]
    fn test_delta_nested_removal_roundtrip() {
        let old = json!({"members": {"u1": {"exp": {"d1": 5, "d2": 7}}, "u2": {"exp": {"d1": 3}}}});
        let new = json!({"members": {"u1": {"exp": {"d2": 7, "d3": 9}}}});
        let delta = calculate_delta(&old, &new).unwrap();
        let mut reconstructed = old.clone();
        deep_merge_mut(&mut reconstructed, &delta);
        assert_eq!(reconstructed, new);
    }
}
