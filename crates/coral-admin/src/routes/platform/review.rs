use axum::extract::State;
use axum::extract::rejection::JsonRejection;
use axum::http::StatusCode;
use axum::{Extension, Json};
use chrono::{DateTime, Utc};
use coral_redis::{SyncEvent, SyncEventPublisher};
use database::{
    MemberRepository, ReviewGuideConfig, ReviewGuideRepository, ReviewRules, ReviewRulesRepository,
    standing,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use serenity::all::{
    ButtonStyle, ChannelFlags, ChannelId, CreateActionRow, CreateButton, CreateComponent,
    CreateContainer, CreateContainerComponent, CreateForumPost, CreateMessage, EditMessage,
    EditThread, GenericChannelId, Http, MessageFlags, MessageId, ThreadId,
};

use super::page::{Accounts, Mentions};
use super::parse_role;
use crate::auth::AdminActor;
use crate::discord;
use crate::error::{AdminError, Context};
use crate::refs::{EntityRef, Labels};
use crate::state::AppState;
use crate::{serde_id, serde_ts};

const GUIDE_TARGET: &str = "review_guide";
const RULES_TARGET: &str = "review_rules";
const GUIDE_THREAD_TITLE: &str = "Tag Review Guide";
const PING_BUTTON_ID: &str = "guide_ping_toggle";
const PING_BUTTON_LABEL: &str = "Ping Me For Reviews";
const DIVIDER_MIN_DASHES: usize = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuideContent {
    body: String,
}

#[derive(Debug, Deserialize)]
struct LegacyTag {
    name: String,
    emoji: String,
    description: String,
}

#[derive(Debug, Deserialize)]
struct LegacySection {
    heading: String,
    body: String,
}

#[derive(Debug, Deserialize)]
struct LegacyGuide {
    title: String,
    tags: Vec<LegacyTag>,
    sections: Vec<LegacySection>,
    footer: String,
}

#[derive(Debug, Serialize)]
pub struct Guide {
    content: GuideContent,
    status: GuideStatus,
    ping_roles: PingRoles,
    labels: Labels,
}

#[derive(Debug, Serialize)]
struct GuideStatus {
    posted: bool,
    #[serde(serialize_with = "serde_id::discord_id_opt")]
    forum_channel_id: Option<i64>,
    #[serde(serialize_with = "serde_id::discord_id_opt")]
    thread_id: Option<i64>,
    #[serde(serialize_with = "serde_ts::micros_opt")]
    posted_at: Option<DateTime<Utc>>,
    posted_by: Option<EntityRef>,
    #[serde(serialize_with = "serde_ts::micros")]
    content_updated_at: DateTime<Utc>,
    up_to_date: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PingRoles {
    review_role_id: Option<String>,
    dispute_role_id: Option<String>,
    confirm_role_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RulesView {
    rules: ReviewRules,
    defaults: ReviewRules,
}

#[derive(Debug, Serialize)]
pub struct RulesUpdate {
    rules: ReviewRules,
    vote_changes: usize,
    tag_changes: usize,
}

impl LegacyGuide {
    fn flatten(self) -> GuideContent {
        let tags = self
            .tags
            .iter()
            .map(|tag| format!("{} **{}**\n-# {}", tag.emoji, tag.name, tag.description))
            .collect::<Vec<_>>()
            .join("\n");
        let mut body = format!("## {}\n---\n### Tags Definitions\n{tags}", self.title);
        for section in &self.sections {
            body.push_str(&format!("\n---\n### {}\n{}", section.heading, section.body));
        }
        body.push_str(&format!("\n---\n{}", self.footer));
        GuideContent { body }
    }
}

pub async fn guide(State(state): State<AppState>) -> Result<Json<Guide>, AdminError> {
    let repo = ReviewGuideRepository::new(state.db.pool());
    let config = stored_guide(&repo).await?;
    let (content, migrated) = parse_content(&config.content)?;
    if migrated {
        repo.update_content(&encode_content(&content)?)
            .await
            .context("store the migrated review guide")?;
    }
    let mut mentions = Mentions::default();
    config
        .posted_by
        .into_iter()
        .for_each(|id| mentions.account(id));
    let accounts = Accounts::load(&state, &mentions.discord_ids()).await?;
    let labels = mentions.labels(&state).await?;
    Ok(Json(Guide {
        status: guide_status(&state, &config, &accounts),
        ping_roles: PingRoles {
            review_role_id: config.review_ping_role_id.map(|id| id.to_string()),
            dispute_role_id: config.dispute_ping_role_id.map(|id| id.to_string()),
            confirm_role_id: config.confirm_ping_role_id.map(|id| id.to_string()),
        },
        content,
        labels,
    }))
}

pub async fn save_guide(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    body: Result<Json<GuideContent>, JsonRejection>,
) -> Result<StatusCode, AdminError> {
    let Json(content) = body?;
    ReviewGuideRepository::new(state.db.pool())
        .update_content(&encode_content(&content)?)
        .await
        .context("store the review guide")?;
    crate::audit::log(
        &state,
        actor.discord_id,
        "update_review_guide_content",
        GUIDE_TARGET,
        json!({ "length": content.body.len() }),
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn set_ping_roles(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    body: Result<Json<PingRoles>, JsonRejection>,
) -> Result<StatusCode, AdminError> {
    let Json(roles) = body?;
    let review = parse_role(roles.review_role_id.as_deref())?;
    let dispute = parse_role(roles.dispute_role_id.as_deref())?;
    let confirm = parse_role(roles.confirm_role_id.as_deref())?;
    ReviewGuideRepository::new(state.db.pool())
        .set_ping_roles(review, dispute, confirm)
        .await
        .context("store the review ping roles")?;
    crate::audit::log(
        &state,
        actor.discord_id,
        "set_review_ping_roles",
        GUIDE_TARGET,
        json!({ "review_role_id": review, "dispute_role_id": dispute, "confirm_role_id": confirm }),
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn publish_guide(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
) -> Result<StatusCode, AdminError> {
    let http = state
        .discord
        .bot()
        .map_err(|error| AdminError::Rejected(error.to_string()))?;
    let forum_id = state
        .discord
        .review_forum_id
        .ok_or_else(|| AdminError::Rejected("REVIEW_FORUM_ID is not configured".into()))?;
    let repo = ReviewGuideRepository::new(state.db.pool());
    let config = stored_guide(&repo).await?;
    let (content, _) = parse_content(&config.content)?;
    let components = guide_message(&content);
    let (thread_id, message_id) = match (config.posted_thread_id, config.posted_message_id) {
        (Some(thread_id), Some(message_id)) => {
            edit_or_repost(http, forum_id, thread_id, message_id, components).await?
        }
        _ => create_thread(http, forum_id, components).await?,
    };
    repo.set_posted(forum_id, thread_id, message_id, actor.discord_id)
        .await
        .context("record the posted review guide")?;
    crate::audit::log(
        &state,
        actor.discord_id,
        "publish_review_guide",
        GUIDE_TARGET,
        json!({ "thread_id": thread_id, "message_id": message_id, "forum_id": forum_id }),
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn rules(State(state): State<AppState>) -> Result<Json<RulesView>, AdminError> {
    let rules = ReviewRulesRepository::new(state.db.pool())
        .fetch()
        .await
        .context("read the review rules")?;
    Ok(Json(RulesView {
        rules,
        defaults: ReviewRules::DEFAULT,
    }))
}

pub async fn save_rules(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    body: Result<Json<ReviewRules>, JsonRejection>,
) -> Result<Json<RulesUpdate>, AdminError> {
    let Json(rules) = body?;
    rules.validate().map_err(AdminError::BadRequest)?;
    let repo = ReviewRulesRepository::new(state.db.pool());
    let before = repo.fetch().await.context("read the review rules")?;
    repo.update(&rules, actor.discord_id)
        .await
        .context("store the review rules")?;
    let (vote_changes, tag_changes) = reevaluate_members(&state, &rules).await?;
    crate::audit::log(
        &state,
        actor.discord_id,
        "update_review_rules",
        RULES_TARGET,
        json!({
            "before": before,
            "after": rules,
            "vote_changes": vote_changes,
            "tag_changes": tag_changes,
        }),
    )
    .await;
    Ok(Json(RulesUpdate {
        rules,
        vote_changes,
        tag_changes,
    }))
}

async fn stored_guide(repo: &ReviewGuideRepository<'_>) -> Result<ReviewGuideConfig, AdminError> {
    repo.get()
        .await
        .context("read the review guide")?
        .ok_or_else(|| AdminError::Database("review_guide_config has no row".into()))
}

fn guide_status(state: &AppState, config: &ReviewGuideConfig, accounts: &Accounts) -> GuideStatus {
    GuideStatus {
        posted: config.posted_thread_id.is_some(),
        forum_channel_id: state.discord.review_forum_id,
        thread_id: config.posted_thread_id,
        posted_at: config.posted_at,
        posted_by: config.posted_by.map(|id| accounts.of(id)),
        content_updated_at: config.content_updated_at,
        up_to_date: config
            .posted_at
            .is_some_and(|posted| posted >= config.content_updated_at),
    }
}

fn parse_content(value: &Value) -> Result<(GuideContent, bool), AdminError> {
    if let Ok(content) = serde_json::from_value::<GuideContent>(value.clone()) {
        return Ok((content, false));
    }
    let legacy: LegacyGuide = serde_json::from_value(value.clone()).map_err(|error| {
        AdminError::Database(format!(
            "the stored review guide has an unknown shape: {error}"
        ))
    })?;
    Ok((legacy.flatten(), true))
}

fn encode_content(content: &GuideContent) -> Result<Value, AdminError> {
    serde_json::to_value(content)
        .map_err(|error| AdminError::Internal(format!("encode the review guide: {error}")))
}

async fn edit_or_repost(
    http: &Http,
    forum_id: i64,
    thread_id: i64,
    message_id: i64,
    components: Vec<CreateComponent<'static>>,
) -> Result<(i64, i64), AdminError> {
    let edit = EditMessage::new()
        .flags(MessageFlags::IS_COMPONENTS_V2)
        .components(components.clone());
    let edited = GenericChannelId::new(thread_id as u64)
        .edit_message(http, MessageId::new(message_id as u64), edit)
        .await;
    match edited {
        Ok(_) => {
            reopen_thread(http, thread_id).await;
            Ok((thread_id, message_id))
        }
        Err(error) => {
            tracing::warn!(%error, "the posted review guide could not be edited; reposting it");
            create_thread(http, forum_id, components).await
        }
    }
}

async fn create_thread(
    http: &Http,
    forum_id: i64,
    components: Vec<CreateComponent<'static>>,
) -> Result<(i64, i64), AdminError> {
    let message = CreateMessage::new()
        .flags(MessageFlags::IS_COMPONENTS_V2)
        .components(components);
    let thread = ChannelId::new(forum_id as u64)
        .create_forum_post(http, CreateForumPost::new(GUIDE_THREAD_TITLE, message))
        .await
        .map_err(|error| {
            AdminError::Upstream(format!("Discord: post the review guide: {error}"))
        })?;
    let thread_id = thread.id.get() as i64;
    if let Err(error) = thread
        .id
        .edit(http, EditThread::new().flags(ChannelFlags::PINNED))
        .await
    {
        tracing::warn!(thread_id, %error, "the review guide thread could not be pinned");
    }
    Ok((thread_id, thread_id))
}

async fn reopen_thread(http: &Http, thread_id: i64) {
    if let Err(error) = ThreadId::new(thread_id as u64)
        .edit(http, EditThread::new().archived(false).locked(false))
        .await
    {
        tracing::warn!(thread_id, %error, "the review guide thread could not be reopened");
    }
}

fn guide_message(content: &GuideContent) -> Vec<CreateComponent<'static>> {
    let mut parts = Vec::new();
    let mut segment: Vec<&str> = Vec::new();
    for line in content.body.lines() {
        if !is_divider(line) {
            segment.push(line);
            continue;
        }
        if !segment.is_empty() {
            parts.push(discord::text(segment.join("\n")));
            segment.clear();
        }
        parts.push(discord::separator());
    }
    if !segment.is_empty() {
        parts.push(discord::text(segment.join("\n")));
    }
    parts.push(CreateContainerComponent::ActionRow(
        CreateActionRow::buttons(vec![
            CreateButton::new(PING_BUTTON_ID)
                .label(PING_BUTTON_LABEL)
                .style(ButtonStyle::Secondary),
        ]),
    ));
    vec![CreateComponent::Container(CreateContainer::new(parts))]
}

fn is_divider(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.len() >= DIVIDER_MIN_DASHES && trimmed.chars().all(|c| c == '-')
}

async fn reevaluate_members(
    state: &AppState,
    rules: &ReviewRules,
) -> Result<(usize, usize), AdminError> {
    let members = MemberRepository::new(state.db.pool());
    let publisher = SyncEventPublisher::new(state.redis.clone());
    let mut changes = (0, 0);
    let standing_members = members
        .with_review_standing()
        .await
        .context("read members with review standing")?;
    for discord_id in standing_members {
        let change = standing::refresh(&members, rules, discord_id)
            .await
            .context(format!("refresh the review standing of {discord_id}"))?;
        changes.0 += usize::from(change.vote.is_some());
        if change.tag.is_some() {
            changes.1 += 1;
            publisher
                .publish(&SyncEvent::SyncUser {
                    discord_id: discord_id as u64,
                })
                .await;
        }
    }
    Ok(changes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_current_shape_is_read_without_migrating() {
        let (content, migrated) = parse_content(&json!({ "body": "hello" })).unwrap();
        assert_eq!(content.body, "hello");
        assert!(!migrated);
    }

    #[test]
    fn the_legacy_shape_is_flattened_into_one_body() {
        let value = json!({
            "title": "Tag Review Guide",
            "tags": [
                { "key": "sniper", "name": "Sniper", "emoji": "<:sniper:1>", "description": "Used for cheating snipers." }
            ],
            "sections": [
                { "key": "submitting", "heading": "Submitting", "body": "Run `/tag add`" }
            ],
            "footer": "Press the button below."
        });
        let (content, migrated) = parse_content(&value).unwrap();
        assert!(migrated);
        assert_eq!(
            content.body,
            "## Tag Review Guide\n---\n### Tags Definitions\n<:sniper:1> **Sniper**\n-# Used for cheating snipers.\n---\n### Submitting\nRun `/tag add`\n---\nPress the button below."
        );
    }

    #[test]
    fn an_unknown_stored_shape_is_an_error() {
        assert_eq!(
            parse_content(&json!({ "nope": 1 })).unwrap_err().code(),
            "database"
        );
    }

    #[test]
    fn dashed_lines_split_the_guide_into_sections() {
        assert!(is_divider(" --- "));
        assert!(is_divider("-----"));
        assert!(!is_divider("--"));
        assert!(!is_divider("-# small"));
    }
}
