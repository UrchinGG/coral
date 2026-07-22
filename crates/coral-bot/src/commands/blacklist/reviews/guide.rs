use anyhow::Result;
use database::ReviewGuideRepository;
use serenity::all::*;

use crate::framework::{AccessRank, Data};
use crate::interact::send_component_error;

/// How often the guide thread is re-checked. Discord greys out message
/// components for everyone who cannot post in the channel, so the guide thread
/// has to stay unarchived and unlocked for the opt-in button to work; stray
/// messages are removed by the review guard instead.
const GUIDE_REFRESH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(15 * 60);

pub fn spawn_guide_watcher(ctx: Context, data: Data) {
    tokio::spawn(async move {
        loop {
            refresh_guide_thread(&ctx, &data).await;
            tokio::time::sleep(GUIDE_REFRESH_INTERVAL).await;
        }
    });
}

async fn refresh_guide_thread(ctx: &Context, data: &Data) {
    let config = match ReviewGuideRepository::new(data.db.pool()).get().await {
        Ok(config) => config,
        Err(e) => {
            tracing::warn!("could not load review guide config: {e}");
            return;
        }
    };
    let Some(thread_id) = config.and_then(|c| c.posted_thread_id) else {
        return;
    };
    *data.guide_thread_id.write().unwrap() = Some(thread_id as u64);

    let thread = ThreadId::new(thread_id as u64);
    let Ok(channel) = ctx.http.get_channel(thread.into()).await else {
        return;
    };
    let Some(guild_thread) = channel.thread() else {
        return;
    };
    if !guild_thread.thread_metadata.archived() && !guild_thread.thread_metadata.locked() {
        return;
    }
    if let Err(e) = thread
        .edit(&ctx.http, EditThread::new().archived(false).locked(false))
        .await
    {
        tracing::warn!("could not reopen review guide thread {thread_id}: {e}");
    }
}

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
    let is_staff = rank >= AccessRank::Helper;
    let (review_role, dispute_role) = ping_roles(data).await;

    if review_role.is_none() && (dispute_role.is_none() || !is_staff) {
        return send_component_error(
            ctx,
            component,
            "Error",
            "Review ping role is not configured",
        )
        .await;
    }

    let (Some(guild_id), user_id) = (data.home_guild_id, component.user.id) else {
        return send_component_error(ctx, component, "Error", "Home guild is not configured").await;
    };

    let message =
        toggle_panel_message(ctx, guild_id, user_id, review_role, dispute_role, is_staff).await;
    component
        .create_response(&ctx.http, CreateInteractionResponse::Message(message))
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

    let discord_id = component.user.id.get();
    let rank = super::super::tag::get_rank(data, discord_id).await?;
    let is_staff = rank >= AccessRank::Helper;
    let (review_role, dispute_role) = ping_roles(data).await;

    let Some(guild_id) = data.home_guild_id else {
        return send_component_error(ctx, component, "Error", "Home guild is not configured").await;
    };
    let target_role = if choice == "disputes" {
        dispute_role
    } else {
        review_role
    };
    let Some(role_id) = target_role else {
        return send_component_error(ctx, component, "Error", "That ping role is not configured")
            .await;
    };

    if let Err(e) = toggle_role(ctx, component, guild_id, role_id).await {
        tracing::warn!("Failed to toggle role for {}: {e}", component.user.id);
        return send_component_error(
            ctx,
            component,
            "Error",
            "Failed to update your roles. Please try again.",
        )
        .await;
    }

    let user_id = component.user.id;
    let message =
        toggle_panel_message(ctx, guild_id, user_id, review_role, dispute_role, is_staff).await;
    component
        .create_response(&ctx.http, CreateInteractionResponse::UpdateMessage(message))
        .await?;
    Ok(())
}

async fn toggle_role(
    ctx: &Context,
    component: &ComponentInteraction,
    guild_id: GuildId,
    role_id: RoleId,
) -> Result<(), serenity::Error> {
    let had_role = component
        .member
        .as_ref()
        .is_some_and(|m| m.roles.contains(&role_id));
    let user_id = component.user.id;

    if had_role {
        ctx.http
            .remove_member_role(
                guild_id,
                user_id,
                role_id,
                Some("Opted out of review pings"),
            )
            .await
    } else {
        ctx.http
            .add_member_role(guild_id, user_id, role_id, Some("Opted in to review pings"))
            .await
    }
}

async fn toggle_panel_message(
    ctx: &Context,
    guild_id: GuildId,
    user_id: UserId,
    review_role: Option<RoleId>,
    dispute_role: Option<RoleId>,
    is_staff: bool,
) -> CreateInteractionResponseMessage<'static> {
    let current_roles = ctx
        .http
        .get_member(guild_id, user_id)
        .await
        .map(|m| m.roles.into_iter().collect::<Vec<_>>())
        .unwrap_or_default();
    let has = |role: RoleId| current_roles.contains(&role);

    let mut lines = Vec::new();
    let mut buttons = Vec::new();

    if let Some(role) = review_role {
        let on = has(role);
        lines.push(format!(
            "All reviews: **{}**",
            if on { "on" } else { "off" }
        ));
        buttons.push(
            CreateButton::new("guide_ping_choice:all")
                .label(if on {
                    "Turn Off All Reviews"
                } else {
                    "Turn On All Reviews"
                })
                .style(if on {
                    ButtonStyle::Success
                } else {
                    ButtonStyle::Secondary
                }),
        );
    }

    if is_staff && let Some(role) = dispute_role {
        let on = has(role);
        lines.push(format!(
            "Disputes only: **{}**",
            if on { "on" } else { "off" }
        ));
        buttons.push(
            CreateButton::new("guide_ping_choice:disputes")
                .label(if on {
                    "Turn Off Disputes Only"
                } else {
                    "Turn On Disputes Only"
                })
                .style(if on {
                    ButtonStyle::Success
                } else {
                    ButtonStyle::Secondary
                }),
        );
    }

    CreateInteractionResponseMessage::new()
        .content(lines.join("\n"))
        .components(vec![CreateComponent::ActionRow(CreateActionRow::Buttons(
            buttons.into(),
        ))])
        .ephemeral(true)
}
