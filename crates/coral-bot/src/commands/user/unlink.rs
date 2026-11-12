use anyhow::Result;
use serenity::all::*;

use crate::framework::Data;
use crate::utils::{separator, text};

pub fn register() -> CreateCommand<'static> {
    CreateCommand::new("unlink").description("Unlink your Minecraft account")
}

pub async fn run(ctx: &Context, command: &CommandInteraction, data: &Data) -> Result<()> {
    let discord_id = command.user.id.get();
    let Some(primary) = data.api.links(discord_id).await?.primary else {
        let container = CreateComponent::Container(CreateContainer::new(vec![text(
            "You don't have a linked Minecraft account.",
        )]));
        command
            .create_response(
                &ctx.http,
                CreateInteractionResponse::Message(
                    CreateInteractionResponseMessage::new()
                        .flags(MessageFlags::IS_COMPONENTS_V2 | MessageFlags::EPHEMERAL)
                        .components(vec![container]),
                ),
            )
            .await?;
        return Ok(());
    };

    data.api.unlink_account(discord_id, &primary.uuid).await?;

    if let Some(guild_id) = command.guild_id {
        let _ = guild_id
            .edit_member(&ctx.http, command.user.id, EditMember::new().nickname(""))
            .await;
    }

    let container = CreateComponent::Container(CreateContainer::new(vec![
        text("## Unlinked"),
        separator(),
        text("Your Minecraft account has been unlinked and your nickname has been reset."),
    ]));
    command
        .create_response(
            &ctx.http,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .flags(MessageFlags::IS_COMPONENTS_V2 | MessageFlags::EPHEMERAL)
                    .components(vec![container]),
            ),
        )
        .await?;

    Ok(())
}
