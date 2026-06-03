use anyhow::Result;
use serenity::all::*;

use database::{BlacklistRepository, MemberRepository};

use crate::commands::blacklist::channel;
use crate::framework::Data;
use crate::interact;


const COLOR_NOT_FOUND: u32 = 0xFF5555;
const FACE_SIZE: u32 = 128;
const FACE_FILENAME: &str = "face.png";


pub fn register() -> CreateCommand<'static> {
    CreateCommand::new("info")
        .description("Look up detailed player information")
        .add_option(
            CreateCommandOption::new(CommandOptionType::String, "player", "Player name or UUID")
                .required(true),
        )
}


pub async fn run(ctx: &Context, command: &CommandInteraction, data: &Data) -> Result<()> {
    let repo = MemberRepository::new(data.db.pool());
    let invoker = repo.get_by_discord_id(command.user.id.get() as i64).await?;
    let is_staff = invoker.as_ref().map(|m| m.access_level >= 2).unwrap_or(false);

    if !is_staff {
        return interact::send_error(
            ctx, command, "Error", "You don't have permission to use this command",
        )
        .await;
    }

    let player = command.data.options.first()
        .and_then(|o| o.value.as_str())
        .unwrap_or("");

    let stats = match data.api.get_player_stats(player).await {
        Ok(s) => s,
        Err(_) => return interact::send_error(ctx, command, "Error", "Player not found").await,
    };

    let blacklist_repo = BlacklistRepository::new(data.db.pool());
    let tags = blacklist_repo.get_active_tags(&stats.uuid).await?;
    let lock_state = blacklist_repo.get_lock_state(&stats.uuid).await?;

    let face = data.skin_provider
        .fetch_face(&stats.uuid, FACE_SIZE)
        .await
        .map(|png| CreateAttachment::bytes(png, FACE_FILENAME));

    let container = build_info_container(
        &stats.uuid, &stats.username, stats.hypixel.as_ref(),
        &tags, &lock_state, face.is_some(),
    );

    let mut msg = CreateInteractionResponseMessage::new()
        .flags(MessageFlags::IS_COMPONENTS_V2)
        .components(vec![CreateComponent::Container(container)]);
    if let Some(attachment) = face {
        msg = msg.add_file(attachment);
    }

    command.create_response(&ctx.http, CreateInteractionResponse::Message(msg)).await?;
    Ok(())
}


fn build_info_container(
    uuid: &str,
    username: &str,
    player: Option<&serde_json::Value>,
    tags: &[database::PlayerEvent],
    lock_state: &database::LockState,
    has_face: bool,
) -> CreateContainer<'static> {
    let name = player
        .and_then(|p| p.get("displayname"))
        .and_then(|n| n.as_str())
        .unwrap_or(username);
    let first_login = player
        .and_then(|p| p.get("firstLogin"))
        .and_then(|t| t.as_i64())
        .map(|ts| format!("<t:{}:D>", ts / 1000))
        .unwrap_or_else(|| "Unknown".into());
    let last_login = player
        .and_then(|p| p.get("lastLogin"))
        .and_then(|t| t.as_i64())
        .map(|ts| format!("<t:{}:R>", ts / 1000))
        .unwrap_or_else(|| "Unknown".into());
    let network_level = player
        .and_then(|p| p.get("networkExp"))
        .and_then(|e| e.as_f64())
        .map(calculate_network_level)
        .unwrap_or(1);

    let tag_list = if tags.is_empty() {
        "None".into()
    } else {
        tags.iter().filter_map(|t| t.tag_type.as_deref()).collect::<Vec<_>>().join(", ")
    };
    let lock_status = if lock_state.locked { "\u{1F512} Locked" } else { "\u{1F513} Unlocked" };

    let header_text = CreateTextDisplay::new(format!("## Player Info: `{name}`"));
    let header = if has_face {
        CreateContainerComponent::Section(CreateSection::new(
            vec![CreateSectionComponent::TextDisplay(header_text)],
            CreateSectionAccessory::Thumbnail(CreateThumbnail::new(CreateUnfurledMediaItem::new(
                format!("attachment://{FACE_FILENAME}"),
            ))),
        ))
    } else {
        CreateContainerComponent::TextDisplay(header_text)
    };

    let details = CreateTextDisplay::new(format!(
        "-# UUID: `{uuid}`\n\
         **Network Level** — {network_level}\n\
         **First Login** — {first_login}\n\
         **Last Login** — {last_login}\n\
         **Tags** — {tag_list} ({})\n\
         **Status** — {lock_status}",
        tags.len()
    ));

    let color = if tags.is_empty() { channel::COLOR_INFO } else { COLOR_NOT_FOUND };

    CreateContainer::new(vec![
        header,
        CreateContainerComponent::Separator(CreateSeparator::new(true)),
        CreateContainerComponent::TextDisplay(details),
    ])
    .accent_color(color)
}


fn calculate_network_level(exp: f64) -> u32 {
    const BASE: f64 = 10_000.0;
    const GROWTH: f64 = 2_500.0;
    const REVERSE_PQ_PREFIX: f64 = -(BASE - 0.5 * GROWTH) / GROWTH;
    const REVERSE_CONST: f64 = REVERSE_PQ_PREFIX * REVERSE_PQ_PREFIX;
    const GROWTH_DIVIDES_2: f64 = 2.0 / GROWTH;
    ((REVERSE_PQ_PREFIX + (REVERSE_CONST + GROWTH_DIVIDES_2 * exp).sqrt()) + 1.0) as u32
}
