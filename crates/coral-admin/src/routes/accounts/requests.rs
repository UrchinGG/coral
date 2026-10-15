use axum::Json;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Path, Query, State};

use super::AccountTarget;
use crate::error::AdminError;
use crate::query::RequestFilter;
use crate::refs::{Page, RequestRow};
use crate::routes::streams::{StreamScope, request_stream};
use crate::state::AppState;

pub async fn requests(
    State(state): State<AppState>,
    Path(raw): Path<String>,
    filter: Result<Query<RequestFilter>, QueryRejection>,
) -> Result<Json<Page<RequestRow>>, AdminError> {
    let Query(filter) = filter?;
    let target = AccountTarget::resolve(&state, &raw).await?;
    let scope = match target.member_id {
        Some(member_id) => StreamScope::Member(member_id),
        None => StreamScope::Actor(target.discord_id),
    };
    Ok(Json(request_stream(&state, scope, filter).await?))
}
