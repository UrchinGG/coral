use anyhow::Result;
use serenity::all::*;

use crate::framework::{AccessRank, Data};
use crate::interact::{send_component_error, send_deferred_error};
use crate::utils::{separator, text};

const GUIDE_TITLE: &str = "Tag Review Guide";

pub async fn run_guide(ctx: &Context, command: &CommandInteraction, data: &Data) -> Result<()> {
    command.defer_ephemeral(&ctx.http).await?;

    let discord_id = command.user.id.get();
    let rank = super::super::tag::get_rank(data, discord_id).await?;
    if rank < AccessRank::Moderator {
        return send_deferred_error(ctx, command, "Error", "Only moderators can do this").await;
    }

    let Some(forum_id) = data.review_forum_id else {
        return send_deferred_error(ctx, command, "Error", "Review forum channel not configured")
            .await;
    };

    let message = CreateMessage::new()
        .flags(MessageFlags::IS_COMPONENTS_V2)
        .components(build_guide_message());
    let forum_post = CreateForumPost::new(GUIDE_TITLE, message);
    let thread = forum_id.create_forum_post(&ctx.http, forum_post).await?;
    thread
        .id
        .edit(
            &ctx.http,
            EditThread::new().locked(true).flags(ChannelFlags::PINNED),
        )
        .await?;

    command
        .edit_response(
            &ctx.http,
            EditInteractionResponse::new()
                .flags(MessageFlags::IS_COMPONENTS_V2)
                .components(vec![CreateComponent::Container(CreateContainer::new(
                    vec![text("## Guide posted and pinned")],
                ))]),
        )
        .await?;
    Ok(())
}

fn build_guide_message() -> Vec<CreateComponent<'static>> {
    let parts: Vec<CreateContainerComponent> = vec![
        text(format!("## {GUIDE_TITLE}")),
        separator(),
        text("### Tags Definitions"),
        text(
            "<:sniper:1459106167270932618> **Sniper**\n\
             -# Used for cheating snipers. Check the tooltip date; if it's old, they may no \
             longer be active.\n\
             <:blatantcheater:1459106183196577812> **Blatant Cheater**\n\
             -# Obvious cheats that would be impossible on a vanilla client, like scaffold, \
             speedmine, or autoblock.\n\
             <:closetcheater:1459106337039323136> **Closet Cheater**\n\
             -# Cheats that can be more subtle, like legit scaffold, aimassist, or lagrange.\n\
             <:confirmedcheater:1459106129765204049> **Confirmed Cheater**\n\
             -# Applied to players that have been confirmed to be cheating by staff. Typically, \
             video evidence is available for these players on request.\n\
             <:replaysneeded:1482502914835615745> **Replays Needed**\n\
             -# Used whenever staff require replays of a player for any reason. Remember to \
             submit replays to staff, it helps us prove players legit and clear their tags.\n\
             <:caution:1459106358098923583> **Caution**\n\
             -# Special tag used for things that don't fit into any of the above categories. \
             Only staff can apply this.",
        ),
        separator(),
        text(
            "### Submitting\n\
             If you don't have direct-tag access yet, a **Blatant Cheater** or **Closet Cheater** \
             tag you submit needs community approval before it's applied.\n\
             1. Run `/tag add`, then press **Create Post** on the preview\n\
             2. Attach proof with the **+ Replay** and **+ Media** buttons in your new post\n\
             3. Press **Submit** once your evidence is ready for review",
        ),
        separator(),
        text(
            "### Voting\n\
             Anyone with voting access, Reviewers and staff, can vote **Accept** or **Reject** \
             on a tag's validity.\n\
             1. If votes stay unanimous, the tag resolves automatically once enough come in\n\
             2. If votes disagree, review will not resolve automatically and a moderator steps \
             in to make the final call\n\
             -# Explain your reasoning when you reject a tag, it helps the submitter understand \
             what to fix.",
        ),
        separator(),
        text(
            "### Standing\n\
             **Default**\n\
             -# **Blatant Cheater** and **Closet Cheater** tags you submit go through review. \
             Sniper tags still apply directly, no review needed. A number of approved \
             submissions with no rejections unlock voting.\n\
             **Reviewer**\n\
             -# You can vote on reviews. Accurate verdicts progress you toward Trusted.\n\
             **Trusted**\n\
             -# You can tag players directly, skipping review.\n\
             Run `/dashboard` to track your standing and progress.",
        ),
        separator(),
        text(
            "Press the button below to toggle tag review pings if you'd like to be alerted \
             when new ones are submitted.",
        ),
        CreateContainerComponent::ActionRow(CreateActionRow::Buttons(
            vec![
                CreateButton::new("guide_ping_toggle")
                    .label("Ping Me For Reviews")
                    .style(ButtonStyle::Secondary),
            ]
            .into(),
        )),
    ];
    vec![CreateComponent::Container(CreateContainer::new(parts))]
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

    let (Some(guild_id), Some(role_id)) = (data.home_guild_id, data.review_ping_role_id) else {
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

    let (role, on_reason, off_reason, on_reply, off_reply) = if choice == "disputes" {
        (
            data.dispute_ping_role_id,
            "Opted in to dispute pings",
            "Opted out of dispute pings",
            "You'll be pinged when a review's votes disagree and need a moderator call.",
            "You won't be pinged for disputed reviews anymore.",
        )
    } else {
        (
            data.review_ping_role_id,
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
