use anyhow::{Result, anyhow};
use serenity::all::*;

use database::{
    RequestTally, RequestVoteChange, Review, ReviewError, ReviewRepository, ReviewStatus, Revision,
    RevisionRequest,
};

use super::render;
use super::vote::{bounce, state_bounce};
use super::{REASON_MAX_CHARS, REVIEW_TAGS, REVISION_VOTES_NEEDED, custom_id};
use crate::commands::blacklist::appeal;
use crate::framework::Data;
use crate::interact;

pub const ACTIONS: &[&str] = &["revise", "rqnew", "rqyes", "rqno"];
pub const MODAL_ACTION: &str = "revise_m";
const TYPE_FIELD: &str = "type";
const REASON_FIELD: &str = "reason";
const UNCHANGED: &str = "Change the tag type or reason to suggest a revision.";
const SUBMITTER_NO: &str =
    "Your No does nothing as the submitter. Press Yes to apply this revision yourself.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Apply,
    Reject,
}

pub fn verdict(tally: RequestTally) -> Option<Verdict> {
    if tally.yes >= REVISION_VOTES_NEEDED {
        Some(Verdict::Apply)
    } else if tally.no >= REVISION_VOTES_NEEDED {
        Some(Verdict::Reject)
    } else {
        None
    }
}

pub async fn handle_component(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
    action: &str,
    review_id: i64,
    extra: Option<&str>,
) -> Result<()> {
    match action {
        "rqyes" | "rqno" => {
            let Some(request_id) = extra.and_then(|e| e.parse().ok()) else {
                return Ok(());
            };
            vote(
                ctx,
                component,
                data,
                review_id,
                request_id,
                action == "rqyes",
            )
            .await
        }
        _ => open(ctx, component, data, review_id, action == "rqnew").await,
    }
}

pub async fn handle_modal(
    ctx: &Context,
    modal: &ModalInteraction,
    data: &Data,
    review_id: i64,
) -> Result<()> {
    modal.defer_ephemeral(&ctx.http).await?;
    let Some(review) = ReviewRepository::new(data.db.pool()).get(review_id).await? else {
        return Ok(());
    };
    let tag_type = selected_type(&modal.data.components);
    let reason = interact::extract_modal_value(&modal.data.components, REASON_FIELD);
    let message = submit(
        ctx,
        data,
        &review,
        modal.user.id.get(),
        tag_type,
        reason.trim(),
    )
    .await?;
    modal
        .edit_response(
            &ctx.http,
            EditInteractionResponse::new()
                .flags(MessageFlags::IS_COMPONENTS_V2)
                .components(render::simple(message)),
        )
        .await?;
    Ok(())
}

pub async fn close_requests(ctx: &Context, data: &Data, review: &Review) {
    let closed = match ReviewRepository::new(data.db.pool())
        .close_open_requests(review.id)
        .await
    {
        Ok(closed) => closed,
        Err(e) => {
            tracing::error!(
                "Failed to close revision requests on review {}: {e}",
                review.id
            );
            return;
        }
    };
    for request in &closed {
        if let Err(e) = edit_request_message(ctx, review, request, false).await {
            tracing::warn!("Failed to retire revision request {}: {e:?}", request.id);
        }
    }
}

async fn open(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
    review_id: i64,
    new_only: bool,
) -> Result<()> {
    let repo = ReviewRepository::new(data.db.pool());
    let Some(review) = repo.get(review_id).await? else {
        return Ok(());
    };
    if let Some(refusal) = bounce(ctx, data, &review, component.user.id.get()).await? {
        return reply(ctx, component, render::simple(refusal)).await;
    }
    let requests = if new_only {
        Vec::new()
    } else {
        repo.open_requests(review_id).await?
    };
    if !requests.is_empty() {
        return reply(ctx, component, render::revise_menu(review_id, &requests)).await;
    }
    let live = repo
        .live_revision(review_id)
        .await?
        .ok_or_else(|| anyhow!("review {review_id} has no live revision"))?;
    component
        .create_response(
            &ctx.http,
            CreateInteractionResponse::Modal(revision_modal(review_id, &live)),
        )
        .await?;
    Ok(())
}

fn revision_modal(review_id: i64, live: &Revision) -> CreateModal<'static> {
    let options: Vec<CreateSelectMenuOption> = blacklist::all()
        .iter()
        .filter(|def| REVIEW_TAGS.contains(&def.name))
        .map(|def| {
            CreateSelectMenuOption::new(def.display_name, def.name)
                .default_selection(def.name == live.tag_type)
        })
        .collect();
    let tag_type = CreateSelectMenu::new(
        TYPE_FIELD,
        CreateSelectMenuKind::String {
            options: options.into(),
        },
    );
    let reason = CreateInputText::new(InputTextStyle::Paragraph, REASON_FIELD)
        .value(live.reason.clone())
        .max_length(REASON_MAX_CHARS)
        .required(true);
    CreateModal::new(custom_id(MODAL_ACTION, review_id), "Revision").components(vec![
        CreateModalComponent::Label(CreateLabel::select_menu("Tag type", tag_type)),
        CreateModalComponent::Label(CreateLabel::input_text("Reason", reason)),
    ])
}

async fn submit(
    ctx: &Context,
    data: &Data,
    review: &Review,
    author: u64,
    tag_type: Option<String>,
    reason: &str,
) -> Result<String> {
    if let Some(refusal) = bounce(ctx, data, review, author).await? {
        return Ok(refusal);
    }
    let Some(tag_type) = tag_type.filter(|t| REVIEW_TAGS.contains(&t.as_str())) else {
        return Ok("Pick a tag type for the revision.".into());
    };
    if reason.is_empty() {
        return Ok("A revision needs a reason.".into());
    }
    let repo = ReviewRepository::new(data.db.pool());
    let live = repo
        .live_revision(review.id)
        .await?
        .ok_or_else(|| anyhow!("review {} has no live revision", review.id))?;
    if live.tag_type == tag_type && live.reason.trim() == reason {
        return Ok(UNCHANGED.into());
    }
    let request = match repo
        .create_request(review.id, author as i64, &tag_type, reason)
        .await
    {
        Ok(request) => request,
        Err(e) => return refusal(e),
    };
    post_request(ctx, data, review, &request).await?;
    spawn_rerender(ctx, data, review.id);
    Ok("Your revision was posted in the thread.".into())
}

async fn post_request(
    ctx: &Context,
    data: &Data,
    review: &Review,
    request: &RevisionRequest,
) -> Result<()> {
    let thread_id = thread_of(review)?;
    let submitter = UserId::new(review.submitter_id as u64);
    let mentions = if review.hide_username {
        CreateAllowedMentions::new()
    } else {
        CreateAllowedMentions::new().users(vec![submitter])
    };
    let message = CreateMessage::new()
        .flags(MessageFlags::IS_COMPONENTS_V2)
        .components(render::revision_request(review, request, true))
        .allowed_mentions(mentions);
    let sent = ctx
        .http
        .send_message(thread_id, Vec::<CreateAttachment>::new(), &message)
        .await?;
    ReviewRepository::new(data.db.pool())
        .set_request_message(request.id, sent.id.get() as i64)
        .await?;
    if review.hide_username {
        let dm = format!("A revision was suggested for your review in <#{thread_id}>.");
        if !appeal::send_dm(ctx, submitter, &dm).await {
            tracing::warn!(
                "Anonymous review {} revision DM was not delivered",
                review.id
            );
        }
    }
    Ok(())
}

async fn vote(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
    review_id: i64,
    request_id: i64,
    yes: bool,
) -> Result<()> {
    component.defer_ephemeral(&ctx.http).await?;
    let Some(review) = ReviewRepository::new(data.db.pool()).get(review_id).await? else {
        return Ok(());
    };
    let voter = component.user.id.get();
    let message = if review.submitter_id as u64 == voter {
        submitter_vote(ctx, data, &review, request_id, yes).await?
    } else {
        voter_vote(ctx, data, &review, request_id, voter, yes).await?
    };
    interact::edit_component(ctx, component, render::simple(message)).await
}

async fn submitter_vote(
    ctx: &Context,
    data: &Data,
    review: &Review,
    request_id: i64,
    yes: bool,
) -> Result<String> {
    if let Some(refusal) = state_bounce(ctx, data, review).await? {
        return Ok(refusal);
    }
    if !yes {
        return Ok(SUBMITTER_NO.into());
    }
    apply(ctx, data, review, request_id, review.submitter_id as u64).await
}

async fn voter_vote(
    ctx: &Context,
    data: &Data,
    review: &Review,
    request_id: i64,
    voter: u64,
    yes: bool,
) -> Result<String> {
    if let Some(refusal) = bounce(ctx, data, review, voter).await? {
        return Ok(refusal);
    }
    let (change, request) = match ReviewRepository::new(data.db.pool())
        .vote_on_request(request_id, voter as i64, yes)
        .await
    {
        Ok(cast) => cast,
        Err(e) => return refusal(e),
    };
    match verdict(request.tally) {
        Some(Verdict::Apply) => return apply(ctx, data, review, request_id, voter).await,
        Some(Verdict::Reject) => return reject(ctx, data, review, &request).await,
        None => {}
    }
    if change != RequestVoteChange::Unchanged {
        edit_request_message(ctx, review, &request, true).await?;
        spawn_rerender(ctx, data, review.id);
    }
    let choice = if yes { "Yes" } else { "No" };
    Ok(match change {
        RequestVoteChange::Recorded => format!("You voted **{choice}** on this revision."),
        RequestVoteChange::Switched => {
            format!("You changed your vote to **{choice}** on this revision.")
        }
        RequestVoteChange::Unchanged => {
            format!("You already voted **{choice}** on this revision.")
        }
    })
}

async fn apply(
    ctx: &Context,
    data: &Data,
    review: &Review,
    request_id: i64,
    applied_by: u64,
) -> Result<String> {
    let request = match ReviewRepository::new(data.db.pool())
        .apply_request(request_id, applied_by as i64)
        .await
    {
        Ok(request) => request,
        Err(e) => return refusal(e),
    };
    super::publish::try_rerender_post(ctx, data, review.id).await?;
    edit_request_message(ctx, review, &request, false).await?;
    let by_submitter = applied_by == review.submitter_id as u64;
    let notice = CreateMessage::new()
        .flags(MessageFlags::IS_COMPONENTS_V2)
        .components(render::applied_revision(&request, by_submitter));
    ctx.http
        .send_message(thread_of(review)?, Vec::<CreateAttachment>::new(), &notice)
        .await?;
    Ok("The revision was applied.".into())
}

async fn reject(
    ctx: &Context,
    data: &Data,
    review: &Review,
    request: &RevisionRequest,
) -> Result<String> {
    if !ReviewRepository::new(data.db.pool())
        .reject_request(request.id)
        .await?
    {
        return Ok("This revision is no longer open.".into());
    }
    if let Some(message_id) = request.message_id {
        ctx.http
            .delete_message(thread_of(review)?, MessageId::new(message_id as u64), None)
            .await?;
    }
    spawn_rerender(ctx, data, review.id);
    Ok("You voted **No**. The revision was turned down.".into())
}

async fn edit_request_message(
    ctx: &Context,
    review: &Review,
    request: &RevisionRequest,
    votable: bool,
) -> Result<()> {
    let Some(message_id) = request.message_id else {
        return Ok(());
    };
    ctx.http
        .edit_message(
            thread_of(review)?,
            MessageId::new(message_id as u64),
            &EditMessage::new()
                .flags(MessageFlags::IS_COMPONENTS_V2)
                .components(render::revision_request(review, request, votable)),
            Vec::new(),
        )
        .await?;
    Ok(())
}

fn refusal(error: ReviewError) -> Result<String> {
    Ok(match error {
        ReviewError::WrongStatus(ReviewStatus::Voided) => "This review was voided.".into(),
        ReviewError::WrongStatus(_) => "This review is already resolved.".into(),
        ReviewError::RequestClosed => "This revision is no longer open.".into(),
        ReviewError::EditInProgress => {
            "Voting is paused while the submitter revises this review.".into()
        }
        e => return Err(anyhow!("{e:?}")),
    })
}

fn selected_type(components: &[Component]) -> Option<String> {
    components.iter().find_map(|component| match component {
        Component::Label(label) => match &label.component {
            LabelComponent::SelectMenu(menu) if menu.custom_id == TYPE_FIELD => {
                menu.values.first().map(|v| v.to_string())
            }
            _ => None,
        },
        _ => None,
    })
}

fn thread_of(review: &Review) -> Result<GenericChannelId> {
    review
        .thread_id
        .map(|id| GenericChannelId::new(id as u64))
        .ok_or_else(|| anyhow!("review {} has no thread", review.id))
}

fn spawn_rerender(ctx: &Context, data: &Data, review_id: i64) {
    let (ctx, data) = (ctx.clone(), data.clone());
    tokio::spawn(async move { super::publish::rerender_post(&ctx, &data, review_id).await });
}

async fn reply(
    ctx: &Context,
    component: &ComponentInteraction,
    components: Vec<CreateComponent<'static>>,
) -> Result<()> {
    component
        .create_response(
            &ctx.http,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .flags(MessageFlags::IS_COMPONENTS_V2 | MessageFlags::EPHEMERAL)
                    .components(components),
            ),
        )
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tally(yes: i64, no: i64) -> RequestTally {
        RequestTally { yes, no }
    }

    #[test]
    fn three_yes_apply_and_three_no_reject() {
        assert_eq!(verdict(tally(2, 2)), None);
        assert_eq!(verdict(tally(3, 0)), Some(Verdict::Apply));
        assert_eq!(verdict(tally(1, 3)), Some(Verdict::Reject));
        assert_eq!(verdict(tally(3, 3)), Some(Verdict::Apply));
    }
}
