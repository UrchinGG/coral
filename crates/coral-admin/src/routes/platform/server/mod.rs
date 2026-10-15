use std::collections::HashMap;

use axum::extract::rejection::JsonRejection;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::{Extension, Json};
use database::{GuildConfig, GuildConfigRepository};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use serenity::all::{
    ButtonStyle, CreateActionRow, CreateButton, CreateComponent, CreateContainer,
    CreateContainerComponent, CreateMessage, GenericChannelId, GuildId, MessageFlags, MessageId,
};

use self::directory::guild_roles;
use self::jobs::{StartedJob, enqueue};
use super::parse_role;
use crate::auth::AdminActor;
use crate::discord;
use crate::error::{AdminError, Context};
use crate::state::AppState;

pub mod directory;
pub mod jobs;
mod preview;

const LINK_PROMPT: &str = "## Account Linking\n\n\
     Link your Minecraft account to get roles and a nickname in this server.\n\n\
     Use the `/link` command or the button below to get started.";
const LINK_BUTTON_ID: &str = "setup_link";
const LINK_BUTTON_LABEL: &str = "Link Account";
const RULE_PRIORITY: i32 = 0;

#[derive(Debug, Serialize)]
pub struct ServerConfig {
    guild_id: String,
    config: ConfigView,
    rules: Vec<RuleView>,
    preview_context: Option<Value>,
}

#[derive(Debug, Default, Serialize)]
struct ConfigView {
    link_role_id: Option<String>,
    unlinked_role_id: Option<String>,
    link_channel_id: Option<String>,
    link_message_id: Option<String>,
    nickname_template: Option<String>,
}

#[derive(Debug, Serialize)]
struct RuleView {
    id: i64,
    role_id: String,
    condition: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoleBody {
    role_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelBody {
    channel_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateBody {
    template: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewRuleBody {
    role_id: String,
    condition: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleBody {
    condition: String,
}

struct RoleNames(HashMap<i64, String>);

#[derive(Clone, Copy)]
enum ConfigRole {
    Linked,
    Unlinked,
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

    fn action(self) -> &'static str {
        match self {
            Self::Linked => "set_link_role",
            Self::Unlinked => "set_unlinked_role",
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
    async fn load(state: &AppState, guild_id: GuildId) -> Result<Self, AdminError> {
        let http = state
            .discord
            .sync()
            .map_err(|error| AdminError::Rejected(error.to_string()))?;
        let roles = guild_roles(http, guild_id).await?;
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

pub fn home_guild(state: &AppState) -> Result<GuildId, AdminError> {
    state
        .discord
        .home_guild_id
        .and_then(|id| u64::try_from(id).ok())
        .filter(|id| *id > 0)
        .map(GuildId::new)
        .ok_or_else(|| AdminError::NotFound("no home server is configured (HOME_GUILD_ID)".into()))
}

pub async fn config(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
) -> Result<Json<ServerConfig>, AdminError> {
    let guild_id = home_guild(&state)?;
    let repo = GuildConfigRepository::new(state.db.pool());
    let (config, rules, preview_context) = tokio::try_join!(
        async {
            repo.get(guild_key(guild_id))
                .await
                .context("read the server config")
        },
        async {
            repo.get_role_rules(guild_key(guild_id))
                .await
                .context("read the autorole rules")
        },
        preview::preview_context(&state, actor.discord_id),
    )?;
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

pub async fn set_link_role(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    body: Result<Json<RoleBody>, JsonRejection>,
) -> Result<Json<StartedJob>, AdminError> {
    let Json(body) = body?;
    swap_config_role(&state, actor, body.role_id, ConfigRole::Linked).await
}

pub async fn set_unlinked_role(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    body: Result<Json<RoleBody>, JsonRejection>,
) -> Result<Json<StartedJob>, AdminError> {
    let Json(body) = body?;
    swap_config_role(&state, actor, body.role_id, ConfigRole::Unlinked).await
}

pub async fn set_link_channel(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    body: Result<Json<ChannelBody>, JsonRejection>,
) -> Result<StatusCode, AdminError> {
    let Json(body) = body?;
    let guild_id = home_guild(&state)?;
    let http = state
        .discord
        .sync()
        .map_err(|error| AdminError::Rejected(error.to_string()))?;
    let new_channel = parse_role(body.channel_id.as_deref())?;
    let repo = GuildConfigRepository::new(state.db.pool());
    let config = repo
        .upsert(guild_key(guild_id), actor.discord_id)
        .await
        .context("create the server config")?;
    if let (Some(channel), Some(message)) = (config.link_channel_id, config.link_message_id)
        && let Err(error) = GenericChannelId::new(channel as u64)
            .delete_message(http, MessageId::new(message as u64), None)
            .await
    {
        tracing::warn!(channel, %error, "the previous link prompt could not be deleted");
    }
    let message_id = match new_channel {
        Some(channel) => Some(post_link_prompt(http, channel).await?),
        None => None,
    };
    repo.set_link_channel(guild_key(guild_id), new_channel, message_id)
        .await
        .context("store the link channel")?;
    crate::audit::log(
        &state,
        actor.discord_id,
        "set_link_channel",
        &guild_id.to_string(),
        json!({ "old_channel_id": config.link_channel_id, "new_channel_id": new_channel }),
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn set_nickname_template(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    body: Result<Json<TemplateBody>, JsonRejection>,
) -> Result<Json<StartedJob>, AdminError> {
    let Json(body) = body?;
    let guild_id = home_guild(&state)?;
    let template = body.template.filter(|template| !template.trim().is_empty());
    if let Some(template) = &template {
        expr::validate_template(template)
            .map_err(|error| AdminError::BadRequest(format!("Invalid template: {error}")))?;
    }
    let repo = GuildConfigRepository::new(state.db.pool());
    repo.upsert(guild_key(guild_id), actor.discord_id)
        .await
        .context("create the server config")?;
    repo.set_nickname_template(guild_key(guild_id), template.as_deref())
        .await
        .context("store the nickname template")?;
    crate::audit::log(
        &state,
        actor.discord_id,
        "set_nickname_template",
        &guild_id.to_string(),
        json!({ "template": template }),
    )
    .await;
    let label = match template {
        Some(_) => "Applying display name format",
        None => "Syncing display names",
    };
    let job = enqueue(
        &state,
        guild_id,
        "resync",
        json!({ "label": label }),
        actor.discord_id,
    )
    .await?;
    Ok(Json(job))
}

pub async fn reset_nicknames(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
) -> Result<Json<StartedJob>, AdminError> {
    let guild_id = home_guild(&state)?;
    crate::audit::log(
        &state,
        actor.discord_id,
        "reset_nicknames",
        &guild_id.to_string(),
        json!({}),
    )
    .await;
    let payload = json!({ "label": "Clearing all nicknames" });
    let job = enqueue(
        &state,
        guild_id,
        "clear_nicknames",
        payload,
        actor.discord_id,
    )
    .await?;
    Ok(Json(job))
}

pub async fn add_rule(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    body: Result<Json<NewRuleBody>, JsonRejection>,
) -> Result<Json<StartedJob>, AdminError> {
    let Json(body) = body?;
    let guild_id = home_guild(&state)?;
    let role_id = parse_role(Some(&body.role_id))?
        .ok_or_else(|| AdminError::BadRequest("a rule needs a role".into()))?;
    validate_condition(&body.condition)?;
    let names = RoleNames::load(&state, guild_id).await?;
    let repo = GuildConfigRepository::new(state.db.pool());
    repo.upsert(guild_key(guild_id), actor.discord_id)
        .await
        .context("create the server config")?;
    let rules = repo
        .get_role_rules(guild_key(guild_id))
        .await
        .context("read the autorole rules")?;
    if rules.iter().any(|rule| rule.role_id == role_id) {
        return Err(AdminError::Rejected(
            "A rule already exists for that role. Edit or remove it first.".into(),
        ));
    }
    repo.add_role_rule(guild_key(guild_id), role_id, &body.condition, RULE_PRIORITY)
        .await
        .context("store the autorole rule")?;
    crate::audit::log(
        &state,
        actor.discord_id,
        "add_autorole_rule",
        &guild_id.to_string(),
        json!({ "role_id": role_id, "condition": body.condition }),
    )
    .await;
    let label = format!("Evaluating @{} rule for all members", names.name(role_id));
    let job = enqueue(
        &state,
        guild_id,
        "resync",
        json!({ "label": label }),
        actor.discord_id,
    )
    .await?;
    Ok(Json(job))
}

pub async fn update_rule(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path(rule_id): Path<i64>,
    body: Result<Json<RuleBody>, JsonRejection>,
) -> Result<Json<StartedJob>, AdminError> {
    let Json(body) = body?;
    let guild_id = home_guild(&state)?;
    validate_condition(&body.condition)?;
    let repo = GuildConfigRepository::new(state.db.pool());
    let role_id = rule_role(&repo, guild_id, rule_id).await?;
    let names = RoleNames::load(&state, guild_id).await?;
    repo.update_role_rule_condition(rule_id, &body.condition)
        .await
        .context("store the autorole condition")?;
    crate::audit::log(
        &state,
        actor.discord_id,
        "update_autorole_rule",
        &guild_id.to_string(),
        json!({ "rule_id": rule_id, "role_id": role_id, "condition": body.condition }),
    )
    .await;
    let label = format!("Re-evaluating @{} rule", names.name(role_id));
    let job = enqueue(
        &state,
        guild_id,
        "resync",
        json!({ "label": label }),
        actor.discord_id,
    )
    .await?;
    Ok(Json(job))
}

pub async fn remove_rule(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path(rule_id): Path<i64>,
) -> Result<StatusCode, AdminError> {
    let guild_id = home_guild(&state)?;
    let repo = GuildConfigRepository::new(state.db.pool());
    let role_id = rule_role(&repo, guild_id, rule_id).await?;
    repo.remove_role_rule(rule_id)
        .await
        .context("remove the autorole rule")?;
    crate::audit::log(
        &state,
        actor.discord_id,
        "remove_autorole_rule",
        &guild_id.to_string(),
        json!({ "rule_id": rule_id, "role_id": role_id }),
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn strip_role(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path(role_id): Path<String>,
) -> Result<Json<StartedJob>, AdminError> {
    let guild_id = home_guild(&state)?;
    let role_id = parse_role(Some(&role_id))?
        .ok_or_else(|| AdminError::BadRequest("a role id is required".into()))?;
    let names = RoleNames::load(&state, guild_id).await?;
    crate::audit::log(
        &state,
        actor.discord_id,
        "strip_role",
        &guild_id.to_string(),
        json!({ "role_id": role_id }),
    )
    .await;
    let label = format!("Stripping @{} from all members", names.name(role_id));
    let payload = json!({ "role_id": role_id.to_string(), "label": label });
    let job = enqueue(&state, guild_id, "strip_role", payload, actor.discord_id).await?;
    Ok(Json(job))
}

async fn swap_config_role(
    state: &AppState,
    actor: AdminActor,
    role_id: Option<String>,
    which: ConfigRole,
) -> Result<Json<StartedJob>, AdminError> {
    let guild_id = home_guild(state)?;
    let new_role = parse_role(role_id.as_deref())?;
    let names = RoleNames::load(state, guild_id).await?;
    let repo = GuildConfigRepository::new(state.db.pool());
    let config = repo
        .upsert(guild_key(guild_id), actor.discord_id)
        .await
        .context("create the server config")?;
    let old_role = which.current(&config);
    let stored = match which {
        ConfigRole::Linked => repo.set_link_role(guild_key(guild_id), new_role).await,
        ConfigRole::Unlinked => repo.set_unlinked_role(guild_key(guild_id), new_role).await,
    };
    stored.context(format!("store the {} role", which.noun()))?;
    crate::audit::log(
        state,
        actor.discord_id,
        which.action(),
        &guild_id.to_string(),
        json!({ "old_role_id": old_role, "new_role_id": new_role }),
    )
    .await;
    if old_role == new_role {
        return Ok(Json(StartedJob::none()));
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
    Ok(Json(
        enqueue(state, guild_id, "swap_role", payload, actor.discord_id).await?,
    ))
}

fn swap_label(which: ConfigRole, old_name: Option<String>, new_name: Option<String>) -> String {
    match (old_name, new_name) {
        (Some(old), Some(new)) => format!("{} role: @{old} → @{new}", which.title()),
        (None, Some(new)) => format!("Assigning @{new} to {} members", which.noun()),
        (Some(old), None) => format!("Removing @{old} from {} members", which.noun()),
        (None, None) => "Role update".to_string(),
    }
}

async fn rule_role(
    repo: &GuildConfigRepository<'_>,
    guild_id: GuildId,
    rule_id: i64,
) -> Result<i64, AdminError> {
    repo.get_role_rules(guild_key(guild_id))
        .await
        .context("read the autorole rules")?
        .into_iter()
        .find(|rule| rule.id == rule_id)
        .map(|rule| rule.role_id)
        .ok_or_else(|| AdminError::NotFound(format!("no autorole rule {rule_id}")))
}

async fn post_link_prompt(http: &serenity::all::Http, channel: i64) -> Result<i64, AdminError> {
    let message = GenericChannelId::new(channel as u64)
        .send_message(http, link_prompt())
        .await
        .map_err(|error| AdminError::Upstream(format!("Discord: post the link prompt: {error}")))?;
    Ok(message.id.get() as i64)
}

fn link_prompt() -> CreateMessage<'static> {
    CreateMessage::new()
        .flags(MessageFlags::IS_COMPONENTS_V2)
        .components(vec![CreateComponent::Container(CreateContainer::new(
            vec![
                discord::text(LINK_PROMPT),
                discord::separator(),
                CreateContainerComponent::ActionRow(CreateActionRow::buttons(vec![
                    CreateButton::new(LINK_BUTTON_ID)
                        .label(LINK_BUTTON_LABEL)
                        .style(ButtonStyle::Primary),
                ])),
            ],
        ))])
}

fn validate_condition(condition: &str) -> Result<(), AdminError> {
    expr::validate_condition(condition)
        .map_err(|error| AdminError::BadRequest(format!("Invalid condition: {error}")))
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

fn guild_key(guild_id: GuildId) -> i64 {
    guild_id.get() as i64
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
}
