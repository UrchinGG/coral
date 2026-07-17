use axum::http::StatusCode;
use axum::{Extension, Json, Router, extract::*, routing::get, routing::post};
use chrono::{DateTime, Duration, Utc};
use coral_redis::{RateLimiter, SESSION_RATE_LIMIT, SESSION_UUID_BUDGET};
use database::{FlagDismissal, FlagDismissalRepository};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::FromRow;

use super::requests::hypixel_ratelimits;
use crate::auth::AdminActor;
use crate::identity;
use crate::state::AppState;

const DISMISS_WINDOW: Duration = Duration::hours(24);
const BUDGET_THRESHOLD: f64 = 0.8;
const PROBE_THRESHOLD: i64 = 10;
const SPIKE_MULTIPLIER: f64 = 3.0;
const SPIKE_BASELINE_FLOOR: f64 = 20.0;
const HEADROOM_THRESHOLD: f64 = 0.2;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(overview))
        .route("/dismiss", post(dismiss))
}

#[derive(Serialize, Clone)]
struct Flag {
    flag_key: String,
    kind: &'static str,
    summary: String,
    #[serde(serialize_with = "crate::serde_id::discord_id_opt")]
    discord_id: Option<i64>,
    discord_username: Option<String>,
    member_id: Option<i64>,
}

#[derive(Serialize, FromRow)]
struct PluginChangeRow {
    slug: String,
    kind: String,
    reason: Option<String>,
    at: DateTime<Utc>,
}

#[derive(Serialize)]
struct OverviewResponse {
    flags: Vec<Flag>,
    recent_plugin_changes: Vec<PluginChangeRow>,
}

async fn overview(State(state): State<AppState>) -> Json<OverviewResponse> {
    let dismissals = FlagDismissalRepository::new(state.db.pool())
        .list_all()
        .await
        .unwrap_or_default();
    let now = Utc::now();

    let (budget, probe, spike, headroom) = tokio::join!(
        budget_flags(&state),
        probe_flags(&state),
        spike_flags(&state),
        headroom_flag(&state),
    );
    let mut flags: Vec<Flag> = budget
        .into_iter()
        .chain(probe)
        .chain(spike)
        .chain(headroom)
        .collect();
    flags.retain(|f| !is_dismissed(&dismissals, &f.flag_key, now));

    let discord_ids: Vec<i64> = flags.iter().filter_map(|f| f.discord_id).collect();
    let (names, member_ids) = tokio::join!(
        identity::resolve_discord_usernames(&state, &discord_ids),
        identity::member_ids_by_discord_id(&state, &discord_ids),
    );
    for f in &mut flags {
        if let Some(id) = f.discord_id {
            f.discord_username = names.get(&id).cloned();
            f.member_id = member_ids.get(&id).copied();
        }
    }

    let recent_plugin_changes = recent_plugin_changes(&state).await;

    Json(OverviewResponse {
        flags,
        recent_plugin_changes,
    })
}

fn is_dismissed(dismissals: &[FlagDismissal], flag_key: &str, now: DateTime<Utc>) -> bool {
    dismissals
        .iter()
        .any(|d| d.flag_key == flag_key && d.dismissed_until > now)
}

fn is_budget_flagged(utilization: f64) -> bool {
    utilization >= BUDGET_THRESHOLD
}

async fn budget_flags(state: &AppState) -> Vec<Flag> {
    let Some(redis) = &state.redis else {
        return vec![];
    };
    let limiter = RateLimiter::new(redis.clone());
    let (session, uuid_batch) = tokio::join!(
        limiter.scan_budgets("sf:", SESSION_RATE_LIMIT),
        limiter.scan_budgets("sfuuids:", SESSION_UUID_BUDGET),
    );

    let mut worst: std::collections::HashMap<i64, f64> = std::collections::HashMap::new();
    for usage in session.unwrap_or_default() {
        if let Some(id) = usage
            .name
            .strip_prefix("sf:")
            .and_then(|s| s.parse::<i64>().ok())
        {
            let u = usage.utilization();
            worst.entry(id).and_modify(|w| *w = w.max(u)).or_insert(u);
        }
    }
    for usage in uuid_batch.unwrap_or_default() {
        if let Some(id) = usage
            .name
            .strip_prefix("sfuuids:")
            .and_then(|s| s.parse::<i64>().ok())
        {
            let u = usage.utilization();
            worst.entry(id).and_modify(|w| *w = w.max(u)).or_insert(u);
        }
    }

    worst
        .into_iter()
        .filter(|(_, u)| is_budget_flagged(*u))
        .map(|(discord_id, utilization)| Flag {
            flag_key: format!("budget:{discord_id}"),
            kind: "budget",
            summary: format!(
                "at {:.0}% of keyless-auth rate-limit budget",
                utilization * 100.0
            ),
            discord_id: Some(discord_id),
            discord_username: None,
            member_id: None,
        })
        .collect()
}

#[derive(FromRow)]
struct ProbeRow {
    discord_id: Option<i64>,
    ip: Option<String>,
    count: i64,
}

async fn probe_flags(state: &AppState) -> Vec<Flag> {
    let rows: Vec<ProbeRow> = sqlx::query_as(
        "SELECT m.discord_id,
                CASE WHEN m.discord_id IS NULL THEN l.ip ELSE NULL END AS ip,
                count(*) AS count
         FROM api_request_log l
         LEFT JOIN members m ON m.id = l.member_id
         WHERE l.ts > now() - interval '24 hours'
           AND l.status IN (401, 403)
           AND l.path ILIKE '%starfish%'
         GROUP BY m.discord_id, CASE WHEN m.discord_id IS NULL THEN l.ip ELSE NULL END
         HAVING count(*) >= $1",
    )
    .bind(PROBE_THRESHOLD)
    .fetch_all(state.db.pool())
    .await
    .unwrap_or_default();

    rows.into_iter()
        .map(|r| match r.discord_id {
            Some(discord_id) => Flag {
                flag_key: format!("probe:{discord_id}"),
                kind: "probe",
                summary: format!(
                    "{} failed auth attempts on Starfish endpoints in 24h",
                    r.count
                ),
                discord_id: Some(discord_id),
                discord_username: None,
                member_id: None,
            },
            None => {
                let ip = r.ip.unwrap_or_else(|| "unknown".into());
                Flag {
                    flag_key: format!("probe:ip:{ip}"),
                    kind: "probe",
                    summary: format!(
                        "{} failed auth attempts from {ip} in 24h (no identity resolved)",
                        r.count
                    ),
                    discord_id: None,
                    discord_username: None,
                    member_id: None,
                }
            }
        })
        .collect()
}

#[derive(FromRow)]
struct SpikeRow {
    discord_id: i64,
    today_count: i64,
    baseline_count: i64,
}

fn is_spike(today_count: i64, baseline_daily_avg: f64) -> bool {
    baseline_daily_avg >= SPIKE_BASELINE_FLOOR
        && today_count as f64 >= SPIKE_MULTIPLIER * baseline_daily_avg
}

async fn spike_flags(state: &AppState) -> Vec<Flag> {
    let rows: Vec<SpikeRow> = sqlx::query_as(
        "SELECT m.discord_id,
                count(*) FILTER (WHERE l.ts > now() - interval '1 day') AS today_count,
                count(*) FILTER (WHERE l.ts <= now() - interval '1 day' AND l.ts > now() - interval '8 days') AS baseline_count
         FROM api_request_log l
         JOIN members m ON m.id = l.member_id
         WHERE l.ts > now() - interval '8 days'
         GROUP BY m.discord_id",
    )
    .fetch_all(state.db.pool())
    .await
    .unwrap_or_default();

    let today = Utc::now().format("%Y-%m-%d").to_string();
    rows.into_iter()
        .filter_map(|r| {
            let baseline_avg = r.baseline_count as f64 / 7.0;
            is_spike(r.today_count, baseline_avg).then(|| Flag {
                flag_key: format!("spike:{}:{today}", r.discord_id),
                kind: "spike",
                summary: format!(
                    "{} requests today vs a {baseline_avg:.0}/day baseline",
                    r.today_count
                ),
                discord_id: Some(r.discord_id),
                discord_username: None,
                member_id: None,
            })
        })
        .collect()
}

async fn headroom_flag(state: &AppState) -> Vec<Flag> {
    let rl = hypixel_ratelimits(state).await;
    match rl.headroom_ratio() {
        Some(ratio) if ratio < HEADROOM_THRESHOLD => vec![Flag {
            flag_key: "hypixel_headroom".to_string(),
            kind: "hypixel_headroom",
            summary: format!(
                "Hypixel headroom down to {:.0}% ({} / {} used)",
                ratio * 100.0,
                rl.used,
                rl.capacity
            ),
            discord_id: None,
            discord_username: None,
            member_id: None,
        }],
        _ => vec![],
    }
}

async fn recent_plugin_changes(state: &AppState) -> Vec<PluginChangeRow> {
    sqlx::query_as(
        "(SELECT slug, 'disabled' AS kind, disabled_reason AS reason, disabled_at AS at
          FROM plugins WHERE disabled AND disabled_at IS NOT NULL)
         UNION ALL
         (SELECT slug, 'unlisted' AS kind, NULL::text AS reason, unlisted_at AS at
          FROM plugins WHERE unlisted AND unlisted_at IS NOT NULL)
         ORDER BY at DESC LIMIT 20",
    )
    .fetch_all(state.db.pool())
    .await
    .unwrap_or_default()
}

#[derive(Serialize)]
struct OkResponse {
    ok: bool,
}

#[derive(Deserialize)]
struct DismissRequest {
    flag_key: String,
}

async fn dismiss(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Json(req): Json<DismissRequest>,
) -> Result<Json<OkResponse>, StatusCode> {
    let until = Utc::now() + DISMISS_WINDOW;
    FlagDismissalRepository::new(state.db.pool())
        .dismiss(&req.flag_key, until)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    crate::audit::log(
        &state,
        actor.discord_id,
        "dismiss_flag",
        &req.flag_key,
        json!({}),
    )
    .await;
    Ok(Json(OkResponse { ok: true }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_flag_threshold_is_inclusive() {
        assert!(!is_budget_flagged(0.79));
        assert!(is_budget_flagged(0.8));
        assert!(is_budget_flagged(0.95));
    }

    #[test]
    fn spike_requires_baseline_floor() {
        assert!(!is_spike(100, 19.9));
        assert!(is_spike(60, 20.0));
        assert!(!is_spike(59, 20.0));
    }

    #[test]
    fn spike_requires_triple_baseline() {
        assert!(!is_spike(59, 20.0));
        assert!(is_spike(60, 20.0));
        assert!(is_spike(1000, 20.0));
    }

    #[test]
    fn spike_ignores_negligible_baselines() {
        assert!(!is_spike(6, 2.0));
        assert!(!is_spike(1000, 0.0));
    }

    #[test]
    fn dismissed_flag_is_filtered_while_active() {
        let now = Utc::now();
        let dismissals = vec![FlagDismissal {
            flag_key: "budget:1".into(),
            dismissed_until: now + Duration::hours(1),
        }];
        assert!(is_dismissed(&dismissals, "budget:1", now));
        assert!(!is_dismissed(&dismissals, "budget:2", now));
    }

    #[test]
    fn dismissal_expires_after_window() {
        let now = Utc::now();
        let dismissals = vec![FlagDismissal {
            flag_key: "budget:1".into(),
            dismissed_until: now - Duration::seconds(1),
        }];
        assert!(!is_dismissed(&dismissals, "budget:1", now));
    }

    #[test]
    fn dismissal_boundary_is_exclusive() {
        let now = Utc::now();
        let dismissals = vec![FlagDismissal {
            flag_key: "budget:1".into(),
            dismissed_until: now,
        }];
        assert!(!is_dismissed(&dismissals, "budget:1", now));
    }
}
