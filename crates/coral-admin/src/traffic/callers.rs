use reqlog::{KeyKind, Service};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool, QueryBuilder};

use super::caller::{CALLER_IDENTITY_COLUMNS, CALLER_IDENTITY_JOINS, CallerKey};
use super::class::{errors_predicate, rate_limited_predicate, requests_where};
use super::range::{RangeBounds, TrafficRange};
use super::rows::TrafficFilter;
use crate::error::{AdminError, Context};
use crate::refs::EntityRef;

pub const DEFAULT_CALLERS: i64 = 50;
pub const MAX_CALLERS: i64 = 200;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallerSort {
    #[default]
    Requests,
    RateLimited,
    Errors,
    Subjects,
}

#[derive(Debug, Clone, Serialize)]
pub struct TopCallers {
    #[serde(flatten)]
    pub range: RangeBounds,
    pub callers: Vec<CallerTraffic>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CallerTraffic {
    pub requester: EntityRef,
    pub actor: Option<EntityRef>,
    pub key_kind: KeyKind,
    pub service: Service,
    pub key_prefix: Option<String>,
    pub requests: i64,
    pub rate_limited: i64,
    pub errors: i64,
    pub subjects: i64,
}

#[derive(Debug, FromRow)]
struct CallerRow {
    #[sqlx(flatten)]
    key: CallerKey,
    requests: i64,
    rate_limited: i64,
    errors: i64,
    subjects: i64,
}

impl CallerSort {
    fn column(self) -> &'static str {
        match self {
            Self::Requests => "requests",
            Self::RateLimited => "rate_limited",
            Self::Errors => "errors",
            Self::Subjects => "subjects",
        }
    }
}

pub fn caller_limit(requested: Option<i64>) -> Result<i64, AdminError> {
    let limit = requested.unwrap_or(DEFAULT_CALLERS);
    match (1..=MAX_CALLERS).contains(&limit) {
        true => Ok(limit),
        false => Err(AdminError::BadRequest(format!(
            "limit must be between 1 and {MAX_CALLERS}"
        ))),
    }
}

pub async fn top_callers(
    pool: &PgPool,
    range: TrafficRange,
    filter: TrafficFilter,
    sort: CallerSort,
    limit: i64,
) -> Result<TopCallers, AdminError> {
    let range = range.hourly();
    let order = format!("{} DESC, requests DESC", sort.column());
    let mut query = QueryBuilder::new(format!(
        "WITH grouped AS (SELECT key_kind, member_id, actor_id, key_prefix, service, \
         sum(requests)::bigint AS requests, {} AS rate_limited, {} AS errors, \
         sum(subjects)::bigint AS subjects FROM api_caller_hour WHERE hour >= ",
        requests_where(&rate_limited_predicate()),
        requests_where(&errors_predicate()),
    ));
    query
        .push_bind(range.span.from)
        .push(" AND hour < ")
        .push_bind(range.span.to);
    filter.push_conditions(&mut query);
    query
        .push(format!(
            " GROUP BY key_kind, member_id, actor_id, key_prefix, service ORDER BY {order} LIMIT "
        ))
        .push_bind(limit)
        .push(format!(
            ") SELECT {CALLER_IDENTITY_COLUMNS}, grouped.requests, grouped.rate_limited, \
             grouped.errors, grouped.subjects FROM grouped {CALLER_IDENTITY_JOINS} ORDER BY {order}"
        ));
    let rows: Vec<CallerRow> = query
        .build_query_as()
        .fetch_all(pool)
        .await
        .context("rank callers in the caller rollup")?;
    Ok(TopCallers {
        range: range.bounds(),
        callers: rows.into_iter().map(caller_traffic).collect(),
    })
}

fn caller_traffic(row: CallerRow) -> CallerTraffic {
    CallerTraffic {
        requester: row.key.requester(),
        actor: row.key.actor(),
        key_kind: row.key.key_kind(),
        service: row.key.service(),
        key_prefix: row.key.key_prefix.clone(),
        requests: row.requests,
        rate_limited: row.rate_limited,
        errors: row.errors,
        subjects: row.subjects,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_limit_defaults_and_stays_bounded() {
        assert_eq!(caller_limit(None).unwrap(), DEFAULT_CALLERS);
        assert_eq!(caller_limit(Some(5)).unwrap(), 5);
        for bad in [0, MAX_CALLERS + 1, -1] {
            assert_eq!(caller_limit(Some(bad)).unwrap_err().code(), "bad_request");
        }
    }

    #[test]
    fn sorts_are_columns_of_the_grouped_callers() {
        assert_eq!(CallerSort::default().column(), "requests");
        assert_eq!(CallerSort::Subjects.column(), "subjects");
    }
}
