use anyhow::{Result, anyhow};
use serenity::all::*;

use database::MemberRepository;

use crate::framework::{AccessRank, AccessRankExt, Data};

pub fn register() -> CreateCommand<'static> {
    CreateCommand::new("strike")
        .description("Manage user strikes")
        .add_option(
            CreateCommandOption::new(CommandOptionType::SubCommand, "add", "Add a strike")
                .add_sub_option(
                    CreateCommandOption::new(CommandOptionType::User, "user", "Target user")
                        .required(true),
                )
                .add_sub_option(
                    CreateCommandOption::new(CommandOptionType::String, "reason", "Strike reason")
                        .required(true),
                ),
        )
}

pub async fn run(ctx: &Context, command: &CommandInteraction, data: &Data) -> Result<()> {
    let invoker_id = command.user.id.get();
    let repo = MemberRepository::new(data.db.pool());
    let invoker = repo.get_by_discord_id(invoker_id as i64).await?;
    let rank = AccessRank::of(data, invoker_id, invoker.as_ref());

    if rank < AccessRank::Moderator {
        return send_reply(
            ctx,
            command,
            "You don't have permission to use this command",
            true,
        )
        .await;
    }

    match command.data.options.first().map(|o| o.name.as_str()) {
        Some("add") => handle_add(ctx, command, data, &repo).await,
        _ => Ok(()),
    }
}

async fn handle_add(
    ctx: &Context,
    command: &CommandInteraction,
    data: &Data,
    repo: &MemberRepository<'_>,
) -> Result<()> {
    let sub_options = command
        .data
        .options
        .first()
        .and_then(|o| match &o.value {
            CommandDataOptionValue::SubCommand(opts) => Some(opts),
            _ => None,
        })
        .ok_or_else(|| anyhow!("Missing subcommand options"))?;

    let target_id = sub_options
        .iter()
        .find(|o| o.name == "user")
        .and_then(|o| match &o.value {
            CommandDataOptionValue::User(id) => Some(id.get()),
            _ => None,
        })
        .ok_or_else(|| anyhow!("Missing user"))?;

    let reason = sub_options
        .iter()
        .find(|o| o.name == "reason")
        .and_then(|o| o.value.as_str())
        .unwrap_or("No reason provided");

    let invoker_id = command.user.id.get();
    let target = repo.get_by_discord_id(target_id as i64).await?;

    if target.is_none() {
        return send_reply(
            ctx,
            command,
            &format!("<@{target_id}> is not registered"),
            true,
        )
        .await;
    }

    let target_rank = AccessRank::of(data, target_id, target.as_ref());
    let invoker = repo.get_by_discord_id(invoker_id as i64).await?;
    let invoker_rank = AccessRank::of(data, invoker_id, invoker.as_ref());

    if invoker_rank <= target_rank {
        return send_reply(
            ctx,
            command,
            "Cannot strike a user with equal or higher rank",
            true,
        )
        .await;
    }

    repo.add_strike(target_id as i64, reason, invoker_id)
        .await?;
    crate::utils::standing::refresh_and_sync(ctx, data, target_id).await;
    send_reply(
        ctx,
        command,
        &format!("Strike added for <@{target_id}>: \"{reason}\""),
        true,
    )
    .await
}

pub async fn handle_strike_author_button(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let target_id: u64 = component
        .data
        .custom_id
        .strip_prefix("strike_tag_author:")
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| anyhow!("Invalid button ID"))?;
    let invoker_id = component.user.id.get();

    if !can_strike(data, invoker_id, target_id).await? {
        return send_component_error(ctx, component, "You cannot strike this user").await;
    }

    let reason_input = CreateInputText::new(InputTextStyle::Short, "reason")
        .placeholder("Why is this user being struck?")
        .min_length(1)
        .max_length(100);
    let modal = CreateModal::new(format!("strike_author_modal:{target_id}"), "Strike User")
        .components(vec![CreateModalComponent::Label(CreateLabel::input_text(
            "Strike Reason",
            reason_input,
        ))]);
    component
        .create_response(&ctx.http, CreateInteractionResponse::Modal(modal))
        .await?;
    Ok(())
}

pub async fn handle_strike_author_modal(
    ctx: &Context,
    modal: &ModalInteraction,
    data: &Data,
) -> Result<()> {
    let target_id: u64 = modal
        .data
        .custom_id
        .strip_prefix("strike_author_modal:")
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| anyhow!("Invalid modal ID"))?;
    let invoker_id = modal.user.id.get();

    if !can_strike(data, invoker_id, target_id).await? {
        let msg = CreateInteractionResponseMessage::new()
            .content("You cannot strike this user")
            .ephemeral(true);
        modal
            .create_response(&ctx.http, CreateInteractionResponse::Message(msg))
            .await?;
        return Ok(());
    }

    let reason = match crate::interact::extract_modal_value(&modal.data.components, "reason") {
        r if r.is_empty() => "No reason provided".to_string(),
        r => r,
    };

    MemberRepository::new(data.db.pool())
        .add_strike(target_id as i64, &reason, invoker_id)
        .await?;
    crate::utils::standing::refresh_and_sync(ctx, data, target_id).await;

    let msg = CreateInteractionResponseMessage::new()
        .content(format!("Strike added for <@{target_id}>: \"{reason}\""))
        .ephemeral(true);
    modal
        .create_response(&ctx.http, CreateInteractionResponse::Message(msg))
        .await?;
    Ok(())
}

async fn can_strike(data: &Data, invoker_id: u64, target_id: u64) -> Result<bool> {
    let repo = MemberRepository::new(data.db.pool());
    let target = repo.get_by_discord_id(target_id as i64).await?;
    if target.is_none() {
        return Ok(false);
    }
    let invoker = repo.get_by_discord_id(invoker_id as i64).await?;
    let invoker_rank = AccessRank::of(data, invoker_id, invoker.as_ref());
    let target_rank = AccessRank::of(data, target_id, target.as_ref());
    Ok(invoker_rank >= AccessRank::Moderator && invoker_rank > target_rank)
}

async fn send_component_error(
    ctx: &Context,
    component: &ComponentInteraction,
    message: &str,
) -> Result<()> {
    let msg = CreateInteractionResponseMessage::new()
        .content(message)
        .ephemeral(true);
    component
        .create_response(&ctx.http, CreateInteractionResponse::Message(msg))
        .await?;
    Ok(())
}

async fn send_reply(
    ctx: &Context,
    command: &CommandInteraction,
    message: &str,
    ephemeral: bool,
) -> Result<()> {
    let mut msg = CreateInteractionResponseMessage::new().content(message);
    if ephemeral {
        msg = msg.ephemeral(true);
    }
    command
        .create_response(&ctx.http, CreateInteractionResponse::Message(msg))
        .await?;
    Ok(())
}
