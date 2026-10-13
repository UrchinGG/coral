use anyhow::Result;
use redis::AsyncCommands;
use serenity::all::*;

use database::{EvidencePost, EvidencePostRepository, PostStatus, ReviewRepository};

use super::evidence_post::menu::add_sources;
use super::evidence_post::render::face_section;
use super::evidence_post::{face_attachment, is_media_filename, normalize};
use super::tag::get_rank;
use crate::framework::{AccessRank, Data};
use crate::interact;
use crate::utils::text;

pub const COMMAND_NAME: &str = "Add to Evidence Post";

const RECENT_LIMIT: i64 = 5;
const SEARCH_LIMIT: i64 = 25;
const SOURCE_TTL_SECS: u64 = 30 * 60;
const EXPIRED: &str = "This picker expired. Run the command again.";

pub fn register() -> CreateCommand<'static> {
    CreateCommand::new(COMMAND_NAME).kind(CommandType::Message)
}

pub async fn run(ctx: &Context, command: &CommandInteraction, data: &Data) -> Result<()> {
    if get_rank(data, command.user.id.get()).await? < AccessRank::Helper {
        return interact::send_error(ctx, command, "Error", "Only staff can add evidence").await;
    }
    let Some(ResolvedTarget::Message(message)) = command.data.target() else {
        return interact::send_error(ctx, command, "Error", "Could not read that message").await;
    };
    let sources: Vec<(String, String)> = message
        .attachments
        .iter()
        .filter(|a| is_media_filename(&a.filename))
        .map(|a| (a.filename.to_string(), a.url.to_string()))
        .collect();
    if sources.is_empty() {
        return interact::send_error(
            ctx,
            command,
            "No Media",
            "That message has no image or video attachments",
        )
        .await;
    }

    if let Some(post) = detect_target(data, command.channel_id.get() as i64).await? {
        command.defer_ephemeral(&ctx.http).await?;
        let view = add_to(ctx, data, &post, &sources, command.user.id.get()).await;
        command
            .edit_response(
                &ctx.http,
                EditInteractionResponse::new()
                    .flags(MessageFlags::IS_COMPONENTS_V2)
                    .components(view)
                    .new_attachment(face_attachment(data, &post.uuid).await),
            )
            .await?;
        return Ok(());
    }

    let key = command.id.get();
    remember_sources(data, key, &sources).await?;
    let recent = EvidencePostRepository::new(data.db.pool())
        .recent_coral(RECENT_LIMIT)
        .await?;
    command
        .create_response(
            &ctx.http,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .flags(MessageFlags::IS_COMPONENTS_V2 | MessageFlags::EPHEMERAL)
                    .components(picker(
                        key,
                        sources.len(),
                        &recent,
                        "No evidence posts yet. Try searching.",
                    )),
            ),
        )
        .await?;
    Ok(())
}

pub async fn handle_search_button(
    ctx: &Context,
    component: &ComponentInteraction,
    _data: &Data,
) -> Result<()> {
    let Some(key) = parse_key(&component.data.custom_id, "evid_search:") else {
        return Ok(());
    };
    let modal =
        CreateModal::new(format!("evid_query:{key}"), "Find Evidence Post").components(vec![
            CreateModalComponent::Label(CreateLabel::input_text(
                "Player name or UUID",
                CreateInputText::new(InputTextStyle::Short, "query").required(true),
            )),
        ]);
    component
        .create_response(&ctx.http, CreateInteractionResponse::Modal(modal))
        .await?;
    Ok(())
}

pub async fn handle_search_modal(
    ctx: &Context,
    modal: &ModalInteraction,
    data: &Data,
) -> Result<()> {
    let Some(key) = parse_key(&modal.data.custom_id, "evid_query:") else {
        return Ok(());
    };
    let Some(sources) = recall_sources(data, key).await? else {
        return interact::update_modal(ctx, modal, simple(EXPIRED)).await;
    };
    let query = interact::extract_modal_value(&modal.data.components, "query");
    let repo = EvidencePostRepository::new(data.db.pool());
    let mut hits = repo.search_coral(&query, SEARCH_LIMIT).await?;
    if hits.is_empty()
        && let Ok(info) = data.api.resolve(query.trim()).await
        && let Some(post) = repo.canonical(&normalize(&info.uuid)).await?
    {
        hits.push(post);
    }
    let empty = format!("No evidence posts matched `{}`.", sanitize(&query));
    interact::update_modal(ctx, modal, picker(key, sources.len(), &hits, &empty)).await
}

pub async fn handle_pick(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    if get_rank(data, component.user.id.get()).await? < AccessRank::Helper {
        return interact::send_component_error(
            ctx,
            component,
            "Error",
            "Only staff can add evidence",
        )
        .await;
    }
    let Some(key) = parse_key(&component.data.custom_id, "evid_pick:") else {
        return Ok(());
    };
    let thread_id = match &component.data.kind {
        ComponentInteractionDataKind::StringSelect { values } => {
            values.first().and_then(|v| v.parse::<i64>().ok())
        }
        _ => None,
    };
    let Some(thread_id) = thread_id else {
        return Ok(());
    };
    component
        .create_response(&ctx.http, CreateInteractionResponse::Acknowledge)
        .await?;
    let view = match (
        recall_sources(data, key).await?,
        EvidencePostRepository::new(data.db.pool())
            .get(thread_id)
            .await?,
    ) {
        (None, _) => simple(EXPIRED),
        (_, None) => error_view("That evidence post no longer exists."),
        (Some(sources), Some(post)) => {
            add_to(ctx, data, &post, &sources, component.user.id.get()).await
        }
    };
    component
        .edit_response(
            &ctx.http,
            EditInteractionResponse::new()
                .flags(MessageFlags::IS_COMPONENTS_V2)
                .components(view),
        )
        .await?;
    Ok(())
}

async fn detect_target(data: &Data, channel_id: i64) -> Result<Option<EvidencePost>> {
    let repo = EvidencePostRepository::new(data.db.pool());
    if let Some(post) = repo.get(channel_id).await?.filter(|p| p.status.is_some()) {
        return Ok(Some(post));
    }
    let Some(review) = ReviewRepository::new(data.db.pool())
        .get_by_thread(channel_id)
        .await?
    else {
        return Ok(None);
    };
    Ok(repo.canonical(&review.player_uuid).await?)
}

async fn add_to(
    ctx: &Context,
    data: &Data,
    post: &EvidencePost,
    sources: &[(String, String)],
    actor: u64,
) -> Vec<CreateComponent<'static>> {
    if post.status == Some(PostStatus::Archived) {
        return error_view(
            "That evidence post is archived. Run /confirm on the player to restore it.",
        );
    }
    let summary = add_sources(ctx, data, post.thread_id, sources, actor).await;
    let username = super::evidence_post::ops::username(data, post).await;
    vec![CreateComponent::Container(CreateContainer::new(vec![
        face_section(format!(
            "## Evidence Added\nIGN - `{username}`\nThread: <#{}>\n{summary}",
            post.thread_id
        )),
    ]))]
}

fn picker(
    key: u64,
    count: usize,
    posts: &[EvidencePost],
    empty_note: &str,
) -> Vec<CreateComponent<'static>> {
    let mut parts = vec![
        text("## Add to Evidence Post"),
        text(format!(
            "-# {count} attachment{} from that message",
            if count == 1 { "" } else { "s" }
        )),
    ];
    if posts.is_empty() {
        parts.push(text(empty_note.to_string()));
    } else {
        let options: Vec<CreateSelectMenuOption> = posts
            .iter()
            .map(|post| {
                CreateSelectMenuOption::new(
                    post.player_name().to_string(),
                    post.thread_id.to_string(),
                )
                .description(status_label(post.status))
            })
            .collect();
        parts.push(CreateContainerComponent::ActionRow(
            CreateActionRow::SelectMenu(
                CreateSelectMenu::new(
                    format!("evid_pick:{key}"),
                    CreateSelectMenuKind::String {
                        options: options.into(),
                    },
                )
                .placeholder("Pick an evidence post..."),
            ),
        ));
    }
    parts.push(CreateContainerComponent::ActionRow(
        CreateActionRow::buttons(vec![
            CreateButton::new(format!("evid_search:{key}"))
                .label("Search")
                .style(ButtonStyle::Secondary),
        ]),
    ));
    vec![CreateComponent::Container(CreateContainer::new(parts))]
}

fn status_label(status: Option<PostStatus>) -> &'static str {
    match status {
        Some(PostStatus::Pending) => "Waiting for media",
        Some(PostStatus::Confirmed) => "Confirmed",
        Some(PostStatus::Archived) => "Archived",
        None => "",
    }
}

async fn remember_sources(data: &Data, key: u64, sources: &[(String, String)]) -> Result<()> {
    let payload = serde_json::to_string(sources)?;
    let _: () = data
        .redis
        .connection()
        .set_ex(source_key(key), payload, SOURCE_TTL_SECS)
        .await?;
    Ok(())
}

async fn recall_sources(data: &Data, key: u64) -> Result<Option<Vec<(String, String)>>> {
    let payload: Option<String> = data.redis.connection().get(source_key(key)).await?;
    Ok(payload.and_then(|raw| serde_json::from_str(&raw).ok()))
}

fn source_key(key: u64) -> String {
    format!("evidence_attach:{key}")
}

fn parse_key(custom_id: &str, prefix: &str) -> Option<u64> {
    custom_id.strip_prefix(prefix)?.parse().ok()
}

fn simple(message: &str) -> Vec<CreateComponent<'static>> {
    vec![CreateComponent::Container(CreateContainer::new(vec![
        text(message.to_string()),
    ]))]
}

fn error_view(message: &str) -> Vec<CreateComponent<'static>> {
    vec![interact::error_container("Error", message)]
}

fn sanitize(s: &str) -> String {
    s.chars()
        .filter(|c| !matches!(c, '`' | '\n' | '\r'))
        .take(32)
        .collect()
}
