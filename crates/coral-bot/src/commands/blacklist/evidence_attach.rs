use anyhow::Result;
use serenity::all::*;

use super::evidence::{
    ALLOWED_MEDIA_EXTENSIONS, ThreadState, append_media_to_op, evidence_thread_entry,
    evidence_thread_state, face_attachment, recent_evidence_threads, search_evidence_threads,
};
use super::tag::get_rank;
use crate::framework::{AccessRank, Data, EvidenceThread};
use crate::utils::text;

pub const COMMAND_NAME: &str = "Add to Evidence Post";

const RECENT_LIMIT: usize = 5;
const SEARCH_LIMIT: usize = 25;

pub fn register() -> CreateCommand<'static> {
    CreateCommand::new(COMMAND_NAME).kind(CommandType::Message)
}

fn is_media(filename: &str) -> bool {
    let ext = filename
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    ALLOWED_MEDIA_EXTENSIONS.contains(&ext.as_str())
}

fn media_sources(message: &Message) -> Vec<(String, String)> {
    message
        .attachments
        .iter()
        .filter(|a| is_media(&a.filename))
        .map(|a| (a.filename.to_string(), a.url.to_string()))
        .collect()
}

fn parse_source(custom_id: &str, prefix: &str) -> Option<(GenericChannelId, MessageId)> {
    let (channel, message) = custom_id.strip_prefix(prefix)?.split_once(':')?;
    Some((
        GenericChannelId::new(channel.parse().ok()?),
        MessageId::new(message.parse().ok()?),
    ))
}

fn picker(
    source: &str,
    count: usize,
    entries: &[(String, EvidenceThread)],
    empty_note: &str,
) -> Vec<CreateComponent<'static>> {
    let mut parts = vec![
        text("## Add to Evidence Post"),
        text(format!(
            "-# {count} attachment{} from that message",
            if count == 1 { "" } else { "s" }
        )),
    ];

    if entries.is_empty() {
        parts.push(text(empty_note));
    } else {
        let options: Vec<CreateSelectMenuOption<'static>> = entries
            .iter()
            .map(|(uuid, entry)| {
                CreateSelectMenuOption::new(entry.name.clone(), uuid.clone())
                    .description(format!("{}…", &uuid[..8.min(uuid.len())]))
            })
            .collect();
        parts.push(CreateContainerComponent::ActionRow(
            CreateActionRow::SelectMenu(
                CreateSelectMenu::new(
                    format!("evid_pick:{source}"),
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
            CreateButton::new(format!("evid_search:{source}"))
                .label("Search")
                .style(ButtonStyle::Secondary),
        ]),
    ));

    vec![CreateComponent::Container(CreateContainer::new(parts))]
}

pub async fn run(ctx: &Context, command: &CommandInteraction, data: &Data) -> Result<()> {
    if get_rank(data, command.user.id.get()).await? < AccessRank::Helper {
        return crate::interact::send_error(
            ctx,
            command,
            "Error",
            "Only staff can add evidence",
        )
        .await;
    }

    let Some(ResolvedTarget::Message(message)) = command.data.target() else {
        return crate::interact::send_error(ctx, command, "Error", "Could not read that message")
            .await;
    };

    let count = media_sources(message).len();
    if count == 0 {
        return crate::interact::send_error(
            ctx,
            command,
            "No Media",
            "That message has no image or video attachments",
        )
        .await;
    }

    let source = format!("{}:{}", command.channel_id.get(), message.id.get());
    let recent = recent_evidence_threads(data, RECENT_LIMIT);
    command
        .create_response(
            &ctx.http,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .flags(MessageFlags::IS_COMPONENTS_V2 | MessageFlags::EPHEMERAL)
                    .components(picker(
                        &source,
                        count,
                        &recent,
                        "No evidence posts indexed yet — try searching.",
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
    let Some((channel_id, message_id)) = parse_source(&component.data.custom_id, "evid_search:")
    else {
        return crate::interact::send_component_error(
            ctx,
            component,
            "Error",
            "Could not read that message",
        )
        .await;
    };

    let modal = CreateModal::new(
        format!("evid_query:{}:{}", channel_id.get(), message_id.get()),
        "Find Evidence Post",
    )
    .components(vec![CreateModalComponent::Label(CreateLabel::input_text(
        "Player name or UUID",
        CreateInputText::new(InputTextStyle::Short, "query").required(true),
    ))]);
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
    modal.defer_ephemeral(&ctx.http).await?;

    let Some((channel_id, message_id)) = parse_source(&modal.data.custom_id, "evid_query:") else {
        return edit_error(ctx, modal, "Could not read that message").await;
    };

    let query = crate::interact::extract_modal_value(&modal.data.components, "query");
    let mut hits = search_evidence_threads(data, &query, SEARCH_LIMIT);

    if hits.is_empty()
        && let Ok(info) = data.api.resolve(query.trim()).await
        && let Some(entry) = evidence_thread_entry(data, &info.uuid)
    {
        hits.push((info.uuid.replace('-', "").to_ascii_lowercase(), entry));
    }

    let count = match ctx.http.get_message(channel_id, message_id).await {
        Ok(message) => media_sources(&message).len(),
        Err(_) => return edit_error(ctx, modal, "That message is no longer available").await,
    };

    let source = format!("{}:{}", channel_id.get(), message_id.get());
    modal
        .edit_response(
            &ctx.http,
            EditInteractionResponse::new()
                .flags(MessageFlags::IS_COMPONENTS_V2)
                .components(picker(
                    &source,
                    count,
                    &hits,
                    &format!("No evidence posts matched `{}`.", sanitize(&query)),
                )),
        )
        .await?;
    Ok(())
}

pub async fn handle_pick(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let uuid = match &component.data.kind {
        ComponentInteractionDataKind::StringSelect { values } => values.first().cloned(),
        _ => None,
    };
    let (Some(uuid), Some((channel_id, message_id))) =
        (uuid, parse_source(&component.data.custom_id, "evid_pick:"))
    else {
        return crate::interact::send_component_error(
            ctx,
            component,
            "Error",
            "Could not read that selection",
        )
        .await;
    };

    if get_rank(data, component.user.id.get()).await? < AccessRank::Helper {
        return crate::interact::send_component_error(
            ctx,
            component,
            "Error",
            "Only staff can add evidence",
        )
        .await;
    }

    component.defer(&ctx.http).await?;

    let Some(entry) = evidence_thread_entry(data, &uuid) else {
        return edit_component_error(ctx, component, "That evidence post no longer exists").await;
    };

    let Ok(message) = ctx.http.get_message(channel_id, message_id).await else {
        return edit_component_error(ctx, component, "That message is no longer available").await;
    };
    let sources = media_sources(&message);
    if sources.is_empty() {
        return edit_component_error(
            ctx,
            component,
            "That message has no image or video attachments",
        )
        .await;
    }

    match evidence_thread_state(ctx, entry.id).await {
        ThreadState::Missing => {
            return edit_component_error(ctx, component, "That evidence post no longer exists")
                .await;
        }
        ThreadState::Closed => {
            return edit_component_error(
                ctx,
                component,
                "That evidence post is archived. Restore it before adding evidence.",
            )
            .await;
        }
        ThreadState::IdleArchived => {
            if let Err(e) = entry
                .id
                .edit(&ctx.http, EditThread::new().archived(false))
                .await
            {
                tracing::warn!("could not unarchive evidence thread {}: {e}", entry.id.get());
                return edit_component_error(
                    ctx,
                    component,
                    "Could not reopen that evidence post. Try again.",
                )
                .await;
            }
        }
        ThreadState::Open => {}
    }

    let outcome =
        match append_media_to_op(ctx, data, entry.id, component.user.id.get(), &sources).await {
            Ok(outcome) => outcome,
            Err(e) => return edit_component_error(ctx, component, &e.message()).await,
        };

    let body = format!(
        "## Evidence Added\nIGN - `{}`\nAdded {} attachment{} to <#{}>{}",
        outcome.username,
        outcome.added,
        if outcome.added == 1 { "" } else { "s" },
        entry.id.get(),
        outcome.notes()
    );

    component
        .edit_response(
            &ctx.http,
            EditInteractionResponse::new()
                .flags(MessageFlags::IS_COMPONENTS_V2)
                .components(vec![CreateComponent::Container(CreateContainer::new(
                    vec![face_section(body)],
                ))])
                .new_attachment(face_attachment(data, &uuid).await),
        )
        .await?;
    Ok(())
}

fn face_section(content: String) -> CreateContainerComponent<'static> {
    CreateContainerComponent::Section(CreateSection::new(
        vec![CreateSectionComponent::TextDisplay(CreateTextDisplay::new(
            content,
        ))],
        CreateSectionAccessory::Thumbnail(CreateThumbnail::new(CreateUnfurledMediaItem::new(
            "attachment://face.png",
        ))),
    ))
}

fn sanitize(s: &str) -> String {
    s.chars()
        .filter(|c| !matches!(c, '`' | '\n' | '\r'))
        .take(32)
        .collect()
}

async fn edit_error(ctx: &Context, modal: &ModalInteraction, message: &str) -> Result<()> {
    modal
        .edit_response(
            &ctx.http,
            EditInteractionResponse::new().content(message.to_string()),
        )
        .await?;
    Ok(())
}

async fn edit_component_error(
    ctx: &Context,
    component: &ComponentInteraction,
    message: &str,
) -> Result<()> {
    component
        .edit_response(
            &ctx.http,
            EditInteractionResponse::new()
                .flags(MessageFlags::IS_COMPONENTS_V2)
                .components(vec![CreateComponent::Container(
                    CreateContainer::new(vec![text(format!("## Error\n{message}"))])
                        .accent_color(super::channel::COLOR_ERROR),
                )]),
        )
        .await?;
    Ok(())
}
