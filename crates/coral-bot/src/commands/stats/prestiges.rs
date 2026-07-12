use anyhow::Result;
use serenity::all::*;

use super::bedwars::cards::prestiges::{prestige_color_codes, render_prestiges};
use super::encode_png;
use crate::framework::Data;
use crate::utils::text;

pub fn register() -> CreateCommand<'static> {
    CreateCommand::new("prestiges").description("View Bed Wars star prestiges 100-10000")
}

#[allow(unused_variables)]
pub async fn run(ctx: &Context, command: &CommandInteraction, data: &Data) -> Result<()> {
    command.defer(&ctx.http).await?;

    let png = encode_png(&render_prestiges())?;

    command
        .edit_response(
            &ctx.http,
            EditInteractionResponse::new()
                .new_attachment(CreateAttachment::bytes(png, "prestiges.png"))
                .components(vec![CreateComponent::ActionRow(CreateActionRow::buttons(
                    vec![
                        CreateButton::new("prestige_color_codes")
                            .label("Show Color Codes")
                            .style(ButtonStyle::Secondary),
                    ],
                ))]),
        )
        .await?;

    Ok(())
}

#[allow(unused_variables)]
pub async fn handle_color_codes(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let body = format!("## Prestige Color Codes\n{}", prestige_color_codes());

    component
        .create_response(
            &ctx.http,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .flags(MessageFlags::IS_COMPONENTS_V2 | MessageFlags::EPHEMERAL)
                    .components(vec![CreateComponent::Container(CreateContainer::new(
                        vec![text(body)],
                    ))]),
            ),
        )
        .await?;

    Ok(())
}
