use axum::extract::rejection::{JsonRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use uuid::Uuid;

use crate::error::AdminError;
use crate::query::{LogQuery, RequestFilter};
use crate::refs::{Page, RequestRow};
use crate::requests::{
    DetailError, GroupFilter, GroupPage, RequestFull, RequestRows, read_groups, read_request,
    read_route_counts,
};
use crate::state::AppState;
use crate::views::{CachedView, View};

const ROUTE_COUNTS_VIEW: &str = "last_24h";

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RequestInstant {
    t: Option<DateTime<Utc>>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/query", post(query))
        .route("/groups", post(groups))
        .route("/routes", get(routes))
        .route("/{request_id}", get(request))
}

async fn query(
    State(state): State<AppState>,
    body: Result<Json<RequestFilter>, JsonRejection>,
) -> Result<Json<Page<RequestRow>>, AdminError> {
    let Json(filter) = body?;
    let coverage = state.coverage.current();
    let page = LogQuery::new(&state.db, &coverage, &state.players, &state.scan_rates)
        .run(filter)
        .await?;
    let rows = RequestRows {
        db: &state.db,
        names: &state.names,
        coverage: &coverage,
    };
    Ok(Json(rows.page(page).await?))
}

async fn groups(
    State(state): State<AppState>,
    body: Result<Json<GroupFilter>, JsonRejection>,
) -> Result<Json<GroupPage>, AdminError> {
    let Json(filter) = body?;
    let coverage = state.coverage.current();
    Ok(Json(
        read_groups(&state.db, &state.names, &coverage, filter).await?,
    ))
}

async fn routes(State(state): State<AppState>) -> Result<CachedView, AdminError> {
    state
        .views
        .get_or_load(View::Routes, ROUTE_COUNTS_VIEW, || {
            read_route_counts(&state.db, Utc::now())
        })
        .await
}

async fn request(
    State(state): State<AppState>,
    Path(raw_id): Path<String>,
    instant: Result<Query<RequestInstant>, QueryRejection>,
) -> Result<Json<RequestFull>, DetailError> {
    let Query(instant) = instant.map_err(AdminError::from)?;
    let request_id = Uuid::try_parse(&raw_id)
        .map_err(|_| AdminError::BadRequest(format!("{raw_id:?} is not a request id")))?;
    let coverage = state.coverage.current();
    let full = read_request(&state.db, &state.names, &coverage, request_id, instant.t).await?;
    Ok(Json(full))
}

#[cfg(test)]
mod tests {
    use axum::http::Uri;

    use super::*;

    fn instant(uri: &'static str) -> Result<Option<DateTime<Utc>>, AdminError> {
        let Query(instant) = Query::<RequestInstant>::try_from_uri(&Uri::from_static(uri))?;
        Ok(instant.t)
    }

    #[test]
    fn the_instant_is_optional_and_must_be_a_timestamp() {
        assert_eq!(instant("/requests/x").unwrap(), None);
        assert_eq!(
            instant("/requests/x?t=2026-10-14T14%3A32%3A10.412318Z")
                .unwrap()
                .map(|t| t.timestamp_subsec_micros()),
            Some(412_318)
        );
        assert_eq!(
            instant("/requests/x?t=yesterday").unwrap_err().code(),
            "bad_request"
        );
        assert_eq!(
            instant("/requests/x?t=2026-10-14T14:32:10Z&x=1")
                .unwrap_err()
                .code(),
            "bad_request"
        );
    }
}
