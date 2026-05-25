use std::time::Instant;

use axum::{Json, Router, extract::*, routing::get};
use database::reconstruct;
use serde::Serialize;
use serde_json::Value;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(diagnostics))
}

#[derive(Serialize)]
struct DiagnosticsResponse {
    storage: StorageStats,
    players: Vec<PlayerCacheStats>,
}

#[derive(Serialize)]
struct StorageStats {
    total_snapshots: i64,
    total_baselines: i64,
    total_deltas: i64,
    total_players: i64,
    total_promotions: i64,
    avg_deltas_per_baseline: f64,
    storage_efficiency: f64,
}

#[derive(Serialize)]
struct PlayerCacheStats {
    uuid: String,
    username: Option<String>,
    baseline_count: i64,
    delta_count: i64,
    latest_baseline_age_hours: Option<f64>,
    reconstruct_time_us: Option<i64>,
    delta_chain_length: i64,
}

async fn diagnostics(State(state): State<AppState>) -> Json<DiagnosticsResponse> {
    let pool = state.db.pool();

    let total_snapshots: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM player_snapshots")
        .fetch_one(pool)
        .await
        .unwrap_or(0);
    let total_baselines: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM player_snapshots WHERE is_baseline = true")
            .fetch_one(pool)
            .await
            .unwrap_or(0);
    let total_deltas: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM player_snapshots WHERE is_baseline = false")
            .fetch_one(pool)
            .await
            .unwrap_or(0);
    let total_players: i64 =
        sqlx::query_scalar("SELECT COUNT(DISTINCT uuid) FROM player_snapshots")
            .fetch_one(pool)
            .await
            .unwrap_or(0);
    let total_promotions: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM player_snapshots WHERE source = 'promotion'")
            .fetch_one(pool)
            .await
            .unwrap_or(0);

    let avg_deltas_per_baseline = if total_baselines > 0 {
        total_deltas as f64 / total_baselines as f64
    } else {
        0.0
    };
    let storage_efficiency = if total_snapshots > 0 {
        (total_deltas as f64 / total_snapshots as f64) * 100.0
    } else {
        0.0
    };

    let player_rows: Vec<(String, Option<String>, i64, i64)> = sqlx::query_as(
        r#"SELECT uuid, MAX(username) as username,
                  SUM(CASE WHEN is_baseline THEN 1 ELSE 0 END) as baseline_count,
                  SUM(CASE WHEN NOT is_baseline THEN 1 ELSE 0 END) as delta_count
           FROM player_snapshots
           GROUP BY uuid ORDER BY delta_count DESC LIMIT 50"#,
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let mut players = Vec::with_capacity(player_rows.len());
    for (uuid, username, baseline_count, delta_count) in player_rows {
        let baseline_age: Option<(f64,)> = sqlx::query_as(
            r#"SELECT EXTRACT(EPOCH FROM (NOW() - timestamp)) / 3600.0
               FROM player_snapshots
               WHERE uuid = $1 AND is_baseline = true
               ORDER BY timestamp DESC LIMIT 1"#,
        )
        .bind(&uuid)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();

        let (reconstruct_time_us, delta_chain_length) = measure_reconstruction(pool, &uuid).await;
        players.push(PlayerCacheStats {
            uuid,
            username,
            baseline_count,
            delta_count,
            latest_baseline_age_hours: baseline_age.map(|r| r.0),
            reconstruct_time_us,
            delta_chain_length,
        });
    }

    Json(DiagnosticsResponse {
        storage: StorageStats {
            total_snapshots,
            total_baselines,
            total_deltas,
            total_players,
            total_promotions,
            avg_deltas_per_baseline,
            storage_efficiency,
        },
        players,
    })
}

async fn measure_reconstruction(pool: &sqlx::PgPool, uuid: &str) -> (Option<i64>, i64) {
    let baseline: Option<(Value, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
        r#"SELECT data, timestamp FROM player_snapshots
           WHERE uuid = $1 AND is_baseline = true
           ORDER BY timestamp DESC LIMIT 1"#,
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    let Some((baseline_data, baseline_ts)) = baseline else {
        return (None, 0);
    };

    let delta_rows: Vec<(Value,)> = sqlx::query_as(
        r#"SELECT data FROM player_snapshots
           WHERE uuid = $1 AND is_baseline = false AND timestamp > $2
           ORDER BY timestamp ASC"#,
    )
    .bind(uuid)
    .bind(baseline_ts)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    if delta_rows.is_empty() {
        return (Some(0), 0);
    }

    let chain_len = delta_rows.len() as i64;
    let deltas: Vec<Value> = delta_rows.into_iter().map(|r| r.0).collect();

    let start = Instant::now();
    let _ = reconstruct(&baseline_data, &deltas);
    (Some(start.elapsed().as_micros() as i64), chain_len)
}
