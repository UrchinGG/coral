use chrono::{DateTime, TimeDelta, Utc};
use database::Database;
use reqlog::ROUTE_TEMPLATES;
use serde::Serialize;
use sqlx::FromRow;

use crate::error::{AdminError, Context};

pub const ROUTE_COUNT_SPAN: TimeDelta = TimeDelta::hours(24);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, FromRow)]
pub struct RouteCount {
    pub route: String,
    pub method: Option<String>,
    pub requests_24h: i64,
}

pub async fn read_route_counts(
    db: &Database,
    now: DateTime<Utc>,
) -> Result<Vec<RouteCount>, AdminError> {
    let counted: Vec<RouteCount> = sqlx::query_as(
        "SELECT route, method, sum(requests)::int8 AS requests_24h
         FROM api_traffic_hour
         WHERE hour >= $1 AND route IS NOT NULL
         GROUP BY route, method
         ORDER BY requests_24h DESC, route, method",
    )
    .bind(now - ROUTE_COUNT_SPAN)
    .fetch_all(db.pool())
    .await
    .context("count requests per route over the last 24 hours")?;
    Ok(with_quiet_templates(counted))
}

fn with_quiet_templates(mut counted: Vec<RouteCount>) -> Vec<RouteCount> {
    let quiet: Vec<RouteCount> = ROUTE_TEMPLATES
        .iter()
        .filter(|template| !counted.iter().any(|count| count.route == **template))
        .map(|template| RouteCount {
            route: template.to_string(),
            method: None,
            requests_24h: 0,
        })
        .collect();
    counted.extend(quiet);
    counted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_without_traffic_follow_the_counted_routes() {
        let counted = vec![RouteCount {
            route: "/v3/player/tags".into(),
            method: Some("GET".into()),
            requests_24h: 12,
        }];
        let routes = with_quiet_templates(counted.clone());
        assert_eq!(routes[0], counted[0]);
        assert_eq!(routes.len(), ROUTE_TEMPLATES.len());
        assert!(
            routes[1..]
                .iter()
                .all(|route| route.requests_24h == 0 && route.method.is_none())
        );
        assert!(
            !routes[1..]
                .iter()
                .any(|route| route.route == "/v3/player/tags")
        );
    }
}
