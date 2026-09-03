//! Correction panel for a member's settled tag reviews.
//!
//! Reviews settle straight into the member's counters (see `settle_verdict`),
//! so a review resolved wrongly permanently skews the standing thresholds.
//! This panel lets an admin restate an outcome — or drop it from the member's
//! score entirely — with a mandatory reason that lands in the staff logs.

use anyhow::{Result, anyhow};
use serenity::all::*;

use database::{MemberRepository, ReviewCounterDelta};

use crate::{
    commands::blacklist::channel,
    framework::{AccessRank, Data},
    interact,
    utils::{separator, text},
};

use super::manage::{build_main_view, fetch_context};

/// The corrections an admin can apply, as (id, label, description).
const CORRECTIONS: &[(&str, &str, &str)] = &[
    (
        "reject_to_accept",
        "Rejected review -> accepted",
        "The review was denied but should have been accepted",
    ),
    (
        "accept_to_reject",
        "Accepted review -> rejected",
        "The review was accepted but should have been denied",
    ),
    (
        "void_rejected",
        "Drop a rejected review",
        "Remove a rejection from the score entirely",
    ),
    (
        "void_accepted",
        "Drop an accepted review",
        "Remove an acceptance from the score entirely",
    ),
    (
        "inaccurate_to_accurate",
        "Inaccurate verdict -> accurate",
        "A vote was scored wrong and was actually correct",
    ),
    (
        "accurate_to_inaccurate",
        "Accurate verdict -> inaccurate",
        "A vote was credited but was actually wrong",
    ),
    (
        "void_inaccurate",
        "Drop an inaccurate verdict",
        "Remove an inaccurate verdict from the score entirely",
    ),
    (
        "void_accurate",
        "Drop an accurate verdict",
        "Remove an accurate verdict from the score entirely",
    ),
    (
        "grant_bonus",
        "Grant a bonus verdict",
        "Credit a bonus verdict for a disputed review",
    ),
    (
        "void_bonus",
        "Drop a bonus verdict",
        "Remove a bonus verdict from the score entirely",
    ),
];

fn correction(id: &str) -> Option<&'static (&'static str, &'static str, &'static str)> {
    CORRECTIONS.iter().find(|(key, _, _)| *key == id)
}

/// The counter movement a correction applies for `count` reviews.
fn delta_for(action: &str, count: i64) -> Option<ReviewCounterDelta> {
    let delta = match action {
        "reject_to_accept" => ReviewCounterDelta {
            accepted_tags: count,
            rejected_tags: -count,
            ..Default::default()
        },
        "accept_to_reject" => ReviewCounterDelta {
            accepted_tags: -count,
            rejected_tags: count,
            ..Default::default()
        },
        "void_rejected" => ReviewCounterDelta {
            rejected_tags: -count,
            ..Default::default()
        },
        "void_accepted" => ReviewCounterDelta {
            accepted_tags: -count,
            ..Default::default()
        },
        "inaccurate_to_accurate" => ReviewCounterDelta {
            accurate_verdicts: count,
            incorrect_verdicts: -count,
            ..Default::default()
        },
        "accurate_to_inaccurate" => ReviewCounterDelta {
            accurate_verdicts: -count,
            incorrect_verdicts: count,
            ..Default::default()
        },
        "void_inaccurate" => ReviewCounterDelta {
            incorrect_verdicts: -count,
            ..Default::default()
        },
        "void_accurate" => ReviewCounterDelta {
            accurate_verdicts: -count,
            ..Default::default()
        },
        "grant_bonus" => ReviewCounterDelta {
            bonus_verdicts: count,
            ..Default::default()
        },
        "void_bonus" => ReviewCounterDelta {
            bonus_verdicts: -count,
            ..Default::default()
        },
        _ => return None,
    };
    Some(delta)
}

/// Rejects a correction that would drive a counter below zero, so a mistyped
/// count clamps loudly instead of silently leaving the score wrong.
fn check_available(
    m: &database::Member,
    delta: ReviewCounterDelta,
) -> std::result::Result<(), String> {
    let checks = [
        (delta.accepted_tags, m.accepted_tags, "accepted tag reviews"),
        (delta.rejected_tags, m.rejected_tags, "rejected tag reviews"),
        (
            delta.accurate_verdicts,
            m.accurate_verdicts,
            "accurate verdicts",
        ),
        (
            delta.incorrect_verdicts,
            m.incorrect_verdicts,
            "inaccurate verdicts",
        ),
        (delta.bonus_verdicts, m.bonus_verdicts, "bonus verdicts"),
    ];
    for (change, current, label) in checks {
        if change < 0 && -change > current {
            return Err(format!(
                "User only has {current} {label}, cannot remove {}.",
                -change
            ));
        }
    }
    Ok(())
}

pub(crate) fn can_correct(invoker_rank: AccessRank, target_rank: AccessRank) -> bool {
    invoker_rank >= AccessRank::Owner
        || (invoker_rank >= AccessRank::Admin && invoker_rank > target_rank)
}

pub async fn build_view(data: &Data, target_id: u64) -> Vec<CreateComponent<'static>> {
    let target = MemberRepository::new(data.db.pool())
        .get_by_discord_id(target_id as i64)
        .await
        .ok()
        .flatten();

    let mut parts: Vec<CreateContainerComponent> =
        vec![text(format!("## Tag Review Scores — <@{target_id}>"))];

    let Some(m) = target else {
        parts.push(separator());
        parts.push(text("User is not registered."));
        parts.push(back_row(target_id));
        return vec![CreateComponent::Container(CreateContainer::new(parts))];
    };

    let standing = database::standing::explain(&m);
    parts.push(separator());
    parts.push(text(format!(
        "**{}** accepted tag reviews · **{}** rejected\n\
         **{}** accurate verdicts · **{}** inaccurate · **{}** bonus",
        m.accepted_tags,
        m.rejected_tags,
        m.accurate_verdicts,
        m.incorrect_verdicts,
        m.bonus_verdicts,
    )));
    parts.push(text(format!(
        "-# Vote: **{}** — {}\n-# Tag: **{}** — {}",
        standing.can_vote, standing.vote_reason, standing.can_tag, standing.tag_reason
    )));
    parts.push(separator());
    parts.push(text(
        "Pick a correction. Each asks for a count and a reason, re-evaluates \
         standing afterwards, and is written to the staff logs.",
    ));

    let options: Vec<CreateSelectMenuOption<'static>> = CORRECTIONS
        .iter()
        .map(|(id, label, desc)| CreateSelectMenuOption::new(*label, *id).description(*desc))
        .collect();

    parts.push(CreateContainerComponent::ActionRow(
        CreateActionRow::SelectMenu(
            CreateSelectMenu::new(
                format!("manage_reviews_action:{target_id}"),
                CreateSelectMenuKind::String {
                    options: options.into(),
                },
            )
            .placeholder("Correct a review outcome"),
        ),
    ));
    parts.push(back_row(target_id));

    vec![CreateComponent::Container(CreateContainer::new(parts))]
}

fn back_row(target_id: u64) -> CreateContainerComponent<'static> {
    CreateContainerComponent::ActionRow(CreateActionRow::buttons(vec![
        CreateButton::new(format!("manage_reviews_back:{target_id}"))
            .label("Back")
            .style(ButtonStyle::Secondary),
    ]))
}

pub async fn handle_open(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let target_id = interact::parse_id(&component.data.custom_id)
        .ok_or_else(|| anyhow!("Invalid button ID"))?;
    let invoker_id = component.user.id.get();
    let (invoker_rank, _, target_rank) = fetch_context(data, invoker_id, target_id).await?;

    if !can_correct(invoker_rank, target_rank) {
        return interact::send_component_error(ctx, component, "Error", "Insufficient permissions")
            .await;
    }

    interact::update_message(ctx, component, build_view(data, target_id).await).await
}

pub async fn handle_back(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let target_id = interact::parse_id(&component.data.custom_id)
        .ok_or_else(|| anyhow!("Invalid button ID"))?;
    let invoker_id = component.user.id.get();
    let (invoker_rank, _, _) = fetch_context(data, invoker_id, target_id).await?;
    interact::update_message(
        ctx,
        component,
        build_main_view(data, invoker_rank, target_id).await,
    )
    .await
}

pub async fn handle_action_select(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let action = match &component.data.kind {
        ComponentInteractionDataKind::StringSelect { values } => values.first().cloned(),
        _ => None,
    };
    let Some(action) = action else { return Ok(()) };

    let target_id = interact::parse_id(&component.data.custom_id)
        .ok_or_else(|| anyhow!("Invalid select ID"))?;
    let invoker_id = component.user.id.get();
    let (invoker_rank, _, target_rank) = fetch_context(data, invoker_id, target_id).await?;

    if !can_correct(invoker_rank, target_rank) {
        return interact::send_component_error(ctx, component, "Error", "Insufficient permissions")
            .await;
    }
    let Some((_, label, _)) = correction(&action) else {
        return interact::send_component_error(ctx, component, "Error", "Unknown correction").await;
    };

    let count = CreateInputText::new(InputTextStyle::Short, "count")
        .value("1")
        .min_length(1)
        .max_length(4);
    let reason = CreateInputText::new(InputTextStyle::Paragraph, "reason")
        .placeholder("Why is this outcome being corrected?")
        .min_length(4)
        .max_length(400);
    let modal = CreateModal::new(
        format!("manage_reviews_modal:{target_id}:{action}"),
        modal_title(label),
    )
    .components(vec![
        CreateModalComponent::Label(CreateLabel::input_text("How many reviews", count)),
        CreateModalComponent::Label(CreateLabel::input_text("Reason", reason)),
    ]);
    component
        .create_response(&ctx.http, CreateInteractionResponse::Modal(modal))
        .await?;
    Ok(())
}

/// Discord caps modal titles at 45 characters.
fn modal_title(label: &str) -> String {
    if label.chars().count() <= 45 {
        return label.to_string();
    }
    label.chars().take(44).collect()
}

pub async fn handle_modal(ctx: &Context, modal: &ModalInteraction, data: &Data) -> Result<()> {
    let (target_id, action) =
        interact::parse_ids(&modal.data.custom_id).ok_or_else(|| anyhow!("Invalid modal ID"))?;
    let invoker_id = modal.user.id.get();
    let (invoker_rank, target, target_rank) = fetch_context(data, invoker_id, target_id).await?;

    if !can_correct(invoker_rank, target_rank) {
        return interact::send_modal_error(ctx, modal, "Error", "Insufficient permissions").await;
    }
    let Some(member) = target else {
        return interact::send_modal_error(ctx, modal, "Error", "User is not registered").await;
    };
    let Some((_, label, _)) = correction(&action) else {
        return interact::send_modal_error(ctx, modal, "Error", "Unknown correction").await;
    };

    let count_raw = interact::extract_modal_value(&modal.data.components, "count");
    let reason = interact::extract_modal_value(&modal.data.components, "reason")
        .trim()
        .to_string();

    let Ok(count) = count_raw.trim().parse::<i64>() else {
        return interact::send_modal_error(ctx, modal, "Error", "Count must be a number").await;
    };
    if !(1..=1000).contains(&count) {
        return interact::send_modal_error(ctx, modal, "Error", "Count must be between 1 and 1000")
            .await;
    }
    if reason.is_empty() {
        return interact::send_modal_error(ctx, modal, "Error", "A reason is required").await;
    }

    let delta = delta_for(&action, count).ok_or_else(|| anyhow!("Unknown correction"))?;
    if let Err(msg) = check_available(&member, delta) {
        return interact::send_modal_error(ctx, modal, "Error", &msg).await;
    }

    let repo = MemberRepository::new(data.db.pool());
    let Some(updated) = repo
        .adjust_review_counters(member.discord_id, delta)
        .await?
    else {
        return interact::send_modal_error(ctx, modal, "Error", "User is not registered").await;
    };

    if let Err(e) = database::AdminActionRepository::new(data.db.pool())
        .log(
            invoker_id as i64,
            "review_score_correction",
            &target_id.to_string(),
            &serde_json::json!({
                "correction": action,
                "label": label,
                "count": count,
                "reason": reason,
                "before": counter_snapshot(&member),
                "after": counter_snapshot(&updated),
            }),
        )
        .await
    {
        tracing::error!("Failed to log review score correction for {target_id}: {e}");
    }

    channel::post_review_score_correction(
        ctx, data, target_id, invoker_id, label, count, &reason, &member, &updated,
    )
    .await;

    crate::utils::standing::refresh_and_sync(ctx, data, target_id).await;

    interact::update_modal(ctx, modal, build_view(data, target_id).await).await
}

fn counter_snapshot(m: &database::Member) -> serde_json::Value {
    serde_json::json!({
        "accepted_tags": m.accepted_tags,
        "rejected_tags": m.rejected_tags,
        "accurate_verdicts": m.accurate_verdicts,
        "incorrect_verdicts": m.incorrect_verdicts,
        "bonus_verdicts": m.bonus_verdicts,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_correction_has_a_delta() {
        for (id, _, _) in CORRECTIONS {
            assert!(delta_for(id, 1).is_some(), "{id} has no delta");
        }
    }

    #[test]
    fn flip_moves_one_counter_into_the_other() {
        let delta = delta_for("reject_to_accept", 2).unwrap();
        assert_eq!(delta.accepted_tags, 2);
        assert_eq!(delta.rejected_tags, -2);
    }

    #[test]
    fn admin_cannot_correct_an_equal_rank() {
        assert!(!can_correct(AccessRank::Admin, AccessRank::Admin));
        assert!(can_correct(AccessRank::Admin, AccessRank::Moderator));
        assert!(!can_correct(AccessRank::Moderator, AccessRank::Default));
        assert!(can_correct(AccessRank::Owner, AccessRank::Owner));
    }
}
