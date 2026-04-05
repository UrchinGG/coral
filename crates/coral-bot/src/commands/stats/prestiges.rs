use anyhow::Result;
use serenity::all::*;

use crate::framework::Data;
use super::{bedwars::cards::prestiges::render_prestiges, encode_png};


pub fn register() -> CreateCommand<'static> {
    CreateCommand::new("prestiges").description("View Bedwars star prestiges 100-5000")
}


#[allow(unused_variables)]
pub async fn run(ctx: &Context, command: &CommandInteraction, data: &Data) -> Result<()> {
    command.defer(&ctx.http).await?;

    let png = encode_png(&render_prestiges())?;

    command
        .edit_response(
            &ctx.http,
            EditInteractionResponse::new()
                .new_attachment(CreateAttachment::bytes(png, "prestiges.png")),
        )
        .await?;

    Ok(())
}
