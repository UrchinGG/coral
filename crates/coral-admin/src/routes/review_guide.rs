use axum::http::StatusCode;
use axum::{Extension, Json, Router, extract::*, routing::get, routing::post, routing::put};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use serenity::all::*;

use database::ReviewGuideRepository;

use crate::auth::AdminActor;
use crate::discord;
use crate::identity;
use crate::state::AppState;

type ApiError = (StatusCode, String);

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(get_guide))
        .route("/", put(update_guide))
        .route("/publish", post(publish_guide))
}

const GUIDE_THREAD_TITLE: &str = "Tag Review Guide";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GuideContent {
    body: String,
}

#[derive(Debug, Deserialize)]
struct LegacyGuideTagDef {
    name: String,
    emoji: String,
    description: String,
}

#[derive(Debug, Deserialize)]
struct LegacyGuideSection {
    heading: String,
    body: String,
}

#[derive(Debug, Deserialize)]
struct LegacyGuideContent {
    title: String,
    tags: Vec<LegacyGuideTagDef>,
    sections: Vec<LegacyGuideSection>,
    footer: String,
}

impl LegacyGuideContent {
    fn flatten(self) -> GuideContent {
        let tags_text = self
            .tags
            .iter()
            .map(|tag| format!("{} **{}**\n-# {}", tag.emoji, tag.name, tag.description))
            .collect::<Vec<_>>()
            .join("\n");

        let mut body = format!("## {}\n---\n### Tags Definitions\n{tags_text}", self.title);
        for section in &self.sections {
            body.push_str(&format!("\n---\n### {}\n{}", section.heading, section.body));
        }
        body.push_str(&format!("\n---\n{}", self.footer));

        GuideContent { body }
    }
}

fn parse_guide_content(value: &Value) -> Result<(GuideContent, bool), ApiError> {
    if let Ok(content) = serde_json::from_value::<GuideContent>(value.clone()) {
        return Ok((content, false));
    }
    let legacy: LegacyGuideContent = serde_json::from_value(value.clone()).map_err(internal)?;
    Ok((legacy.flatten(), true))
}

#[derive(Serialize)]
struct GuideStatus {
    posted: bool,
    forum_channel_id: Option<String>,
    forum_channel_name: Option<String>,
    thread_id: Option<String>,
    posted_at: Option<DateTime<Utc>>,
    posted_by_username: Option<String>,
    up_to_date: bool,
}

#[derive(Serialize)]
struct PingRolesView {
    review_role_id: Option<String>,
    dispute_role_id: Option<String>,
    review_opt_ins: usize,
    dispute_opt_ins: usize,
}

#[derive(Serialize)]
struct HomeRoleView {
    id: String,
    name: String,
    color: Option<String>,
    position: i64,
    managed: bool,
    assignable: bool,
}

#[derive(Serialize)]
struct ReviewGuideResponse {
    content: GuideContent,
    status: GuideStatus,
    ping_roles: PingRolesView,
    home_roles: Vec<HomeRoleView>,
}

async fn get_guide(State(state): State<AppState>) -> Result<Json<ReviewGuideResponse>, ApiError> {
    let repo = ReviewGuideRepository::new(state.db.pool());
    let config = repo
        .get()
        .await
        .map_err(internal)?
        .ok_or_else(|| internal("review_guide_config row missing"))?;

    let (content, migrated) = parse_guide_content(&config.content)?;
    if migrated {
        repo.update_content(&serde_json::to_value(&content).map_err(internal)?)
            .await
            .map_err(internal)?;
    }

    let http = discord::bot_http(&state).map_err(service_unavailable)?;
    let home_guild = state.home_guild_id.map(|id| GuildId::new(id as u64));

    let (home_roles, opt_ins) = match home_guild {
        Some(guild_id) => tokio::join!(
            fetch_home_roles(&http, guild_id),
            fetch_opt_in_counts(&http, guild_id, &config),
        ),
        None => (Vec::new(), (0, 0)),
    };

    let forum_channel_name = match state.review_forum_id {
        Some(forum_id) => http
            .get_channel(GenericChannelId::new(forum_id as u64))
            .await
            .ok()
            .and_then(Channel::guild)
            .map(|c| c.base.name.to_string()),
        None => None,
    };

    let posted_by_username = match config.posted_by {
        Some(id) => identity::resolve_discord_usernames(&state, &[id])
            .await
            .get(&id)
            .cloned(),
        None => None,
    };

    let up_to_date = match (config.posted_at, config.content_updated_at) {
        (Some(posted), updated) => posted >= updated,
        _ => false,
    };

    Ok(Json(ReviewGuideResponse {
        content,
        status: GuideStatus {
            posted: config.posted_thread_id.is_some(),
            forum_channel_id: state.review_forum_id.map(|v| v.to_string()),
            forum_channel_name,
            thread_id: config.posted_thread_id.map(|v| v.to_string()),
            posted_at: config.posted_at,
            posted_by_username,
            up_to_date,
        },
        ping_roles: PingRolesView {
            review_role_id: config.review_ping_role_id.map(|v| v.to_string()),
            dispute_role_id: config.dispute_ping_role_id.map(|v| v.to_string()),
            review_opt_ins: opt_ins.0,
            dispute_opt_ins: opt_ins.1,
        },
        home_roles,
    }))
}

#[derive(Deserialize)]
struct PingRolesRequest {
    review_role_id: Option<String>,
    dispute_role_id: Option<String>,
}

#[derive(Deserialize)]
struct UpdateGuideRequest {
    content: Option<GuideContent>,
    ping_roles: Option<PingRolesRequest>,
}

async fn update_guide(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Json(req): Json<UpdateGuideRequest>,
) -> Result<StatusCode, ApiError> {
    let repo = ReviewGuideRepository::new(state.db.pool());

    if let Some(content) = &req.content {
        let value = serde_json::to_value(content).map_err(internal)?;
        repo.update_content(&value).await.map_err(internal)?;
        crate::audit::log(
            &state,
            actor.discord_id,
            "update_review_guide_content",
            "review_guide",
            json!({"length": content.body.len()}),
        )
        .await;
    }

    if let Some(ping_roles) = &req.ping_roles {
        let review = parse_optional_id(ping_roles.review_role_id.as_deref())?;
        let dispute = parse_optional_id(ping_roles.dispute_role_id.as_deref())?;
        repo.set_ping_roles(review, dispute)
            .await
            .map_err(internal)?;
        crate::audit::log(
            &state,
            actor.discord_id,
            "set_review_ping_roles",
            "review_guide",
            json!({"review_role_id": review, "dispute_role_id": dispute}),
        )
        .await;
    }

    Ok(StatusCode::NO_CONTENT)
}

async fn publish_guide(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
) -> Result<StatusCode, ApiError> {
    let http = discord::bot_http(&state).map_err(service_unavailable)?;
    let forum_id = state.review_forum_id.ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "REVIEW_FORUM_ID not configured".to_string(),
    ))?;

    let repo = ReviewGuideRepository::new(state.db.pool());
    let config = repo
        .get()
        .await
        .map_err(internal)?
        .ok_or_else(|| internal("review_guide_config row missing"))?;
    let (content, _) = parse_guide_content(&config.content)?;

    let components = build_guide_message(&content);

    let (thread_id, message_id) = match (config.posted_thread_id, config.posted_message_id) {
        (Some(thread_id), Some(message_id)) => {
            let edit = EditMessage::new()
                .flags(MessageFlags::IS_COMPONENTS_V2)
                .components(components.clone());
            match GenericChannelId::new(thread_id as u64)
                .edit_message(&http, MessageId::new(message_id as u64), edit)
                .await
            {
                Ok(_) => (thread_id, message_id),
                Err(e) => {
                    tracing::warn!(
                        "Failed to edit existing guide message ({e:#}), reposting instead"
                    );
                    create_guide_thread(&http, forum_id, components).await?
                }
            }
        }
        _ => create_guide_thread(&http, forum_id, components).await?,
    };

    repo.set_posted(forum_id, thread_id, message_id, actor.discord_id)
        .await
        .map_err(internal)?;

    crate::audit::log(
        &state,
        actor.discord_id,
        "publish_review_guide",
        "review_guide",
        json!({"thread_id": thread_id, "message_id": message_id, "forum_id": forum_id}),
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

async fn create_guide_thread(
    http: &Http,
    forum_id: i64,
    components: Vec<CreateComponent<'static>>,
) -> Result<(i64, i64), ApiError> {
    let message = CreateMessage::new()
        .flags(MessageFlags::IS_COMPONENTS_V2)
        .components(components);
    let thread = ChannelId::new(forum_id as u64)
        .create_forum_post(http, CreateForumPost::new(GUIDE_THREAD_TITLE, message))
        .await
        .map_err(bad_gateway)?;
    let thread_id = thread.id.get() as i64;

    if let Err(e) = thread
        .id
        .edit(
            http,
            EditThread::new().locked(true).flags(ChannelFlags::PINNED),
        )
        .await
    {
        tracing::warn!("Failed to lock/pin guide thread {thread_id}: {e:#}");
    }

    Ok((thread_id, thread_id))
}

fn build_guide_message(content: &GuideContent) -> Vec<CreateComponent<'static>> {
    let mut parts = Vec::new();
    let mut segment = Vec::new();
    for line in content.body.lines() {
        let trimmed = line.trim();
        if trimmed.len() >= 3 && trimmed.chars().all(|c| c == '-') {
            if !segment.is_empty() {
                parts.push(discord::text(segment.join("\n")));
                segment.clear();
            }
            parts.push(discord::separator());
        } else {
            segment.push(line);
        }
    }
    if !segment.is_empty() {
        parts.push(discord::text(segment.join("\n")));
    }

    parts.push(CreateContainerComponent::ActionRow(
        CreateActionRow::buttons(vec![
            CreateButton::new("guide_ping_toggle")
                .label("Ping Me For Reviews")
                .style(ButtonStyle::Secondary),
        ]),
    ));

    vec![CreateComponent::Container(CreateContainer::new(parts))]
}

async fn fetch_home_roles(http: &Http, guild_id: GuildId) -> Vec<HomeRoleView> {
    let (roles, bot_user) = tokio::join!(
        discord::guild_roles(http, guild_id),
        http.get_current_user(),
    );
    let Ok(roles) = roles else { return Vec::new() };

    let bot_role_ids: Vec<RoleId> = match bot_user {
        Ok(user) => http
            .get_member(guild_id, user.id)
            .await
            .map(|m| m.roles.into_iter().collect())
            .unwrap_or_default(),
        Err(_) => Vec::new(),
    };
    let bot_top_position = roles
        .iter()
        .filter(|r| bot_role_ids.contains(&r.id))
        .map(|r| r.position)
        .max()
        .unwrap_or(0);

    let mut views: Vec<HomeRoleView> = roles
        .iter()
        .filter(|r| r.id.get() != guild_id.get())
        .map(|r| HomeRoleView {
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

async fn fetch_opt_in_counts(
    http: &Http,
    guild_id: GuildId,
    config: &database::ReviewGuideConfig,
) -> (usize, usize) {
    if config.review_ping_role_id.is_none() && config.dispute_ping_role_id.is_none() {
        return (0, 0);
    }
    let Ok(members) = discord::list_guild_members(http, guild_id).await else {
        return (0, 0);
    };

    let count = |role: Option<i64>| {
        role.map(|id| {
            let id = RoleId::new(id as u64);
            members.iter().filter(|m| m.roles.contains(&id)).count()
        })
        .unwrap_or(0)
    };
    (
        count(config.review_ping_role_id),
        count(config.dispute_ping_role_id),
    )
}

fn parse_optional_id(value: Option<&str>) -> Result<Option<i64>, ApiError> {
    match value {
        Some(raw) => raw
            .parse::<i64>()
            .map(Some)
            .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid id".to_string())),
        None => Ok(None),
    }
}

fn internal(e: impl std::fmt::Display) -> ApiError {
    tracing::error!("review guide route error: {e}");
    (StatusCode::INTERNAL_SERVER_ERROR, "Internal error".into())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_current_shape_without_migrating() {
        let value = json!({"body": "hello"});
        let (content, migrated) = parse_guide_content(&value).unwrap();
        assert_eq!(content.body, "hello");
        assert!(!migrated);
    }

    #[test]
    fn migrates_legacy_shape_preserving_real_guide_text() {
        let value = json!({
            "title": "Tag Review Guide",
            "tags": [
                {
                    "key": "sniper",
                    "name": "Sniper",
                    "emoji": "<:sniper:1459106167270932618>",
                    "description": "Used for cheating snipers."
                },
                {
                    "key": "caution",
                    "name": "Caution",
                    "emoji": "<:caution:1459106358098923583>",
                    "description": "Special tag used for things that don't fit into any of the above categories."
                }
            ],
            "sections": [
                {
                    "key": "submitting",
                    "heading": "Submitting",
                    "body": "Run `/tag add`, then press **Create Post** on the preview"
                }
            ],
            "footer": "Press the button below to toggle tag review pings."
        });

        let (content, migrated) = parse_guide_content(&value).unwrap();
        assert!(migrated);
        assert_eq!(
            content.body,
            "## Tag Review Guide\n\
             ---\n\
             ### Tags Definitions\n\
             <:sniper:1459106167270932618> **Sniper**\n\
             -# Used for cheating snipers.\n\
             <:caution:1459106358098923583> **Caution**\n\
             -# Special tag used for things that don't fit into any of the above categories.\n\
             ---\n\
             ### Submitting\n\
             Run `/tag add`, then press **Create Post** on the preview\n\
             ---\n\
             Press the button below to toggle tag review pings."
        );
    }
}
