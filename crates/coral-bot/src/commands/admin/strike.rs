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
    send_reply(
        ctx,
        command,
        &format!("Strike added for <@{target_id}>: \"{reason}\""),
        false,
    )
    .await
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
