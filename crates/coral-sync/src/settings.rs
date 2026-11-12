//! The sync's settings API: the only way other services read or change a server's
//! sync settings and jobs. Private network, internal key. Callers keep their own
//! sign-in, permissions and audit log; every write answers with the audit detail.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{FromRequestParts, Path, Query, Request, State};
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize, Serializer};
use serde_json::{Value, json};
use serenity::all::{
    ButtonStyle, CreateActionRow, CreateButton, CreateComponent, CreateContainer,
    CreateContainerComponent, CreateMessage, CreateSeparator, CreateTextDisplay, GenericChannelId,
    GuildId, Http, MessageFlags, MessageId,
};
use tokio::net::TcpListener;

use coral_redis::{SyncEvent, SyncEventPublisher};
use database::{
    DiscordUsernameCacheRepository, GuildConfig, GuildConfigRepository, GuildSyncJob,
    GuildSyncJobRepository, MemberRepository, SqlxError,
};

use crate::framework::Data;
use crate::sources::{self, HypixelRead};

const API_KEY_HEADER: &str = "X-API-Key";
const ACTOR_HEADER: &str = "X-Coral-Actor";
const FINISHED_JOBS_SHOWN: i64 = 5;
const RULE_PRIORITY: i32 = 0;
const LINK_PROMPT: &str = "## Account Linking\n\n\
     Link your Minecraft account to get roles and a nickname in this server.\n\n\
     Use the `/link` command or the button below to get started.";
const LINK_BUTTON_ID: &str = "setup_link";
const LINK_BUTTON_LABEL: &str = "Link Account";

#[derive(Clone)]
struct ApiState {
    data: Data,
    http: Arc<Http>,
    key: Arc<str>,
}

#[derive(Debug)]
pub enum SettingsError {
    NotFound(String),
    BadRequest(String),
    Rejected(String),
    Upstream(String),
    Internal(String),
}

/// The Discord user a write is made for, as vouched by the caller.
struct Actor(i64);

#[derive(Serialize)]
struct ServerConfig {
    guild_id: String,
    config: ConfigView,
    rules: Vec<RuleView>,
    preview_context: Option<Value>,
}

#[derive(Default, Serialize)]
struct ConfigView {
    link_role_id: Option<String>,
    unlinked_role_id: Option<String>,
    link_channel_id: Option<String>,
    link_message_id: Option<String>,
    nickname_template: Option<String>,
}

#[derive(Serialize)]
struct RuleView {
    id: i64,
    role_id: String,
    condition: String,
}

#[derive(Serialize)]
pub struct SyncJob {
    id: i64,
    kind: String,
    label: String,
    state: String,
    processed: i32,
    total: i32,
    error: Option<String>,
    #[serde(serialize_with = "micros")]
    started_at: DateTime<Utc>,
    #[serde(serialize_with = "micros_opt")]
    finished_at: Option<DateTime<Utc>>,
}

#[derive(Serialize)]
struct SyncJobs {
    jobs: Vec<SyncJob>,
}

/// What a write did: the job it started, if any, and what the caller should audit.
#[derive(Serialize)]
struct Change {
    job: Option<SyncJob>,
    audit: Value,
}

#[derive(Deserialize)]
struct PreviewQuery {
    preview_for: Option<i64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RoleBody {
    role_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ChannelBody {
    channel_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TemplateBody {
    template: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NewRuleBody {
    role_id: String,
    condition: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleBody {
    condition: String,
}

#[derive(Clone, Copy)]
enum ConfigRole {
    Linked,
    Unlinked,
}

struct RoleNames(HashMap<i64, String>);

impl IntoResponse for SettingsError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::NotFound(m) => (StatusCode::NOT_FOUND, m),
            Self::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
            Self::Rejected(m) => (StatusCode::UNPROCESSABLE_ENTITY, m),
            Self::Upstream(m) => (StatusCode::BAD_GATEWAY, m),
            Self::Internal(m) => {
                tracing::error!("settings API: {m}");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal error".into())
            }
        };
        (status, Json(json!({ "error": message }))).into_response()
    }
}

impl From<SqlxError> for SettingsError {
    fn from(error: SqlxError) -> Self {
        Self::Internal(error.to_string())
    }
}

impl<S: Send + Sync> FromRequestParts<S> for Actor {
    type Rejection = SettingsError;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, SettingsError> {
        parts
            .headers
            .get(ACTOR_HEADER)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse().ok())
            .map(Actor)
            .ok_or_else(|| SettingsError::BadRequest(format!("{ACTOR_HEADER} is required")))
    }
}

impl ConfigRole {
    fn field(self) -> &'static str {
        match self {
            Self::Linked => "link",
            Self::Unlinked => "unlinked",
        }
    }

    fn noun(self) -> &'static str {
        match self {
            Self::Linked => "linked",
            Self::Unlinked => "unlinked",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::Linked => "Linked",
            Self::Unlinked => "Unlinked",
        }
    }

    fn current(self, config: &GuildConfig) -> Option<i64> {
        match self {
            Self::Linked => config.link_role_id,
            Self::Unlinked => config.unlinked_role_id,
        }
    }
}

impl RoleNames {
    async fn load(http: &Http, guild_id: GuildId) -> Result<Self, SettingsError> {
        let roles = http
            .get_guild_roles(guild_id)
            .await
            .map_err(|e| SettingsError::Upstream(format!("Discord: read the roles: {e}")))?;
        Ok(Self(
            roles
                .into_iter()
                .map(|role| (role.id.get() as i64, role.name.to_string()))
                .collect(),
        ))
    }

    fn name(&self, role_id: i64) -> String {
        self.0
            .get(&role_id)
            .cloned()
            .unwrap_or_else(|| role_id.to_string())
    }
}

/// Serves until the process exits; a failure to bind is logged, not fatal, so the
/// bot keeps syncing even if the port is taken.
pub async fn serve(data: Data, http: Arc<Http>, key: String, port: u16) {
    let app = router(ApiState {
        data,
        http,
        key: key.into(),
    });
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    match TcpListener::bind(addr).await {
        Ok(listener) => {
            tracing::info!("Settings API listening on {addr}");
            if let Err(e) = axum::serve(listener, app).await {
                tracing::error!("Settings API stopped: {e}");
            }
        }
        Err(e) => tracing::error!("Settings API could not bind {addr}: {e}"),
    }
}

fn router(state: ApiState) -> Router {
    Router::new()
        .route("/guilds/{guild_id}/config", get(config))
        .route("/guilds/{guild_id}/link-role", put(set_link_role))
        .route("/guilds/{guild_id}/unlinked-role", put(set_unlinked_role))
        .route("/guilds/{guild_id}/link-channel", put(set_link_channel))
        .route(
            "/guilds/{guild_id}/nickname-template",
            put(set_nickname_template),
        )
        .route("/guilds/{guild_id}/nicknames/reset", post(reset_nicknames))
        .route("/guilds/{guild_id}/rules", post(add_rule))
        .route(
            "/guilds/{guild_id}/rules/{rule_id}",
            put(update_rule).delete(remove_rule),
        )
        .route("/guilds/{guild_id}/roles/{role_id}/strip", post(strip_role))
        .route("/guilds/{guild_id}/jobs", get(list_jobs))
        .route(
            "/guilds/{guild_id}/jobs/{job_id}",
            get(get_job).delete(cancel_job),
        )
        .route_layer(middleware::from_fn_with_state(state.clone(), require_key))
        .with_state(state)
}

async fn require_key(State(state): State<ApiState>, request: Request, next: Next) -> Response {
    let presented = request
        .headers()
        .get(API_KEY_HEADER)
        .and_then(|value| value.to_str().ok());
    if presented != Some(&*state.key) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    next.run(request).await
}

async fn config(
    State(state): State<ApiState>,
    Path(guild_id): Path<u64>,
    Query(query): Query<PreviewQuery>,
) -> Result<Json<ServerConfig>, SettingsError> {
    let repo = GuildConfigRepository::new(state.data.db.pool());
    let config = repo.get(guild_id as i64).await?;
    let rules = repo.get_role_rules(guild_id as i64).await?;
    let preview_context = match query.preview_for {
        Some(discord_id) => preview_context(&state.data, discord_id).await?,
        None => None,
    };
    Ok(Json(ServerConfig {
        guild_id: guild_id.to_string(),
        config: config.as_ref().map(config_view).unwrap_or_default(),
        rules: rules
            .into_iter()
            .map(|rule| RuleView {
                id: rule.id,
                role_id: rule.role_id.to_string(),
                condition: rule.condition,
            })
            .collect(),
        preview_context,
    }))
}

async fn set_link_role(
    State(state): State<ApiState>,
    actor: Actor,
    Path(guild_id): Path<u64>,
    Json(body): Json<RoleBody>,
) -> Result<Json<Change>, SettingsError> {
    swap_config_role(&state, actor, guild_id, body.role_id, ConfigRole::Linked).await
}

async fn set_unlinked_role(
    State(state): State<ApiState>,
    actor: Actor,
    Path(guild_id): Path<u64>,
    Json(body): Json<RoleBody>,
) -> Result<Json<Change>, SettingsError> {
    swap_config_role(&state, actor, guild_id, body.role_id, ConfigRole::Unlinked).await
}

async fn set_link_channel(
    State(state): State<ApiState>,
    Actor(actor): Actor,
    Path(guild_id): Path<u64>,
    Json(body): Json<ChannelBody>,
) -> Result<Json<Change>, SettingsError> {
    let new_channel = parse_id(body.channel_id.as_deref())?;
    let repo = GuildConfigRepository::new(state.data.db.pool());
    let config = repo.upsert(guild_id as i64, actor).await?;
    if let (Some(channel), Some(message)) = (config.link_channel_id, config.link_message_id)
        && let Err(error) = GenericChannelId::new(channel as u64)
            .delete_message(&state.http, MessageId::new(message as u64), None)
            .await
    {
        tracing::warn!(channel, %error, "the previous link prompt could not be deleted");
    }
    let message_id = match new_channel {
        Some(channel) => Some(post_link_prompt(&state.http, channel).await?),
        None => None,
    };
    repo.set_link_channel(guild_id as i64, new_channel, message_id)
        .await?;
    Ok(Json(Change {
        job: None,
        audit: json!({ "old_channel_id": config.link_channel_id, "new_channel_id": new_channel }),
    }))
}

async fn set_nickname_template(
    State(state): State<ApiState>,
    Actor(actor): Actor,
    Path(guild_id): Path<u64>,
    Json(body): Json<TemplateBody>,
) -> Result<Json<Change>, SettingsError> {
    let template = body.template.filter(|template| !template.trim().is_empty());
    if let Some(template) = &template {
        expr::validate_template(template)
            .map_err(|error| SettingsError::BadRequest(format!("Invalid template: {error}")))?;
    }
    let repo = GuildConfigRepository::new(state.data.db.pool());
    repo.upsert(guild_id as i64, actor).await?;
    repo.set_nickname_template(guild_id as i64, template.as_deref())
        .await?;
    let label = match template {
        Some(_) => "Applying display name format",
        None => "Syncing display names",
    };
    let job = enqueue(
        &state.data,
        guild_id,
        "resync",
        json!({ "label": label }),
        actor,
    )
    .await?;
    Ok(Json(Change {
        job: Some(job),
        audit: json!({ "template": template }),
    }))
}

async fn reset_nicknames(
    State(state): State<ApiState>,
    Actor(actor): Actor,
    Path(guild_id): Path<u64>,
) -> Result<Json<Change>, SettingsError> {
    let payload = json!({ "label": "Clearing all nicknames" });
    let job = enqueue(&state.data, guild_id, "clear_nicknames", payload, actor).await?;
    Ok(Json(Change {
        job: Some(job),
        audit: json!({}),
    }))
}

async fn add_rule(
    State(state): State<ApiState>,
    Actor(actor): Actor,
    Path(guild_id): Path<u64>,
    Json(body): Json<NewRuleBody>,
) -> Result<Json<Change>, SettingsError> {
    let role_id = parse_id(Some(&body.role_id))?
        .ok_or_else(|| SettingsError::BadRequest("a rule needs a role".into()))?;
    validate_condition(&body.condition)?;
    let names = RoleNames::load(&state.http, GuildId::new(guild_id)).await?;
    let repo = GuildConfigRepository::new(state.data.db.pool());
    repo.upsert(guild_id as i64, actor).await?;
    let rules = repo.get_role_rules(guild_id as i64).await?;
    if rules.iter().any(|rule| rule.role_id == role_id) {
        return Err(SettingsError::Rejected(
            "A rule already exists for that role. Edit or remove it first.".into(),
        ));
    }
    repo.add_role_rule(guild_id as i64, role_id, &body.condition, RULE_PRIORITY)
        .await?;
    let label = format!("Evaluating @{} rule for all members", names.name(role_id));
    let job = enqueue(
        &state.data,
        guild_id,
        "resync",
        json!({ "label": label }),
        actor,
    )
    .await?;
    Ok(Json(Change {
        job: Some(job),
        audit: json!({ "role_id": role_id, "condition": body.condition }),
    }))
}

async fn update_rule(
    State(state): State<ApiState>,
    Actor(actor): Actor,
    Path((guild_id, rule_id)): Path<(u64, i64)>,
    Json(body): Json<RuleBody>,
) -> Result<Json<Change>, SettingsError> {
    validate_condition(&body.condition)?;
    let repo = GuildConfigRepository::new(state.data.db.pool());
    let role_id = rule_role(&repo, guild_id, rule_id).await?;
    let names = RoleNames::load(&state.http, GuildId::new(guild_id)).await?;
    repo.update_role_rule_condition(rule_id, &body.condition)
        .await?;
    let label = format!("Re-evaluating @{} rule", names.name(role_id));
    let job = enqueue(
        &state.data,
        guild_id,
        "resync",
        json!({ "label": label }),
        actor,
    )
    .await?;
    Ok(Json(Change {
        job: Some(job),
        audit: json!({ "rule_id": rule_id, "role_id": role_id, "condition": body.condition }),
    }))
}

async fn remove_rule(
    State(state): State<ApiState>,
    Path((guild_id, rule_id)): Path<(u64, i64)>,
) -> Result<Json<Change>, SettingsError> {
    let repo = GuildConfigRepository::new(state.data.db.pool());
    let role_id = rule_role(&repo, guild_id, rule_id).await?;
    repo.remove_role_rule(rule_id).await?;
    Ok(Json(Change {
        job: None,
        audit: json!({ "rule_id": rule_id, "role_id": role_id }),
    }))
}

async fn strip_role(
    State(state): State<ApiState>,
    Actor(actor): Actor,
    Path((guild_id, role_id)): Path<(u64, String)>,
) -> Result<Json<Change>, SettingsError> {
    let role_id = parse_id(Some(&role_id))?
        .ok_or_else(|| SettingsError::BadRequest("a role id is required".into()))?;
    let names = RoleNames::load(&state.http, GuildId::new(guild_id)).await?;
    let label = format!("Stripping @{} from all members", names.name(role_id));
    let payload = json!({ "role_id": role_id.to_string(), "label": label });
    let job = enqueue(&state.data, guild_id, "strip_role", payload, actor).await?;
    Ok(Json(Change {
        job: Some(job),
        audit: json!({ "role_id": role_id }),
    }))
}

async fn list_jobs(
    State(state): State<ApiState>,
    Path(guild_id): Path<u64>,
) -> Result<Json<SyncJobs>, SettingsError> {
    let jobs = GuildSyncJobRepository::new(state.data.db.pool())
        .list_recent(guild_id as i64, FINISHED_JOBS_SHOWN)
        .await?;
    Ok(Json(SyncJobs {
        jobs: jobs.iter().map(job_view).collect(),
    }))
}

async fn get_job(
    State(state): State<ApiState>,
    Path((guild_id, job_id)): Path<(u64, i64)>,
) -> Result<Json<SyncJob>, SettingsError> {
    let repo = GuildSyncJobRepository::new(state.data.db.pool());
    Ok(Json(job_view(&guild_job(&repo, guild_id, job_id).await?)))
}

async fn cancel_job(
    State(state): State<ApiState>,
    Path((guild_id, job_id)): Path<(u64, i64)>,
) -> Result<Json<Change>, SettingsError> {
    let repo = GuildSyncJobRepository::new(state.data.db.pool());
    let job = guild_job(&repo, guild_id, job_id).await?;
    repo.request_cancel(job_id).await?;
    publish(&state.data, SyncEvent::GuildJobCancelRequested { job_id }).await;
    Ok(Json(Change {
        job: None,
        audit: json!({ "job_id": job_id, "kind": job.kind }),
    }))
}

async fn swap_config_role(
    state: &ApiState,
    Actor(actor): Actor,
    guild_id: u64,
    role_id: Option<String>,
    which: ConfigRole,
) -> Result<Json<Change>, SettingsError> {
    let new_role = parse_id(role_id.as_deref())?;
    let names = RoleNames::load(&state.http, GuildId::new(guild_id)).await?;
    let repo = GuildConfigRepository::new(state.data.db.pool());
    let config = repo.upsert(guild_id as i64, actor).await?;
    let old_role = which.current(&config);
    match which {
        ConfigRole::Linked => repo.set_link_role(guild_id as i64, new_role).await?,
        ConfigRole::Unlinked => repo.set_unlinked_role(guild_id as i64, new_role).await?,
    };
    let audit = json!({ "old_role_id": old_role, "new_role_id": new_role });
    if old_role == new_role {
        return Ok(Json(Change { job: None, audit }));
    }
    let label = swap_label(
        which,
        old_role.map(|id| names.name(id)),
        new_role.map(|id| names.name(id)),
    );
    let payload = json!({
        "old_role_id": old_role.map(|id| id.to_string()),
        "new_role_id": new_role.map(|id| id.to_string()),
        "field": which.field(),
        "label": label,
    });
    let job = enqueue(&state.data, guild_id, "swap_role", payload, actor).await?;
    Ok(Json(Change {
        job: Some(job),
        audit,
    }))
}

async fn preview_context(data: &Data, discord_id: i64) -> Result<Option<Value>, SettingsError> {
    let Some(uuid) = MemberRepository::new(data.db.pool())
        .get_by_discord_id(discord_id)
        .await?
        .and_then(|member| member.uuid)
    else {
        return Ok(None);
    };
    let Some(hypixel) = sources::hypixel(data, &uuid, HypixelRead::Stored).await else {
        return Ok(None);
    };
    let name = DiscordUsernameCacheRepository::new(data.db.pool())
        .get(discord_id)
        .await?
        .map(|cached| cached.username);
    let coral = sources::coral(data, discord_id as u64, &uuid).await;
    Ok(Some(sources::context(&hypixel, name.as_deref(), &coral)))
}

async fn enqueue(
    data: &Data,
    guild_id: u64,
    kind: &str,
    payload: Value,
    created_by: i64,
) -> Result<SyncJob, SettingsError> {
    let job = GuildSyncJobRepository::new(data.db.pool())
        .enqueue(guild_id as i64, kind, &payload, Some(created_by))
        .await?;
    publish(data, SyncEvent::GuildJobQueued { job_id: job.id }).await;
    Ok(job_view(&job))
}

async fn publish(data: &Data, event: SyncEvent) {
    match &data.redis {
        Some(redis) => SyncEventPublisher::new(redis.clone()).publish(&event).await,
        None => tracing::warn!("Redis is unavailable; a sync event was not published"),
    }
}

async fn guild_job(
    repo: &GuildSyncJobRepository<'_>,
    guild_id: u64,
    job_id: i64,
) -> Result<GuildSyncJob, SettingsError> {
    repo.get(job_id)
        .await?
        .filter(|job| job.guild_id == guild_id as i64)
        .ok_or_else(|| SettingsError::NotFound(format!("no sync job {job_id} on this server")))
}

async fn rule_role(
    repo: &GuildConfigRepository<'_>,
    guild_id: u64,
    rule_id: i64,
) -> Result<i64, SettingsError> {
    repo.get_role_rules(guild_id as i64)
        .await?
        .into_iter()
        .find(|rule| rule.id == rule_id)
        .map(|rule| rule.role_id)
        .ok_or_else(|| SettingsError::NotFound(format!("no autorole rule {rule_id}")))
}

async fn post_link_prompt(http: &Http, channel: i64) -> Result<i64, SettingsError> {
    let message = GenericChannelId::new(channel as u64)
        .send_message(http, link_prompt())
        .await
        .map_err(|error| {
            SettingsError::Upstream(format!("Discord: post the link prompt: {error}"))
        })?;
    Ok(message.id.get() as i64)
}

fn link_prompt() -> CreateMessage<'static> {
    CreateMessage::new()
        .flags(MessageFlags::IS_COMPONENTS_V2)
        .components(vec![CreateComponent::Container(CreateContainer::new(
            vec![
                CreateContainerComponent::TextDisplay(CreateTextDisplay::new(LINK_PROMPT)),
                CreateContainerComponent::Separator(CreateSeparator::new(true)),
                CreateContainerComponent::ActionRow(CreateActionRow::buttons(vec![
                    CreateButton::new(LINK_BUTTON_ID)
                        .label(LINK_BUTTON_LABEL)
                        .style(ButtonStyle::Primary),
                ])),
            ],
        ))])
}

fn swap_label(which: ConfigRole, old_name: Option<String>, new_name: Option<String>) -> String {
    match (old_name, new_name) {
        (Some(old), Some(new)) => format!("{} role: @{old} → @{new}", which.title()),
        (None, Some(new)) => format!("Assigning @{new} to {} members", which.noun()),
        (Some(old), None) => format!("Removing @{old} from {} members", which.noun()),
        (None, None) => "Role update".to_string(),
    }
}

fn validate_condition(condition: &str) -> Result<(), SettingsError> {
    expr::validate_condition(condition)
        .map_err(|error| SettingsError::BadRequest(format!("Invalid condition: {error}")))
}

fn parse_id(raw: Option<&str>) -> Result<Option<i64>, SettingsError> {
    match raw.map(str::trim).filter(|raw| !raw.is_empty()) {
        None => Ok(None),
        Some(raw) => raw
            .parse::<i64>()
            .ok()
            .filter(|id| *id > 0)
            .map(Some)
            .ok_or_else(|| SettingsError::BadRequest(format!("{raw:?} is not a Discord id"))),
    }
}

fn config_view(config: &GuildConfig) -> ConfigView {
    ConfigView {
        link_role_id: config.link_role_id.map(|id| id.to_string()),
        unlinked_role_id: config.unlinked_role_id.map(|id| id.to_string()),
        link_channel_id: config.link_channel_id.map(|id| id.to_string()),
        link_message_id: config.link_message_id.map(|id| id.to_string()),
        nickname_template: config.nickname_template.clone(),
    }
}

fn job_view(job: &GuildSyncJob) -> SyncJob {
    let label = job
        .payload
        .get("label")
        .and_then(Value::as_str)
        .unwrap_or_else(|| default_label(&job.kind))
        .to_string();
    SyncJob {
        id: job.id,
        kind: job.kind.clone(),
        label,
        state: job.status.clone(),
        processed: job.processed,
        total: job.total.unwrap_or(0),
        error: job.error.clone(),
        started_at: job.started_at.unwrap_or(job.created_at),
        finished_at: job.finished_at,
    }
}

fn default_label(kind: &str) -> &'static str {
    match kind {
        "resync" => "Syncing all members",
        "clear_nicknames" => "Clearing all nicknames",
        "strip_role" => "Stripping role",
        "swap_role" => "Swapping role",
        _ => "Bulk update",
    }
}

fn micros<S: Serializer>(value: &DateTime<Utc>, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&value.to_rfc3339_opts(SecondsFormat::Micros, true))
}

fn micros_opt<S: Serializer>(
    value: &Option<DateTime<Utc>>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match value {
        Some(value) => micros(value, serializer),
        None => serializer.serialize_none(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swap_labels_name_the_roles_on_each_side() {
        let name = |text: &str| Some(text.to_string());
        assert_eq!(
            swap_label(ConfigRole::Linked, name("Old"), name("New")),
            "Linked role: @Old → @New"
        );
        assert_eq!(
            swap_label(ConfigRole::Unlinked, None, name("Guest")),
            "Assigning @Guest to unlinked members"
        );
        assert_eq!(
            swap_label(ConfigRole::Linked, name("Verified"), None),
            "Removing @Verified from linked members"
        );
        assert_eq!(swap_label(ConfigRole::Linked, None, None), "Role update");
    }

    #[test]
    fn deleted_roles_fall_back_to_their_id() {
        let names = RoleNames(HashMap::from([(7, "Verified".to_string())]));
        assert_eq!(names.name(7), "Verified");
        assert_eq!(names.name(9), "9");
    }

    #[test]
    fn ids_must_be_positive_numbers() {
        assert_eq!(parse_id(None).unwrap(), None);
        assert_eq!(parse_id(Some(" ")).unwrap(), None);
        assert_eq!(parse_id(Some("42")).unwrap(), Some(42));
        assert!(parse_id(Some("-1")).is_err());
        assert!(parse_id(Some("role")).is_err());
    }
}

#[cfg(test)]
mod api_tests {
    use std::sync::Mutex;
    use std::sync::atomic::AtomicUsize;

    use axum::body::Body;
    use axum::http::{Method, Request};
    use database::Database;
    use tower::ServiceExt;

    use super::*;
    use crate::api::CoralApiClient;

    const KEY: &str = "settings-test-key";
    const ACTOR: i64 = 42;

    async fn api() -> Option<Router> {
        dotenvy::dotenv().ok();
        let db = Database::connect(&std::env::var("DATABASE_URL").ok()?)
            .await
            .ok()?;
        let redis_url = std::env::var("REDIS_URL").ok()?;
        let data = Data {
            db: Arc::new(db),
            api: Arc::new(CoralApiClient::new("http://unused".into(), KEY.into())),
            owner_ids: Vec::new(),
            home_guild_id: None,
            redis: crate::jobs::connect_redis(&redis_url).await,
            redis_url,
            sync_cooldowns: Arc::new(Mutex::new(HashMap::new())),
            job_cancel_tokens: Arc::new(Mutex::new(HashMap::new())),
            active_interactions: Arc::new(AtomicUsize::new(0)),
        };
        Some(router(ApiState {
            data,
            http: Arc::new(Http::new("never.used.token".parse().unwrap())),
            key: KEY.into(),
        }))
    }

    async fn call(
        router: &Router,
        method: Method,
        uri: &str,
        key: &str,
        body: Value,
    ) -> (u16, Value) {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .header(API_KEY_HEADER, key)
            .header(ACTOR_HEADER, ACTOR)
            .header("Content-Type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        let response = router.clone().oneshot(request).await.unwrap();
        let status = response.status().as_u16();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    fn guild() -> u64 {
        990_000_000_000_000_000 + u64::from(uuid::Uuid::new_v4().as_fields().1)
    }

    #[tokio::test]
    async fn the_internal_key_is_required() {
        let Some(api) = api().await else {
            return;
        };
        let uri = format!("/guilds/{}/config", guild());
        assert_eq!(
            call(&api, Method::GET, &uri, "wrong", Value::Null).await.0,
            401
        );
        let (status, config) = call(&api, Method::GET, &uri, KEY, Value::Null).await;
        assert_eq!(status, 200);
        assert_eq!(config["config"]["nickname_template"], Value::Null);
        assert_eq!(config["rules"], json!([]));
    }

    #[tokio::test]
    async fn a_template_is_validated_stored_and_applied_by_a_job() {
        let Some(api) = api().await else {
            return;
        };
        let guild = guild();
        let uri = format!("/guilds/{guild}/nickname-template");
        let bad = call(
            &api,
            Method::PUT,
            &uri,
            KEY,
            json!({"template": "{level >=}"}),
        )
        .await;
        assert_eq!(bad.0, 400);

        let (status, change) = call(
            &api,
            Method::PUT,
            &uri,
            KEY,
            json!({"template": "{displayname}"}),
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(change["audit"], json!({"template": "{displayname}"}));
        assert_eq!(change["job"]["kind"], "resync");
        assert_eq!(change["job"]["label"], "Applying display name format");

        let config_uri = format!("/guilds/{guild}/config");
        let (_, config) = call(&api, Method::GET, &config_uri, KEY, Value::Null).await;
        assert_eq!(config["config"]["nickname_template"], "{displayname}");
    }

    #[tokio::test]
    async fn jobs_are_listed_and_cancelled_only_on_their_own_server() {
        let Some(api) = api().await else {
            return;
        };
        let (guild, other) = (guild(), guild());
        let (_, change) = call(
            &api,
            Method::POST,
            &format!("/guilds/{guild}/nicknames/reset"),
            KEY,
            Value::Null,
        )
        .await;
        let job_id = change["job"]["id"].as_i64().unwrap();

        let (_, jobs) = call(
            &api,
            Method::GET,
            &format!("/guilds/{guild}/jobs"),
            KEY,
            Value::Null,
        )
        .await;
        assert_eq!(jobs["jobs"][0]["id"], job_id);

        let elsewhere = format!("/guilds/{other}/jobs/{job_id}");
        assert_eq!(
            call(&api, Method::DELETE, &elsewhere, KEY, Value::Null)
                .await
                .0,
            404
        );
        let here = format!("/guilds/{guild}/jobs/{job_id}");
        let (status, cancelled) = call(&api, Method::DELETE, &here, KEY, Value::Null).await;
        assert_eq!(status, 200);
        assert_eq!(cancelled["audit"]["kind"], "clear_nicknames");
    }

    #[tokio::test]
    async fn removing_an_unknown_rule_is_not_found() {
        let Some(api) = api().await else {
            return;
        };
        let uri = format!("/guilds/{}/rules/{}", guild(), i64::MAX);
        assert_eq!(
            call(&api, Method::DELETE, &uri, KEY, Value::Null).await.0,
            404
        );
    }
}
