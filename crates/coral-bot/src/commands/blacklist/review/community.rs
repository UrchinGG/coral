use std::collections::HashMap;

use anyhow::{Result, anyhow};
use serenity::all::*;

use coral_redis::{ThreadAttachment, review_media};
use database::{
    ContributedMedia, PromptOutcome, PromptTally, RequestVoteChange, Review, ReviewError,
    ReviewRepository, ReviewStatus,
};

use super::publish::thread_link;
use super::render::{self, MediaUrls, PostState};
use super::vote::bounce;
use super::{ALLOWED_MEDIA_EXTENSIONS, EVIDENCE_VOTES_NEEDED, MAX_MEDIA, face_filename, load_live};
use crate::commands::blacklist::appeal;
use crate::framework::Data;
use crate::utils::discord_media::message_media;
use crate::utils::text;

pub const ACTIONS: &[&str] = &[ADD, DISMISS];
pub const ADD: &str = "media_add";
pub const DISMISS: &str = "media_dismiss";

const OWN_MEDIA: &str = "You posted this media.";
const PROMPT_CLOSED: &str = "This prompt is no longer open.";
const MESSAGE_GONE: &str = "That message is gone. Ask the sender to post it again.";
const EDIT_PAUSED: &str = "Voting is paused while the submitter is editing.";
const ADD_FAILED: &str = "The files could not be added. Try again.";

struct Download {
    filename: String,
    content_type: Option<String>,
    bytes: Vec<u8>,
}

enum Refusal {
    Reply(String),
    Closed(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PromptVerdict {
    Add,
    Dismiss,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Adder {
    Submitter,
    Vote,
}

fn verdict(tally: PromptTally) -> Option<PromptVerdict> {
    if tally.add >= EVIDENCE_VOTES_NEEDED {
        Some(PromptVerdict::Add)
    } else if tally.dismiss >= EVIDENCE_VOTES_NEEDED {
        Some(PromptVerdict::Dismiss)
    } else {
        None
    }
}

pub async fn prompt(
    ctx: &Context,
    data: &Data,
    review_id: i64,
    thread_id: u64,
    message_id: u64,
    author_id: u64,
    attachments: Vec<ThreadAttachment>,
) -> Result<()> {
    let media: Vec<ThreadAttachment> = attachments
        .into_iter()
        .filter(|a| ALLOWED_MEDIA_EXTENSIONS.contains(&extension(&a.filename).as_str()))
        .collect();
    if media.is_empty() {
        return Ok(());
    }
    let Some(review) = ReviewRepository::new(data.db.pool()).get(review_id).await? else {
        return Ok(());
    };
    if review.status != ReviewStatus::Open || review.thread_id != Some(thread_id as i64) {
        return Ok(());
    }
    review_media::store(&mut data.redis.connection(), message_id, &media).await?;

    let thread = GenericChannelId::new(thread_id);
    let message = CreateMessage::new()
        .flags(MessageFlags::IS_COMPONENTS_V2)
        .components(render::evidence_prompt(
            review.id,
            message_id,
            PromptTally::default(),
            true,
        ))
        .allowed_mentions(CreateAllowedMentions::new().replied_user(false))
        .reference_message(
            MessageReference::new(MessageReferenceKind::Default, thread)
                .message_id(MessageId::new(message_id)),
        );
    let sent = ctx
        .http
        .send_message(thread, Vec::<CreateAttachment>::new(), &message)
        .await?;
    ReviewRepository::new(data.db.pool())
        .create_evidence_prompt(
            message_id as i64,
            review.id,
            sent.id.get() as i64,
            author_id as i64,
        )
        .await?;
    if review.hide_username {
        alert_submitter(ctx, data, &review, thread_id, sent.id).await;
    }
    Ok(())
}

pub async fn handle_component(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
    action: &str,
    review_id: i64,
    extra: Option<&str>,
) -> Result<()> {
    let Some(source_id) = extra.and_then(|e| e.parse::<u64>().ok()) else {
        return Ok(());
    };
    component
        .create_response(&ctx.http, CreateInteractionResponse::Acknowledge)
        .await?;
    let Some(review) = ReviewRepository::new(data.db.pool()).get(review_id).await? else {
        return Ok(());
    };
    if review.submitter_id as u64 == component.user.id.get() {
        return submitter_action(ctx, component, data, &review, action == ADD, source_id).await;
    }
    vote(ctx, component, data, &review, action == ADD, source_id).await
}

pub async fn close_prompts(ctx: &Context, data: &Data, review: &Review) {
    let closed = match ReviewRepository::new(data.db.pool())
        .close_open_evidence_prompts(review.id)
        .await
    {
        Ok(closed) => closed,
        Err(e) => {
            tracing::error!(
                "Failed to close evidence prompts on review {}: {e}",
                review.id
            );
            return;
        }
    };
    let Some(thread_id) = review.thread_id else {
        return;
    };
    for prompt in closed {
        let source_id = prompt.source_message_id as u64;
        let retired = EditMessage::new()
            .flags(MessageFlags::IS_COMPONENTS_V2)
            .components(render::evidence_prompt(
                review.id,
                source_id,
                prompt.tally,
                false,
            ));
        if let Err(e) = ctx
            .http
            .edit_message(
                GenericChannelId::new(thread_id as u64),
                MessageId::new(prompt.prompt_message_id as u64),
                &retired,
                Vec::new(),
            )
            .await
        {
            tracing::warn!("Failed to retire evidence prompt {source_id}: {e}");
        }
        forget(data, source_id).await;
    }
}

async fn submitter_action(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
    review: &Review,
    add_media: bool,
    source_id: u64,
) -> Result<()> {
    if let Some(closed) = closed_notice(review.status) {
        reply(ctx, component, closed).await?;
        remove_prompt(ctx, data, component, source_id).await;
        return Ok(());
    }
    if !add_media {
        remove_prompt(ctx, data, component, source_id).await;
        return Ok(());
    }
    add(ctx, component, data, review, source_id, Adder::Submitter).await?;
    Ok(())
}

async fn vote(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
    review: &Review,
    add_media: bool,
    source_id: u64,
) -> Result<()> {
    let voter = component.user.id.get();
    if let Some(refusal) = bounce(ctx, data, review, voter).await? {
        return reply(ctx, component, &refusal).await;
    }
    let repo = ReviewRepository::new(data.db.pool());
    let Some(prompt) = repo.evidence_prompt(source_id as i64).await? else {
        return reply(ctx, component, PROMPT_CLOSED).await;
    };
    if prompt.author_id as u64 == voter {
        return reply(ctx, component, OWN_MEDIA).await;
    }
    let (change, prompt) = match repo
        .vote_on_evidence_prompt(source_id as i64, voter as i64, add_media)
        .await
    {
        Ok(cast) => cast,
        Err(ReviewError::WrongStatus(status)) => {
            let notice = closed_notice(status).unwrap_or(PROMPT_CLOSED);
            return reply(ctx, component, notice).await;
        }
        Err(ReviewError::RequestClosed) => return reply(ctx, component, PROMPT_CLOSED).await,
        Err(e) => return Err(anyhow!("{e:?}")),
    };
    let choice = if add_media { "Add" } else { "Dismiss" };
    match verdict(prompt.tally) {
        Some(PromptVerdict::Add) => {
            return add_by_vote(ctx, component, data, review, source_id).await;
        }
        Some(PromptVerdict::Dismiss) => {
            remove_prompt(ctx, data, component, source_id).await;
            return reply(
                ctx,
                component,
                "You voted **Dismiss**. The media was dismissed.",
            )
            .await;
        }
        None => {}
    }
    if change != RequestVoteChange::Unchanged {
        component
            .edit_response(
                &ctx.http,
                EditInteractionResponse::new()
                    .flags(MessageFlags::IS_COMPONENTS_V2)
                    .components(render::evidence_prompt(
                        review.id,
                        source_id,
                        prompt.tally,
                        true,
                    )),
            )
            .await?;
    }
    let message = match change {
        RequestVoteChange::Recorded => format!("You voted **{choice}** on this media."),
        RequestVoteChange::Switched => {
            format!("You changed your vote to **{choice}** on this media.")
        }
        RequestVoteChange::Unchanged => format!("You already voted **{choice}** on this media."),
    };
    reply(ctx, component, &message).await
}

async fn add_by_vote(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
    review: &Review,
    source_id: u64,
) -> Result<()> {
    let repo = ReviewRepository::new(data.db.pool());
    if !repo.claim_evidence_prompt(source_id as i64).await? {
        return reply(ctx, component, PROMPT_CLOSED).await;
    }
    let added = add(ctx, component, data, review, source_id, Adder::Vote).await;
    repo.release_evidence_prompt(source_id as i64).await?;
    if added? {
        reply(ctx, component, "You voted **Add**. The media was added.").await?;
    }
    Ok(())
}

async fn add(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
    review: &Review,
    source_id: u64,
    adder: Adder,
) -> Result<bool> {
    if super::vote::paused(ctx, data, review.id).await? {
        reply(ctx, component, EDIT_PAUSED).await?;
        return Ok(false);
    }
    let stored = review_media::load(&mut data.redis.connection(), source_id).await?;
    let source = ctx
        .http
        .get_message(component.channel_id, MessageId::new(source_id))
        .await;
    let (Some(files), Ok(source)) = (stored, source) else {
        reply(ctx, component, MESSAGE_GONE).await?;
        remove_prompt(ctx, data, component, source_id).await;
        return Ok(false);
    };
    let room = room_left(data, review.id).await?;
    if files.len() as i64 > room {
        reply(ctx, component, &room_notice(room)).await?;
        return Ok(false);
    }
    let Some(downloads) = download(ctx, data, review.id, &files).await? else {
        reply(ctx, component, MESSAGE_GONE).await?;
        remove_prompt(ctx, data, component, source_id).await;
        return Ok(false);
    };
    match contribute(ctx, data, review, source.author.id, &downloads).await {
        Ok(()) => {}
        Err(Refusal::Reply(message)) => {
            reply(ctx, component, &message).await?;
            return Ok(false);
        }
        Err(Refusal::Closed(message)) => {
            reply(ctx, component, message).await?;
            remove_prompt(ctx, data, component, source_id).await;
            return Ok(false);
        }
    }
    show_contribution(ctx, component, source.author.id, downloads, adder).await?;
    settle(data, source_id, PromptOutcome::Added).await;
    forget(data, source_id).await;
    Ok(true)
}

async fn contribute(
    ctx: &Context,
    data: &Data,
    review: &Review,
    contributor: UserId,
    downloads: &[Download],
) -> Result<(), Refusal> {
    let repo = ReviewRepository::new(data.db.pool());
    let items: Vec<ContributedMedia> = downloads
        .iter()
        .map(|d| ContributedMedia {
            filename: d.filename.clone(),
            content_type: d.content_type.clone(),
        })
        .collect();
    match repo
        .add_contributed_media(review.id, &items, contributor.get() as i64, MAX_MEDIA)
        .await
    {
        Ok(_) => {}
        Err(ReviewError::NoRoom(room)) => return Err(Refusal::Reply(room_notice(room))),
        Err(ReviewError::EditInProgress) => return Err(Refusal::Reply(EDIT_PAUSED.into())),
        Err(ReviewError::WrongStatus(status)) => {
            return Err(Refusal::Closed(
                closed_notice(status).unwrap_or("This review is already resolved."),
            ));
        }
        Err(e) => {
            tracing::error!("Failed to record media for review {}: {e:?}", review.id);
            return Err(Refusal::Reply(ADD_FAILED.into()));
        }
    }
    if let Err(e) = attach_to_post(ctx, data, review.id, downloads).await {
        tracing::error!("Failed to add media to review {}: {e:?}", review.id);
        let filenames: Vec<String> = items.into_iter().map(|i| i.filename).collect();
        if let Err(e) = repo.discard_unattached_media(review.id, &filenames).await {
            tracing::error!("Failed to discard media rows for review {}: {e}", review.id);
        }
        super::publish::rerender_post(ctx, data, review.id).await;
        return Err(Refusal::Reply(ADD_FAILED.into()));
    }
    Ok(())
}

async fn attach_to_post(
    ctx: &Context,
    data: &Data,
    review_id: i64,
    downloads: &[Download],
) -> Result<()> {
    let Some(loaded) = load_live(ctx, data, review_id).await? else {
        anyhow::bail!("review has no live revision");
    };
    let thread_id = loaded
        .review
        .thread_id
        .ok_or_else(|| anyhow::anyhow!("review has no thread"))?;
    let repo = ReviewRepository::new(data.db.pool());
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
    let votes = repo.active_votes(review_id).await?;
    let components = render::public_post(
        &loaded,
        &PostState {
            tally: repo.tally(review_id).await?,
            revise_supporters: repo.revise_supporters(review_id).await?,
            votes: &votes,
            paused: repo.edit_session(review_id).await?.is_some(),
            edited_at: (repo.revision_count(review_id).await? > 1)
                .then_some(loaded.revision.published_at)
                .flatten(),
            rules: crate::utils::standing::rules(data).await?,
        },
        &urls,
    );
    let mut attachments = EditAttachments::new();
    for item in &existing {
        attachments = attachments.keep(item.attachment_id);
    }
    let uploads: Vec<CreateAttachment<'static>> = downloads
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
    let refs: Vec<(String, i64)> = downloads
        .iter()
        .filter_map(|file| {
            media
                .iter()
                .find(|m| m.in_gallery && m.filename == file.filename)
                .map(|m| (file.filename.clone(), m.attachment_id.get() as i64))
        })
        .collect();
    repo.attach_contributed_media(review_id, thread_id, &refs)
        .await?;
    Ok(())
}

async fn show_contribution(
    ctx: &Context,
    component: &ComponentInteraction,
    contributor: UserId,
    downloads: Vec<Download>,
    adder: Adder,
) -> Result<()> {
    let gallery = CreateMediaGallery::new(
        downloads
            .iter()
            .map(|d| {
                CreateMediaGalleryItem::new(CreateUnfurledMediaItem::new(format!(
                    "attachment://{}",
                    d.filename
                )))
            })
            .collect::<Vec<_>>(),
    );
    let mut parts = vec![
        CreateContainerComponent::MediaGallery(gallery),
        text(format!("-# Evidence contributed by <@{contributor}>")),
    ];
    if adder == Adder::Vote {
        parts.push(text("-# Added by vote."));
    }
    let mut response = EditInteractionResponse::new()
        .flags(MessageFlags::IS_COMPONENTS_V2)
        .components(vec![CreateComponent::Container(CreateContainer::new(
            parts,
        ))])
        .allowed_mentions(CreateAllowedMentions::new());
    for file in downloads {
        response = response.new_attachment(CreateAttachment::bytes(file.bytes, file.filename));
    }
    component.edit_response(&ctx.http, response).await?;
    Ok(())
}

async fn download(
    ctx: &Context,
    data: &Data,
    review_id: i64,
    files: &[ThreadAttachment],
) -> Result<Option<Vec<Download>>> {
    let Some(loaded) = load_live(ctx, data, review_id).await? else {
        return Ok(None);
    };
    let mut number = next_media_number(&loaded.evidence);
    let mut downloads = Vec::with_capacity(files.len());
    for file in files {
        let response = match reqwest::get(file.url.as_str()).await {
            Ok(response) => response.error_for_status(),
            Err(e) => Err(e),
        };
        let bytes = match response {
            Ok(response) => response.bytes().await,
            Err(e) => Err(e),
        };
        let bytes = match bytes {
            Ok(bytes) => bytes,
            Err(e) => {
                tracing::warn!("Failed to download thread media {}: {e}", file.filename);
                return Ok(None);
            }
        };
        downloads.push(Download {
            filename: format!("{}_{number}.{}", loaded.username, extension(&file.filename)),
            content_type: file.content_type.clone(),
            bytes: bytes.to_vec(),
        });
        number += 1;
    }
    Ok(Some(downloads))
}

async fn alert_submitter(
    ctx: &Context,
    data: &Data,
    review: &Review,
    thread_id: u64,
    prompt_id: MessageId,
) {
    let link = thread_link(data, Some(thread_id as i64))
        .map(|thread| format!("\n[Jump to message]({thread}/{prompt_id})"))
        .unwrap_or_default();
    let alert = format!("Someone posted media in your review's thread.{link}");
    if !appeal::send_dm(ctx, UserId::new(review.submitter_id as u64), &alert).await {
        tracing::warn!(
            "Anonymous review {} media alert DM was not delivered",
            review.id
        );
    }
}

async fn remove_prompt(
    ctx: &Context,
    data: &Data,
    component: &ComponentInteraction,
    source_id: u64,
) {
    if let Err(e) = ctx
        .http
        .delete_message(component.channel_id, component.message.id, None)
        .await
    {
        tracing::warn!(
            "Failed to delete media prompt {}: {e}",
            component.message.id
        );
    }
    settle(data, source_id, PromptOutcome::Dismissed).await;
    forget(data, source_id).await;
}

async fn settle(data: &Data, source_id: u64, outcome: PromptOutcome) {
    if let Err(e) = ReviewRepository::new(data.db.pool())
        .settle_evidence_prompt(source_id as i64, outcome)
        .await
    {
        tracing::error!("Failed to settle evidence prompt {source_id}: {e}");
    }
}

async fn forget(data: &Data, source_id: u64) {
    if let Err(e) = review_media::forget(&mut data.redis.connection(), source_id).await {
        tracing::warn!("Failed to clear staged thread media {source_id}: {e}");
    }
}

async fn room_left(data: &Data, review_id: i64) -> Result<i64> {
    let repo = ReviewRepository::new(data.db.pool());
    let Some(revision) = repo.live_revision(review_id).await? else {
        return Ok(0);
    };
    let used = repo.media_count(review_id, revision.revision).await?;
    Ok((MAX_MEDIA - used).max(0))
}

fn room_notice(room: i64) -> String {
    match room {
        0 => "This review has no room for more files.".to_string(),
        1 => "This review has room for 1 more file.".to_string(),
        n => format!("This review has room for {n} more files."),
    }
}

fn closed_notice(status: ReviewStatus) -> Option<&'static str> {
    match status {
        ReviewStatus::Voided => Some("This review was voided."),
        status if status.is_terminal() => Some("This review is already resolved."),
        _ => None,
    }
}

fn next_media_number(evidence: &[database::Evidence]) -> usize {
    evidence
        .iter()
        .filter_map(|e| e.filename.as_deref())
        .filter_map(|f| {
            let stem = f.rsplit_once('.').map_or(f, |(stem, _)| stem);
            stem.rsplit_once('_')?.1.parse::<usize>().ok()
        })
        .max()
        .unwrap_or(0)
        + 1
}

fn extension(filename: &str) -> String {
    filename
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
}

async fn reply(ctx: &Context, component: &ComponentInteraction, message: &str) -> Result<()> {
    component
        .create_followup(
            &ctx.http,
            CreateInteractionResponseFollowup::new()
                .flags(MessageFlags::IS_COMPONENTS_V2 | MessageFlags::EPHEMERAL)
                .components(render::simple(message)),
        )
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_adds_attach_and_three_dismisses_drop() {
        let tally = |add, dismiss| PromptTally { add, dismiss };
        assert_eq!(verdict(tally(2, 2)), None);
        assert_eq!(verdict(tally(3, 0)), Some(PromptVerdict::Add));
        assert_eq!(verdict(tally(1, 3)), Some(PromptVerdict::Dismiss));
        assert_eq!(verdict(tally(3, 3)), Some(PromptVerdict::Add));
    }

    #[test]
    fn room_notice_counts_files() {
        assert_eq!(room_notice(2), "This review has room for 2 more files.");
        assert_eq!(room_notice(1), "This review has room for 1 more file.");
    }
}
