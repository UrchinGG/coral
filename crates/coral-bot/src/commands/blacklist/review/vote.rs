use anyhow::Result;
use serenity::all::*;

use database::{
    CastOutcome, MemberRepository, Review, ReviewError, ReviewRepository, ReviewRules,
    ReviewStatus, VoteKind,
};

use super::resolve::{self, Outcome};
use super::{custom_id_with, edit_idle_window, load_live};
use crate::commands::blacklist::channel;
use crate::framework::{AccessRank, AccessRankExt, Data};
use crate::utils::text;

const OWN_SUBMISSION: &str = "You cannot review your own submission";

pub async fn handle_component(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
    action: &str,
    review_id: i64,
    extra: Option<&str>,
) -> Result<()> {
    let voter = component.user.id.get();
    let Some(review) = ReviewRepository::new(data.db.pool()).get(review_id).await? else {
        return Ok(());
    };
    if matches!(action, "switch" | "addvote" | "resolve") {
        component
            .create_response(&ctx.http, CreateInteractionResponse::Acknowledge)
            .await?;
    } else {
        component.defer_ephemeral(&ctx.http).await?;
    }
    if let Some(bounce) = bounce(ctx, data, &review, voter).await? {
        return respond(ctx, component, vec![text(bounce)]).await;
    }
    let kind = match action {
        "accept" => VoteKind::Accept,
        "reject" => VoteKind::Reject,
        "switch" | "addvote" => {
            let kind = parse_kind(extra);
            return record_from_prompt(ctx, component, data, &review, voter, kind).await;
        }
        "resolve" => return resolve_now(ctx, component, data, &review, parse_kind(extra)).await,
        _ => return Ok(()),
    };
    if is_staff(data, voter).await? {
        return staff_prompt(ctx, component, data, &review, voter, kind).await;
    }
    let held = ReviewRepository::new(data.db.pool())
        .active_vote(review_id, voter as i64)
        .await?;
    match held.map(|v| v.kind) {
        None => {
            let message = cast(ctx, data, &review, voter, kind).await?;
            respond(ctx, component, vec![text(message)]).await
        }
        Some(held) if held == kind => {
            respond(
                ctx,
                component,
                vec![text(format!(
                    "You have already voted to {} this player",
                    verb(kind)
                ))],
            )
            .await
        }
        Some(held) => {
            let lead = match held {
                VoteKind::Revise => "You've asked for a revision of this review.".to_string(),
                _ => format!("You've voted to {} this review.", verb(held)),
            };
            respond(
                ctx,
                component,
                vec![
                    text(format!("{lead} Switch to {}?", verb(kind))),
                    CreateContainerComponent::ActionRow(CreateActionRow::buttons(vec![
                        CreateButton::new(custom_id_with("switch", review.id, verb(kind)))
                            .label("Switch")
                            .style(ButtonStyle::Primary),
                    ])),
                ],
            )
            .await
        }
    }
}

pub(super) async fn bounce(
    ctx: &Context,
    data: &Data,
    review: &Review,
    voter: u64,
) -> Result<Option<String>> {
    if let Some(bounce) = state_bounce(ctx, data, review).await? {
        return Ok(Some(bounce));
    }
    if review.submitter_id as u64 == voter {
        return Ok(Some(OWN_SUBMISSION.into()));
    }
    if is_staff(data, voter).await? {
        return Ok(None);
    }
    let member = MemberRepository::new(data.db.pool())
        .get_by_discord_id(voter as i64)
        .await?;
    let rules = crate::utils::standing::rules(data).await?;
    if member
        .as_ref()
        .is_some_and(|m| database::standing::evaluate(m, &rules).can_vote)
    {
        return Ok(None);
    }
    Ok(Some(vote_access_hint(member.as_ref(), &rules).to_string()))
}

pub(super) async fn state_bounce(
    ctx: &Context,
    data: &Data,
    review: &Review,
) -> Result<Option<String>> {
    match review.status {
        ReviewStatus::Voided => return Ok(Some("This review was voided.".into())),
        status if status.is_terminal() => {
            return Ok(Some("This review is already resolved.".into()));
        }
        _ => {}
    }
    if paused(ctx, data, review.id).await? {
        return Ok(Some(
            "Voting is paused while the submitter revises this review.".into(),
        ));
    }
    Ok(None)
}

pub async fn paused(ctx: &Context, data: &Data, review_id: i64) -> Result<bool> {
    let repo = ReviewRepository::new(data.db.pool());
    if repo.edit_session(review_id).await?.is_none() {
        return Ok(false);
    }
    if repo.cancel_idle_edit(review_id, edit_idle_window()).await? {
        let (ctx, data) = (ctx.clone(), data.clone());
        tokio::spawn(async move { super::publish::rerender_post(&ctx, &data, review_id).await });
        return Ok(false);
    }
    Ok(true)
}

fn vote_access_hint(member: Option<&database::Member>, rules: &ReviewRules) -> &'static str {
    let strikes = member.map_or(0, database::standing::strike_count);
    if rules.suspends(strikes) {
        "Voting is suspended while you have active strikes."
    } else {
        "You can't vote on tag reviews yet. Earn voting permissions by submitting tag reviews \
         yourself. Voting will be automatically unlocked after a short while based on your \
         performance."
    }
}

async fn staff_prompt(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
    review: &Review,
    voter: u64,
    kind: VoteKind,
) -> Result<()> {
    let held = ReviewRepository::new(data.db.pool())
        .active_vote(review.id, voter as i64)
        .await?;
    let (resolve_label, resolve_style, now) = match kind {
        VoteKind::Reject => ("Reject now", ButtonStyle::Danger, "reject the tag now"),
        _ => ("Approve now", ButtonStyle::Success, "approve the tag now"),
    };
    let (question, vote_label, vote_disabled) = match held.map(|v| v.kind) {
        None => (
            format!("Add your vote to the count, or {now} and close this review?"),
            "Add my vote",
            false,
        ),
        Some(held) if held == kind => (
            format!(
                "You already voted to {} this review. You can still {now} and close it.",
                verb(kind)
            ),
            "Add my vote",
            true,
        ),
        Some(held) => {
            let lead = match held {
                VoteKind::Revise => "You asked for a revision of this review.".to_string(),
                _ => format!("You voted to {} this review.", verb(held)),
            };
            (
                format!(
                    "{lead} Switch your vote to {}, or {now} and close this review?",
                    verb(kind)
                ),
                "Switch my vote",
                false,
            )
        }
    };
    let mut parts = vec![text(question)];
    if review.hide_username {
        parts.push(text(format!(
            "-# Submitted anonymously by <@{}>.",
            review.submitter_id
        )));
    }
    parts.push(CreateContainerComponent::ActionRow(
        CreateActionRow::buttons(vec![
            CreateButton::new(custom_id_with("addvote", review.id, verb(kind)))
                .label(vote_label)
                .style(ButtonStyle::Secondary)
                .disabled(vote_disabled),
            CreateButton::new(custom_id_with("resolve", review.id, verb(kind)))
                .label(resolve_label)
                .style(resolve_style),
        ]),
    ));
    respond(ctx, component, parts).await
}

async fn record_from_prompt(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
    review: &Review,
    voter: u64,
    kind: VoteKind,
) -> Result<()> {
    let message = cast(ctx, data, review, voter, kind).await?;
    respond(ctx, component, vec![text(message)]).await
}

async fn resolve_now(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
    review: &Review,
    kind: VoteKind,
) -> Result<()> {
    let voter = component.user.id.get();
    if !is_staff(data, voter).await? {
        return respond(
            ctx,
            component,
            vec![text("Only staff can resolve a review.")],
        )
        .await;
    }
    let outcome = if kind == VoteKind::Reject {
        Outcome::Reject
    } else {
        Outcome::Accept
    };
    let resolved = resolve::resolve(ctx, data, review.id, outcome, Some(voter)).await?;
    let message = if resolved {
        let label = tag_label(data, review.id).await?;
        let player = player_name(ctx, data, review.id).await?;
        let action = if outcome == Outcome::Accept {
            "approved"
        } else {
            "rejected"
        };
        format!("You **{action}** the {label} tag on `{player}`.")
    } else {
        "This review is already resolved.".to_string()
    };
    respond(ctx, component, vec![text(message)]).await
}

async fn cast(
    ctx: &Context,
    data: &Data,
    review: &Review,
    voter: u64,
    kind: VoteKind,
) -> Result<String> {
    let repo = ReviewRepository::new(data.db.pool());
    let before = repo.tally(review.id).await?;
    let outcome = match repo.cast_vote(review.id, voter as i64, kind, None).await {
        Ok(outcome) => outcome,
        Err(ReviewError::WrongStatus(ReviewStatus::Voided)) => {
            return Ok("This review was voided.".into());
        }
        Err(ReviewError::WrongStatus(_)) => return Ok("This review is already resolved.".into()),
        Err(e) => return Err(anyhow::anyhow!("{e:?}")),
    };
    let label = tag_label(data, review.id).await?;
    let player = player_name(ctx, data, review.id).await?;
    let message = match outcome {
        CastOutcome::Unchanged => {
            return Ok(format!(
                "You have already voted to {} this player",
                verb(kind)
            ));
        }
        CastOutcome::Recorded => format!(
            "You voted to **{}** the {label} tag on `{player}`.",
            title(kind)
        ),
        CastOutcome::Switched(_) => {
            format!(
                "You changed your vote to **{}** the {label} tag on `{player}`.",
                title(kind)
            )
        }
    };
    let (ctx, data, review) = (ctx.clone(), data.clone(), review.clone());
    tokio::spawn(async move {
        if let Err(e) = after_vote(&ctx, &data, &review, before).await {
            tracing::error!("Failed to settle votes on review {}: {e:?}", review.id);
        }
    });
    Ok(message)
}

async fn after_vote(
    ctx: &Context,
    data: &Data,
    review: &Review,
    before: database::Tally,
) -> Result<()> {
    let repo = ReviewRepository::new(data.db.pool());
    let tally = repo.tally(review.id).await?;
    if !resolve::is_contested(before) && resolve::is_contested(tally) {
        ping_dispute(ctx, data, review).await;
    }
    let rules = crate::utils::standing::rules(data).await?;
    match resolve::community_outcome(tally, &rules) {
        Some(outcome) => {
            resolve::resolve(ctx, data, review.id, outcome, None).await?;
        }
        None => super::publish::rerender_post(ctx, data, review.id).await,
    }
    Ok(())
}

async fn ping_dispute(ctx: &Context, data: &Data, review: &Review) {
    let Some(thread_id) = review.thread_id else {
        return;
    };
    let (_, dispute_role, _) = database::ReviewGuideRepository::new(data.db.pool())
        .get_ping_roles()
        .await
        .unwrap_or((None, None, None));
    let Some(role_id) = dispute_role else {
        return;
    };
    let player = player_name(ctx, data, review.id).await.unwrap_or_default();
    let notice = CreateMessage::new().content(format!(
        "<@&{role_id}> Votes disagree on **{player}** and need a moderator to resolve them."
    ));
    if let Err(e) = ctx
        .http
        .send_message(
            GenericChannelId::new(thread_id as u64),
            Vec::<CreateAttachment>::new(),
            &notice,
        )
        .await
    {
        tracing::warn!(
            "Failed to ping the dispute role in review {}: {e}",
            review.id
        );
    }
}

async fn is_staff(data: &Data, user: u64) -> Result<bool> {
    let member = MemberRepository::new(data.db.pool())
        .get_by_discord_id(user as i64)
        .await?;
    Ok(data.is_owner(user) || AccessRank::of(data, user, member.as_ref()) >= AccessRank::Helper)
}

pub(super) async fn tag_label(data: &Data, review_id: i64) -> Result<String> {
    let revision = ReviewRepository::new(data.db.pool())
        .live_revision(review_id)
        .await?;
    Ok(revision
        .map(|r| channel::tag_label(&r.tag_type))
        .unwrap_or_default())
}

pub(super) async fn player_name(ctx: &Context, data: &Data, review_id: i64) -> Result<String> {
    Ok(load_live(ctx, data, review_id)
        .await?
        .map(|l| l.username)
        .unwrap_or_default())
}

fn parse_kind(extra: Option<&str>) -> VoteKind {
    match extra {
        Some("reject") => VoteKind::Reject,
        _ => VoteKind::Accept,
    }
}

fn verb(kind: VoteKind) -> &'static str {
    match kind {
        VoteKind::Accept => "accept",
        VoteKind::Reject => "reject",
        VoteKind::Revise => "revise",
    }
}

fn title(kind: VoteKind) -> &'static str {
    match kind {
        VoteKind::Accept => "Accept",
        VoteKind::Reject => "Reject",
        VoteKind::Revise => "Revise",
    }
}

fn container(parts: Vec<CreateContainerComponent<'static>>) -> Vec<CreateComponent<'static>> {
    vec![CreateComponent::Container(CreateContainer::new(parts))]
}

async fn respond(
    ctx: &Context,
    component: &ComponentInteraction,
    parts: Vec<CreateContainerComponent<'static>>,
) -> Result<()> {
    crate::interact::edit_component(ctx, component, container(parts)).await
}
