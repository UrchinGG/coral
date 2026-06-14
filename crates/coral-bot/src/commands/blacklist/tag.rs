use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::Result;
use blacklist::{EMOTE_ADDTAG, EMOTE_EDITTAG, EMOTE_REMOVETAG, EMOTE_TAG, lookup as lookup_tag};
use coral_redis::BlacklistEvent;
use database::{BlacklistRepository, CacheRepository, MemberRepository, TagOp, TagOpError};
use serenity::all::*;

use super::channel::{COLOR_ERROR, evidence_indicator, format_added_line, format_tag_block};
use crate::framework::{AccessRank, AccessRankExt, Data};
use crate::interact;
use crate::interact::send_deferred_error;
use crate::utils::{format_tag_detail, format_uuid_dashed, sanitize_reason};

const FACE_SIZE: u32 = 128;
const FACE_FILENAME: &str = "face.png";

fn face_thumbnail() -> CreateThumbnail<'static> {
    CreateThumbnail::new(CreateUnfurledMediaItem::new(format!(
        "attachment://{FACE_FILENAME}"
    )))
}

async fn face_attachment(data: &Data, uuid: &str) -> CreateAttachment<'static> {
    let png = data
        .skin_provider
        .fetch_face(uuid, FACE_SIZE)
        .await
        .unwrap_or_else(default_face);
    CreateAttachment::bytes(png, FACE_FILENAME)
}

fn default_face() -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(FACE_SIZE, FACE_SIZE, image::Rgba([0, 0, 0, 0]));
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png).unwrap();
    buf.into_inner()
}

fn face_section(parts: Vec<String>) -> CreateContainerComponent<'static> {
    CreateContainerComponent::Section(CreateSection::new(
        vec![CreateSectionComponent::TextDisplay(CreateTextDisplay::new(
            parts.join("\n"),
        ))],
        CreateSectionAccessory::Thumbnail(face_thumbnail()),
    ))
}

fn container_response(container: CreateContainer<'static>) -> Vec<CreateComponent<'static>> {
    vec![CreateComponent::Container(container)]
}

fn simple_result(emote: &str, msg: &str) -> CreateContainer<'static> {
    CreateContainer::new(vec![CreateContainerComponent::TextDisplay(
        CreateTextDisplay::new(format!("## {emote} {msg}")),
    )])
}

fn op_error_message(e: &TagOpError) -> &'static str {
    match e {
        TagOpError::PlayerLocked => "This player's tags are locked",
        TagOpError::InsufficientPermissions => "You don't have permission to do this",
        TagOpError::InvalidTagType => "Invalid tag type",
        TagOpError::TagAlreadyExists => "Player already has this tag type",
        TagOpError::PriorityConflict(_) => "Conflicts with an existing tag",
        TagOpError::TagNotFound => "Tag not found or already removed",
        TagOpError::EditWindowExpired => "The 30-minute edit window has passed",
        TagOpError::ModeratorRequired => "Only moderators can do this",
        TagOpError::Database(_) => "A database error occurred",
    }
}

pub struct PendingOverwrite {
    pub uuid: String,
    pub old_tag_type: String,
    pub tag_type: String,
    pub reason: String,
    pub hide: bool,
    pub silent: bool,
}

fn tag_choices(option: CreateCommandOption<'static>) -> CreateCommandOption<'static> {
    blacklist::user_addable()
        .iter()
        .filter(|tag| tag.name != "replays_needed")
        .fold(option, |opt, tag| {
            opt.add_string_choice(tag.display_name, tag.name)
        })
}

fn remove_tag_choices(option: CreateCommandOption<'static>) -> CreateCommandOption<'static> {
    blacklist::all().iter().fold(option, |opt, tag| {
        opt.add_string_choice(tag.display_name, tag.name)
    })
}

pub fn register() -> CreateCommand<'static> {
    CreateCommand::new("tag")
        .description("Manage player tags")
        .add_option(
            CreateCommandOption::new(
                CommandOptionType::SubCommand,
                "view",
                "View a player's tags",
            )
            .add_sub_option(
                CreateCommandOption::new(
                    CommandOptionType::String,
                    "player",
                    "Player name or UUID",
                )
                .required(true),
            ),
        )
        .add_option(
            CreateCommandOption::new(
                CommandOptionType::SubCommand,
                "add",
                "Add a tag to a player",
            )
            .add_sub_option(
                CreateCommandOption::new(
                    CommandOptionType::String,
                    "player",
                    "Player name or UUID",
                )
                .required(true),
            )
            .add_sub_option(tag_choices(
                CreateCommandOption::new(CommandOptionType::String, "type", "Tag type")
                    .required(true),
            ))
            .add_sub_option(
                CreateCommandOption::new(CommandOptionType::String, "reason", "Reason for the tag")
                    .required(true)
                    .max_length(120),
            )
            .add_sub_option(CreateCommandOption::new(
                CommandOptionType::Boolean,
                "hide",
                "Hide your username",
            )),
        )
        .add_option(
            CreateCommandOption::new(
                CommandOptionType::SubCommand,
                "remove",
                "Remove a tag from a player",
            )
            .add_sub_option(
                CreateCommandOption::new(
                    CommandOptionType::String,
                    "player",
                    "Player name or UUID",
                )
                .required(true),
            )
            .add_sub_option(remove_tag_choices(
                CreateCommandOption::new(CommandOptionType::String, "type", "Tag type to remove")
                    .required(true),
            )),
        )
        .add_option(
            CreateCommandOption::new(
                CommandOptionType::SubCommand,
                "manage",
                "Staff: manage a player's tags with history",
            )
            .add_sub_option(
                CreateCommandOption::new(
                    CommandOptionType::String,
                    "player",
                    "Player name or UUID",
                )
                .required(true),
            ),
        )
        .add_option(
            CreateCommandOption::new(
                CommandOptionType::SubCommand,
                "lock",
                "Lock a player's tags from modification",
            )
            .add_sub_option(
                CreateCommandOption::new(
                    CommandOptionType::String,
                    "player",
                    "Player name or UUID",
                )
                .required(true),
            )
            .add_sub_option(
                CreateCommandOption::new(CommandOptionType::String, "reason", "Reason for locking")
                    .required(true)
                    .max_length(120),
            ),
        )
        .add_option(
            CreateCommandOption::new(
                CommandOptionType::SubCommand,
                "unlock",
                "Unlock a player's tags",
            )
            .add_sub_option(
                CreateCommandOption::new(
                    CommandOptionType::String,
                    "player",
                    "Player name or UUID",
                )
                .required(true),
            ),
        )
}

pub async fn run(ctx: &Context, command: &CommandInteraction, data: &Data) -> Result<()> {
    match command.data.options.first().map(|o| o.name.as_str()) {
        Some("view") => run_view(ctx, command, data).await,
        Some("add") => run_add(ctx, command, data).await,
        Some("remove") => run_remove(ctx, command, data).await,
        Some("manage") => run_manage(ctx, command, data).await,
        Some("lock") => run_lock(ctx, command, data).await,
        Some("unlock") => run_unlock(ctx, command, data).await,
        _ => Ok(()),
    }
}

fn get_sub_options(command: &CommandInteraction) -> Vec<ResolvedOption<'_>> {
    command
        .data
        .options()
        .first()
        .map(|o| match &o.value {
            ResolvedValue::SubCommand(opts) => opts.to_vec(),
            _ => vec![],
        })
        .unwrap_or_default()
}

fn get_string<'a>(options: &'a [ResolvedOption<'a>], name: &str) -> &'a str {
    options
        .iter()
        .find(|o| o.name == name)
        .and_then(|o| match o.value {
            ResolvedValue::String(s) => Some(s),
            _ => None,
        })
        .unwrap_or("")
}

fn get_bool(options: &[ResolvedOption<'_>], name: &str) -> bool {
    options
        .iter()
        .find(|o| o.name == name)
        .and_then(|o| match o.value {
            ResolvedValue::Boolean(b) => Some(b),
            _ => None,
        })
        .unwrap_or(false)
}

pub(super) async fn get_rank(data: &Data, discord_id: u64) -> Result<AccessRank> {
    let member_repo = MemberRepository::new(data.db.pool());
    let member = member_repo.get_by_discord_id(discord_id as i64).await?;
    Ok(AccessRank::of(data, discord_id, member.as_ref()))
}

async fn get_rank_and_member(
    data: &Data,
    discord_id: u64,
) -> Result<(AccessRank, Option<database::Member>)> {
    let member_repo = MemberRepository::new(data.db.pool());
    let member = member_repo.get_by_discord_id(discord_id as i64).await?;
    let rank = AccessRank::of(data, discord_id, member.as_ref());
    Ok((rank, member))
}

pub(super) enum MemberCheck {
    Ok(AccessRank, database::Member),
    NotLinked,
    NotInGuild,
}

pub(super) async fn require_linked_member(
    ctx: &Context,
    data: &Data,
    discord_id: u64,
) -> Result<MemberCheck> {
    let (rank, member) = get_rank_and_member(data, discord_id).await?;
    let Some(member) = member.filter(|m| m.uuid.is_some()) else {
        return Ok(MemberCheck::NotLinked);
    };
    if let Some(guild_id) = data.home_guild_id {
        if guild_id
            .member(&ctx.http, UserId::new(discord_id))
            .await
            .is_err()
        {
            return Ok(MemberCheck::NotInGuild);
        }
    }
    Ok(MemberCheck::Ok(rank, member))
}

async fn resolve_names(http: &Arc<Http>, ids: impl Iterator<Item = i64>) -> HashMap<i64, String> {
    let mut seen = HashSet::new();
    let mut join_set = tokio::task::JoinSet::new();
    for id in ids.filter(|id| seen.insert(*id)) {
        let http = http.clone();
        join_set.spawn(async move {
            let name = http
                .get_user(UserId::new(id as u64))
                .await
                .map(|u| u.name.to_string())
                .unwrap_or_else(|_| id.to_string());
            (id, name)
        });
    }
    let mut map = HashMap::new();
    while let Some(Ok((id, name))) = join_set.join_next().await {
        map.insert(id, name);
    }
    map
}

async fn send_tag_response(
    ctx: &Context,
    command: &CommandInteraction,
    data: &Data,
    uuid: &str,
    container: CreateContainer<'static>,
) -> Result<()> {
    let mut resp = EditInteractionResponse::new()
        .flags(MessageFlags::IS_COMPONENTS_V2)
        .components(container_response(container));
    resp = resp.new_attachment(face_attachment(data, uuid).await);
    command.edit_response(&ctx.http, resp).await?;
    Ok(())
}

pub async fn send_tag_applied(
    ctx: &Context,
    command: &CommandInteraction,
    data: &Data,
    username: &str,
    uuid: &str,
    tag: &database::PlayerEvent,
    hint: &str,
) -> Result<()> {
    let added_line = format_added_line(ctx, tag).await;
    let block = format_tag_block(
        tag.tag_type.as_deref().unwrap_or(""),
        &format_tag_detail(tag),
        "",
        Some(&added_line),
        None,
        false,
    );
    let container = CreateContainer::new(vec![
        face_section(vec![
            format!("## {} New Tag Applied\nIGN - `{username}`\n", EMOTE_ADDTAG),
            block,
            format!("-# UUID: {}", format_uuid_dashed(uuid)),
        ]),
        CreateContainerComponent::Separator(CreateSeparator::new(true)),
        CreateContainerComponent::ActionRow(CreateActionRow::buttons(vec![
            CreateButton::new(format!("tag_undo:{}", tag.id))
                .label("Undo")
                .style(ButtonStyle::Danger),
        ])),
        CreateContainerComponent::TextDisplay(CreateTextDisplay::new(hint.to_string())),
    ]);
    send_tag_response(ctx, command, data, uuid, container).await
}

async fn run_view(ctx: &Context, command: &CommandInteraction, data: &Data) -> Result<()> {
    command.defer(&ctx.http).await?;

    let options = get_sub_options(command);
    let player = get_string(&options, "player");

    let player_info = match data.api.resolve(player).await {
        Ok(info) => info,
        Err(_) => return send_deferred_error(ctx, command, "Error", "Player not found").await,
    };

    let repo = BlacklistRepository::new(data.db.pool());
    let (lock_state, player_tags, face) = tokio::join!(
        repo.get_lock_state(&player_info.uuid),
        repo.get_active_tags(&player_info.uuid),
        face_attachment(data, &player_info.uuid),
    );
    let lock_state = lock_state?;
    let player_tags = player_tags?;

    let is_locked = lock_state.locked;
    let dashed_uuid = format_uuid_dashed(&player_info.uuid);

    if player_tags.is_empty() {
        let container = CreateContainer::new(vec![
            face_section(vec![
                format!(
                    "## {} No Tags\n`{}` is not tagged.",
                    EMOTE_TAG, player_info.username
                ),
                format!("-# UUID: {dashed_uuid}"),
            ]),
            CreateContainerComponent::Separator(CreateSeparator::new(true)),
        ]);
        let mut resp = EditInteractionResponse::new()
            .flags(MessageFlags::IS_COMPONENTS_V2)
            .components(container_response(container));
        resp = resp.new_attachment(face);
        command.edit_response(&ctx.http, resp).await?;
        return Ok(());
    }

    let evidence_url = super::evidence::evidence_thread_url(data, &player_info.uuid);
    let lock_indicator = if is_locked { " \u{1F512}" } else { "" };

    let adders = player_tags
        .iter()
        .filter(|t| !t.hide_username.unwrap_or(false))
        .filter_map(|t| t.author);
    let reviewers = player_tags
        .iter()
        .flat_map(|t| t.reviewed_by.iter().flatten().copied());
    let resolved_names = resolve_names(&ctx.http, adders.chain(reviewers)).await;

    let header = format!(
        "## {} Tagged User{}\nIGN - `{}`\n",
        EMOTE_TAG, lock_indicator, player_info.username
    );

    let mut tag_texts = vec![];
    for tag in &player_tags {
        let ts = tag.ts.timestamp();
        let added_line = match tag.author.filter(|_| !tag.hide_username.unwrap_or(false)) {
            Some(author) => {
                let fallback = author.to_string();
                let username = resolved_names
                    .get(&author)
                    .map(|s| s.as_str())
                    .unwrap_or(&fallback);
                format!("> -# **\\- Added by `@{username}` <t:{ts}:R>**")
            }
            None => format!("> -# **\\- <t:{ts}:R>**"),
        };

        let reviewed_line = tag.reviewed_by.as_ref().map(|ids| {
            let formatted: Vec<String> = ids
                .iter()
                .map(|id| {
                    let name = resolved_names
                        .get(id)
                        .cloned()
                        .unwrap_or_else(|| id.to_string());
                    format!("`@{name}`")
                })
                .collect();
            format!("> -# **\\- Reviewed by {}**", formatted.join(", "))
        });

        let tag_type = tag.tag_type.as_deref().unwrap_or("");
        let indicator = evidence_indicator(tag_type, evidence_url.is_some());

        tag_texts.push(format_tag_block(
            tag_type,
            &format_tag_detail(tag),
            &indicator,
            Some(&added_line),
            reviewed_line.as_deref(),
            false,
        ));
    }

    let mut footer = format!("-# UUID: {dashed_uuid}");
    if let Some(url) = &evidence_url {
        footer.push_str(&format!(" | [Evidence]({url})"));
    }

    let first_tag = tag_texts.first().cloned().unwrap_or_default();
    let mut components: Vec<CreateContainerComponent> = vec![face_section(vec![header, first_tag])];
    for text in tag_texts.iter().skip(1) {
        components.push(CreateContainerComponent::TextDisplay(
            CreateTextDisplay::new(text.clone()),
        ));
    }
    components.push(CreateContainerComponent::TextDisplay(
        CreateTextDisplay::new(footer),
    ));
    components.push(CreateContainerComponent::Separator(CreateSeparator::new(
        true,
    )));

    let mut resp = EditInteractionResponse::new()
        .flags(MessageFlags::IS_COMPONENTS_V2)
        .components(container_response(CreateContainer::new(components)));
    resp = resp.new_attachment(face);
    command.edit_response(&ctx.http, resp).await?;
    Ok(())
}

async fn run_add(ctx: &Context, command: &CommandInteraction, data: &Data) -> Result<()> {
    command.defer_ephemeral(&ctx.http).await?;

    let discord_id = command.user.id.get();
    let (rank, member) = match require_linked_member(ctx, data, discord_id).await? {
        MemberCheck::Ok(r, m) => (r, m),
        MemberCheck::NotInGuild => {
            return send_deferred_error(
                ctx,
                command,
                "Error",
                "You must be in the Urchin server to use this command",
            )
            .await;
        }
        MemberCheck::NotLinked => {
            return send_deferred_error(
                ctx,
                command,
                "Error",
                "You must link your account to add tags",
            )
            .await;
        }
    };
    if rank < AccessRank::Helper && member.tagging_disabled {
        return send_deferred_error(
            ctx,
            command,
            "Error",
            "Your tagging ability has been disabled",
        )
        .await;
    }

    let options = get_sub_options(command);
    let player = get_string(&options, "player");
    let tag_type = get_string(&options, "type");
    let reason = get_string(&options, "reason");
    let hide = get_bool(&options, "hide");

    if tag_type == "confirmed_cheater" {
        return send_deferred_error(
            ctx,
            command,
            "Error",
            "Confirmed cheater tags can only be applied through the review system",
        )
        .await;
    }
    if tag_type == "replays_needed" {
        return send_deferred_error(
            ctx,
            command,
            "Error",
            "Use the /watch command to add replays needed tags",
        )
        .await;
    }
    if reason.is_empty() {
        return send_deferred_error(
            ctx,
            command,
            "Error",
            "A reason is required for this tag type",
        )
        .await;
    }

    let needs_review =
        rank == AccessRank::Default && super::reviews::REVIEW_TAGS.contains(&tag_type);

    let player_info = match data.api.resolve(player).await {
        Ok(info) => info,
        Err(_) => return send_deferred_error(ctx, command, "Error", "Player not found").await,
    };

    if needs_review {
        let current = match conflicting_tag(data, &player_info.uuid, tag_type).await {
            Some(tag) => Some(super::reviews::CurrentTag {
                tag_type: tag.tag_type.clone().unwrap_or_default(),
                detail: format_tag_detail(&tag),
                added_line: format_added_line(ctx, &tag).await,
            }),
            None => None,
        };
        let components = super::reviews::build_confirmation_message(
            discord_id,
            &player_info.username,
            &player_info.uuid,
            tag_type,
            reason,
            data.review_forum_id,
            current,
        );
        let png = data
            .skin_provider
            .fetch_face(&player_info.uuid, super::reviews::FACE_SIZE)
            .await
            .unwrap_or_else(default_face);
        let face = CreateAttachment::bytes(png, super::reviews::face_filename(&player_info.uuid));
        command
            .edit_response(
                &ctx.http,
                EditInteractionResponse::new()
                    .flags(MessageFlags::IS_COMPONENTS_V2)
                    .components(components)
                    .new_attachment(face),
            )
            .await?;
        return Ok(());
    }

    let ops = TagOp::new(data.db.pool());
    match ops
        .add(
            &player_info.uuid,
            tag_type,
            reason,
            discord_id as i64,
            rank.to_level(),
            hide,
            None,
            None,
        )
        .await
    {
        Ok(new_tag) => {
            data.event_publisher
                .publish(&BlacklistEvent::TagAdded {
                    uuid: player_info.uuid.clone(),
                    tag_id: new_tag.id,
                    added_by: discord_id as i64,
                    silent: false,
                })
                .await;

            let hint = if rank >= AccessRank::Trusted {
                "-# You can remove this tag within 30 minutes using /tag remove."
            } else {
                "-# You can overwrite or remove this tag within 30 minutes using /tag add and /tag remove."
            };

            send_tag_applied(
                ctx,
                command,
                data,
                &player_info.username,
                &player_info.uuid,
                &new_tag,
                hint,
            )
            .await
        }
        Err(TagOpError::PriorityConflict(conflict)) => {
            show_overwrite_prompt(
                ctx,
                command,
                data,
                &player_info,
                &conflict,
                tag_type,
                reason,
                hide,
            )
            .await
        }
        Err(e) => send_deferred_error(ctx, command, "Error", op_error_message(&e)).await,
    }
}

async fn conflicting_tag(data: &Data, uuid: &str, tag_type: &str) -> Option<database::PlayerEvent> {
    let priority = lookup_tag(tag_type)?.priority;
    let tags = BlacklistRepository::new(data.db.pool())
        .get_active_tags(uuid)
        .await
        .ok()?;
    tags.into_iter().find(|t| {
        lookup_tag(t.tag_type.as_deref().unwrap_or("")).map(|d| d.priority) == Some(priority)
    })
}

async fn show_overwrite_prompt(
    ctx: &Context,
    command: &CommandInteraction,
    data: &Data,
    player_info: &crate::api::ResolveResponse,
    conflict: &database::PlayerEvent,
    tag_type: &str,
    reason: &str,
    hide: bool,
) -> Result<()> {
    let conflict_type = conflict.tag_type.as_deref().unwrap_or("");
    let dashed_uuid = format_uuid_dashed(&player_info.uuid);

    let overwrite_key = command.id.to_string();
    data.pending_overwrites.lock().unwrap().insert(
        overwrite_key.clone(),
        PendingOverwrite {
            uuid: player_info.uuid.clone(),
            old_tag_type: conflict_type.to_string(),
            tag_type: tag_type.to_string(),
            reason: reason.to_string(),
            hide,
            silent: false,
        },
    );

    let button = CreateButton::new(format!("tag_overwrite:{overwrite_key}"))
        .label("Overwrite Tag")
        .style(ButtonStyle::Danger);
    let old_added = format_added_line(ctx, conflict).await;
    let new_added = (!hide).then(|| format!("> -# **\\- Added by `@{}`**", command.user.name));

    let current_block = format_tag_block(
        conflict_type,
        &format_tag_detail(conflict),
        "",
        Some(&old_added),
        None,
        false,
    );
    let proposed_block = format_tag_block(
        tag_type,
        &sanitize_reason(reason),
        "",
        new_added.as_deref(),
        None,
        false,
    );

    let container = CreateContainer::new(vec![
        CreateContainerComponent::TextDisplay(CreateTextDisplay::new(format!(
            "## {} Tag Overwrite",
            EMOTE_EDITTAG
        ))),
        CreateContainerComponent::TextDisplay(CreateTextDisplay::new(
            "This player already has an incompatible tag. Overwriting replaces it with your tag.",
        )),
        CreateContainerComponent::Separator(CreateSeparator::new(true)),
        CreateContainerComponent::TextDisplay(CreateTextDisplay::new("-# Current")),
        face_section(vec![
            format!("IGN - `{}`\n", player_info.username),
            current_block,
            format!("-# UUID: {dashed_uuid}"),
        ]),
        CreateContainerComponent::Separator(CreateSeparator::new(true)),
        CreateContainerComponent::TextDisplay(CreateTextDisplay::new("-# New")),
        CreateContainerComponent::Section(CreateSection::new(
            vec![CreateSectionComponent::TextDisplay(CreateTextDisplay::new(
                proposed_block,
            ))],
            CreateSectionAccessory::Button(button),
        )),
    ]);

    let resp = EditInteractionResponse::new()
        .flags(MessageFlags::IS_COMPONENTS_V2)
        .components(vec![CreateComponent::Container(container)])
        .new_attachment(face_attachment(data, &player_info.uuid).await);
    command.edit_response(&ctx.http, resp).await?;
    Ok(())
}

pub async fn handle_overwrite_button(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let key = component
        .data
        .custom_id
        .strip_prefix("tag_overwrite:")
        .unwrap_or_default();
    let overwrite = data.pending_overwrites.lock().unwrap().remove(key);
    let Some(overwrite) = overwrite else {
        return send_component_message(ctx, component, "This overwrite has expired").await;
    };

    let uuid = &overwrite.uuid;
    let discord_id = component.user.id.get();
    let rank = get_rank(data, discord_id).await?;

    let cache = CacheRepository::new(data.db.pool());
    let player_name = cache
        .get_username(uuid)
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| uuid.to_string());

    let ops = TagOp::new(data.db.pool());
    let (old_tag, new_tag) = match ops
        .overwrite(
            uuid,
            &overwrite.old_tag_type,
            &overwrite.tag_type,
            &overwrite.reason,
            discord_id as i64,
            rank.to_level(),
            overwrite.hide,
        )
        .await
    {
        Ok(result) => result,
        Err(e) => return send_component_message(ctx, component, op_error_message(&e)).await,
    };

    let dashed_uuid = format_uuid_dashed(uuid);
    let added_line = format_added_line(ctx, &new_tag).await;
    let block = format_tag_block(
        new_tag.tag_type.as_deref().unwrap_or(""),
        &format_tag_detail(&new_tag),
        "",
        Some(&added_line),
        None,
        false,
    );

    let container = CreateContainer::new(vec![
        face_section(vec![
            format!(
                "## {} Tag Overwritten\nIGN - `{}`\n",
                EMOTE_EDITTAG, player_name
            ),
            block,
            format!("-# UUID: {dashed_uuid}"),
        ]),
        CreateContainerComponent::Separator(CreateSeparator::new(true)),
    ]);

    let mut msg = CreateInteractionResponseMessage::new()
        .flags(MessageFlags::IS_COMPONENTS_V2)
        .components(container_response(container));
    msg = msg.add_file(face_attachment(data, uuid).await);
    component
        .create_response(&ctx.http, CreateInteractionResponse::UpdateMessage(msg))
        .await?;

    data.event_publisher
        .publish(&BlacklistEvent::TagOverwritten {
            uuid: uuid.to_string(),
            old_tag_id: old_tag.id,
            old_tag_type: old_tag.tag_type.clone().unwrap_or_default(),
            old_reason: old_tag.reason.clone().unwrap_or_default(),
            new_tag_id: new_tag.id,
            overwritten_by: discord_id as i64,
            silent: overwrite.silent,
        })
        .await;

    Ok(())
}

async fn run_remove(ctx: &Context, command: &CommandInteraction, data: &Data) -> Result<()> {
    command.defer_ephemeral(&ctx.http).await?;

    let discord_id = command.user.id.get();
    match require_linked_member(ctx, data, discord_id).await? {
        MemberCheck::Ok(..) => {}
        MemberCheck::NotInGuild => {
            return send_deferred_error(
                ctx,
                command,
                "Error",
                "You must be in the Urchin server to use this command",
            )
            .await;
        }
        MemberCheck::NotLinked => {
            return send_deferred_error(
                ctx,
                command,
                "Error",
                "You must link your account to remove tags",
            )
            .await;
        }
    };
    let rank = get_rank(data, discord_id).await?;
    let options = get_sub_options(command);
    let player = get_string(&options, "player");
    let tag_type = get_string(&options, "type");

    let player_info = match data.api.resolve(player).await {
        Ok(info) => info,
        Err(_) => return send_deferred_error(ctx, command, "Error", "Player not found").await,
    };

    let ops = TagOp::new(data.db.pool());
    let tag = match ops
        .remove(
            &player_info.uuid,
            tag_type,
            discord_id as i64,
            rank.to_level(),
        )
        .await
    {
        Ok(tag) => tag,
        Err(e) => return send_deferred_error(ctx, command, "Error", op_error_message(&e)).await,
    };

    if tag_type == "confirmed_cheater" {
        super::evidence::archive_evidence_for_uuid(ctx, data, &player_info.uuid).await?;
    }

    let dashed_uuid = format_uuid_dashed(&player_info.uuid);
    let added_line = format_added_line(ctx, &tag).await;
    let block = format_tag_block(
        tag_type,
        &format_tag_detail(&tag),
        "",
        Some(&added_line),
        None,
        true,
    );

    let container = CreateContainer::new(vec![
        face_section(vec![
            format!(
                "## {} Tag Removed\nIGN - `{}`\n",
                EMOTE_REMOVETAG, player_info.username
            ),
            block,
            format!("-# UUID: {dashed_uuid}"),
        ]),
        CreateContainerComponent::Separator(CreateSeparator::new(true)),
    ])
    .accent_color(COLOR_ERROR);

    send_tag_response(ctx, command, data, &player_info.uuid, container).await?;

    data.event_publisher
        .publish(&BlacklistEvent::TagRemoved {
            uuid: player_info.uuid.clone(),
            tag_id: tag.id,
            removed_by: discord_id as i64,
            silent: false,
        })
        .await;

    Ok(())
}

async fn run_lock(ctx: &Context, command: &CommandInteraction, data: &Data) -> Result<()> {
    command.defer_ephemeral(&ctx.http).await?;

    let discord_id = command.user.id.get();
    let rank = get_rank(data, discord_id).await?;

    let options = get_sub_options(command);
    let player = get_string(&options, "player");
    let reason = get_string(&options, "reason");

    let player_info = match data.api.resolve(player).await {
        Ok(info) => info,
        Err(_) => return send_deferred_error(ctx, command, "Error", "Player not found").await,
    };

    let ops = TagOp::new(data.db.pool());
    if let Err(e) = ops
        .lock_player(
            &player_info.uuid,
            reason,
            discord_id as i64,
            rank.to_level(),
        )
        .await
    {
        return send_deferred_error(ctx, command, "Error", op_error_message(&e)).await;
    }

    let dashed_uuid = format_uuid_dashed(&player_info.uuid);
    let container = CreateContainer::new(vec![
        face_section(vec![
            format!(
                "## {} Player Locked \u{1F512}\nIGN - `{}`\n",
                EMOTE_TAG, player_info.username
            ),
            format!("> {}", sanitize_reason(reason)),
            format!("-# UUID: {dashed_uuid}"),
        ]),
        CreateContainerComponent::Separator(CreateSeparator::new(true)),
    ]);

    send_tag_response(ctx, command, data, &player_info.uuid, container).await?;

    data.event_publisher
        .publish(&BlacklistEvent::PlayerLocked {
            uuid: player_info.uuid.clone(),
            locked_by: discord_id as i64,
            reason: reason.to_string(),
        })
        .await;

    Ok(())
}

async fn run_unlock(ctx: &Context, command: &CommandInteraction, data: &Data) -> Result<()> {
    command.defer_ephemeral(&ctx.http).await?;

    let discord_id = command.user.id.get();
    let rank = get_rank(data, discord_id).await?;

    let options = get_sub_options(command);
    let player = get_string(&options, "player");

    let player_info = match data.api.resolve(player).await {
        Ok(info) => info,
        Err(_) => return send_deferred_error(ctx, command, "Error", "Player not found").await,
    };

    let ops = TagOp::new(data.db.pool());
    let unlocked = match ops
        .unlock_player(&player_info.uuid, discord_id as i64, rank.to_level())
        .await
    {
        Ok(u) => u,
        Err(e) => return send_deferred_error(ctx, command, "Error", op_error_message(&e)).await,
    };

    let dashed_uuid = format_uuid_dashed(&player_info.uuid);
    let face = face_attachment(data, &player_info.uuid).await;

    let container = if unlocked {
        CreateContainer::new(vec![
            face_section(vec![
                format!(
                    "## {} Player Unlocked \u{1F513}\nIGN - `{}`",
                    EMOTE_TAG, player_info.username
                ),
                format!("-# UUID: {dashed_uuid}"),
            ]),
            CreateContainerComponent::Separator(CreateSeparator::new(true)),
        ])
    } else {
        CreateContainer::new(vec![
            face_section(vec![
                format!("## Not Locked\nIGN - `{}`", player_info.username),
                format!("-# UUID: {dashed_uuid}"),
            ]),
            CreateContainerComponent::Separator(CreateSeparator::new(true)),
        ])
    };

    let mut resp = EditInteractionResponse::new()
        .flags(MessageFlags::IS_COMPONENTS_V2)
        .components(container_response(container));
    resp = resp.new_attachment(face);
    command.edit_response(&ctx.http, resp).await?;

    if unlocked {
        data.event_publisher
            .publish(&BlacklistEvent::PlayerUnlocked {
                uuid: player_info.uuid.clone(),
                unlocked_by: discord_id as i64,
            })
            .await;
    }

    Ok(())
}

async fn try_archive_evidence(ctx: &Context, data: &Data, uuid: &str) {
    if let Err(e) = super::evidence::archive_evidence_for_uuid(ctx, data, uuid).await {
        tracing::error!("Failed to archive evidence for {uuid}: {e}");
    }
}

pub async fn handle_undo(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let tag_id = interact::parse_id(&component.data.custom_id).unwrap_or(0) as i64;
    let discord_id = component.user.id.get();
    let rank = get_rank(data, discord_id).await?;

    let ops = TagOp::new(data.db.pool());
    match ops
        .remove_by_id(tag_id, discord_id as i64, rank.to_level())
        .await
    {
        Ok((uuid, _tag)) => {
            data.event_publisher
                .publish(&BlacklistEvent::TagRemoved {
                    uuid,
                    tag_id,
                    removed_by: discord_id as i64,
                    silent: false,
                })
                .await;
            component
                .create_response(
                    &ctx.http,
                    CreateInteractionResponse::UpdateMessage(
                        CreateInteractionResponseMessage::new()
                            .flags(MessageFlags::IS_COMPONENTS_V2)
                            .components(container_response(simple_result(
                                EMOTE_REMOVETAG,
                                "Tag Removed",
                            ))),
                    ),
                )
                .await?;
            Ok(())
        }
        Err(e) => send_component_message(ctx, component, op_error_message(&e)).await,
    }
}

async fn run_manage(ctx: &Context, command: &CommandInteraction, data: &Data) -> Result<()> {
    command.defer_ephemeral(&ctx.http).await?;

    let discord_id = command.user.id.get();
    let rank = get_rank(data, discord_id).await?;
    if rank < AccessRank::Moderator {
        return send_deferred_error(ctx, command, "Error", "Only moderators can manage tags").await;
    }

    let options = get_sub_options(command);
    let player = get_string(&options, "player");
    let player_info = match data.api.resolve(player).await {
        Ok(info) => info,
        Err(_) => return send_deferred_error(ctx, command, "Error", "Player not found").await,
    };

    let components = build_manage_main(ctx, data, &player_info.uuid, None).await?;
    let mut resp = EditInteractionResponse::new()
        .flags(MessageFlags::IS_COMPONENTS_V2)
        .components(components);
    resp = resp.new_attachment(face_attachment(data, &player_info.uuid).await);
    command.edit_response(&ctx.http, resp).await?;
    Ok(())
}

async fn build_manage_main(
    ctx: &Context,
    data: &Data,
    uuid: &str,
    confirming: Option<i64>,
) -> Result<Vec<CreateComponent<'static>>> {
    let repo = BlacklistRepository::new(data.db.pool());
    let cache = CacheRepository::new(data.db.pool());
    let (active, username) = tokio::join!(repo.get_active_tags(uuid), cache.get_username(uuid));
    let active = active?;
    let username = username.ok().flatten().unwrap_or_else(|| uuid.to_string());
    let dashed_uuid = format_uuid_dashed(uuid);
    let names = resolve_names(&ctx.http, active.iter().filter_map(|t| t.author)).await;

    let mut parts: Vec<CreateContainerComponent> = vec![face_section(vec![format!(
        "## {} Manage Tags\nIGN - `{}`",
        EMOTE_EDITTAG, username
    )])];

    if active.is_empty() {
        parts.push(CreateContainerComponent::TextDisplay(
            CreateTextDisplay::new("*No active tags*"),
        ));
    }

    for tag in &active {
        parts.push(CreateContainerComponent::Separator(CreateSeparator::new(
            true,
        )));
        let tag_type = tag.tag_type.as_deref().unwrap_or("");
        let ts = tag.ts.timestamp();
        let added_line = match tag.author {
            Some(id) => {
                let name = names.get(&id).map(|s| s.as_str()).unwrap_or("unknown");
                let hidden = if tag.hide_username.unwrap_or(false) {
                    " (hidden)"
                } else {
                    ""
                };
                format!("> -# **\\- Added by `@{name}`{hidden} <t:{ts}:R>**")
            }
            None => format!("> -# **\\- <t:{ts}:R>**"),
        };
        parts.push(CreateContainerComponent::TextDisplay(
            CreateTextDisplay::new(format_tag_block(
                tag_type,
                &format_tag_detail(tag),
                "",
                Some(&added_line),
                None,
                false,
            )),
        ));

        if confirming == Some(tag.id) {
            parts.push(CreateContainerComponent::ActionRow(
                CreateActionRow::buttons(vec![
                    CreateButton::new(format!("mt_confirm:{uuid}:{}", tag.id))
                        .label("Confirm Remove")
                        .style(ButtonStyle::Danger),
                    CreateButton::new(format!("mt_back:{uuid}"))
                        .label("Cancel")
                        .style(ButtonStyle::Secondary),
                ]),
            ));
        } else {
            parts.push(CreateContainerComponent::ActionRow(
                CreateActionRow::buttons(vec![
                    CreateButton::new(format!("mt_remove:{uuid}:{}", tag.id))
                        .label("Remove")
                        .style(ButtonStyle::Danger),
                ]),
            ));
        }
    }

    parts.push(CreateContainerComponent::Separator(CreateSeparator::new(
        true,
    )));
    parts.push(CreateContainerComponent::TextDisplay(
        CreateTextDisplay::new(format!("-# UUID: {dashed_uuid}")),
    ));
    parts.push(CreateContainerComponent::ActionRow(
        CreateActionRow::buttons(vec![
            CreateButton::new(format!("tag_history:{uuid}"))
                .label("Tag History")
                .style(ButtonStyle::Secondary),
            CreateButton::new(format!("mt_addbtn:{uuid}"))
                .label("New Tag")
                .style(ButtonStyle::Primary),
        ]),
    ));

    Ok(vec![CreateComponent::Container(CreateContainer::new(
        parts,
    ))])
}

async fn update_manage_view(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
    uuid: &str,
) -> Result<()> {
    let components = build_manage_main(ctx, data, uuid, None).await?;
    component
        .create_response(
            &ctx.http,
            CreateInteractionResponse::UpdateMessage(
                CreateInteractionResponseMessage::new()
                    .flags(MessageFlags::IS_COMPONENTS_V2)
                    .components(components)
                    .add_file(face_attachment(data, uuid).await),
            ),
        )
        .await?;
    Ok(())
}

fn parse_manage_uuid(custom_id: &str) -> &str {
    custom_id.split(':').nth(1).unwrap_or("")
}

pub async fn handle_manage_remove(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let payload = component
        .data
        .custom_id
        .strip_prefix("mt_remove:")
        .unwrap_or("");
    let (uuid, tag_id_str) = payload.rsplit_once(':').unwrap_or(("", ""));
    let tag_id: i64 = tag_id_str.parse().unwrap_or(0);

    let components = build_manage_main(ctx, data, uuid, Some(tag_id)).await?;
    component
        .create_response(
            &ctx.http,
            CreateInteractionResponse::UpdateMessage(
                CreateInteractionResponseMessage::new()
                    .flags(MessageFlags::IS_COMPONENTS_V2)
                    .components(components)
                    .add_file(face_attachment(data, uuid).await),
            ),
        )
        .await?;
    Ok(())
}

pub async fn handle_manage_confirm(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let payload = component
        .data
        .custom_id
        .strip_prefix("mt_confirm:")
        .unwrap_or("");
    let (uuid, tag_id_str) = payload.rsplit_once(':').unwrap_or(("", ""));
    let tag_id: i64 = tag_id_str.parse().unwrap_or(0);

    let discord_id = component.user.id.get();
    let rank = get_rank(data, discord_id).await?;

    let ops = TagOp::new(data.db.pool());
    match ops
        .remove_by_id(tag_id, discord_id as i64, rank.to_level())
        .await
    {
        Ok((_, tag)) => {
            data.event_publisher
                .publish(&BlacklistEvent::TagRemoved {
                    uuid: uuid.to_string(),
                    tag_id: tag.id,
                    removed_by: discord_id as i64,
                    silent: true,
                })
                .await;

            if tag.tag_type.as_deref() == Some("confirmed_cheater") {
                try_archive_evidence(ctx, data, uuid).await;
            }
            update_manage_view(ctx, component, data, uuid).await
        }
        Err(e) => send_component_message(ctx, component, op_error_message(&e)).await,
    }
}

pub async fn handle_manage_back(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let uuid = parse_manage_uuid(&component.data.custom_id);
    update_manage_view(ctx, component, data, uuid).await
}

pub async fn handle_manage_add_button(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let uuid = parse_manage_uuid(&component.data.custom_id);
    let type_options: Vec<CreateSelectMenuOption<'static>> = blacklist::all()
        .iter()
        .filter(|t| t.name != "confirmed_cheater")
        .map(|t| CreateSelectMenuOption::new(t.display_name, t.name))
        .collect();
    let picker = CreateContainer::new(vec![
        face_section(vec![format!(
            "## {} Add Tag\nSelect a tag type to add:",
            EMOTE_ADDTAG
        )]),
        CreateContainerComponent::ActionRow(CreateActionRow::SelectMenu(
            CreateSelectMenu::new(
                format!("mt_add:{uuid}"),
                CreateSelectMenuKind::String {
                    options: type_options.into(),
                },
            )
            .placeholder("Tag type..."),
        )),
        CreateContainerComponent::ActionRow(CreateActionRow::buttons(vec![
            CreateButton::new(format!("mt_back:{uuid}"))
                .label("Cancel")
                .style(ButtonStyle::Secondary),
        ])),
    ]);
    component
        .create_response(
            &ctx.http,
            CreateInteractionResponse::UpdateMessage(
                CreateInteractionResponseMessage::new()
                    .flags(MessageFlags::IS_COMPONENTS_V2)
                    .components(vec![CreateComponent::Container(picker)])
                    .add_file(face_attachment(data, uuid).await),
            ),
        )
        .await?;
    Ok(())
}

pub async fn handle_manage_add_select(
    ctx: &Context,
    component: &ComponentInteraction,
    _data: &Data,
) -> Result<()> {
    let tag_type = match &component.data.kind {
        ComponentInteractionDataKind::StringSelect { values } => {
            values.first().map(|s| s.as_str()).unwrap_or("")
        }
        _ => return Ok(()),
    };
    let uuid = component
        .data
        .custom_id
        .strip_prefix("mt_add:")
        .unwrap_or("");

    if tag_type == "replays_needed" {
        let input = CreateInputText::new(InputTextStyle::Short, "manage_days")
            .placeholder("Days until expiry (0 = permanent)")
            .max_length(3)
            .required(true)
            .value("14");
        let modal = CreateModal::new(format!("mt_expiry:{uuid}"), "Add Replays Needed Tag")
            .components(vec![CreateModalComponent::Label(CreateLabel::input_text(
                "Expiry (days)",
                input,
            ))]);
        component
            .create_response(&ctx.http, CreateInteractionResponse::Modal(modal))
            .await?;
    } else {
        let input = CreateInputText::new(InputTextStyle::Paragraph, "manage_reason")
            .placeholder("Reason for this tag")
            .max_length(120)
            .required(true);
        let display = lookup_tag(tag_type)
            .map(|d| d.display_name)
            .unwrap_or(tag_type);
        let modal = CreateModal::new(
            format!("mt_reason:{uuid}:{tag_type}"),
            format!("Add {display} Tag"),
        )
        .components(vec![CreateModalComponent::Label(CreateLabel::input_text(
            "Reason", input,
        ))]);
        component
            .create_response(&ctx.http, CreateInteractionResponse::Modal(modal))
            .await?;
    }
    Ok(())
}

enum ManagePlaceOutcome {
    Added,
    NeedsConfirmation { conflict: database::PlayerEvent },
}

async fn manage_place_tag(
    data: &Data,
    uuid: &str,
    modal_id: u64,
    tag_type: &str,
    reason: &str,
    discord_id: u64,
    rank: AccessRank,
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<ManagePlaceOutcome> {
    let ops = TagOp::new(data.db.pool());
    match ops
        .add(
            uuid,
            tag_type,
            reason,
            discord_id as i64,
            rank.to_level(),
            false,
            None,
            expires_at,
        )
        .await
    {
        Ok(new_tag) => {
            data.event_publisher
                .publish(&BlacklistEvent::TagAdded {
                    uuid: uuid.to_string(),
                    tag_id: new_tag.id,
                    added_by: discord_id as i64,
                    silent: true,
                })
                .await;
            Ok(ManagePlaceOutcome::Added)
        }
        Err(TagOpError::PriorityConflict(conflict)) => {
            let old_tag_type = conflict.tag_type.clone().unwrap_or_default();
            data.pending_overwrites.lock().unwrap().insert(
                modal_id.to_string(),
                PendingOverwrite {
                    uuid: uuid.to_string(),
                    old_tag_type,
                    tag_type: tag_type.to_string(),
                    reason: reason.to_string(),
                    hide: false,
                    silent: true,
                },
            );
            Ok(ManagePlaceOutcome::NeedsConfirmation { conflict })
        }
        Err(e) => Err(anyhow::anyhow!("{}", op_error_message(&e))),
    }
}

async fn show_manage_replace_prompt(
    ctx: &Context,
    modal: &ModalInteraction,
    conflict: &database::PlayerEvent,
    new_tag_type: &str,
    new_reason: &str,
) -> Result<()> {
    let conflict_type = conflict.tag_type.as_deref().unwrap_or("");
    let old_added = format_added_line(ctx, conflict).await;
    let current_block = format_tag_block(
        conflict_type,
        &format_tag_detail(conflict),
        "",
        Some(&old_added),
        None,
        false,
    );
    let new_block = format_tag_block(
        new_tag_type,
        &sanitize_reason(new_reason),
        "",
        None,
        None,
        false,
    );

    let key = modal.id.get().to_string();
    let replace_btn = CreateButton::new(format!("tag_overwrite:{key}"))
        .label("Replace Tag")
        .style(ButtonStyle::Danger);

    let container = CreateContainer::new(vec![
        CreateContainerComponent::TextDisplay(CreateTextDisplay::new(format!(
            "## {} Replace Tag",
            EMOTE_EDITTAG
        ))),
        CreateContainerComponent::TextDisplay(CreateTextDisplay::new(
            "Replaces the current tag. Recorded in tag history; no public log entry.",
        )),
        CreateContainerComponent::Separator(CreateSeparator::new(true)),
        CreateContainerComponent::TextDisplay(CreateTextDisplay::new("-# Current")),
        CreateContainerComponent::TextDisplay(CreateTextDisplay::new(current_block)),
        CreateContainerComponent::Separator(CreateSeparator::new(true)),
        CreateContainerComponent::TextDisplay(CreateTextDisplay::new("-# New")),
        CreateContainerComponent::Section(CreateSection::new(
            vec![CreateSectionComponent::TextDisplay(CreateTextDisplay::new(
                new_block,
            ))],
            CreateSectionAccessory::Button(replace_btn),
        )),
    ]);

    modal
        .create_response(
            &ctx.http,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .flags(MessageFlags::IS_COMPONENTS_V2)
                    .ephemeral(true)
                    .components(vec![CreateComponent::Container(container)]),
            ),
        )
        .await?;
    Ok(())
}

pub async fn handle_manage_reason_modal(
    ctx: &Context,
    modal: &ModalInteraction,
    data: &Data,
) -> Result<()> {
    let payload = modal
        .data
        .custom_id
        .strip_prefix("mt_reason:")
        .unwrap_or("");
    let (uuid, tag_type) = payload.rsplit_once(':').unwrap_or(("", ""));
    if uuid.is_empty() || tag_type.is_empty() {
        return Ok(());
    }

    let reason = interact::extract_modal_value(&modal.data.components, "manage_reason");
    let discord_id = modal.user.id.get();
    let rank = get_rank(data, discord_id).await?;

    match manage_place_tag(
        data,
        uuid,
        modal.id.get(),
        tag_type,
        &reason,
        discord_id,
        rank,
        None,
    )
    .await
    {
        Ok(ManagePlaceOutcome::Added) => refresh_manage_modal(ctx, modal, data, uuid).await,
        Ok(ManagePlaceOutcome::NeedsConfirmation { conflict }) => {
            show_manage_replace_prompt(ctx, modal, &conflict, tag_type, &reason).await
        }
        Err(e) => manage_simple_response(ctx, modal, &e.to_string()).await,
    }
}

pub async fn handle_manage_expiry_modal(
    ctx: &Context,
    modal: &ModalInteraction,
    data: &Data,
) -> Result<()> {
    let uuid = modal
        .data
        .custom_id
        .strip_prefix("mt_expiry:")
        .unwrap_or("");
    if uuid.is_empty() {
        return Ok(());
    }

    let days_str = interact::extract_modal_value(&modal.data.components, "manage_days");
    let days: i64 = days_str.trim().parse().unwrap_or(14);
    let expires_at = if days == 0 {
        None
    } else {
        Some(chrono::Utc::now() + chrono::Duration::days(days))
    };

    let discord_id = modal.user.id.get();
    let rank = get_rank(data, discord_id).await?;

    match manage_place_tag(
        data,
        uuid,
        modal.id.get(),
        "replays_needed",
        "",
        discord_id,
        rank,
        expires_at,
    )
    .await
    {
        Ok(ManagePlaceOutcome::Added) => refresh_manage_modal(ctx, modal, data, uuid).await,
        Ok(ManagePlaceOutcome::NeedsConfirmation { conflict }) => {
            show_manage_replace_prompt(ctx, modal, &conflict, "replays_needed", "").await
        }
        Err(e) => manage_simple_response(ctx, modal, &e.to_string()).await,
    }
}

async fn refresh_manage_modal(
    ctx: &Context,
    modal: &ModalInteraction,
    data: &Data,
    uuid: &str,
) -> Result<()> {
    let components = build_manage_main(ctx, data, uuid, None).await?;
    modal
        .create_response(
            &ctx.http,
            CreateInteractionResponse::UpdateMessage(
                CreateInteractionResponseMessage::new()
                    .flags(MessageFlags::IS_COMPONENTS_V2)
                    .components(components)
                    .add_file(face_attachment(data, uuid).await),
            ),
        )
        .await?;
    Ok(())
}

async fn manage_simple_response(
    ctx: &Context,
    modal: &ModalInteraction,
    message: &str,
) -> Result<()> {
    modal
        .create_response(
            &ctx.http,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .content(message)
                    .ephemeral(true),
            ),
        )
        .await?;
    Ok(())
}

async fn send_component_message(
    ctx: &Context,
    component: &ComponentInteraction,
    message: &str,
) -> Result<()> {
    component
        .create_response(
            &ctx.http,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .content(message)
                    .ephemeral(true),
            ),
        )
        .await?;
    Ok(())
}
