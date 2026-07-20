use anyhow::Result;
use database::ReviewGuideRepository;
use serenity::all::*;

use crate::framework::{AccessRank, Data};
use crate::interact::send_component_error;

async fn ping_roles(data: &Data) -> (Option<RoleId>, Option<RoleId>) {
    let (review, dispute) = ReviewGuideRepository::new(data.db.pool())
        .get_ping_roles()
        .await
        .unwrap_or((None, None));
    (
        review.map(|id| RoleId::new(id as u64)),
        dispute.map(|id| RoleId::new(id as u64)),
    )
}

pub async fn handle_ping_toggle(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let discord_id = component.user.id.get();
    let rank = super::super::tag::get_rank(data, discord_id).await?;

    if rank >= AccessRank::Helper {
        component
            .create_response(
                &ctx.http,
                CreateInteractionResponse::Message(
                    CreateInteractionResponseMessage::new()
                        .content(
                            "Would you like to be pinged for every tag review, or only \
                             disputed ones that need a moderator call?",
                        )
                        .components(vec![CreateComponent::ActionRow(CreateActionRow::Buttons(
                            vec![
                                CreateButton::new("guide_ping_choice:all")
                                    .label("All Reviews")
                                    .style(ButtonStyle::Secondary),
                                CreateButton::new("guide_ping_choice:disputes")
                                    .label("Disputes Only")
                                    .style(ButtonStyle::Secondary),
                            ]
                            .into(),
                        ))])
                        .ephemeral(true),
                ),
            )
            .await?;
        return Ok(());
    }

    let (review_role, _) = ping_roles(data).await;
    let (Some(guild_id), Some(role_id)) = (data.home_guild_id, review_role) else {
        return send_component_error(
            ctx,
            component,
            "Error",
            "Review ping role is not configured",
        )
        .await;
    };
    let reply = toggle_ping_role(
        ctx,
        component,
        guild_id,
        role_id,
        "Opted in to review pings",
        "Opted out of review pings",
        "You'll be pinged when a new tag review is submitted.",
        "You won't be pinged for new tag reviews anymore.",
    )
    .await;

    component
        .create_response(
            &ctx.http,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .content(reply)
                    .ephemeral(true),
            ),
        )
        .await?;
    Ok(())
}

pub async fn handle_ping_choice(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let choice = component
        .data
        .custom_id
        .strip_prefix("guide_ping_choice:")
        .unwrap_or("");

    let (review_role, dispute_role) = ping_roles(data).await;
    let (role, on_reason, off_reason, on_reply, off_reply) = if choice == "disputes" {
        (
            dispute_role,
            "Opted in to dispute pings",
            "Opted out of dispute pings",
            "You'll be pinged when a review's votes disagree and need a moderator call.",
            "You won't be pinged for disputed reviews anymore.",
        )
    } else {
        (
            review_role,
            "Opted in to review pings",
            "Opted out of review pings",
            "You'll be pinged when a new tag review is submitted.",
            "You won't be pinged for new tag reviews anymore.",
        )
    };

    let (Some(guild_id), Some(role_id)) = (data.home_guild_id, role) else {
        return send_component_error(ctx, component, "Error", "That ping role is not configured")
            .await;
    };
    let reply = toggle_ping_role(
        ctx, component, guild_id, role_id, on_reason, off_reason, on_reply, off_reply,
    )
    .await;

    component
        .create_response(
            &ctx.http,
            CreateInteractionResponse::UpdateMessage(
                CreateInteractionResponseMessage::new()
                    .content(reply)
                    .components(vec![]),
            ),
        )
        .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn toggle_ping_role(
    ctx: &Context,
    component: &ComponentInteraction,
    guild_id: GuildId,
    role_id: RoleId,
    on_reason: &str,
    off_reason: &str,
    on_reply: &'static str,
    off_reply: &'static str,
) -> &'static str {
    let has_role = component
        .member
        .as_ref()
        .is_some_and(|m| m.roles.contains(&role_id));
    let user_id = component.user.id;

    let result = if has_role {
        ctx.http
            .remove_member_role(guild_id, user_id, role_id, Some(off_reason))
            .await
    } else {
        ctx.http
            .add_member_role(guild_id, user_id, role_id, Some(on_reason))
            .await
    };

    match result {
        Ok(()) if has_role => off_reply,
        Ok(()) => on_reply,
        Err(e) => {
            tracing::warn!("Failed to toggle role for {user_id}: {e}");
            "Failed to update your roles. Please try again."
        }
    }
}
