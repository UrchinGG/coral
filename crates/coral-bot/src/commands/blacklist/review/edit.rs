use std::collections::HashMap;

use anyhow::Result;
use serenity::all::*;

use database::{EvidenceKind, ReviewError, ReviewRepository, ReviewStatus, Tally, VoteKind};

use super::publish::{published_attachments, rerender_post};
use super::render::{self, MediaUrls, PostState};
use super::{Loaded, custom_id, deadline, face_attachment, face_filename};
use crate::framework::Data;
use crate::interact;
use crate::utils::discord_media::message_media;
use crate::utils::text;

pub async fn handle_edit_click(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
    review_id: i64,
) -> Result<()> {
    component.defer_ephemeral(&ctx.http).await?;
    let repo = ReviewRepository::new(data.db.pool());
    let Some(review) = repo.get(review_id).await? else {
        return Ok(());
    };
    let refusal = match review.status {
        ReviewStatus::Voided => Some("This review was voided."),
        status if status.is_terminal() => Some("This review is already resolved."),
        _ if review.submitter_id as u64 != component.user.id.get() => {
            Some("Only the submitter can edit this review.")
        }
        _ => None,
    };
    if let Some(refusal) = refusal {
        return reply(ctx, component, refusal).await;
    }
    let started_new = repo.edit_session(review_id).await?.is_none();
    match repo.begin_edit(review_id).await {
        Ok(_) => {}
        Err(ReviewError::WrongStatus(_)) => {
            return reply(ctx, component, "This review is already resolved.").await;
        }
        Err(e) => return Err(anyhow::anyhow!("{e:?}")),
    }
    if started_new {
        deadline::arm(ctx.clone(), data.clone(), review_id, deadline::Kind::Edit);
        let (ctx, data) = (ctx.clone(), data.clone());
        tokio::spawn(async move { rerender_post(&ctx, &data, review_id).await });
    }
    let view = super::console::console_view(ctx, data, review_id, None).await?;
    component
        .edit_response(
            &ctx.http,
            EditInteractionResponse::new()
                .flags(MessageFlags::IS_COMPONENTS_V2)
                .components(view)
                .new_attachment(face_attachment(data, &review.player_uuid).await),
        )
        .await?;
    Ok(())
}

pub async fn save(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
    loaded: Loaded,
) -> Result<()> {
    if !loaded
        .evidence
        .iter()
        .any(|e| e.kind == EvidenceKind::Replay)
    {
        return super::console::show_console(ctx, component, data, loaded.review.id, None).await;
    }
    let outcome = publish_session(ctx, data, &loaded).await;
    let reply = match outcome {
        Ok(()) => render::simple(format!("Your review of `{}` was updated.", loaded.username)),
        Err(e) => {
            tracing::error!("Failed to save edit of review {}: {e:?}", loaded.review.id);
            super::console::console_view(
                ctx,
                data,
                loaded.review.id,
                Some("Your changes could not be saved. Nothing was published; try again."),
            )
            .await?
        }
    };
    component
        .edit_response(
            &ctx.http,
            EditInteractionResponse::new()
                .flags(MessageFlags::IS_COMPONENTS_V2)
                .components(reply)
                .new_attachment(face_attachment(data, &loaded.review.player_uuid).await),
        )
        .await?;
    Ok(())
}

async fn publish_session(ctx: &Context, data: &Data, loaded: &Loaded) -> Result<()> {
    let thread_id = loaded
        .review
        .thread_id
        .ok_or_else(|| anyhow::anyhow!("review has no thread"))?;
    let repo = ReviewRepository::new(data.db.pool());
    let staged = repo
        .staged_media(loaded.review.id, loaded.revision.revision)
        .await?;
    let channel_id = GenericChannelId::new(thread_id as u64);
    let message_id = MessageId::new(thread_id as u64);
    let existing = message_media(&ctx.http, channel_id, message_id).await?;
    let by_attachment: HashMap<u64, String> = existing
        .iter()
        .map(|m| (m.attachment_id.get(), m.url.clone()))
        .collect();
    let urls = MediaUrls {
        evidence: loaded
            .evidence
            .iter()
            .filter_map(|e| Some((e.id, by_attachment.get(&(e.attachment_id? as u64))?.clone())))
            .collect(),
        face: existing
            .iter()
            .find(|m| !m.in_gallery && m.filename == face_filename(&loaded.review.player_uuid))
            .map(|m| m.url.clone()),
    };
    let votes = repo.active_votes(loaded.review.id).await?;
    let kept_votes: Vec<_> = votes
        .into_iter()
        .filter(|v| v.kind == VoteKind::Reject)
        .collect();
    let tally = Tally {
        reject: kept_votes.len() as i64,
        ..Default::default()
    };
    let components = render::public_post(
        loaded,
        &PostState {
            tally,
            votes: &kept_votes,
            paused: false,
            edited_at: Some(chrono::Utc::now()),
            rules: crate::utils::standing::rules(data).await?,
        },
        &urls,
    );
    let mut attachments = EditAttachments::new();
    for item in &existing {
        attachments = attachments.keep(item.attachment_id);
    }
    let uploads: Vec<CreateAttachment<'static>> = staged
        .iter()
        .map(|file| CreateAttachment::bytes(file.bytes.clone(), file.filename.clone()))
        .collect();
    for upload in &uploads {
        attachments = attachments.add(upload.clone());
    }
    ctx.http
        .edit_message(
            channel_id,
            message_id,
            &EditMessage::new()
                .flags(MessageFlags::IS_COMPONENTS_V2)
                .components(components)
                .attachments(attachments),
            uploads,
        )
        .await?;
    let media = message_media(&ctx.http, channel_id, message_id).await?;
    let refs = published_attachments(&staged, &media, thread_id);
    repo.save_edit(loaded.review.id, &refs)
        .await
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    rerender_post(ctx, data, loaded.review.id).await;
    Ok(())
}

pub async fn withdraw_prompt(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
    loaded: &Loaded,
) -> Result<()> {
    if ReviewRepository::new(data.db.pool())
        .ever_voted(loaded.review.id)
        .await?
    {
        return super::console::show_console(ctx, component, data, loaded.review.id, None).await;
    }
    interact::edit_component(
        ctx,
        component,
        vec![CreateComponent::Container(CreateContainer::new(vec![
            text(format!(
                "Withdraw your review of `{}`? It closes without a tag and nobody is scored.",
                loaded.username
            )),
            CreateContainerComponent::ActionRow(CreateActionRow::buttons(vec![
                CreateButton::new(custom_id("withdraw_go", loaded.review.id))
                    .label("Withdraw")
                    .style(ButtonStyle::Danger),
            ])),
        ]))],
    )
    .await
}

pub async fn withdraw(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
    loaded: &Loaded,
) -> Result<()> {
    let repo = ReviewRepository::new(data.db.pool());
    let message = match repo
        .withdraw(loaded.review.id, loaded.review.submitter_id)
        .await
    {
        Ok(review) => {
            super::resolve::close_out(ctx, data, &review).await;
            format!("Your review of `{}` was withdrawn.", loaded.username)
        }
        Err(ReviewError::VotesExist) => {
            "You can't withdraw a review once anyone has voted on it. If you made a mistake, ask a \
             staff member."
                .to_string()
        }
        Err(ReviewError::WrongStatus(ReviewStatus::Voided)) => {
            "This review was voided.".to_string()
        }
        Err(ReviewError::WrongStatus(_)) => "This review is already resolved.".to_string(),
        Err(e) => return Err(anyhow::anyhow!("{e:?}")),
    };
    interact::edit_component(ctx, component, render::simple(message)).await
}

async fn reply(ctx: &Context, component: &ComponentInteraction, message: &str) -> Result<()> {
    interact::edit_component(ctx, component, render::simple(message)).await
}
