use anyhow::Result;
use serenity::all::*;

use crate::{
    framework::Data,
    utils::{separator, text},
};

const CUBELIFY_GIF: &[u8] = include_bytes!("../../../assets/coral_instructions.gif");
const CUBELIFY_GIF_NAME: &str = "coral_instructions.gif";

pub fn register() -> CreateCommand<'static> {
    CreateCommand::new("help").description("Setup guide and frequently asked questions")
}

pub async fn run(ctx: &Context, command: &CommandInteraction, data: &Data) -> Result<()> {
    let discord_id = command.user.id.get() as i64;
    let api_key = database::MemberRepository::new(data.db.pool())
        .get_by_discord_id(discord_id)
        .await?
        .and_then(|m| m.api_key);
    let components = build_help_view(api_key.as_deref(), false);
    command
        .create_response(
            &ctx.http,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .flags(MessageFlags::IS_COMPONENTS_V2 | MessageFlags::EPHEMERAL)
                    .add_file(CreateAttachment::bytes(CUBELIFY_GIF, CUBELIFY_GIF_NAME))
                    .components(components),
            ),
        )
        .await?;
    Ok(())
}

pub async fn handle_help_button(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let discord_id = component.user.id.get() as i64;
    let api_key = database::MemberRepository::new(data.db.pool())
        .get_by_discord_id(discord_id)
        .await?
        .and_then(|m| m.api_key);
    let components = build_help_view(api_key.as_deref(), true);
    component
        .create_response(
            &ctx.http,
            CreateInteractionResponse::UpdateMessage(
                CreateInteractionResponseMessage::new()
                    .flags(MessageFlags::IS_COMPONENTS_V2)
                    .add_file(CreateAttachment::bytes(CUBELIFY_GIF, CUBELIFY_GIF_NAME))
                    .components(components),
            ),
        )
        .await?;
    Ok(())
}

pub async fn handle_help_back(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let discord_id = component.user.id.get() as i64;
    let repo = database::MemberRepository::new(data.db.pool());
    let Some(member) = repo.get_by_discord_id(discord_id).await? else {
        return Ok(());
    };
    let components = super::dashboard::build_dashboard_view(&member, data).await;
    crate::interact::update_message(ctx, component, components).await
}

fn build_help_view(api_key: Option<&str>, from_dashboard: bool) -> Vec<CreateComponent<'static>> {
    let mut parts: Vec<CreateContainerComponent> = vec![text("## Help")];
    parts.push(separator());
    parts.push(text(
        "### How to add Urchin to Cubelify\n\
         1. Run `/dashboard` to register and get your API key\n\
         2. Open the **Settings** menu in Cubelify and navigate to **Integrations**\n\
         3. Under **Custom**,paste the URL below\n\
         4. Go to the **Columns** page\n\
         5. Add the **Custom Anti-Sniper Tags** column to your overlay",
    ));
    match api_key {
        Some(key) => parts.push(text(format!(
            "Your URL (click to reveal):\n||```\nhttps://api.urchin.gg/v3/cubelify?uuid={{{{id}}}}&key={key}\n```||"
        ))),
        None => parts.push(text(
            "```\nhttps://api.urchin.gg/v3/cubelify?uuid={{id}}&key=YOUR_KEY\n```"
        )),
    }

    parts.push(CreateContainerComponent::MediaGallery(
        CreateMediaGallery::new(vec![CreateMediaGalleryItem::new(
            CreateUnfurledMediaItem::new(format!("attachment://{CUBELIFY_GIF_NAME}")),
        )]),
    ));
    parts.push(separator());
    parts.push(text(
        "### Tags\n\
         These are the tags you'll see in your Cubelify overlay:",
    ));

    parts.push(text(
        "<:sniper:1459106167270932618> **Sniper**\n\
         -# Used for cheating snipers. Check the tooltip date; if it's old, they may no longer be active.\n\
         <:blatantcheater:1459106183196577812> **Blatant Cheater**\n\
         -# Obvious cheats that would be impossible on a vanilla client, like scaffold, speedmine, or autoblock.\n\
         <:closetcheater:1459106337039323136> **Closet Cheater**\n\
         -# Cheats that can be more subtle, like legit scaffold, aimassist, or lagrange.\n\
         <:confirmedcheater:1459106129765204049> **Confirmed Cheater**\n\
         -# Applied to players that have been confirmed to be cheating by staff. Typically, video evidence is available for these players on request.\n\
         <:replaysneeded:1482502914835615745> **Replays Needed**\n\
         -# Used whenever staff require replays of a player for any reason. Remember to submit replays to staff, it helps us prove players legit and clear their tags.\n\
         <:caution:1459106358098923583> **Caution**\n\
         -# Special tag used for things that don't fit into any of the above categories. Only staff can apply this."
    ));
    parts.push(separator());
    parts.push(text(
        "### FAQ\n\
         **How do I find my API key?**\n\
         -# Run `/dashboard`. A key is generated automatically when you first open it.\n\
         **What's a developer key?**\n\
         -# A separate key that can be granted access to private endpoints, for development purposes. Open a ticket in our server to request one.\n\
         **How do I report a cheater?**\n\
         -# Use the `/tag add` command with their username and the appropriate tag type. Make sure you read the rules before attempting to report or tag any account."
    ));

    if from_dashboard {
        parts.push(separator());
        parts.push(CreateContainerComponent::ActionRow(
            CreateActionRow::buttons(vec![
                CreateButton::new("help_back")
                    .label("Back")
                    .style(ButtonStyle::Secondary),
            ]),
        ));
    }

    vec![CreateComponent::Container(CreateContainer::new(parts))]
}
