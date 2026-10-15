use axum::extract::rejection::{JsonRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::routing::{delete, get, post};
use axum::{Extension, Json, Router};
use serde::Serialize;

use crate::auth::AdminActor;
use crate::error::AdminError;
use crate::players::{
    self, NewLock, NewTag, PlayerCore, PlayerRequestRow, PlayerTags, RecentlyTagged, Requesters,
    RequestersParams, SnapshotAt, SnapshotAtQuery, SnapshotPage, SnapshotQuery, StreamParams,
};
use crate::refs::Page;
use crate::state::AppState;

#[derive(Serialize)]
pub struct Done {
    ok: bool,
}

const DONE: Done = Done { ok: true };

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/recently-tagged", get(recently_tagged))
        .route("/{uuid}", get(core))
        .route("/{uuid}/requests", get(requests))
        .route("/{uuid}/requesters", get(requesters))
        .route("/{uuid}/tags", get(tags).post(add_tag))
        .route("/{uuid}/tags/{tag_type}", delete(remove_tag))
        .route("/{uuid}/lock", post(lock))
        .route("/{uuid}/unlock", post(unlock))
        .route("/{uuid}/snapshots", get(snapshots))
        .route("/{uuid}/snapshots/at", get(snapshot_at))
}

async fn recently_tagged(
    State(state): State<AppState>,
) -> Result<Json<RecentlyTagged>, AdminError> {
    Ok(Json(players::read_recently_tagged(&state).await?))
}

async fn core(
    State(state): State<AppState>,
    Path(uuid): Path<String>,
) -> Result<Json<PlayerCore>, AdminError> {
    let player = players::parse_player(&uuid)?;
    Ok(Json(players::read_core(&state, player).await?))
}

async fn requests(
    State(state): State<AppState>,
    Path(uuid): Path<String>,
    params: Result<Query<StreamParams>, QueryRejection>,
) -> Result<Json<Page<PlayerRequestRow>>, AdminError> {
    let player = players::parse_player(&uuid)?;
    let Query(params) = params?;
    Ok(Json(players::read_stream(&state, player, params).await?))
}

async fn requesters(
    State(state): State<AppState>,
    Path(uuid): Path<String>,
    params: Result<Query<RequestersParams>, QueryRejection>,
) -> Result<Json<Requesters>, AdminError> {
    let player = players::parse_player(&uuid)?;
    let Query(params) = params?;
    Ok(Json(
        players::read_requesters(&state, player, params).await?,
    ))
}

async fn tags(
    State(state): State<AppState>,
    Path(uuid): Path<String>,
) -> Result<Json<PlayerTags>, AdminError> {
    let player = players::parse_player(&uuid)?;
    Ok(Json(players::read_tags(&state, player).await?))
}

async fn snapshots(
    State(state): State<AppState>,
    Path(uuid): Path<String>,
    query: Result<Query<SnapshotQuery>, QueryRejection>,
) -> Result<Json<SnapshotPage>, AdminError> {
    let player = players::parse_player(&uuid)?;
    let Query(query) = query?;
    Ok(Json(
        players::read_snapshots(&state.db, player, query).await?,
    ))
}

async fn snapshot_at(
    State(state): State<AppState>,
    Path(uuid): Path<String>,
    query: Result<Query<SnapshotAtQuery>, QueryRejection>,
) -> Result<Json<SnapshotAt>, AdminError> {
    let player = players::parse_player(&uuid)?;
    let Query(query) = query?;
    Ok(Json(
        players::read_snapshot_at(&state.db, player, query).await?,
    ))
}

async fn add_tag(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path(uuid): Path<String>,
    tag: Result<Json<NewTag>, JsonRejection>,
) -> Result<Json<Done>, AdminError> {
    let player = players::parse_player(&uuid)?;
    let Json(tag) = tag?;
    players::add_tag(&state.db, actor.discord_id, player, tag).await?;
    Ok(Json(DONE))
}

async fn remove_tag(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path((uuid, tag_type)): Path<(String, String)>,
) -> Result<Json<Done>, AdminError> {
    let player = players::parse_player(&uuid)?;
    players::remove_tag(&state.db, actor.discord_id, player, &tag_type).await?;
    Ok(Json(DONE))
}

async fn lock(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path(uuid): Path<String>,
    lock: Result<Json<NewLock>, JsonRejection>,
) -> Result<Json<Done>, AdminError> {
    let player = players::parse_player(&uuid)?;
    let Json(lock) = lock?;
    players::lock(&state.db, actor.discord_id, player, lock).await?;
    Ok(Json(DONE))
}

async fn unlock(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path(uuid): Path<String>,
) -> Result<Json<Done>, AdminError> {
    let player = players::parse_player(&uuid)?;
    players::unlock(&state.db, actor.discord_id, player).await?;
    Ok(Json(DONE))
}
