use std::sync::LazyLock;

use chrono::{DateTime, TimeDelta, Utc};
use clients::FlagLevel;
use sqlx::{FromRow, PgPool};

use super::{Flag, FlagKind, grouped};
use crate::coverage::Coverage;
use crate::error::{AdminError, Context};
use crate::refs::EntityRef;

pub const PROBE_THRESHOLD: i64 = 10;
pub const PROBE_SPAN: TimeDelta = TimeDelta::hours(24);
pub const STARFISH_ROUTE_PREFIX: &str = "/api/v1/starfish/";
pub const AUTH_FAILURES: [i16; 2] = [401, 403];

static STARFISH_ROUTES: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    reqlog::ROUTE_TEMPLATES
        .iter()
        .copied()
        .filter(|route| route.starts_with(STARFISH_ROUTE_PREFIX))
        .collect()
});

#[derive(Debug, FromRow)]
struct ProbeRow {
    member_id: Option<i64>,
    discord_id: Option<i64>,
    ip: Option<String>,
    attempts: i64,
}

pub async fn flags(
    pool: &PgPool,
    coverage: &Coverage,
    now: DateTime<Utc>,
) -> Result<Vec<Flag>, AdminError> {
    let Some(attributed_from) = coverage.attribution_from else {
        return Ok(Vec::new());
    };
    let rows: Vec<ProbeRow> = sqlx::query_as(
        "SELECT l.member_id, m.discord_id,
                CASE WHEN l.member_id IS NULL THEN l.ip END AS ip,
                count(*)::bigint AS attempts
         FROM api_request_log l
         LEFT JOIN members m ON m.id = l.member_id
         WHERE l.route = ANY($1) AND l.ts >= $2
           AND coalesce(l.outcome_status, l.status) = ANY($3)
         GROUP BY 1, 2, 3
         HAVING count(*) >= $4",
    )
    .bind(&*STARFISH_ROUTES)
    .bind((now - PROBE_SPAN).max(attributed_from))
    .bind(AUTH_FAILURES)
    .bind(PROBE_THRESHOLD)
    .fetch_all(pool)
    .await
    .context("count failed Starfish sign-ins")?;
    Ok(rows.into_iter().filter_map(probe_flag).collect())
}

fn probe_flag(row: ProbeRow) -> Option<Flag> {
    let attempts = grouped(row.attempts);
    if let (Some(member_id), Some(discord_id)) = (row.member_id, row.discord_id) {
        let flag = Flag::new(
            FlagKind::Probe,
            FlagLevel::Danger,
            format!("probe:{discord_id}"),
            format!("{attempts} failed auth attempts on Starfish endpoints in 24 h"),
        );
        return Some(flag.with_refs(vec![EntityRef::Account {
            discord_id,
            member_id: Some(member_id),
        }]));
    }
    let ip = row.ip?;
    let flag = Flag::new(
        FlagKind::Probe,
        FlagLevel::Danger,
        format!("probe:ip:{ip}"),
        format!(
            "{attempts} failed auth attempts on Starfish endpoints in 24 h from one IP with no account"
        ),
    );
    Some(flag.with_refs(vec![EntityRef::Ip {
        ip,
        country: None,
        proxied: false,
    }]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_starfish_templates_are_watched() {
        assert!(!STARFISH_ROUTES.is_empty());
        assert!(STARFISH_ROUTES.contains(&"/api/v1/starfish/auth/poll"));
        assert!(
            STARFISH_ROUTES
                .iter()
                .all(|route| route.starts_with(STARFISH_ROUTE_PREFIX))
        );
    }

    #[test]
    fn probes_name_the_account_or_else_the_ip() {
        let account = probe_flag(ProbeRow {
            member_id: Some(5),
            discord_id: Some(42),
            ip: None,
            attempts: 12,
        })
        .unwrap();
        assert_eq!(account.key, "probe:42");
        let anonymous = probe_flag(ProbeRow {
            member_id: None,
            discord_id: None,
            ip: Some("2001:db8::3".into()),
            attempts: 37,
        })
        .unwrap();
        assert_eq!(anonymous.key, "probe:ip:2001:db8::3");
        assert!(matches!(anonymous.refs[0], EntityRef::Ip { .. }));
        let unknown = ProbeRow {
            member_id: None,
            discord_id: None,
            ip: None,
            attempts: 40,
        };
        assert!(probe_flag(unknown).is_none());
    }
}
