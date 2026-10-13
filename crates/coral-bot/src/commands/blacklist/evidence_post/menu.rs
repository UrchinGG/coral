use anyhow::Result;
use serenity::all::*;

use database::{EvidencePostRepository, PostStatus};

use super::convert::Conversion;
use super::media::{self, current_media};
use super::ops::{self, OpError};
use super::{MAX_FILES_PER_UPLOAD, is_media_filename, media_extension, media_label};
use crate::commands::blacklist::tag::get_rank;
use crate::framework::{AccessRank, Data};
use crate::interact;
use crate::utils::discord_media::MessageMedia;
use crate::utils::text;

const STAFF_ONLY: &str = "Only staff can change evidence posts";

pub async fn handle_retired_button(ctx: &Context, component: &ComponentInteraction) -> Result<()> {
    interact::send_component_error(
        ctx,
        component,
        "Error",
        "Use /tag manage to change or remove this tag.",
    )
    .await
}

pub async fn handle_add_media(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    if get_rank(data, component.user.id.get()).await? < AccessRank::Helper {
        return interact::send_component_error(ctx, component, "Error", STAFF_ONLY).await;
    }
    let thread_id = component.channel_id.get() as i64;
    let Some(post) = EvidencePostRepository::new(data.db.pool())
        .get(thread_id)
        .await?
    else {
        return interact::send_component_error(
            ctx,
            component,
            "Error",
            "This evidence post is not indexed yet. Try again shortly.",
        )
        .await;
    };
    if post.status == Some(PostStatus::Archived) {
        return interact::send_component_error(
            ctx,
            component,
            "Error",
            &OpError::Archived.to_string(),
        )
        .await;
    }
    let upload = CreateFileUpload::new("evidence")
        .max_values(MAX_FILES_PER_UPLOAD)
        .required(true);
    let modal = CreateModal::new(format!("evidence_media_modal:{thread_id}"), "Add Media")
        .components(vec![CreateModalComponent::Label(
            CreateLabel::file_upload("Screenshots or clips", upload)
                .description("png, jpg, gif, webp, mp4, webm or mov, up to 4 files"),
        )]);
    component
        .create_response(&ctx.http, CreateInteractionResponse::Modal(modal))
        .await?;
    Ok(())
}

pub async fn handle_media_modal(
    ctx: &Context,
    modal: &ModalInteraction,
    data: &Data,
) -> Result<()> {
    let staff = modal.user.id.get();
    if get_rank(data, staff).await? < AccessRank::Helper {
        return interact::send_modal_error(ctx, modal, "Error", STAFF_ONLY).await;
    }
    let Some(thread_id) = modal
        .data
        .custom_id
        .strip_prefix("evidence_media_modal:")
        .and_then(|raw| raw.parse::<i64>().ok())
    else {
        return Ok(());
    };
    modal.defer_ephemeral(&ctx.http).await?;
    let uploads: Vec<&Attachment> = modal
        .data
        .components
        .iter()
        .filter_map(|c| match c {
            Component::Label(label) => match &label.component {
                LabelComponent::FileUpload(upload) => Some(upload.values.iter()),
                _ => None,
            },
            _ => None,
        })
        .flatten()
        .filter_map(|id| modal.data.resolved.attachments.get(id))
        .collect();
    let sources: Vec<(String, String)> = uploads
        .iter()
        .map(|a| (a.filename.to_string(), a.url.to_string()))
        .collect();
    let message = add_sources(ctx, data, thread_id, &sources, staff).await;
    modal
        .edit_response(
            &ctx.http,
            EditInteractionResponse::new()
                .flags(MessageFlags::IS_COMPONENTS_V2)
                .components(vec![CreateComponent::Container(CreateContainer::new(
                    vec![text(message)],
                ))]),
        )
        .await?;
    Ok(())
}

pub async fn add_sources(
    ctx: &Context,
    data: &Data,
    thread_id: i64,
    sources: &[(String, String)],
    actor: u64,
) -> String {
    let rejected: Vec<&str> = sources
        .iter()
        .filter(|(name, _)| !is_media_filename(name))
        .map(|(name, _)| name.as_str())
        .collect();
    if !rejected.is_empty() {
        return format!(
            "Only images and videos are accepted (png, jpg, gif, webp, mp4, webm, mov). Not added: {}",
            rejected.join(", ")
        );
    }
    let mut files = Vec::new();
    let mut failed = Vec::new();
    for (name, url) in sources {
        match media::download(url).await {
            Some(bytes) => files.push((media_extension(name), bytes)),
            None => failed.push(name.as_str()),
        }
    }
    if !failed.is_empty() {
        return format!(
            "These files could not be downloaded, so nothing was added: {}",
            failed.join(", ")
        );
    }
    match ops::upload(ctx, data, thread_id, files, actor).await {
        Ok(result) => upload_summary(&result),
        Err(e) => e.to_string(),
    }
}

fn upload_summary(result: &ops::UploadResult) -> String {
    let added = result.report.added;
    let mut lines = vec![format!(
        "Added {added} attachment{}.",
        if added == 1 { "" } else { "s" }
    )];
    match &result.conversion {
        Conversion::Converted => lines.push("The tag is now confirmed.".to_string()),
        Conversion::Failed(cause) => lines.push(format!("The tag could not be confirmed. {cause}")),
        Conversion::AlreadyConfirmed | Conversion::NotApplicable => {}
    }
    if !result.report.reattached.is_empty() {
        lines.push(format!(
            "Discord dropped {} during the edit; they were re-attached.",
            result.report.reattached.join(", ")
        ));
    }
    if !result.report.lost.is_empty() {
        lines.push(format!(
            "Evidence was lost: {}",
            result.report.lost.join(", ")
        ));
    }
    lines.join("\n")
}

pub async fn handle_manage(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    if get_rank(data, component.user.id.get()).await? < AccessRank::Helper {
        return interact::send_component_error(ctx, component, "Error", STAFF_ONLY).await;
    }
    let thread_id = component.channel_id.get() as i64;
    let view = picker(ctx, thread_id, None).await;
    component
        .create_response(
            &ctx.http,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .flags(MessageFlags::IS_COMPONENTS_V2 | MessageFlags::EPHEMERAL)
                    .components(view),
            ),
        )
        .await?;
    Ok(())
}

pub async fn handle_menu_action(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    if get_rank(data, component.user.id.get()).await? < AccessRank::Helper {
        return interact::send_component_error(ctx, component, "Error", STAFF_ONLY).await;
    }
    let id = component.data.custom_id.as_str();
    let mut parts = id.splitn(3, ':');
    let (Some(action), Some(thread)) = (parts.next(), parts.next()) else {
        return Ok(());
    };
    let Ok(thread_id) = thread.parse::<i64>() else {
        return Ok(());
    };
    let item: Option<u64> = parts.next().and_then(|raw| raw.parse().ok());
    let view = match action {
        "evidence_msel" => {
            let selected = match &component.data.kind {
                ComponentInteractionDataKind::StringSelect { values } => {
                    values.first().and_then(|v| v.parse().ok())
                }
                _ => None,
            };
            picker(ctx, thread_id, selected).await
        }
        "evidence_mrem" => confirm_removal(ctx, thread_id, item).await,
        "evidence_mpick" => picker(ctx, thread_id, item).await,
        "evidence_mrem_go" => {
            let Some(item) = item else {
                return Ok(());
            };
            component
                .create_response(&ctx.http, CreateInteractionResponse::Acknowledge)
                .await?;
            let view = match ops::remove(ctx, data, thread_id, AttachmentId::new(item)).await {
                Ok(_) => picker(ctx, thread_id, None).await,
                Err(e) => vec![interact::error_container("Error", &e.to_string())],
            };
            component
                .edit_response(
                    &ctx.http,
                    EditInteractionResponse::new()
                        .flags(MessageFlags::IS_COMPONENTS_V2)
                        .components(view),
                )
                .await?;
            return Ok(());
        }
        _ => return Ok(()),
    };
    interact::update_message(ctx, component, view).await
}

async fn gallery(ctx: &Context, thread_id: i64) -> Vec<MessageMedia> {
    current_media(ctx, thread_id)
        .await
        .map(|media| media.into_iter().filter(|m| m.in_gallery).collect())
        .unwrap_or_default()
}

async fn picker(
    ctx: &Context,
    thread_id: i64,
    selected: Option<u64>,
) -> Vec<CreateComponent<'static>> {
    let items = gallery(ctx, thread_id).await;
    let Some(current) = selected
        .and_then(|id| items.iter().find(|m| m.attachment_id.get() == id))
        .or_else(|| items.first())
    else {
        return vec![CreateComponent::Container(CreateContainer::new(vec![
            text("Removed"),
        ]))];
    };
    let mut parts = vec![
        text(format!(
            "## Remove Media\n{}",
            media_label(&current.filename)
        )),
        CreateContainerComponent::MediaGallery(CreateMediaGallery::new(vec![
            CreateMediaGalleryItem::new(CreateUnfurledMediaItem::new(current.url.clone())),
        ])),
    ];
    if items.len() > 1 {
        let options: Vec<CreateSelectMenuOption> = items
            .iter()
            .filter(|m| m.attachment_id != current.attachment_id)
            .map(|m| {
                CreateSelectMenuOption::new(
                    media_label(&m.filename),
                    m.attachment_id.get().to_string(),
                )
            })
            .collect();
        parts.push(CreateContainerComponent::ActionRow(
            CreateActionRow::SelectMenu(
                CreateSelectMenu::new(
                    format!("evidence_msel:{thread_id}"),
                    CreateSelectMenuKind::String {
                        options: options.into(),
                    },
                )
                .placeholder("View another piece..."),
            ),
        ));
    }
    parts.push(CreateContainerComponent::ActionRow(
        CreateActionRow::buttons(vec![
            CreateButton::new(format!(
                "evidence_mrem:{thread_id}:{}",
                current.attachment_id
            ))
            .label("Remove")
            .style(ButtonStyle::Danger),
        ]),
    ));
    vec![CreateComponent::Container(CreateContainer::new(parts))]
}

async fn confirm_removal(
    ctx: &Context,
    thread_id: i64,
    item: Option<u64>,
) -> Vec<CreateComponent<'static>> {
    let items = gallery(ctx, thread_id).await;
    let Some(current) = item.and_then(|id| items.iter().find(|m| m.attachment_id.get() == id))
    else {
        return picker(ctx, thread_id, None).await;
    };
    vec![CreateComponent::Container(CreateContainer::new(vec![
        text(format!("Remove `{}`?", media_label(&current.filename))),
        CreateContainerComponent::ActionRow(CreateActionRow::buttons(vec![
            CreateButton::new(format!(
                "evidence_mrem_go:{thread_id}:{}",
                current.attachment_id
            ))
            .label("Remove")
            .style(ButtonStyle::Danger),
            CreateButton::new(format!(
                "evidence_mpick:{thread_id}:{}",
                current.attachment_id
            ))
            .label("Cancel")
            .style(ButtonStyle::Secondary),
        ])),
    ]))]
}
