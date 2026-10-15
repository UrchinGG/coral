use std::time::Duration;

use axum::extract::rejection::QueryRejection;
use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use reqlog::{Cause, KeyKind};
use serde::Deserialize;

use crate::error::AdminError;
use crate::state::AppState;
use crate::traffic::by_route::routes;
use crate::traffic::callers::{CallerSort, caller_limit, top_callers};
use crate::traffic::class::{RouteFilter, StatusClass, known_method};
use crate::traffic::pressure::{Pressure, pressure};
use crate::traffic::range::{RangeQuery, TrafficRange, Window};
use crate::traffic::rows::TrafficFilter;
use crate::traffic::series::{SeriesFilter, series};
use crate::traffic::summary::summary;
use crate::traffic::upstream::{Upstream, upstream};
use crate::views::{CachedView, View};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Deserialize)]
pub enum Toggle {
    #[default]
    #[serde(rename = "0")]
    Off,
    #[serde(rename = "1")]
    On,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RangeParams {
    window: Option<Window>,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SummaryParams {
    window: Option<Window>,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
    #[serde(default)]
    compare: Toggle,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeriesParams {
    window: Option<Window>,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
    route: Option<String>,
    class: Option<StatusClass>,
    cause: Option<Cause>,
    member: Option<i64>,
    actor: Option<String>,
    key_kind: Option<KeyKind>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CallerParams {
    window: Option<Window>,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
    route: Option<String>,
    method: Option<String>,
    class: Option<StatusClass>,
    cause: Option<Cause>,
    #[serde(default)]
    sort: CallerSort,
    limit: Option<i64>,
}

struct Resolved {
    query: RangeQuery,
    range: TrafficRange,
    now: DateTime<Utc>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/summary", get(traffic_summary))
        .route("/series", get(traffic_series))
        .route("/routes", get(traffic_routes))
        .route("/callers", get(traffic_callers))
        .route("/pressure", get(limit_pressure))
        .route("/upstream", get(upstream_traffic))
}

async fn traffic_summary(
    State(state): State<AppState>,
    params: Result<Query<SummaryParams>, QueryRejection>,
) -> Result<CachedView, AdminError> {
    let Query(params) = params?;
    let resolved = resolve(RangeQuery {
        window: params.window,
        from: params.from,
        to: params.to,
    })?;
    let compare = params.compare == Toggle::On;
    let key = format!("summary:{}:{compare}", resolved.cache_key());
    state
        .views
        .get_or_load(resolved.view(), key, || {
            summary(state.db.pool(), resolved.range, compare, resolved.now)
        })
        .await
}

async fn traffic_series(
    State(state): State<AppState>,
    params: Result<Query<SeriesParams>, QueryRejection>,
) -> Result<CachedView, AdminError> {
    let Query(params) = params?;
    let resolved = resolve(RangeQuery {
        window: params.window,
        from: params.from,
        to: params.to,
    })?;
    let filter = SeriesFilter {
        traffic: TrafficFilter {
            route: params
                .route
                .as_deref()
                .map(RouteFilter::parse)
                .transpose()?,
            method: None,
            class: params.class,
            cause: params.cause,
            key_kind: params.key_kind,
        },
        member: params.member,
        actor: params.actor.as_deref().map(discord_id).transpose()?,
    };
    let key = format!("series:{}:{filter:?}", resolved.cache_key());
    state
        .views
        .get_or_load(resolved.view(), key, || {
            series(state.db.pool(), resolved.range, filter)
        })
        .await
}

async fn traffic_routes(
    State(state): State<AppState>,
    params: Result<Query<RangeParams>, QueryRejection>,
) -> Result<CachedView, AdminError> {
    let Query(params) = params?;
    let resolved = resolve(RangeQuery {
        window: params.window,
        from: params.from,
        to: params.to,
    })?;
    let key = format!("routes:{}", resolved.cache_key());
    state
        .views
        .get_or_load(resolved.view(), key, || {
            routes(state.db.pool(), resolved.range)
        })
        .await
}

async fn traffic_callers(
    State(state): State<AppState>,
    params: Result<Query<CallerParams>, QueryRejection>,
) -> Result<CachedView, AdminError> {
    let Query(params) = params?;
    let resolved = resolve(RangeQuery {
        window: params.window,
        from: params.from,
        to: params.to,
    })?;
    let filter = TrafficFilter {
        route: params
            .route
            .as_deref()
            .map(RouteFilter::parse)
            .transpose()?,
        method: params.method.as_deref().map(known_method).transpose()?,
        class: params.class,
        cause: params.cause,
        key_kind: None,
    };
    let limit = caller_limit(params.limit)?;
    let key = format!(
        "callers:{}:{filter:?}:{:?}:{limit}",
        resolved.cache_key(),
        params.sort
    );
    state
        .views
        .get_or_load(resolved.view(), key, || {
            top_callers(state.db.pool(), resolved.range, filter, params.sort, limit)
        })
        .await
}

async fn limit_pressure(State(state): State<AppState>) -> Result<Json<Pressure>, AdminError> {
    Ok(Json(
        pressure(state.db.pool(), &state.redis, Utc::now()).await?,
    ))
}

async fn upstream_traffic(
    State(state): State<AppState>,
    params: Result<Query<RangeParams>, QueryRejection>,
) -> Result<Json<Upstream>, AdminError> {
    let Query(params) = params?;
    let resolved = resolve(RangeQuery {
        window: params.window,
        from: params.from,
        to: params.to,
    })?;
    Ok(Json(
        upstream(
            &state.redis,
            &state.upstream_history,
            resolved.range,
            resolved.now,
        )
        .await?,
    ))
}

impl Resolved {
    fn cache_key(&self) -> String {
        self.query.cache_key(self.range)
    }

    fn view(&self) -> View {
        let seconds = self.range.length().num_seconds().unsigned_abs();
        View::traffic_summary(Duration::from_secs(seconds))
    }
}

fn resolve(query: RangeQuery) -> Result<Resolved, AdminError> {
    let now = Utc::now();
    Ok(Resolved {
        query,
        range: query.resolve(now)?,
        now,
    })
}

fn discord_id(raw: &str) -> Result<i64, AdminError> {
    raw.parse::<i64>()
        .ok()
        .filter(|id| *id > 0)
        .ok_or_else(|| AdminError::BadRequest(format!("{raw:?} is not a Discord id")))
}

#[cfg(test)]
mod tests {
    use axum::http::Uri;

    use super::*;

    fn parsed<T: serde::de::DeserializeOwned>(uri: &'static str) -> Result<T, AdminError> {
        let Query(params) = Query::<T>::try_from_uri(&Uri::from_static(uri))?;
        Ok(params)
    }

    #[test]
    fn summaries_take_a_window_and_an_optional_comparison() {
        let params: SummaryParams = parsed("/traffic/summary?window=24h&compare=1").unwrap();
        assert_eq!(params.window, Some(Window::Day));
        assert_eq!(params.compare, Toggle::On);
        let plain: SummaryParams = parsed("/traffic/summary?window=1h").unwrap();
        assert_eq!(plain.compare, Toggle::Off);
        for uri in [
            "/traffic/summary?window=2h",
            "/traffic/summary?window=1h&compare=yes",
            "/traffic/summary?window=1h&member=1",
        ] {
            assert_eq!(
                parsed::<SummaryParams>(uri).unwrap_err().code(),
                "bad_request",
                "{uri}"
            );
        }
    }

    #[test]
    fn series_filters_parse_their_wire_names() {
        let params: SeriesParams = parsed(
            "/traffic/series?from=2026-10-15T10:00:00Z&to=2026-10-15T11:00:00Z&class=rate_limited&cause=upstream_limit&key_kind=session&actor=702913845516288031",
        )
        .unwrap();
        assert_eq!(params.class, Some(StatusClass::RateLimited));
        assert_eq!(params.cause, Some(Cause::UpstreamLimit));
        assert_eq!(params.key_kind, Some(KeyKind::Session));
        assert_eq!(
            discord_id(params.actor.as_deref().unwrap()).unwrap(),
            702_913_845_516_288_031
        );
        assert_eq!(discord_id("abc").unwrap_err().code(), "bad_request");
    }

    #[test]
    fn callers_sort_by_any_listed_measure() {
        let params: CallerParams =
            parsed("/traffic/callers?window=7d&sort=subjects&method=POST").unwrap();
        assert_eq!(params.sort, CallerSort::Subjects);
        assert_eq!(params.method.as_deref(), Some("POST"));
        assert_eq!(
            parsed::<CallerParams>("/traffic/callers?window=7d&sort=latency")
                .unwrap_err()
                .code(),
            "bad_request"
        );
    }
}
