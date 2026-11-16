//! Each server's sync settings. coral-sync owns them; these routes check the
//! staff member, forward to its settings API and write the audit log.

use std::sync::Arc;

use axum::extract::rejection::JsonRejection;
use axum::extract::{FromRequestParts, Path, RawPathParams, State};
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::{Extension, Json};
use reqwest::Method;
use serde_json::{Value, json};
use serenity::all::GuildId;

use crate::auth::AdminActor;
use crate::error::AdminError;
use crate::state::AppState;
use crate::sync_settings::SyncSettings;

pub mod directory;
pub mod jobs;
pub mod list;

const SOURCES_TARGET: &str = "sync_sources";

/// The server a route names in its `{guild_id}` path segment.
pub struct Server(pub GuildId);

impl<S: Send + Sync> FromRequestParts<S> for Server {
    type Rejection = AdminError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, AdminError> {
        let params = RawPathParams::from_request_parts(parts, state)
            .await
            .map_err(|error| AdminError::BadRequest(error.to_string()))?;
        params
            .iter()
            .find(|(name, _)| *name == "guild_id")
            .and_then(|(_, value)| value.parse::<u64>().ok())
            .filter(|id| *id > 0)
            .map(|id| Server(GuildId::new(id)))
            .ok_or_else(|| AdminError::BadRequest("the server id must be a Discord id".into()))
    }
}

pub fn home_guild(state: &AppState) -> Result<GuildId, AdminError> {
    state
        .discord
        .home_guild_id
        .and_then(|id| u64::try_from(id).ok())
        .filter(|id| *id > 0)
        .map(GuildId::new)
        .ok_or_else(|| AdminError::NotFound("no home server is configured (HOME_GUILD_ID)".into()))
}

pub fn sync_settings(state: &AppState) -> Result<Arc<SyncSettings>, AdminError> {
    state
        .discord
        .sync_settings()
        .map_err(|error| AdminError::Rejected(error.to_string()))
}

pub async fn config(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Server(guild_id): Server,
) -> Result<Json<Value>, AdminError> {
    let path = format!("/guilds/{guild_id}/config?preview_for={}", actor.discord_id);
    Ok(Json(sync_settings(&state)?.read(&path).await?))
}

pub async fn set_enabled(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Server(guild_id): Server,
    body: Result<Json<Value>, JsonRejection>,
) -> Result<StatusCode, AdminError> {
    let Json(body) = body?;
    change(
        &state,
        &actor,
        guild_id,
        Method::PUT,
        "/enabled",
        Some(body),
        "set_sync_enabled",
    )
    .await?;
    list::forget_servers(&state).await;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn set_link_role(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Server(guild_id): Server,
    body: Result<Json<Value>, JsonRejection>,
) -> Result<Json<Value>, AdminError> {
    let Json(body) = body?;
    let job = change(
        &state,
        &actor,
        guild_id,
        Method::PUT,
        "/link-role",
        Some(body),
        "set_link_role",
    )
    .await?;
    Ok(started(job))
}

pub async fn set_unlinked_role(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Server(guild_id): Server,
    body: Result<Json<Value>, JsonRejection>,
) -> Result<Json<Value>, AdminError> {
    let Json(body) = body?;
    let job = change(
        &state,
        &actor,
        guild_id,
        Method::PUT,
        "/unlinked-role",
        Some(body),
        "set_unlinked_role",
    )
    .await?;
    Ok(started(job))
}

pub async fn set_link_channel(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Server(guild_id): Server,
    body: Result<Json<Value>, JsonRejection>,
) -> Result<StatusCode, AdminError> {
    let Json(body) = body?;
    change(
        &state,
        &actor,
        guild_id,
        Method::PUT,
        "/link-channel",
        Some(body),
        "set_link_channel",
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn set_nickname_template(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Server(guild_id): Server,
    body: Result<Json<Value>, JsonRejection>,
) -> Result<Json<Value>, AdminError> {
    let Json(body) = body?;
    let job = change(
        &state,
        &actor,
        guild_id,
        Method::PUT,
        "/nickname-template",
        Some(body),
        "set_nickname_template",
    )
    .await?;
    Ok(started(job))
}

pub async fn reset_nicknames(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Server(guild_id): Server,
) -> Result<Json<Value>, AdminError> {
    let job = change(
        &state,
        &actor,
        guild_id,
        Method::POST,
        "/nicknames/reset",
        None,
        "reset_nicknames",
    )
    .await?;
    Ok(started(job))
}

pub async fn add_rule(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Server(guild_id): Server,
    body: Result<Json<Value>, JsonRejection>,
) -> Result<Json<Value>, AdminError> {
    let Json(body) = body?;
    let job = change(
        &state,
        &actor,
        guild_id,
        Method::POST,
        "/rules",
        Some(body),
        "add_autorole_rule",
    )
    .await?;
    Ok(started(job))
}

pub async fn update_rule(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Server(guild_id): Server,
    Path((_, rule_id)): Path<(u64, i64)>,
    body: Result<Json<Value>, JsonRejection>,
) -> Result<Json<Value>, AdminError> {
    let Json(body) = body?;
    let path = format!("/rules/{rule_id}");
    let job = change(
        &state,
        &actor,
        guild_id,
        Method::PUT,
        &path,
        Some(body),
        "update_autorole_rule",
    )
    .await?;
    Ok(started(job))
}

pub async fn remove_rule(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Server(guild_id): Server,
    Path((_, rule_id)): Path<(u64, i64)>,
) -> Result<StatusCode, AdminError> {
    let path = format!("/rules/{rule_id}");
    change(
        &state,
        &actor,
        guild_id,
        Method::DELETE,
        &path,
        None,
        "remove_autorole_rule",
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn strip_role(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Server(guild_id): Server,
    Path((_, role_id)): Path<(u64, String)>,
) -> Result<Json<Value>, AdminError> {
    let path = format!("/roles/{role_id}/strip");
    let job = change(
        &state,
        &actor,
        guild_id,
        Method::POST,
        &path,
        None,
        "strip_role",
    )
    .await?;
    Ok(started(job))
}

pub async fn sources(State(state): State<AppState>) -> Result<Json<Value>, AdminError> {
    Ok(Json(sync_settings(&state)?.read("/sources").await?))
}

pub async fn add_source(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    body: Result<Json<Value>, JsonRejection>,
) -> Result<StatusCode, AdminError> {
    let Json(body) = body?;
    let target = Target::Sources;
    forward(
        &state,
        &actor,
        Method::POST,
        "/sources",
        target,
        Some(body),
        "add_sync_source",
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn remove_source(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path(source_id): Path<i64>,
) -> Result<StatusCode, AdminError> {
    let path = format!("/sources/{source_id}");
    let target = Target::Sources;
    forward(
        &state,
        &actor,
        Method::DELETE,
        &path,
        target,
        None,
        "remove_sync_source",
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn set_used_sources(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Server(guild_id): Server,
    body: Result<Json<Value>, JsonRejection>,
) -> Result<Json<Value>, AdminError> {
    let Json(body) = body?;
    let job = change(
        &state,
        &actor,
        guild_id,
        Method::PUT,
        "/sources",
        Some(body),
        "set_sync_sources",
    )
    .await?;
    Ok(started(job))
}

/// Forwards a write for one server, audits what the sync reports it did, and
/// returns the job it started, if any.
pub async fn change(
    state: &AppState,
    actor: &AdminActor,
    guild_id: GuildId,
    method: Method,
    path: &str,
    body: Option<Value>,
    action: &str,
) -> Result<Value, AdminError> {
    let path = format!("/guilds/{guild_id}{path}");
    forward(
        state,
        actor,
        method,
        &path,
        Target::Server(guild_id),
        body,
        action,
    )
    .await
}

/// What an audit row names: the server, or the sync's source registry.
enum Target {
    Server(GuildId),
    Sources,
}

async fn forward(
    state: &AppState,
    actor: &AdminActor,
    method: Method,
    path: &str,
    target: Target,
    body: Option<Value>,
    action: &str,
) -> Result<Value, AdminError> {
    let mut change = sync_settings(state)?
        .write(method, path, actor.discord_id, body)
        .await?;
    let target = match target {
        Target::Server(guild_id) => guild_id.to_string(),
        Target::Sources => SOURCES_TARGET.to_string(),
    };
    crate::audit::log(
        state,
        actor.discord_id,
        action,
        &target,
        change["audit"].take(),
    )
    .await;
    Ok(change["job"].take())
}

fn started(job: Value) -> Json<Value> {
    Json(json!({ "job": job }))
}
