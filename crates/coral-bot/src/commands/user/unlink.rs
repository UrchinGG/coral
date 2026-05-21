use anyhow::Result;
use serenity::all::*;

use database::MemberRepository;

use crate::framework::Data;
use crate::utils::{separator, text};


pub fn register() -> CreateCommand<'static> {
    CreateCommand::new("unlink").description("Unlink your Minecraft account")
}


pub async fn run(ctx: &Context, command: &CommandInteraction, data: &Data) -> Result<()> {
    let discord_id = command.user.id.get();
    let repo = MemberRepository::new(data.db.pool());

    let member = repo.get_by_discord_id(discord_id as i64).await?;
    let has_account = member.as_ref().and_then(|m| m.uuid.as_ref()).is_some();

    if !has_account {
        let container = CreateComponent::Container(
            CreateContainer::new(vec![text("You don't have a linked Minecraft account.")])
        );
        command.create_response(&ctx.http, CreateInteractionResponse::Message(
            CreateInteractionResponseMessage::new()
                .flags(MessageFlags::IS_COMPONENTS_V2 | MessageFlags::EPHEMERAL)
                .components(vec![container]),
        )).await?;
        return Ok(());
    }

    repo.clear_uuid(discord_id as i64).await?;

    if let Some(guild_id) = command.guild_id {
        let _ = guild_id.edit_member(
            &ctx.http,
            command.user.id,
            EditMember::new().nickname(""),
        ).await;
    }

    let container = CreateComponent::Container(
        CreateContainer::new(vec![
            text("## Unlinked"),
            separator(),
            text("Your Minecraft account has been unlinked and your nickname has been reset."),
        ])
    );
    command.create_response(&ctx.http, CreateInteractionResponse::Message(
        CreateInteractionResponseMessage::new()
            .flags(MessageFlags::IS_COMPONENTS_V2 | MessageFlags::EPHEMERAL)
            .components(vec![container]),
    )).await?;

    Ok(())
}
