use anyhow::Result;
use serenity::all::*;

use database::MemberRepository;

use crate::commands::admin::accounts_panel;
use crate::framework::Data;
use crate::utils::{separator, text};

pub fn register() -> CreateCommand<'static> {
    CreateCommand::new("link").description("Link or manage your Minecraft account")
}

pub async fn run(ctx: &Context, command: &CommandInteraction, data: &Data) -> Result<()> {
    let discord_id = command.user.id.get();
    let repo = MemberRepository::new(data.db.pool());

    if repo.get_by_discord_id(discord_id as i64).await?.is_none() {
        repo.create(discord_id as i64).await?;
    }

    let components =
        accounts_panel::build_accounts_for_self(data, discord_id, &command.user.name).await?;

    command
        .create_response(
            &ctx.http,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .flags(MessageFlags::IS_COMPONENTS_V2 | MessageFlags::EPHEMERAL)
                    .components(components),
            ),
        )
        .await?;

    Ok(())
}

pub fn build_link_parts(
    matches: &[(String, String)],
    prefix: &str,
    target_id: u64,
) -> Vec<CreateContainerComponent<'static>> {
    let is_self = prefix != "manage";
    let mut parts: Vec<CreateContainerComponent> = Vec::new();

    if !matches.is_empty() {
        let label = if is_self {
            "We found accounts that may be yours. Select one to link instantly:"
        } else {
            "Recommended accounts based on Hypixel social settings:"
        };
        parts.push(text(label));

        let options: Vec<CreateSelectMenuOption> = matches
            .iter()
            .take(25)
            .map(|(uuid, username)| CreateSelectMenuOption::new(username.clone(), uuid.clone()))
            .collect();

        parts.push(CreateContainerComponent::ActionRow(
            CreateActionRow::SelectMenu(
                CreateSelectMenu::new(
                    format!("{prefix}_link_pick:{target_id}"),
                    CreateSelectMenuKind::String {
                        options: options.into(),
                    },
                )
                .placeholder("Select an account"),
            ),
        ));

        parts.push(separator());
    }

    if is_self {
        parts.push(text(
            "**Verification Server**\nJoin `link.urchin.gg` with the Minecraft account you want to link, and then enter the 4 digit code you receive here.",
        ));
        parts.push(CreateContainerComponent::ActionRow(
            CreateActionRow::buttons(vec![
                CreateButton::new(format!("{prefix}_add_code:{target_id}"))
                    .label("Enter Code")
                    .style(ButtonStyle::Primary),
            ]),
        ));
        parts.push(separator());
    }

    let hypixel_label = if is_self {
        "**Hypixel Verification**\nSet your Discord username in Hypixel's Social Media settings, then enter your IGN."
    } else {
        "**Link by Username**\nEnter a Minecraft username to link. Hypixel social verification will be checked automatically."
    };
    parts.push(text(hypixel_label));
    parts.push(CreateContainerComponent::ActionRow(
        CreateActionRow::buttons(vec![
            CreateButton::new(format!("{prefix}_add_account:{target_id}"))
                .label("Enter Username")
                .style(ButtonStyle::Primary),
        ]),
    ));

    parts
}
