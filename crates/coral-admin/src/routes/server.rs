use std::convert::Infallible;

use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::{Extension, Json, Router, extract::*, routing::get, routing::post, routing::put};
use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use serenity::all::*;
use tokio_stream::wrappers::ReceiverStream;

use coral_redis::{SYNC_CHANNEL, SyncEvent, SyncEventPublisher};
use database::{
    BlacklistRepository, CacheRepository, GuildConfigRepository, GuildSyncJob,
    GuildSyncJobRepository, MemberRepository,
};

use crate::auth::AdminActor;
use crate::discord;
use crate::state::AppState;

const FINISHED_JOBS_SHOWN: i64 = 5;

type ApiError = (StatusCode, String);

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(server_detail))
        .route("/link-role", put(set_link_role))
        .route("/unlinked-role", put(set_unlinked_role))
        .route("/link-channel", put(set_link_channel))
        .route("/nickname-template", put(set_nickname_template))
        .route("/nicknames/reset", post(reset_nicknames))
        .route("/rules", post(add_rule))
        .route("/rules/{rule_id}", put(update_rule).delete(remove_rule))
        .route("/roles/{role_id}/strip", post(strip_role))
        .route("/jobs", get(list_jobs))
        .route("/jobs/{job_id}/cancel", post(cancel_job))
        .route("/jobs/{job_id}/events", get(job_events))
}

fn home_guild(state: &AppState) -> Result<u64, ApiError> {
    state.home_guild_id.map(|id| id as u64).ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Home guild not configured".into(),
    ))
}

#[derive(Serialize)]
struct DiscordRoleView {
    id: String,
    name: String,
    color: Option<String>,
    position: i64,
    managed: bool,
    assignable: bool,
}

#[derive(Serialize)]
struct DiscordChannelView {
    id: String,
    name: String,
    category: Option<String>,
}

#[derive(Serialize)]
struct ServerGuildView {
    guild_id: String,
    name: String,
    icon_url: Option<String>,
    member_count: usize,
    linked_members: i64,
}

#[derive(Serialize)]
struct ServerSyncConfigView {
    link_role_id: Option<String>,
    unlinked_role_id: Option<String>,
    link_channel_id: Option<String>,
    link_message_id: Option<String>,
    nickname_template: Option<String>,
}

#[derive(Serialize)]
struct AutoroleRuleView {
    id: i64,
    role_id: String,
    condition: String,
}

#[derive(Serialize)]
struct ServerDetailResponse {
    guild: ServerGuildView,
    roles: Vec<DiscordRoleView>,
    channels: Vec<DiscordChannelView>,
    config: ServerSyncConfigView,
    rules: Vec<AutoroleRuleView>,
    preview_context: Option<Value>,
}

#[derive(Serialize)]
struct SyncJobView {
    id: i64,
    kind: String,
    label: String,
    state: String,
    processed: i32,
    total: i32,
    started_at: DateTime<Utc>,
    finished_at: Option<DateTime<Utc>>,
}

#[derive(Serialize)]
struct SyncJobsResponse {
    jobs: Vec<SyncJobView>,
}

#[derive(Serialize)]
struct StartedJobResponse {
    job: Option<SyncJobView>,
}

async fn server_detail(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
) -> Result<Json<ServerDetailResponse>, ApiError> {
    let guild_id = home_guild(&state)?;
    let http = discord::sync_http(&state).map_err(service_unavailable)?;

    let guild = http
        .get_guild(GuildId::new(guild_id))
        .await
        .map_err(bad_gateway)?;
    let guild_id = guild.id;

    let config_repo = GuildConfigRepository::new(state.db.pool());
    let (roles, channels, population, bot_user, config, rules, preview_context) = tokio::join!(
        discord::guild_roles(&http, guild_id),
        discord::guild_channels(&http, guild_id),
        guild_population(&state, &http, guild_id),
        http.get_current_user(),
        config_repo.get(guild_id.get() as i64),
        config_repo.get_role_rules(guild_id.get() as i64),
        build_preview_context(&state, &http, guild_id, actor.discord_id),
    );

    let roles = roles.map_err(bad_gateway)?;
    let channels = channels.map_err(bad_gateway)?;
    let (member_count, linked_members) = population.map_err(bad_gateway)?;
    let bot_user = bot_user.map_err(bad_gateway)?;
    let config = config.map_err(internal)?;
    let rules = rules.map_err(internal)?;

    let bot_member = http
        .get_member(guild_id, bot_user.id)
        .await
        .map_err(bad_gateway)?;

    Ok(Json(ServerDetailResponse {
        guild: ServerGuildView {
            icon_url: icon_url(guild.id, guild.icon),
            guild_id: guild.id.to_string(),
            name: guild.name.to_string(),
            member_count,
            linked_members,
        },
        roles: role_views(&roles, &bot_member.roles, guild_id),
        channels: channel_views(&channels),
        config: ServerSyncConfigView {
            link_role_id: config
                .as_ref()
                .and_then(|c| c.link_role_id.map(|v| v.to_string())),
            unlinked_role_id: config
                .as_ref()
                .and_then(|c| c.unlinked_role_id.map(|v| v.to_string())),
            link_channel_id: config
                .as_ref()
                .and_then(|c| c.link_channel_id.map(|v| v.to_string())),
            link_message_id: config
                .as_ref()
                .and_then(|c| c.link_message_id.map(|v| v.to_string())),
            nickname_template: config.as_ref().and_then(|c| c.nickname_template.clone()),
        },
        rules: rules
            .iter()
            .map(|r| AutoroleRuleView {
                id: r.id,
                role_id: r.role_id.to_string(),
                condition: r.condition.clone(),
            })
            .collect(),
        preview_context,
    }))
}

#[derive(Deserialize)]
struct RoleUpdateRequest {
    role_id: Option<String>,
}

async fn set_link_role(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Json(req): Json<RoleUpdateRequest>,
) -> Result<Json<StartedJobResponse>, ApiError> {
    update_config_role(&state, actor, req.role_id, ConfigRole::Link).await
}

async fn set_unlinked_role(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Json(req): Json<RoleUpdateRequest>,
) -> Result<Json<StartedJobResponse>, ApiError> {
    update_config_role(&state, actor, req.role_id, ConfigRole::Unlinked).await
}

#[derive(Clone, Copy)]
enum ConfigRole {
    Link,
    Unlinked,
}

impl ConfigRole {
    fn field(self) -> &'static str {
        match self {
            Self::Link => "link",
            Self::Unlinked => "unlinked",
        }
    }

    fn noun(self) -> &'static str {
        match self {
            Self::Link => "linked",
            Self::Unlinked => "unlinked",
        }
    }

    fn action(self) -> &'static str {
        match self {
            Self::Link => "set_link_role",
            Self::Unlinked => "set_unlinked_role",
        }
    }
}

async fn update_config_role(
    state: &AppState,
    actor: AdminActor,
    role_id: Option<String>,
    which: ConfigRole,
) -> Result<Json<StartedJobResponse>, ApiError> {
    let guild_id = home_guild(state)?;
    let new_role = parse_optional_id(role_id.as_deref())?;

    let repo = GuildConfigRepository::new(state.db.pool());
    let config = repo
        .upsert(guild_id as i64, actor.discord_id)
        .await
        .map_err(internal)?;
    let old_role = match which {
        ConfigRole::Link => config.link_role_id,
        ConfigRole::Unlinked => config.unlinked_role_id,
    };

    let update = match which {
        ConfigRole::Link => repo.set_link_role(guild_id as i64, new_role).await,
        ConfigRole::Unlinked => repo.set_unlinked_role(guild_id as i64, new_role).await,
    };
    update.map_err(internal)?;

    crate::audit::log(
        state,
        actor.discord_id,
        which.action(),
        &guild_id.to_string(),
        json!({"old_role_id": old_role, "new_role_id": new_role}),
    )
    .await;

    if old_role == new_role {
        return Ok(Json(StartedJobResponse { job: None }));
    }

    let label = swap_label(state, guild_id, which, old_role, new_role).await;
    let payload = json!({
        "old_role_id": old_role.map(|v| v.to_string()),
        "new_role_id": new_role.map(|v| v.to_string()),
        "field": which.field(),
        "label": label,
    });
    let job = enqueue_job(state, guild_id, "swap_role", payload, actor.discord_id).await?;
    Ok(Json(StartedJobResponse { job: Some(job) }))
}

#[derive(Deserialize)]
struct ChannelUpdateRequest {
    channel_id: Option<String>,
}

async fn set_link_channel(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Json(req): Json<ChannelUpdateRequest>,
) -> Result<StatusCode, ApiError> {
    let guild_id = home_guild(&state)?;
    let http = discord::sync_http(&state).map_err(service_unavailable)?;
    let new_channel = parse_optional_id(req.channel_id.as_deref())?;

    let repo = GuildConfigRepository::new(state.db.pool());
    let config = repo
        .upsert(guild_id as i64, actor.discord_id)
        .await
        .map_err(internal)?;

    if let (Some(channel), Some(message)) = (config.link_channel_id, config.link_message_id)
        && let Err(e) = GenericChannelId::new(channel as u64)
            .delete_message(&http, MessageId::new(message as u64), None)
            .await
    {
        tracing::debug!("Failed to delete old link embed in {channel}: {e}");
    }

    let message_id = match new_channel {
        Some(channel_id) => {
            let message = GenericChannelId::new(channel_id as u64)
                .send_message(&http, link_embed_message())
                .await
                .map_err(bad_gateway)?;
            Some(message.id.get() as i64)
        }
        None => None,
    };

    repo.set_link_channel(guild_id as i64, new_channel, message_id)
        .await
        .map_err(internal)?;

    crate::audit::log(
        &state,
        actor.discord_id,
        "set_link_channel",
        &guild_id.to_string(),
        json!({"old_channel_id": config.link_channel_id, "new_channel_id": new_channel}),
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct TemplateUpdateRequest {
    template: Option<String>,
}

async fn set_nickname_template(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Json(req): Json<TemplateUpdateRequest>,
) -> Result<Json<StartedJobResponse>, ApiError> {
    let guild_id = home_guild(&state)?;
    let template = req.template.filter(|t| !t.trim().is_empty());
    if let Some(t) = &template
        && let Err(e) = expr::validate_template(t)
    {
        return Err(bad_request(format!("Invalid template: {e}")));
    }

    let repo = GuildConfigRepository::new(state.db.pool());
    repo.upsert(guild_id as i64, actor.discord_id)
        .await
        .map_err(internal)?;
    repo.set_nickname_template(guild_id as i64, template.as_deref())
        .await
        .map_err(internal)?;

    crate::audit::log(
        &state,
        actor.discord_id,
        "set_nickname_template",
        &guild_id.to_string(),
        json!({"template": template}),
    )
    .await;

    let label = if template.is_some() {
        "Applying display name format"
    } else {
        "Syncing display names"
    };
    let job = enqueue_job(
        &state,
        guild_id,
        "resync",
        json!({"label": label}),
        actor.discord_id,
    )
    .await?;
    Ok(Json(StartedJobResponse { job: Some(job) }))
}

async fn reset_nicknames(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
) -> Result<Json<StartedJobResponse>, ApiError> {
    let guild_id = home_guild(&state)?;
    crate::audit::log(
        &state,
        actor.discord_id,
        "reset_nicknames",
        &guild_id.to_string(),
        json!({}),
    )
    .await;
    let job = enqueue_job(
        &state,
        guild_id,
        "clear_nicknames",
        json!({"label": "Clearing all nicknames"}),
        actor.discord_id,
    )
    .await?;
    Ok(Json(StartedJobResponse { job: Some(job) }))
}

#[derive(Deserialize)]
struct AddRuleRequest {
    role_id: String,
    condition: String,
}

async fn add_rule(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Json(req): Json<AddRuleRequest>,
) -> Result<Json<StartedJobResponse>, ApiError> {
    let guild_id = home_guild(&state)?;
    let role_id: i64 = req
        .role_id
        .parse()
        .map_err(|_| bad_request("Invalid role id".into()))?;
    if let Err(e) = expr::validate_condition(&req.condition) {
        return Err(bad_request(format!("Invalid condition: {e}")));
    }

    let repo = GuildConfigRepository::new(state.db.pool());
    repo.upsert(guild_id as i64, actor.discord_id)
        .await
        .map_err(internal)?;
    let rules = repo
        .get_role_rules(guild_id as i64)
        .await
        .map_err(internal)?;
    if rules.iter().any(|r| r.role_id == role_id) {
        return Err((
            StatusCode::CONFLICT,
            "A rule already exists for that role. Edit or remove it first.".into(),
        ));
    }

    repo.add_role_rule(guild_id as i64, role_id, &req.condition, 0)
        .await
        .map_err(internal)?;

    crate::audit::log(
        &state,
        actor.discord_id,
        "add_autorole_rule",
        &guild_id.to_string(),
        json!({"role_id": role_id, "condition": req.condition}),
    )
    .await;

    let label = format!(
        "Evaluating @{} rule for all members",
        role_name(&state, guild_id, role_id as u64).await
    );
    let job = enqueue_job(
        &state,
        guild_id,
        "resync",
        json!({"label": label}),
        actor.discord_id,
    )
    .await?;
    Ok(Json(StartedJobResponse { job: Some(job) }))
}

#[derive(Deserialize)]
struct UpdateRuleRequest {
    condition: String,
}

async fn update_rule(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path(rule_id): Path<i64>,
    Json(req): Json<UpdateRuleRequest>,
) -> Result<Json<StartedJobResponse>, ApiError> {
    let guild_id = home_guild(&state)?;
    if let Err(e) = expr::validate_condition(&req.condition) {
        return Err(bad_request(format!("Invalid condition: {e}")));
    }

    let repo = GuildConfigRepository::new(state.db.pool());
    let rules = repo
        .get_role_rules(guild_id as i64)
        .await
        .map_err(internal)?;
    let rule = rules
        .iter()
        .find(|r| r.id == rule_id)
        .ok_or_else(not_found)?;
    let role_id = rule.role_id;

    repo.update_role_rule_condition(rule_id, &req.condition)
        .await
        .map_err(internal)?;

    crate::audit::log(
        &state,
        actor.discord_id,
        "update_autorole_rule",
        &guild_id.to_string(),
        json!({"rule_id": rule_id, "role_id": role_id, "condition": req.condition}),
    )
    .await;

    let label = format!(
        "Re-evaluating @{} rule",
        role_name(&state, guild_id, role_id as u64).await
    );
    let job = enqueue_job(
        &state,
        guild_id,
        "resync",
        json!({"label": label}),
        actor.discord_id,
    )
    .await?;
    Ok(Json(StartedJobResponse { job: Some(job) }))
}

async fn remove_rule(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path(rule_id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    let guild_id = home_guild(&state)?;
    let repo = GuildConfigRepository::new(state.db.pool());
    let rules = repo
        .get_role_rules(guild_id as i64)
        .await
        .map_err(internal)?;
    let rule = rules
        .iter()
        .find(|r| r.id == rule_id)
        .ok_or_else(not_found)?;
    let role_id = rule.role_id;

    repo.remove_role_rule(rule_id).await.map_err(internal)?;

    crate::audit::log(
        &state,
        actor.discord_id,
        "remove_autorole_rule",
        &guild_id.to_string(),
        json!({"rule_id": rule_id, "role_id": role_id}),
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

async fn strip_role(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path(role_id): Path<u64>,
) -> Result<Json<StartedJobResponse>, ApiError> {
    let guild_id = home_guild(&state)?;
    crate::audit::log(
        &state,
        actor.discord_id,
        "strip_role",
        &guild_id.to_string(),
        json!({"role_id": role_id}),
    )
    .await;

    let label = format!(
        "Stripping @{} from all members",
        role_name(&state, guild_id, role_id).await
    );
    let payload = json!({"role_id": role_id.to_string(), "label": label});
    let job = enqueue_job(&state, guild_id, "strip_role", payload, actor.discord_id).await?;
    Ok(Json(StartedJobResponse { job: Some(job) }))
}

async fn list_jobs(State(state): State<AppState>) -> Result<Json<SyncJobsResponse>, ApiError> {
    let guild_id = home_guild(&state)?;
    let jobs = GuildSyncJobRepository::new(state.db.pool())
        .list_recent(guild_id as i64, FINISHED_JOBS_SHOWN)
        .await
        .map_err(internal)?;
    Ok(Json(SyncJobsResponse {
        jobs: jobs.iter().map(job_view).collect(),
    }))
}

async fn cancel_job(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path(job_id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    let guild_id = home_guild(&state)?;
    let repo = GuildSyncJobRepository::new(state.db.pool());
    let job = repo
        .get(job_id)
        .await
        .map_err(internal)?
        .filter(|j| j.guild_id == guild_id as i64)
        .ok_or_else(not_found)?;

    repo.request_cancel(job_id).await.map_err(internal)?;
    publish_sync_event(&state, &SyncEvent::GuildJobCancelRequested { job_id }).await;

    crate::audit::log(
        &state,
        actor.discord_id,
        "cancel_sync_job",
        &guild_id.to_string(),
        json!({"job_id": job_id, "kind": job.kind}),
    )
    .await;
    Ok(StatusCode::ACCEPTED)
}

async fn job_events(
    State(state): State<AppState>,
    Path(job_id): Path<i64>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let guild_id = home_guild(&state)?;
    let redis_url = state
        .redis_url
        .clone()
        .ok_or((StatusCode::SERVICE_UNAVAILABLE, "Redis unavailable".into()))?;
    let db = state.db.clone();

    let (tx, rx) = tokio::sync::mpsc::channel::<Event>(32);
    tokio::spawn(async move {
        let Ok(client) = redis::Client::open(redis_url.as_str()) else {
            return;
        };
        let Ok(mut pubsub) = client.get_async_pubsub().await else {
            return;
        };
        if pubsub.subscribe(SYNC_CHANNEL).await.is_err() {
            return;
        }

        let repo = GuildSyncJobRepository::new(db.pool());
        match repo.get(job_id).await {
            Ok(Some(job)) if job.guild_id == guild_id as i64 => {
                let finished = is_finished(&job.status);
                let _ = tx
                    .send(sse_event(
                        json!({"type": "snapshot", "job": job_view(&job)}),
                    ))
                    .await;
                if finished {
                    return;
                }
            }
            _ => return,
        }

        let mut stream = pubsub.into_on_message();
        while let Some(msg) = stream.next().await {
            let Ok(payload) = msg.get_payload::<String>() else {
                continue;
            };
            let Ok(event) = serde_json::from_str::<SyncEvent>(&payload) else {
                continue;
            };
            match event {
                SyncEvent::GuildJobProgress {
                    job_id: id,
                    processed,
                    total,
                    ..
                } if id == job_id => {
                    let event = sse_event(
                        json!({"type": "progress", "processed": processed, "total": total}),
                    );
                    if tx.send(event).await.is_err() {
                        break;
                    }
                }
                SyncEvent::GuildJobFinished {
                    job_id: id, status, ..
                } if id == job_id => {
                    let _ = tx
                        .send(sse_event(json!({"type": "finished", "status": status})))
                        .await;
                    break;
                }
                _ => {}
            }
        }
    });

    let stream = ReceiverStream::new(rx).map(Ok::<_, Infallible>);
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

fn sse_event(data: Value) -> Event {
    Event::default().data(data.to_string())
}

fn is_finished(status: &str) -> bool {
    matches!(status, "done" | "cancelled" | "failed")
}

fn job_view(job: &GuildSyncJob) -> SyncJobView {
    let label = job
        .payload
        .get("label")
        .and_then(|v| v.as_str())
        .unwrap_or(match job.kind.as_str() {
            "resync" => "Syncing all members",
            "clear_nicknames" => "Clearing all nicknames",
            "strip_role" => "Stripping role",
            "swap_role" => "Swapping role",
            _ => "Bulk update",
        })
        .to_string();

    SyncJobView {
        id: job.id,
        kind: job.kind.clone(),
        label,
        state: job.status.clone(),
        processed: job.processed,
        total: job.total.unwrap_or(0),
        started_at: job.started_at.unwrap_or(job.created_at),
        finished_at: job.finished_at,
    }
}

async fn enqueue_job(
    state: &AppState,
    guild_id: u64,
    kind: &str,
    payload: Value,
    created_by: i64,
) -> Result<SyncJobView, ApiError> {
    let job = GuildSyncJobRepository::new(state.db.pool())
        .enqueue(guild_id as i64, kind, &payload, Some(created_by))
        .await
        .map_err(internal)?;
    publish_sync_event(state, &SyncEvent::GuildJobQueued { job_id: job.id }).await;
    Ok(job_view(&job))
}

async fn publish_sync_event(state: &AppState, event: &SyncEvent) {
    match &state.redis {
        Some(pool) => SyncEventPublisher::new(pool.clone()).publish(event).await,
        None => tracing::warn!("Redis unavailable — sync event not published: {event:?}"),
    }
}

async fn swap_label(
    state: &AppState,
    guild_id: u64,
    which: ConfigRole,
    old_role: Option<i64>,
    new_role: Option<i64>,
) -> String {
    let old_name = match old_role {
        Some(id) => Some(role_name(state, guild_id, id as u64).await),
        None => None,
    };
    let new_name = match new_role {
        Some(id) => Some(role_name(state, guild_id, id as u64).await),
        None => None,
    };
    match (old_name, new_name) {
        (Some(old), Some(new)) => {
            let noun = which.noun();
            let capitalized = format!("{}{}", noun[..1].to_uppercase(), &noun[1..]);
            format!("{capitalized} role: @{old} → @{new}")
        }
        (None, Some(new)) => format!("Assigning @{new} to {} members", which.noun()),
        (Some(old), None) => format!("Removing @{old} from {} members", which.noun()),
        (None, None) => "Role update".to_string(),
    }
}

async fn role_name(state: &AppState, guild_id: u64, role_id: u64) -> String {
    let Ok(http) = discord::sync_http(state) else {
        return role_id.to_string();
    };
    discord::guild_roles(&http, GuildId::new(guild_id))
        .await
        .ok()
        .and_then(|roles| {
            roles
                .into_iter()
                .find(|r| r.id.get() == role_id)
                .map(|r| r.name.to_string())
        })
        .unwrap_or_else(|| role_id.to_string())
}

async fn guild_population(
    state: &AppState,
    http: &Http,
    guild_id: GuildId,
) -> anyhow::Result<(usize, i64)> {
    let members = discord::list_guild_members(http, guild_id).await?;
    let ids: Vec<i64> = members
        .iter()
        .filter(|m| !m.user.bot())
        .map(|m| m.user.id.get() as i64)
        .collect();

    let (linked,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM members WHERE discord_id = ANY($1) AND uuid IS NOT NULL",
    )
    .bind(&ids)
    .fetch_one(state.db.pool())
    .await?;

    Ok((ids.len(), linked))
}

fn role_views(roles: &[Role], bot_role_ids: &[RoleId], guild_id: GuildId) -> Vec<DiscordRoleView> {
    let bot_top_position = roles
        .iter()
        .filter(|r| bot_role_ids.contains(&r.id))
        .map(|r| r.position)
        .max()
        .unwrap_or(0);

    let mut views: Vec<DiscordRoleView> = roles
        .iter()
        .filter(|r| r.id.get() != guild_id.get())
        .map(|r| DiscordRoleView {
            id: r.id.to_string(),
            name: r.name.to_string(),
            color: discord::role_color_hex(r.colour),
            position: i64::from(r.position),
            managed: r.managed(),
            assignable: !r.managed() && r.position < bot_top_position,
        })
        .collect();
    views.sort_by(|a, b| b.position.cmp(&a.position));
    views
}

fn channel_views(channels: &[GuildChannel]) -> Vec<DiscordChannelView> {
    let categories: std::collections::HashMap<ChannelId, &str> = channels
        .iter()
        .filter(|c| c.base.kind == ChannelType::Category)
        .map(|c| (c.id, c.base.name.as_str()))
        .collect();

    channels
        .iter()
        .filter(|c| c.base.kind == ChannelType::Text)
        .map(|c| DiscordChannelView {
            id: c.id.to_string(),
            name: c.base.name.to_string(),
            category: c
                .parent_id
                .and_then(|p| categories.get(&p).map(|s| s.to_string())),
        })
        .collect()
}

async fn build_preview_context(
    state: &AppState,
    http: &Http,
    guild_id: GuildId,
    discord_id: i64,
) -> Option<Value> {
    let member = MemberRepository::new(state.db.pool())
        .get_by_discord_id(discord_id)
        .await
        .ok()??;
    let access = member.access_level;
    let uuid = member.uuid?;

    let cache_repo = CacheRepository::new(state.db.pool());
    let blacklist_repo = BlacklistRepository::new(state.db.pool());
    let (snapshot, tags, discord_member) = tokio::join!(
        cache_repo.get_latest_snapshot(&uuid),
        blacklist_repo.get_active_tags(&uuid),
        http.get_member(guild_id, UserId::new(discord_id as u64)),
    );

    let mut ctx = snapshot.ok()??;
    let discord_member = discord_member.ok()?;
    let active_tags: Vec<String> = tags
        .unwrap_or_default()
        .into_iter()
        .filter_map(|row| row.tag_type)
        .collect();

    if ctx.pointer("/achievements/bedwars_level").is_none() {
        ctx["achievements"]["bedwars_level"] = json!(0);
    }
    ctx["discord"] = json!({
        "name": discord_member
            .user
            .global_name
            .as_deref()
            .unwrap_or(&discord_member.user.name),
    });
    ctx["coral"] = json!({ "access": access });

    let highest = active_tags
        .iter()
        .filter_map(|t| blacklist::lookup(t).map(|def| (def.priority, t.as_str())))
        .min_by_key(|(p, _)| *p)
        .map(|(_, name)| name);
    let mut bl = json!({ "tag": highest });
    for def in blacklist::all() {
        bl[def.name] = Value::Bool(active_tags.iter().any(|t| t == def.name));
    }
    ctx["blacklist"] = bl;

    Some(ctx)
}

fn link_embed_message() -> CreateMessage<'static> {
    CreateMessage::new()
        .flags(MessageFlags::IS_COMPONENTS_V2)
        .components(vec![CreateComponent::Container(CreateContainer::new(
            vec![
                discord::text(
                    "## Account Linking\n\n\
                     Link your Minecraft account to get roles and a nickname in this server.\n\n\
                     Use the `/link` command or the button below to get started.",
                ),
                discord::separator(),
                CreateContainerComponent::ActionRow(CreateActionRow::buttons(vec![
                    CreateButton::new("setup_link")
                        .label("Link Account")
                        .style(ButtonStyle::Primary),
                ])),
            ],
        ))])
}

fn icon_url(guild_id: GuildId, icon: Option<ImageHash>) -> Option<String> {
    icon.map(|hash| format!("https://cdn.discordapp.com/icons/{guild_id}/{hash}.png?size=128"))
}

fn parse_optional_id(value: Option<&str>) -> Result<Option<i64>, ApiError> {
    match value {
        Some(raw) => raw
            .parse::<i64>()
            .map(Some)
            .map_err(|_| bad_request("Invalid id".into())),
        None => Ok(None),
    }
}

fn internal(e: impl std::fmt::Display) -> ApiError {
    tracing::error!("server route error: {e}");
    (StatusCode::INTERNAL_SERVER_ERROR, "Internal error".into())
}

fn not_found() -> ApiError {
    (StatusCode::NOT_FOUND, "Not found".into())
}

fn service_unavailable(e: anyhow::Error) -> ApiError {
    tracing::warn!("{e:#}");
    (StatusCode::SERVICE_UNAVAILABLE, format!("{e}"))
}

fn bad_gateway(e: impl Into<anyhow::Error>) -> ApiError {
    let e = e.into();
    tracing::warn!("Discord request failed: {e:#}");
    (StatusCode::BAD_GATEWAY, "Discord request failed".into())
}

fn bad_request(message: String) -> ApiError {
    (StatusCode::BAD_REQUEST, message)
}
