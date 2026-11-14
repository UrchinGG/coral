//! The home server's sync settings. coral-sync owns them; these routes check the
//! staff member, forward to its settings API and write the audit log.

use std::sync::Arc;

use axum::extract::rejection::JsonRejection;
use axum::extract::{Path, State};
use axum::http::StatusCode;
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

const SOURCES_TARGET: &str = "sync_sources";

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
) -> Result<Json<Value>, AdminError> {
    let guild_id = home_guild(&state)?;
    let path = format!("/guilds/{guild_id}/config?preview_for={}", actor.discord_id);
    Ok(Json(sync_settings(&state)?.read(&path).await?))
}

pub async fn set_link_role(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    body: Result<Json<Value>, JsonRejection>,
) -> Result<Json<Value>, AdminError> {
    let Json(body) = body?;
    let job = change(
        &state,
        &actor,
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
    body: Result<Json<Value>, JsonRejection>,
) -> Result<Json<Value>, AdminError> {
    let Json(body) = body?;
    let job = change(
        &state,
        &actor,
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
    body: Result<Json<Value>, JsonRejection>,
) -> Result<StatusCode, AdminError> {
    let Json(body) = body?;
    change(
        &state,
        &actor,
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
    body: Result<Json<Value>, JsonRejection>,
) -> Result<Json<Value>, AdminError> {
    let Json(body) = body?;
    let job = change(
        &state,
        &actor,
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
) -> Result<Json<Value>, AdminError> {
    let job = change(
        &state,
        &actor,
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
    body: Result<Json<Value>, JsonRejection>,
) -> Result<Json<Value>, AdminError> {
    let Json(body) = body?;
    let job = change(
        &state,
        &actor,
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
    Path(rule_id): Path<i64>,
    body: Result<Json<Value>, JsonRejection>,
) -> Result<Json<Value>, AdminError> {
    let Json(body) = body?;
    let path = format!("/rules/{rule_id}");
    let job = change(
        &state,
        &actor,
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
    Path(rule_id): Path<i64>,
) -> Result<StatusCode, AdminError> {
    let path = format!("/rules/{rule_id}");
    change(
        &state,
        &actor,
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
    Path(role_id): Path<String>,
) -> Result<Json<Value>, AdminError> {
    let path = format!("/roles/{role_id}/strip");
    let job = change(&state, &actor, Method::POST, &path, None, "strip_role").await?;
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
    body: Result<Json<Value>, JsonRejection>,
) -> Result<Json<Value>, AdminError> {
    let Json(body) = body?;
    let job = change(
        &state,
        &actor,
        Method::PUT,
        "/sources",
        Some(body),
        "set_sync_sources",
    )
    .await?;
    Ok(started(job))
}

/// Forwards a write for the home server, audits what the sync reports it did, and
/// returns the job it started, if any.
pub async fn change(
    state: &AppState,
    actor: &AdminActor,
    method: Method,
    path: &str,
    body: Option<Value>,
    action: &str,
) -> Result<Value, AdminError> {
    let guild_id = home_guild(state)?;
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
